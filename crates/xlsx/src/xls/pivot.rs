//! PivotTables in `.xls` files ([MS-XLS] 2.2.5, PivotTables): the pivot caches the Globals
//! substream declares (SXStreamID 2.4.276, SXVS 2.4.317, DConRef 2.4.86, DConName 2.4.85,
//! DConBin 2.4.83), each cache's stream in the `_SX_DB_CUR` storage (SXDB 2.4.275, SXFDB 2.4.283
//! and the cache items SXString, SXNum, SXBool, SXErr, SXInt, SXDtr, SxNil), and the PivotTable
//! views in the worksheet substreams (SxView 2.4.313, Sxvd 2.4.314, SXVI 2.4.312, SXVDEx 2.4.310,
//! SxIvd 2.4.292, SXPI 2.4.297, SXDI 2.4.278).
//!
//! A view on a worksheet-range or defined-name source becomes a model [`PivotTable`] that
//! refreshes from its source cells, like one read from an `.xlsx`. Views on external, OLAP,
//! consolidation or scenario sources, and those using calculated fields, calculated items or
//! grouping, are skipped with a warning: their report cells stay as plain values.

use std::collections::HashMap;
use std::rc::Rc;

use gridcraft_core::date::serial_from_ymd;
use gridcraft_core::{CellRef, DateSystem, MAX_COLS, MAX_ROWS, RangeRef, number_to_text};
use gridcraft_formula::quote_sheet;
use gridcraft_model::{PivotField, PivotFilter, PivotFunc, PivotLayout, PivotShowAs, PivotSort, PivotTable, PivotValue};

use super::records::{Cursor, Records};
use super::{EOF, cell_error};
use crate::cfb::Compound;
use crate::tables::builtin_format;
use crate::write::abs_range;
use crate::{IoError, ReadReport};

/// The storage holding one stream per pivot cache ([MS-XLS] 2.1.7.2).
const CACHE_STORAGE: &str = "_SX_DB_CUR";
/// Records read from one cache stream at most.
const MAX_CACHE_RECORDS: usize = 1_000_000;
/// Pivot caches, views per sheet, fields per view or cache, and items per field kept at most.
const MAX_CACHES: usize = 65_536;
const MAX_VIEWS: usize = 4096;
const MAX_FIELDS: usize = 16_384;
const MAX_ITEMS: usize = 1_048_576;

/// SXVS: the cache's source data is a worksheet range or defined name.
const SXVS_SHEET: u16 = 0x0001;
/// SxIvd and SXPI: the data field (Σ Values) among the row or column fields.
const DATA_FIELD: i16 = -2;
/// SXPI_Item.isxvi: every item is shown.
const ALL_ITEMS: i16 = 0x7FFD;

// ================================================================ globals: cache declarations

/// Where a pivot cache's records come from.
enum Source {
    /// A range on a sheet of this workbook.
    Range { sheet: String, range: RangeRef },
    /// A defined name (with its sheet when it is sheet-scoped).
    Name { sheet: Option<String>, name: String },
    /// Another workbook or a built-in name.
    Unsupported,
}

/// One PIVOTCACHEDEFINITION of the Globals substream.
struct CacheDef {
    stream: u16,
    kind: Option<u16>,
    source: Option<Source>,
}

/// The pivot caches the Globals substream declares, in SXStreamID order (`SxView.iCache`).
#[derive(Default)]
pub(super) struct Caches {
    defs: Vec<CacheDef>,
}

/// DConFile ([MS-XLS] 2.5.69): `cch` characters after the `fHighByte` flag. A self-reference
/// (first character 2) names a sheet of this workbook; anything else is another file.
fn dcon_file(c: &mut Cursor<'_, '_>, cch: usize) -> Result<Option<String>, IoError> {
    let high = c.u8()? & 1 != 0;
    let s = c.chars(cch, high)?;
    Ok(s.strip_prefix('\u{2}').map(str::to_string))
}

impl Caches {
    /// The definition the source records that follow belong to: the last one, while its source
    /// is unset.
    fn open(&mut self) -> Option<&mut CacheDef> {
        self.defs.last_mut().filter(|d| d.source.is_none())
    }

    /// SXStreamID: starts a cache definition.
    pub(super) fn stream_id(&mut self, c: &mut Cursor<'_, '_>) {
        if self.defs.len() >= MAX_CACHES {
            return;
        }
        if let Ok(stream) = c.u16() {
            self.defs.push(CacheDef { stream, kind: None, source: None });
        }
    }

    /// SXVS: the source type.
    pub(super) fn source_type(&mut self, c: &mut Cursor<'_, '_>) {
        if let (Some(d), Ok(kind)) = (self.defs.last_mut(), c.u16()) {
            d.kind = Some(kind);
        }
    }

    /// DConRef: a range source.
    pub(super) fn dcon_ref(&mut self, c: &mut Cursor<'_, '_>) {
        let Some(d) = self.open() else { return };
        let read = |c: &mut Cursor<'_, '_>| -> Result<Source, IoError> {
            let (r0, r1, c0, c1) = (c.u16()?, c.u16()?, c.u8()?, c.u8()?);
            let cch = usize::from(c.u16()?);
            let sheet = dcon_file(c, cch)?;
            let (r0, r1, c0, c1) = (u32::from(r0), u32::from(r1), u32::from(c0), u32::from(c1));
            Ok(match sheet {
                Some(sheet) if r0 <= r1 && c0 <= c1 => Source::Range { sheet, range: RangeRef::new(CellRef::new(r0, c0), CellRef::new(r1, c1)) },
                _ => Source::Unsupported,
            })
        };
        d.source = Some(read(c).unwrap_or(Source::Unsupported));
    }

    /// DConName: a defined-name source.
    pub(super) fn dcon_name(&mut self, c: &mut Cursor<'_, '_>) {
        let Some(d) = self.open() else { return };
        let read = |c: &mut Cursor<'_, '_>| -> Result<Source, IoError> {
            let name = c.string()?;
            let cch = usize::from(c.u16()?);
            if cch == 0 {
                return Ok(Source::Name { sheet: None, name });
            }
            Ok(match dcon_file(c, cch)? {
                Some(sheet) => Source::Name { sheet: Some(sheet), name },
                None => Source::Unsupported,
            })
        };
        d.source = Some(read(c).unwrap_or(Source::Unsupported));
    }

    /// DConBin: a built-in name (such as `_FilterDatabase`) as the source; not supported.
    pub(super) fn dcon_bin(&mut self) {
        if let Some(d) = self.open() {
            d.source = Some(Source::Unsupported);
        }
    }
}

// ================================================================ the cache stream

/// A cache field: its name, the captions of its cache items, and whether it is a calculated or
/// grouping field.
struct CacheField {
    name: String,
    items: Vec<String>,
    catm: usize,
    unsupported: bool,
}

struct Cache {
    fields: Vec<CacheField>,
}

/// Reads the cache fields of a pivot cache stream (records are not needed: the model
/// recalculates from the source cells).
fn parse_cache(stream: &[u8], sys: DateSystem) -> Result<Cache, &'static str> {
    let mut recs = Records::new(stream, 0);
    let mut fields: Vec<CacheField> = Vec::new();
    let mut seen_db = false;
    for _ in 0..MAX_CACHE_RECORDS {
        let Some(rec) = recs.next_record().map_err(|_| "is truncated")? else {
            return if seen_db { Ok(Cache { fields }) } else { Err("is empty") };
        };
        let mut c = Cursor::new(&rec.parts);
        let item = match rec.id {
            0x00C6 => {
                seen_db = true;
                continue;
            }
            EOF | 0x00C8 => break,
            _ if !seen_db => return Err("does not start with an SXDB record"),
            0x00C7 => {
                if fields.len() >= MAX_FIELDS {
                    return Err("has too many fields");
                }
                let read = |c: &mut Cursor<'_, '_>| -> Result<CacheField, IoError> {
                    let flags = c.u16()?;
                    c.skip(10)?;
                    let catm = usize::from(c.u16()?);
                    let name = c.string()?;
                    // fHasParent, fRangeGroup, fCalculatedField.
                    Ok(CacheField { name, items: vec![], catm, unsupported: flags & 0x8018 != 0 })
                };
                fields.push(read(&mut c).map_err(|_| "has a damaged field")?);
                continue;
            }
            0x00CD => c.string().ok(),
            0x00C9 => c.f64().ok().filter(|n| n.is_finite()).map(number_to_text),
            0x00CA => c.u16().ok().map(|b| if b != 0 { "TRUE" } else { "FALSE" }.to_string()),
            0x00CB => c.u8().ok().map(|e| cell_error(e).as_str().to_string()),
            0x00CC => c.u16().ok().map(|n| (n as i16).to_string()),
            0x00CF => Some("(blank)".to_string()),
            0x00CE => {
                let read = |c: &mut Cursor<'_, '_>| -> Result<(u16, u16, u8, u8, u8, u8), IoError> {
                    Ok((c.u16()?, c.u16()?, c.u8()?, c.u8()?, c.u8()?, c.u8()?))
                };
                read(&mut c).ok().map(|(y, m, d, hh, mm, ss)| {
                    let day = serial_from_ymd(sys, i64::from(y), i64::from(m), i64::from(d)).unwrap_or(0.0);
                    let time = (f64::from(hh) * 3600.0 + f64::from(mm) * 60.0 + f64::from(ss)) / 86_400.0;
                    number_to_text(day + time)
                })
            }
            _ => continue,
        };
        // A cache item belongs to the last field until that field has all `catm` of its own.
        if let (Some(f), Some(label)) = (fields.last_mut(), item)
            && f.items.len() < f.catm.min(MAX_ITEMS)
        {
            f.items.push(label);
        }
    }
    if !seen_db {
        return Err("does not start with an SXDB record");
    }
    Ok(Cache { fields })
}

// ================================================================ the sheet: PivotTable views

/// A pivot item (SXVI).
struct ItemView {
    data: bool,
    hidden: bool,
    collapsed: bool,
    formula: bool,
    cache: i16,
}

/// A pivot field (Sxvd, its SXVI records and SXVDEx).
#[derive(Default)]
struct FieldView {
    subtotals: bool,
    items: Vec<ItemView>,
    outline: bool,
    subtotal_top: bool,
    /// AutoSort by the item captions: `Some(ascending)`.
    sort: Option<bool>,
}

/// A data item (SXDI).
struct DataItem {
    field: i16,
    func: u16,
    show_as: u16,
    ifmt: u16,
    name: Option<String>,
}

/// One PivotTable view: an SxView record and the records that follow it.
struct View {
    body: Option<RangeRef>,
    cache: u16,
    data_on_rows: bool,
    has_rows: bool,
    name: String,
    row_grand: bool,
    col_grand: bool,
    fields: Vec<FieldView>,
    rows: Vec<i16>,
    cols: Vec<i16>,
    ivds: u8,
    pages: Vec<(i16, i16)>,
    data: Vec<DataItem>,
}

/// The PivotTable views of one worksheet, as read.
#[derive(Default)]
pub(super) struct Views {
    views: Vec<View>,
    damaged: bool,
}

/// An XLUnicodeStringNoCch of `cch` characters; `0xFFFF` means none.
fn no_cch(c: &mut Cursor<'_, '_>, cch: u16) -> Result<Option<String>, IoError> {
    if cch == 0xFFFF || cch == 0 {
        return Ok(None);
    }
    let high = c.u8()? & 1 != 0;
    Ok(Some(c.chars(usize::from(cch), high)?))
}

fn i16s(c: &mut Cursor<'_, '_>) -> Vec<i16> {
    let n = (c.remaining() / 2).min(MAX_FIELDS);
    (0..n).map_while(|_| c.u16().ok().map(|v| v as i16)).collect()
}

impl Views {
    /// Reads a record of a PivotTable view. Returns whether `id` is one.
    pub(super) fn record(&mut self, id: u16, c: &mut Cursor<'_, '_>) -> bool {
        let r = match id {
            0x00B0 => self.sx_view(c),
            0x00B1 => self.sxvd(c),
            0x00B2 => self.sxvi(c),
            0x0100 => self.sxvdex(c),
            0x00B4 => {
                if let Some(v) = self.views.last_mut() {
                    let list = i16s(c);
                    if v.ivds == 0 && v.has_rows {
                        v.rows = list;
                    } else if v.cols.is_empty() {
                        v.cols = list;
                    }
                    v.ivds = v.ivds.saturating_add(1);
                }
                Ok(())
            }
            0x00B6 => {
                if let Some(v) = self.views.last_mut() {
                    let list = i16s(c);
                    v.pages = list.as_chunks::<3>().0.iter().map(|[field, item, _]| (*field, *item)).collect();
                }
                Ok(())
            }
            0x00C5 => self.sxdi(c),
            _ => return false,
        };
        if r.is_err() {
            self.damaged = true;
        }
        true
    }

    fn sx_view(&mut self, c: &mut Cursor<'_, '_>) -> Result<(), IoError> {
        if self.views.len() >= MAX_VIEWS {
            return Ok(());
        }
        // Push first, so that the records that follow a damaged SxView are not taken for the
        // previous view's.
        self.views.push(View {
            body: None,
            cache: 0,
            data_on_rows: false,
            has_rows: false,
            name: String::new(),
            row_grand: true,
            col_grand: true,
            fields: vec![],
            rows: vec![],
            cols: vec![],
            ivds: 0,
            pages: vec![],
            data: vec![],
        });
        let Some(v) = self.views.last_mut() else { return Ok(()) };
        let (r0, r1, c0, c1) = (c.u16()?, c.u16()?, c.u16()?, c.u16()?);
        c.skip(6)?;
        v.cache = c.u16()?;
        c.skip(2)?;
        v.data_on_rows = c.u16()? & 1 != 0;
        c.skip(4)?;
        v.has_rows = c.u16()? > 0;
        c.skip(10)?;
        let flags = c.u16()?;
        v.row_grand = flags & 1 != 0;
        v.col_grand = flags & 2 != 0;
        c.skip(2)?;
        let cch_table = c.u16()?;
        c.skip(2)?;
        if let Some(name) = no_cch(c, cch_table)? {
            v.name = name;
        }
        let (r0, r1, c0, c1) = (u32::from(r0), u32::from(r1), u32::from(c0), u32::from(c1));
        if r0 <= r1 && c0 <= c1 && r1 < MAX_ROWS && c1 < MAX_COLS {
            v.body = Some(RangeRef::new(CellRef::new(r0, c0), CellRef::new(r1, c1)));
        }
        Ok(())
    }

    fn sxvd(&mut self, c: &mut Cursor<'_, '_>) -> Result<(), IoError> {
        let Some(v) = self.views.last_mut() else { return Ok(()) };
        if v.fields.len() >= MAX_FIELDS {
            return Ok(());
        }
        c.skip(4)?;
        let sub = c.u16()?;
        // Any of the subtotal functions (fDefault … fVariancep).
        v.fields.push(FieldView { subtotals: sub & 0x0FFF != 0, ..FieldView::default() });
        Ok(())
    }

    fn sxvi(&mut self, c: &mut Cursor<'_, '_>) -> Result<(), IoError> {
        let Some(f) = self.views.last_mut().and_then(|v| v.fields.last_mut()) else { return Ok(()) };
        if f.items.len() >= MAX_ITEMS {
            return Ok(());
        }
        let kind = c.u16()?;
        let flags = c.u16()?;
        let cache = c.u16()? as i16;
        f.items.push(ItemView { data: kind == 0, hidden: flags & 1 != 0, collapsed: flags & 2 != 0, formula: flags & 8 != 0, cache });
        Ok(())
    }

    fn sxvdex(&mut self, c: &mut Cursor<'_, '_>) -> Result<(), IoError> {
        let Some(f) = self.views.last_mut().and_then(|v| v.fields.last_mut()) else { return Ok(()) };
        let flags = c.u32()?;
        let by_data = c.u16()? as i16;
        f.outline = flags & (1 << 21) != 0;
        f.subtotal_top = flags & (1 << 23) != 0;
        // fAutoSort on the item captions (isxdiAutoSort -1), fAscendSort.
        if flags & (1 << 9) != 0 && by_data < 0 {
            f.sort = Some(flags & (1 << 10) != 0);
        }
        Ok(())
    }

    fn sxdi(&mut self, c: &mut Cursor<'_, '_>) -> Result<(), IoError> {
        let Some(v) = self.views.last_mut() else { return Ok(()) };
        if v.data.len() >= MAX_FIELDS {
            return Ok(());
        }
        let field = c.u16()? as i16;
        let func = c.u16()?;
        let show_as = c.u16()?;
        c.skip(4)?;
        let ifmt = c.u16()?;
        let cch = c.u16()?;
        let name = no_cch(c, cch)?;
        v.data.push(DataItem { field, func, show_as, ifmt, name });
        Ok(())
    }
}

// ================================================================ building model PivotTables

/// Turns PivotTable views into model PivotTables, reading each pivot cache stream once.
pub(super) struct Loader<'a> {
    cf: &'a Compound<'a>,
    caches: &'a Caches,
    formats: &'a HashMap<u16, String>,
    sys: DateSystem,
    parsed: HashMap<usize, Result<Rc<Cache>, String>>,
    next_id: u32,
}

fn func(iiftab: u16) -> Option<PivotFunc> {
    Some(match iiftab {
        0 => PivotFunc::Sum,
        1 => PivotFunc::Count,
        2 => PivotFunc::Average,
        3 => PivotFunc::Max,
        4 => PivotFunc::Min,
        5 => PivotFunc::Product,
        6 => PivotFunc::CountNumbers,
        7 => PivotFunc::StdDev,
        8 => PivotFunc::StdDevP,
        9 => PivotFunc::Var,
        10 => PivotFunc::VarP,
        _ => return None,
    })
}

impl<'a> Loader<'a> {
    pub(super) fn new(cf: &'a Compound<'a>, caches: &'a Caches, formats: &'a HashMap<u16, String>, sys: DateSystem) -> Self {
        Loader { cf, caches, formats, sys, parsed: HashMap::new(), next_id: 1 }
    }

    fn cache(&mut self, index: usize, stream: u16) -> Result<Rc<Cache>, String> {
        if let Some(c) = self.parsed.get(&index) {
            return c.clone();
        }
        let name = format!("{stream:04X}");
        let c = match self.cf.storage_stream(CACHE_STORAGE, &name) {
            Ok(Some(bytes)) => parse_cache(&bytes, self.sys).map(Rc::new).map_err(|why| format!("its pivot cache {why}")),
            Ok(None) => Err("its pivot cache is missing".to_string()),
            Err(e) => Err(format!("its pivot cache can't be read ({e})")),
        };
        self.parsed.insert(index, c.clone());
        c
    }

    /// The model PivotTables of one sheet's views; the others become warnings.
    pub(super) fn build(&mut self, views: Views, report: &mut ReadReport) -> Vec<PivotTable> {
        if views.damaged {
            report.warn("Some PivotTable records in this .xls file are damaged; those PivotTables may be incomplete.");
        }
        let mut out = Vec::new();
        for v in views.views {
            let label = if v.name.is_empty() { "A PivotTable".to_string() } else { format!("PivotTable \"{}\"", v.name) };
            match self.table(v) {
                Ok(pt) => out.push(pt),
                Err(why) => report.warn(format!("{label} was kept as plain cells: {why}.")),
            }
        }
        out
    }

    fn table(&mut self, v: View) -> Result<PivotTable, String> {
        let body = v.body.ok_or("its location is invalid")?;
        let index = usize::from(v.cache);
        let caches = self.caches;
        let def = caches.defs.get(index).ok_or("its pivot cache is not declared")?;
        if def.kind != Some(SXVS_SHEET) {
            return Err("its data source (external data, OLAP, consolidation or scenario) is not supported".into());
        }
        let source = match &def.source {
            Some(Source::Range { sheet, range }) => format!("{}!{}", quote_sheet(sheet), abs_range(range)),
            Some(Source::Name { sheet: None, name }) => name.clone(),
            Some(Source::Name { sheet: Some(sheet), name }) => format!("{}!{name}", quote_sheet(sheet)),
            Some(Source::Unsupported) => return Err("its source is in another workbook or is a built-in name".into()),
            None => return Err("its source range is missing".into()),
        };
        let cache = self.cache(index, def.stream)?;
        if cache.fields.iter().any(|f| f.unsupported) {
            return Err("calculated fields and grouped fields are not supported".into());
        }
        if v.fields.iter().any(|f| f.items.iter().any(|i| i.formula)) {
            return Err("calculated items are not supported".into());
        }
        let field_name = |i: i16| -> Result<String, String> {
            usize::try_from(i)
                .ok()
                .and_then(|i| cache.fields.get(i))
                .map(|f| f.name.clone())
                .ok_or_else(|| "a field index is out of range".to_string())
        };
        // The caption of a pivot item through its cache item.
        let caption = |f: usize, it: &ItemView| -> Option<String> { cache.fields.get(f)?.items.get(usize::try_from(it.cache).ok()?).cloned() };

        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        let mut pt = PivotTable { id, source, anchor: body.start, ..PivotTable::default() };
        pt.name = if v.name.trim().is_empty() { format!("PivotTable{id}") } else { v.name.clone() };
        pt.grand_totals_rows = v.col_grand;
        pt.grand_totals_cols = v.row_grand;

        // Layout from the first row or column field: Excel 97-2003 PivotTables are in tabular
        // form unless the field is in outline form.
        let first_axis = v.rows.iter().chain(v.cols.iter()).find_map(|i| usize::try_from(*i).ok().and_then(|i| v.fields.get(i)));
        pt.layout = PivotLayout::Tabular;
        if let Some(f) = first_axis {
            pt.layout = if f.outline { PivotLayout::Outline } else { PivotLayout::Tabular };
            pt.subtotals = f.subtotals;
            pt.subtotals_top = f.subtotal_top;
        }

        let mut axis_filters: Vec<PivotFilter> = Vec::new();
        let mut axis = |list: &[i16]| -> Result<(Vec<PivotField>, Option<usize>), String> {
            let mut out = Vec::new();
            let mut values_at = None;
            for &i in list {
                if i == DATA_FIELD {
                    values_at = Some(out.len());
                    continue;
                }
                let name = field_name(i)?;
                let Some((k, fv)) = usize::try_from(i).ok().and_then(|k| Some((k, v.fields.get(k)?))) else {
                    out.push(PivotField { source_col: name, sort: PivotSort::None, ..PivotField::default() });
                    continue;
                };
                let data: Vec<&ItemView> = fv.items.iter().filter(|it| it.data).collect();
                if data.iter().any(|it| it.hidden) {
                    let shown = data.iter().filter(|it| !it.hidden).filter_map(|it| caption(k, it)).collect();
                    axis_filters.push(PivotFilter { source_col: name.clone(), selected: Some(shown) });
                }
                out.push(PivotField {
                    source_col: name,
                    sort: match fv.sort {
                        Some(true) => PivotSort::Asc,
                        Some(false) => PivotSort::Desc,
                        None => PivotSort::None,
                    },
                    collapsed_items: data.iter().filter(|it| it.collapsed).filter_map(|it| caption(k, it)).collect(),
                    ..PivotField::default()
                });
            }
            Ok((out, values_at))
        };
        let (rows, rows_values) = axis(&v.rows)?;
        let (columns, cols_values) = axis(&v.cols)?;
        pt.rows = rows;
        pt.columns = columns;
        pt.values_on_rows = rows_values.is_some() || (cols_values.is_none() && v.data_on_rows);
        pt.values_position = if pt.values_on_rows { rows_values.filter(|p| *p < pt.rows.len()) } else { None };

        let mut filters = Vec::new();
        for &(field, item) in &v.pages {
            let name = field_name(field)?;
            let k = usize::try_from(field).unwrap_or(usize::MAX);
            let fv = v.fields.get(k);
            let selected = if item == ALL_ITEMS {
                fv.and_then(|fv| {
                    let data: Vec<&ItemView> = fv.items.iter().filter(|it| it.data).collect();
                    data.iter().any(|it| it.hidden).then(|| data.iter().filter(|it| !it.hidden).filter_map(|it| caption(k, it)).collect())
                })
            } else {
                let it = usize::try_from(item).ok().and_then(|i| fv?.items.get(i));
                it.and_then(|it| caption(k, it)).map(|c| vec![c])
            };
            filters.push(PivotFilter { source_col: name, selected });
        }
        filters.extend(axis_filters);
        pt.filters = filters;

        for d in &v.data {
            let field = field_name(d.field)?;
            let func = func(d.func).ok_or("it uses an unknown summary function")?;
            let show_as = match d.show_as {
                4 => PivotShowAs::RunningTotal,
                5 => PivotShowAs::PercentOfRowTotal,
                6 => PivotShowAs::PercentOfColumnTotal,
                7 => PivotShowAs::PercentOfGrandTotal,
                _ => PivotShowAs::Normal,
            };
            let number_format =
                (d.ifmt != 0).then(|| builtin_format(u32::from(d.ifmt)).map(str::to_string).or_else(|| self.formats.get(&d.ifmt).cloned())).flatten();
            let name = d.name.clone().unwrap_or_else(|| format!("{} of {field}", crate::pivot::func_caption(func)));
            pt.values.push(PivotValue { source_col: field, func, name, show_as, number_format });
        }
        if v.data.iter().any(|d| !matches!(d.show_as, 0 | 4..=7)) {
            return Err("a show-values-as calculation it uses is not supported".into());
        }

        let above = if v.pages.is_empty() { 0 } else { v.pages.len() as u32 + 1 };
        pt.last_range = Some(RangeRef::new(CellRef::new(body.start.row.saturating_sub(above), body.start.col), body.end));
        Ok(pt)
    }
}
