//! XLSB PivotTables, from [MS-XLSB] 2.1.7.38 (PivotCache Definition part), 2.1.7.40 (PivotTable
//! part) and the records of 2.4. The workbook maps cache ids to cache parts with
//! `BrtBeginPivotCacheID`; a worksheet reaches its PivotTable parts through its relationships, and
//! each PivotTable names its cache by id (and usually by relationship too).
//!
//! Both parts become the model's [`PivotTable`] the way the .xlsx reader builds it from
//! `pivotCacheDefinition` and `pivotTableDefinition`: the cache's source and field names, the
//! table's areas, functions, show-values-as, layout flags, grand totals and style. The records
//! part is not read: the report is rebuilt from the source on refresh. Anything the model can't
//! hold becomes a warning, and a damaged part keeps the report as plain cells.
//!
//! A calculated field (2.2.5.2.5) is a `BrtBeginPCDField` with `fLoadFmla` set: its formula is
//! the record's `fldFmla`, a `PivotParsedFormula` (2.5.98.15), after the name and the optional
//! caption. The field's `PNAMES` collection (`BrtBeginPName` records, part ABNF 2.1.7.38) lists the cache fields the
//! formula uses by index (`ifdb`), and each `PtgSxName` token names one of them by its position
//! there. The formula becomes [`PivotCalcField`] text with field names as Excel writes them
//! (`=Sales/Units`, `='Unit Price'*0.1`), as the .xlsx reader reads `cacheField/@formula`.

use std::collections::HashMap;

use gridcraft_core::{CellRef, DateSystem, MAX_COLS, MAX_ROWS, RangeRef, number_to_text};
use gridcraft_formula::quote_sheet;
use gridcraft_model::{
    PivotCalcField, PivotDateGroup, PivotField, PivotFilter, PivotFunc, PivotLayout, PivotShowAs, PivotSort, PivotTable, PivotValue,
};

use super::{Bytes, Entry, Records, check_type, error, invalid, parse_relationships, rels_path};
use crate::package::{Package, Rel};
use crate::pivot::{MAX_FIELDS, MAX_ITEMS, func_caption, same};
use crate::write::abs_range;
use crate::xls::formula::{Links, Mode, decode_xlsb};
use crate::{IoError, ReadReport};

const CACHE_TYPE: &str = "application/vnd.ms-excel.pivotCacheDefinition";
const TABLE_TYPES: [&str; 2] = ["application/vnd.ms-excel.pivotTable", "application/vnd.ms-excel.PivotTable"];
const MAX_PIVOT_PART: u64 = 64 * 1024 * 1024;
const MAX_RELS_PART: u64 = 1024 * 1024;
/// PivotTables read per sheet and cache ids read per workbook.
const MAX_PIVOTS: usize = 1024;
const MAX_CACHE_IDS: usize = 65_536;
const DAMAGED_CACHE: &str = "A damaged XLSB pivot cache was skipped; its PivotTables were kept as plain cells.";
const DAMAGED_TABLE: &str = "A damaged XLSB PivotTable was kept as plain cells.";

// Workbook part.
const BEGIN_PIVOT_CACHE_ID: u16 = 386;
// PivotCache Definition part.
const BEGIN_PIVOT_CACHE_DEF: u16 = 179;
const END_PIVOT_CACHE_DEF: u16 = 180;
const BEGIN_PCD_FIELD: u16 = 183;
const END_PCD_FIELD: u16 = 184;
const BEGIN_PCD_SOURCE: u16 = 185;
const BEGIN_PCD_SRANGE: u16 = 187;
const BEGIN_PCD_FATBL: u16 = 189;
const END_PCD_FATBL: u16 = 190;
const BEGIN_PCDI_RUN: u16 = 191;
const BEGIN_PCD_SCONSOL: u16 = 207;
const BEGIN_PCD_FGROUP: u16 = 219;
const BEGIN_PCD_FG_ITEMS: u16 = 221;
const END_PCD_FG_ITEMS: u16 = 222;
const BEGIN_PCD_FG_RANGE: u16 = 223;
const BEGIN_PNAME: u16 = 255;
const PCDI_MISSING: u16 = 20;
const PCDI_NUMBER: u16 = 21;
const PCDI_BOOLEAN: u16 = 22;
const PCDI_ERROR: u16 = 23;
const PCDI_STRING: u16 = 24;
const PCDI_DATETIME: u16 = 25;
/// `BrtPCDIAMissing` … `BrtPCDIADatetime` (27 … 32) are the same items as 20 … 25.
const PCDIA_OFFSET: u16 = 7;
// PivotTable part.
const BEGIN_SX_VIEW: u16 = 280;
const BEGIN_SXVI: u16 = 282;
const BEGIN_SXVD: u16 = 285;
const BEGIN_SXPI: u16 = 289;
const BEGIN_SXDI: u16 = 293;
const BEGIN_ISXVD_RWS: u16 = 309;
const BEGIN_ISXVD_COLS: u16 = 311;
const BEGIN_SX_LOCATION: u16 = 314;
const END_SX_VIEW: u16 = 315;
const TABLE_STYLE_CLIENT: u16 = 513;
const BEGIN_SX_FILTERS: u16 = 599;

/// `BrtBeginSXPI.isxvi` for a page field showing all (or several) items.
const PAGE_ALL_ITEMS: u32 = 0x0010_00FE;

/// The `BrtBeginPivotCacheID` records of the workbook part: (cache id, relationship id). The part
/// was already validated by the workbook reader; anything unreadable here just ends the list.
pub(super) fn cache_ids(book: &[u8]) -> Vec<(u32, String)> {
    let mut scratch = ReadReport::default();
    let mut records = Records::new(book);
    let mut out = vec![];
    while let Ok(Some((id, payload))) = records.next(&mut scratch) {
        if id != BEGIN_PIVOT_CACHE_ID || out.len() >= MAX_CACHE_IDS {
            continue;
        }
        let mut b = Bytes::new(payload);
        if let (Ok(cache), Ok(Some(rel))) = (b.u32(), b.string(255, true)) {
            out.push((cache, rel));
        }
    }
    out
}

/// A cache field as read.
#[derive(Clone, Debug, Default)]
struct FieldIn {
    name: String,
    database: bool,
    /// A calculated field (`fLoadFmla`).
    calculated: bool,
    /// A calculated field's formula as read: (`rgce`, `rgcb`).
    tokens: Option<(Vec<u8>, Vec<u8>)>,
    /// The cache fields (`ifdb`) its `PtgSxName` tokens refer to, in `BrtBeginPName` order.
    pnames: Vec<u32>,
    /// The decoded formula (`=Sales/Units`); `None` for a calculated field that isn't imported.
    formula: Option<String>,
    items: Vec<String>,
    /// (base field, grouping, group item captions); grouping `None` = unsupported grouping.
    group: Option<(Option<usize>, Option<PivotDateGroup>, Vec<String>)>,
}

impl FieldIn {
    fn labels(&self) -> &[String] {
        match &self.group {
            Some((_, _, items)) if !items.is_empty() => items,
            _ => &self.items,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct CacheIn {
    source: String,
    fields: Vec<FieldIn>,
}

/// Workbook-level pivot cache lookup; each cache part is parsed once.
pub(super) struct Caches {
    by_id: HashMap<u32, String>,
    parsed: HashMap<String, Option<CacheIn>>,
    next_id: u32,
    /// Custom number formats of the styles part, by id.
    formats: HashMap<u16, String>,
    date_system: DateSystem,
}

impl Caches {
    pub(super) fn new(ids: &[(u32, String)], book_rels: &[Rel], formats: HashMap<u16, String>, date_system: DateSystem) -> Caches {
        let mut by_id = HashMap::new();
        for (cache, rid) in ids {
            if let Some(r) = book_rels.iter().find(|r| &r.id == rid && r.kind == "pivotCacheDefinition" && !r.external) {
                by_id.insert(*cache, r.target.clone());
            }
        }
        Caches { by_id, parsed: HashMap::new(), next_id: 1, formats, date_system }
    }
}

/// A part's relationships; none when it has none or they can't be read.
fn part_rels(package: &mut Package<'_>, path: &str) -> Vec<Rel> {
    match package.read_limited(&rels_path(path), MAX_RELS_PART) {
        Ok(Some(data)) => parse_relationships(&data, path).unwrap_or_default(),
        _ => vec![],
    }
}

fn read_part(package: &mut Package<'_>, types: &[Entry], path: &str, expected: &[&str]) -> Option<Vec<u8>> {
    if !expected.iter().any(|t| check_type(types, path, t).is_ok()) {
        return None;
    }
    package.read_limited(path, MAX_PIVOT_PART).ok().flatten()
}

/// Reads the PivotTables related to a worksheet (`sheet_rels`, the sheet named `sheet_name`).
pub(super) fn read_pivots(
    package: &mut Package<'_>,
    types: &[Entry],
    sheet_rels: &[Rel],
    sheet_name: &str,
    caches: &mut Caches,
    report: &mut ReadReport,
) -> Vec<PivotTable> {
    let mut out = Vec::new();
    for rel in sheet_rels.iter().filter(|r| r.kind == "pivotTable" && !r.external) {
        if out.len() >= MAX_PIVOTS {
            report.warn(format!("XLSB PivotTables past the first {MAX_PIVOTS} on a sheet were kept as plain cells."));
            break;
        }
        let Some(view) = read_part(package, types, &rel.target, &TABLE_TYPES).and_then(|bytes| view(&bytes).ok()) else {
            report.warn(DAMAGED_TABLE);
            continue;
        };
        // The cache: the table's own relationship, else the workbook's cache id.
        let cache_part = part_rels(package, &rel.target)
            .iter()
            .find(|r| r.kind == "pivotCacheDefinition" && !r.external)
            .map(|r| r.target.clone())
            .or_else(|| caches.by_id.get(&view.cache_id).cloned());
        let Some(cache_part) = cache_part else {
            report.warn("An XLSB PivotTable has no cache; kept as plain cells.");
            continue;
        };
        let cache = match caches.parsed.get(&cache_part) {
            Some(c) => c.clone(),
            None => {
                let c = match read_part(package, types, &cache_part, &[CACHE_TYPE]) {
                    None => {
                        report.warn("An XLSB PivotTable's cache part is missing; kept as plain cells.");
                        None
                    }
                    Some(bytes) => cache(&bytes, sheet_name, caches.date_system, report).unwrap_or_else(|_| {
                        report.warn(DAMAGED_CACHE);
                        None
                    }),
                };
                caches.parsed.insert(cache_part, c.clone());
                c
            }
        };
        let Some(cache) = cache else { continue };
        if let Some(pt) = table(&view, &cache, caches, report) {
            caches.next_id = caches.next_id.saturating_add(1);
            out.push(pt);
        }
    }
    out
}

// ================================================================ cache definition

/// The caption of a cache item, as the .xlsx reader shows `<s>`, `<n>`, `<b>`, `<e>`, `<m>`, `<d>`.
fn item_label(id: u16, b: &mut Bytes<'_>, sys: DateSystem) -> Result<String, IoError> {
    Ok(match id {
        PCDI_MISSING => "(blank)".into(),
        PCDI_NUMBER => number_to_text(b.f64()?),
        PCDI_BOOLEAN => if b.u8()? != 0 { "TRUE" } else { "FALSE" }.into(),
        PCDI_ERROR => error(b.u8()?)?.as_str().into(),
        PCDI_STRING => b.text(32_767)?,
        PCDI_DATETIME => datetime_label(b, sys)?,
        _ => return Err(invalid("unknown cache item")),
    })
}

/// A `PCDIDateTime` (yr, mon, dom, hr, min, sec) as its date serial's text.
fn datetime_label(b: &mut Bytes<'_>, sys: DateSystem) -> Result<String, IoError> {
    let (yr, mon) = (b.u16()?, b.u16()?);
    let (dom, hr, min, sec) = (b.u8()?, b.u8()?, b.u8()?, b.u8()?);
    let iso = format!("{yr:04}-{mon:02}-{dom:02}T{hr:02}:{min:02}:{sec:02}");
    Ok(crate::sheet_read::parse_iso_datetime(&iso, sys).map(number_to_text).unwrap_or(iso))
}

/// `BrtBeginPCDIRun`: a run of items of one type.
fn item_run(payload: &[u8], items: &mut Vec<String>, sys: DateSystem) -> Result<(), IoError> {
    let mut b = Bytes::new(payload);
    let id = match b.u16()? {
        0x0001 => PCDI_NUMBER,
        0x0002 => PCDI_STRING,
        0x0010 => PCDI_ERROR,
        0x0020 => PCDI_DATETIME,
        _ => return Err(invalid("unknown cache item run type")),
    };
    let count = b.u32()?;
    for _ in 0..count {
        let label = item_label(id, &mut b, sys)?;
        if items.len() < MAX_ITEMS {
            items.push(label);
        }
    }
    b.finish()
}

/// Where cache items currently go.
#[derive(Clone, Copy, PartialEq)]
enum ItemsTo {
    None,
    Shared,
    Group,
}

/// A PivotCache Definition part. `Ok(None)`: a cache the model can't use (warned).
fn cache(bytes: &[u8], sheet_name: &str, sys: DateSystem, report: &mut ReadReport) -> Result<Option<CacheIn>, IoError> {
    let mut scratch = ReadReport::default();
    let mut records = Records::new(bytes);
    match records.next(&mut scratch)? {
        Some((BEGIN_PIVOT_CACHE_DEF, _)) => {}
        _ => return Err(invalid("not a pivot cache definition")),
    }
    let mut source = None;
    let mut fields: Vec<FieldIn> = Vec::new();
    let mut field: Option<FieldIn> = None;
    let mut to = ItemsTo::None;
    while let Some((id, payload)) = records.next(&mut scratch)? {
        let mut b = Bytes::new(payload);
        match id {
            END_PIVOT_CACHE_DEF => {
                calculated_formulas(&mut fields, report);
                let Some(source) = source else {
                    report.warn("An XLSB PivotTable's cache has no worksheet source; kept as plain cells.");
                    return Ok(None);
                };
                if fields.is_empty() {
                    report.warn("An XLSB PivotTable's cache has no fields; kept as plain cells.");
                    return Ok(None);
                }
                return Ok(Some(CacheIn { source, fields }));
            }
            BEGIN_PCD_SOURCE => {
                let kind = match b.u32()? {
                    0 => None,
                    1 => Some("external"),
                    2 => Some("consolidation"),
                    _ => Some("scenario"),
                };
                if let Some(kind) = kind {
                    report.warn(format!("PivotTables with a {kind} data source are not supported; kept as plain cells"));
                    return Ok(None);
                }
            }
            BEGIN_PCD_SCONSOL => {
                report.warn("PivotTables with a consolidation data source are not supported; kept as plain cells");
                return Ok(None);
            }
            BEGIN_PCD_SRANGE => {
                let flags = b.take(3)?;
                let flag = |byte: usize, bit: u8| flags.get(byte).is_some_and(|f| f & (1 << bit) != 0);
                let (by_name, other_book, scoped) = (flag(0, 0), flag(2, 0), flag(2, 1));
                let sheet = if scoped { b.string(255, true)? } else { None };
                if other_book {
                    b.string(255, true)?;
                    report.warn("a PivotTable's source is in another workbook; its source reference may not resolve");
                }
                if by_name {
                    let name = b.text(255)?;
                    if name.trim().is_empty() {
                        return Err(invalid("empty pivot source name"));
                    }
                    source = Some(name.trim().to_string());
                } else {
                    let (r0, r1, c0, c1) = (b.u32()?, b.u32()?, b.u32()?, b.u32()?);
                    if r0 > r1 || c0 > c1 || r1 >= MAX_ROWS || c1 >= MAX_COLS {
                        report.warn("a PivotTable's source reference is missing or invalid; kept as plain cells");
                        return Ok(None);
                    }
                    let r = RangeRef::new(CellRef::new(r0, c0), CellRef::new(r1, c1));
                    let sheet = sheet.filter(|s| !s.is_empty()).unwrap_or_else(|| sheet_name.to_string());
                    source = Some(format!("{}!{}", quote_sheet(&sheet), abs_range(&r)));
                }
            }
            BEGIN_PCD_FIELD => {
                if fields.len() >= MAX_FIELDS {
                    field = None;
                    continue;
                }
                // Flags: fSrcField 0x0004, fCaption 0x0008, fLoadFmla 0x0100.
                let flags = b.u16()?;
                b.take(18)?; // ifmt, wTypeSql, ihdb, isxtl, cIsxtmps
                let name = b.text(32_767)?;
                let is_calculated = flags & 0x0100 != 0;
                // stFldCaption, then fldFmla. One that can't be read is left out (warned below).
                let tokens = is_calculated
                    .then(|| {
                        if flags & 0x0008 != 0 {
                            b.text(32_767).ok()?;
                        }
                        pivot_formula(&mut b).ok()
                    })
                    .flatten();
                field = Some(FieldIn { name, database: flags & 0x0004 != 0, calculated: is_calculated, tokens, ..FieldIn::default() });
            }
            BEGIN_PNAME => {
                // Only a cache field's PNAMES; a calculated item's (out of a field) is not read.
                if let Some(f) = field.as_mut().filter(|f| f.calculated && f.pnames.len() < MAX_FIELDS) {
                    f.pnames.push(b.u32()?);
                }
            }
            END_PCD_FIELD => {
                if let Some(f) = field.take() {
                    fields.push(f);
                }
                to = ItemsTo::None;
            }
            BEGIN_PCD_FATBL => to = ItemsTo::Shared,
            BEGIN_PCD_FG_ITEMS => to = ItemsTo::Group,
            END_PCD_FATBL | END_PCD_FG_ITEMS => to = ItemsTo::None,
            BEGIN_PCD_FGROUP => {
                let _parent = b.u32()?;
                let base = b.u32()? as i32;
                if let Some(f) = field.as_mut() {
                    f.group = Some((usize::try_from(base).ok(), None, vec![]));
                }
            }
            BEGIN_PCD_FG_RANGE => {
                let by = match b.u8()? {
                    4 => Some(PivotDateGroup::Days),
                    5 => Some(PivotDateGroup::Months),
                    6 => Some(PivotDateGroup::Quarters),
                    7 => Some(PivotDateGroup::Years),
                    _ => None,
                };
                if let Some((_, g, _)) = field.as_mut().and_then(|f| f.group.as_mut()) {
                    *g = by;
                }
            }
            PCDI_MISSING..=PCDI_DATETIME | 27..=32 if to != ItemsTo::None => {
                let kind = if id > PCDI_DATETIME { id - PCDIA_OFFSET } else { id };
                let label = item_label(kind, &mut b, sys)?;
                push_item(field.as_mut(), to, label);
            }
            BEGIN_PCDI_RUN if to != ItemsTo::None => {
                let mut run = vec![];
                item_run(payload, &mut run, sys)?;
                for label in run {
                    push_item(field.as_mut(), to, label);
                }
            }
            _ => {}
        }
    }
    Err(invalid("pivot cache definition has no end"))
}

/// `PivotParsedFormula` ([MS-XLSB] 2.5.98.15): `cce` (under 16,385), `rgce`, `cb`, `rgcb`.
fn pivot_formula(b: &mut Bytes<'_>) -> Result<(Vec<u8>, Vec<u8>), IoError> {
    let cce = b.u32()?;
    if cce >= 16_385 {
        return Err(invalid("PivotTable formula too long"));
    }
    let rgce = b.take(cce as usize)?.to_vec();
    let cb = b.u32()?;
    let rgcb = b.take(usize::try_from(cb).map_err(|_| invalid("PivotTable formula too long"))?)?.to_vec();
    Ok((rgce, rgcb))
}

/// Decodes each calculated field's formula; one that can't be expressed is warned about and
/// left out (its data fields are skipped), the rest of the cache is kept.
fn calculated_formulas(fields: &mut [FieldIn], report: &mut ReadReport) {
    let names: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
    for f in fields.iter_mut().filter(|f| f.calculated) {
        let decoded = f.tokens.as_ref().ok_or("unreadable formula").and_then(|(rgce, rgcb)| {
            let pivot_fields = f.pnames.iter().map(|i| usize::try_from(*i).ok().and_then(|i| names.get(i)).cloned().unwrap_or_default()).collect();
            let links = Links { pivot_fields, ..Links::default() };
            // Quoted field names are not worksheet syntax: the text is checked when the
            // PivotTable compiles it, as for the .xlsx reader's `formula` attribute.
            decode_xlsb(rgce, rgcb, CellRef::new(0, 0), Mode::Cell, &links).map(|e| gridcraft_formula::print(&e)).map_err(|u| u.0)
        });
        match decoded {
            Ok(text) if !f.name.trim().is_empty() => f.formula = Some(format!("={text}")),
            Ok(_) => report.warn("A calculated PivotTable field without a name was not imported."),
            Err(why) => report.warn(format!("Calculated PivotTable field \"{}\" was not imported: {why}.", f.name)),
        }
        f.tokens = None;
    }
}

fn push_item(field: Option<&mut FieldIn>, to: ItemsTo, label: String) {
    let Some(f) = field else { return };
    let list = match (to, f.group.as_mut()) {
        (ItemsTo::Group, Some((_, _, items))) => items,
        (ItemsTo::Shared, _) => &mut f.items,
        _ => return,
    };
    if list.len() < MAX_ITEMS {
        list.push(label);
    }
}

// ================================================================ PivotTable view

/// A pivot item (`BrtBeginSXVI`).
#[derive(Default)]
struct ItemIn {
    data: bool,
    hidden: bool,
    collapsed: bool,
    cache: Option<usize>,
}

/// A pivot field (`BrtBeginSXVD`).
#[derive(Default)]
struct VdIn {
    compact: bool,
    outline: bool,
    default_subtotal: bool,
    subtotal_top: bool,
    sort: Option<PivotSort>,
    items: Vec<ItemIn>,
}

/// A data item (`BrtBeginSXDI`).
struct DiIn {
    field: i32,
    func: u32,
    show_as: u32,
    format: u16,
    name: Option<String>,
}

#[derive(Default)]
struct ViewIn {
    name: String,
    cache_id: u32,
    show_headers: bool,
    enable_drill: bool,
    row_grand: bool,
    col_grand: bool,
    default_compact: bool,
    default_outline: bool,
    data_on_rows: bool,
    data_position: Option<usize>,
    location: Option<RangeRef>,
    fields: Vec<VdIn>,
    rows: Vec<i32>,
    cols: Vec<i32>,
    pages: Vec<(i32, u32)>,
    data: Vec<DiIn>,
    style: String,
    filters: bool,
}

fn bit(word: u32, n: u32) -> bool {
    word & (1 << n) != 0
}

/// `BrtBeginISXVDRws` / `BrtBeginISXVDCols`: a count and that many field indices.
fn field_list(b: &mut Bytes<'_>) -> Result<Vec<i32>, IoError> {
    let count = b.u32()? as usize;
    if count > MAX_FIELDS {
        return Err(invalid("too many PivotTable axis fields"));
    }
    (0..count).map(|_| Ok(b.u32()? as i32)).collect()
}

/// A PivotTable part. `Err` for a part that can't be decoded; the caller skips the table.
fn view(bytes: &[u8]) -> Result<ViewIn, IoError> {
    let mut scratch = ReadReport::default();
    let mut records = Records::new(bytes);
    let begin = loop {
        match records.next(&mut scratch)? {
            Some((BEGIN_SX_VIEW, payload)) => break payload,
            // An optional ACUID record may come first.
            Some(_) => {}
            None => return Err(invalid("PivotTable part has no BrtBeginSXView")),
        }
    };
    let mut b = Bytes::new(begin);
    let (w0, w1, w2) = (b.u32()?, b.u32()?, b.u32()?);
    let axis4data = b.u8()?;
    b.take(3)?; // cWrapPage and two version bytes
    let pos4data = b.u32()? as i32;
    b.take(8)?; // itblAutoFmt, reserved and dwCrtFmtId
    let cache_id = b.u32()?;
    let name = b.text(255)?;
    let mut v = ViewIn {
        name,
        cache_id,
        show_headers: !bit(w0, 31),
        enable_drill: bit(w1, 5),
        row_grand: bit(w1, 13),
        col_grand: bit(w1, 14),
        default_compact: bit(w2, 0),
        default_outline: bit(w2, 1),
        data_on_rows: axis4data == 1,
        data_position: usize::try_from(pos4data).ok(),
        ..ViewIn::default()
    };
    while let Some((id, payload)) = records.next(&mut scratch)? {
        let mut b = Bytes::new(payload);
        match id {
            END_SX_VIEW => {
                if v.location.is_none() {
                    return Err(invalid("PivotTable has no valid location"));
                }
                return Ok(v);
            }
            BEGIN_SX_LOCATION => {
                let (r0, r1, c0, c1) = (b.u32()?, b.u32()?, b.u32()?, b.u32()?);
                if r0 > r1 || c0 > c1 || r1 >= MAX_ROWS || c1 >= MAX_COLS {
                    return Err(invalid("PivotTable location is out of bounds"));
                }
                v.location = Some(RangeRef::new(CellRef::new(r0, c0), CellRef::new(r1, c1)));
            }
            BEGIN_SXVD => {
                if v.fields.len() >= MAX_FIELDS {
                    return Err(invalid("too many pivot fields"));
                }
                let w0 = b.u32()?;
                b.u32()?; // ifmt
                let w2 = b.u32()?;
                let sort = bit(w2, 12).then(|| if bit(w2, 13) { PivotSort::Asc } else { PivotSort::Desc });
                v.fields.push(VdIn {
                    compact: bit(w0, 28),
                    outline: bit(w2, 6),
                    default_subtotal: bit(w0, 8),
                    subtotal_top: bit(w2, 8),
                    sort,
                    items: vec![],
                });
            }
            BEGIN_SXVI => {
                let kind = b.u8()?;
                let flags = b.u16()?;
                let cache = b.u32()? as i32;
                let field = v.fields.last_mut().ok_or_else(|| invalid("pivot item outside a pivot field"))?;
                if field.items.len() >= MAX_ITEMS {
                    return Err(invalid("too many pivot items"));
                }
                field.items.push(ItemIn { data: kind == 0, hidden: flags & 1 != 0, collapsed: flags & 2 != 0, cache: usize::try_from(cache).ok() });
            }
            BEGIN_ISXVD_RWS => v.rows = field_list(&mut b)?,
            BEGIN_ISXVD_COLS => v.cols = field_list(&mut b)?,
            BEGIN_SXPI => {
                if v.pages.len() >= 256 {
                    return Err(invalid("too many page fields"));
                }
                let field = b.u32()? as i32;
                let item = b.u32()?;
                v.pages.push((field, item));
            }
            BEGIN_SXDI => {
                if v.data.len() >= MAX_FIELDS {
                    return Err(invalid("too many data fields"));
                }
                let field = b.u32()? as i32;
                let func = b.u32()?;
                let show_as = b.u32()?;
                b.take(8)?; // isxvd, isxvi
                let format = b.u16()?;
                b.u16()?;
                let name = if b.u8()? != 0 { Some(b.text(32_767)?) } else { None };
                v.data.push(DiIn { field, func, show_as, format, name });
            }
            TABLE_STYLE_CLIENT => {
                b.u16()?;
                v.style = b.string(255, true)?.unwrap_or_default();
            }
            BEGIN_SX_FILTERS => v.filters = true,
            _ => {}
        }
    }
    Err(invalid("PivotTable part has no BrtEndSXView"))
}

/// A pivot field's data items (not subtotal items).
fn data_items(f: &VdIn) -> Vec<&ItemIn> {
    f.items.iter().filter(|it| it.data).collect()
}

/// (source column name, date grouping) of a cache field used on an axis.
fn field_ref(cache: &CacheIn, i: usize, report: &mut ReadReport) -> Option<(String, PivotDateGroup)> {
    let f = cache.fields.get(i)?;
    if f.calculated {
        report.warn("calculated PivotTable fields are not supported");
        return None;
    }
    match &f.group {
        Some((base, by, _)) => {
            let by = match by {
                Some(b) => *b,
                None => {
                    report.warn(format!("PivotTable grouping of \"{}\" is not supported; the field is ungrouped", f.name));
                    PivotDateGroup::None
                }
            };
            let base_name = if f.database { f.name.clone() } else { base.and_then(|b| cache.fields.get(b))?.name.clone() };
            Some((base_name, by))
        }
        None if !f.database => {
            report.warn(format!("PivotTable field \"{}\" (grouped items) is not supported", f.name));
            None
        }
        None => Some((f.name.clone(), PivotDateGroup::None)),
    }
}

fn func_of(code: u32) -> Option<PivotFunc> {
    Some(match code {
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

/// A `ShowDataAs` value as the model's transform, or the .xlsx name of one it doesn't have.
fn show_as_of(code: u32) -> Result<PivotShowAs, &'static str> {
    Ok(match code {
        0 => PivotShowAs::Normal,
        4 => PivotShowAs::RunningTotal,
        5 => PivotShowAs::PercentOfRowTotal,
        6 => PivotShowAs::PercentOfColumnTotal,
        7 => PivotShowAs::PercentOfGrandTotal,
        1 => return Err("difference"),
        2 => return Err("percent"),
        3 => return Err("percentDiff"),
        8 => return Err("index"),
        _ => return Err("unknown"),
    })
}

/// Builds the model PivotTable the way the .xlsx reader does from the same definition.
fn table(v: &ViewIn, cache: &CacheIn, caches: &Caches, report: &mut ReadReport) -> Option<PivotTable> {
    let id = caches.next_id;
    let loc = v.location?;
    let mut pt = PivotTable { id, ..PivotTable::default() };
    pt.name = if v.name.trim().is_empty() { format!("PivotTable{id}") } else { v.name.clone() };
    pt.source = cache.source.clone();
    let vd = |i: i32| usize::try_from(i).ok().and_then(|i| v.fields.get(i));

    // Layout from the first axis field.
    let first_axis = v.rows.iter().chain(v.cols.iter()).find(|i| **i >= 0).and_then(|i| vd(*i));
    let (compact, outline) = match first_axis {
        Some(f) => (f.compact, f.outline),
        None => (v.default_compact, v.default_outline),
    };
    pt.layout = if compact {
        PivotLayout::Compact
    } else if outline {
        PivotLayout::Outline
    } else {
        PivotLayout::Tabular
    };
    if let Some(f) = first_axis {
        pt.subtotals = f.default_subtotal;
        pt.subtotals_top = f.subtotal_top;
    }
    pt.grand_totals_rows = v.col_grand;
    pt.grand_totals_cols = v.row_grand;
    pt.show_headers = v.show_headers;
    pt.enable_show_details = v.enable_drill;
    pt.style = v.style.clone();
    if v.filters {
        report.warn(format!("PivotTable \"{}\": calculated items and label/value filters are not supported", pt.name));
    }

    let labels_of = |i: usize| cache.fields.get(i).map(|f| f.labels().to_vec()).unwrap_or_default();
    let label = |labels: &[String], it: &ItemIn| it.cache.and_then(|k| labels.get(k)).cloned();

    // (fields, position of Σ Values (`-2`) among them)
    let axis = |list: &[i32], filters: &mut Vec<PivotFilter>, report: &mut ReadReport| -> (Vec<PivotField>, Option<usize>) {
        let mut out = Vec::new();
        let mut values_at = None;
        for &i in list {
            if i == -2 {
                values_at = Some(out.len());
                continue;
            }
            let Ok(i) = usize::try_from(i) else { continue };
            let Some((name, group)) = field_ref(cache, i, report) else { continue };
            let labels = labels_of(i);
            let mut f = PivotField { source_col: name.clone(), date_group: group, sort: PivotSort::None, collapsed_items: vec![] };
            if let Some(e) = v.fields.get(i) {
                f.sort = e.sort.unwrap_or(PivotSort::None);
                let items = data_items(e);
                f.collapsed_items = items.iter().filter(|it| it.collapsed).filter_map(|it| label(&labels, it)).collect();
                if items.iter().any(|it| it.hidden) {
                    let shown: Vec<String> = items.iter().filter(|it| !it.hidden).filter_map(|it| label(&labels, it)).collect();
                    if !filters.iter().any(|ff| same(&ff.source_col, &name)) {
                        filters.push(PivotFilter { source_col: name.clone(), selected: Some(shown) });
                    }
                }
            }
            out.push(f);
        }
        (out, values_at)
    };

    let mut page_filters = Vec::new();
    for &(field, item) in &v.pages {
        let Ok(i) = usize::try_from(field) else { continue };
        let Some((name, _)) = field_ref(cache, i, report) else { continue };
        let labels = labels_of(i);
        let selected = if item != PAGE_ALL_ITEMS {
            // The page item is a pivot item index; its cache item gives the caption.
            let by_item = v.fields.get(i).and_then(|f| data_items(f).get(item as usize).and_then(|it| label(&labels, it)));
            by_item.or_else(|| labels.get(item as usize).cloned()).map(|l| vec![l])
        } else {
            v.fields.get(i).and_then(|e| {
                let items = data_items(e);
                items.iter().any(|it| it.hidden).then(|| items.iter().filter(|it| !it.hidden).filter_map(|it| label(&labels, it)).collect())
            })
        };
        page_filters.push(PivotFilter { source_col: name, selected });
    }
    let mut axis_filters = Vec::new();
    let (rows, rows_values) = axis(&v.rows, &mut axis_filters, report);
    let (columns, cols_values) = axis(&v.cols, &mut axis_filters, report);
    pt.rows = rows;
    pt.columns = columns;
    pt.values_on_rows = rows_values.is_some() || (cols_values.is_none() && v.data_on_rows);
    pt.values_position = if pt.values_on_rows { rows_values.or(v.data_position).filter(|p| *p < pt.rows.len()) } else { None };
    pt.filters = page_filters;
    pt.filters.extend(axis_filters);

    for d in &v.data {
        let Some(f) = usize::try_from(d.field).ok().and_then(|i| cache.fields.get(i)) else { continue };
        if f.calculated && f.formula.is_none() {
            continue;
        }
        let func = func_of(d.func).unwrap_or_else(|| {
            report.warn(format!("PivotTable function {} is not supported; using Sum", d.func));
            PivotFunc::Sum
        });
        let show_as = show_as_of(d.show_as).unwrap_or_else(|other| {
            report.warn(format!("PivotTable show-values-as {other} is not supported"));
            PivotShowAs::Normal
        });
        let number_format = Some(d.format)
            .filter(|id| *id != 0)
            .and_then(|id| crate::tables::builtin_format(u32::from(id)).map(str::to_string).or_else(|| caches.formats.get(&id).cloned()));
        let name = d.name.clone().unwrap_or_else(|| format!("{} of {}", func_caption(func), f.name));
        pt.values.push(PivotValue { source_col: f.name.clone(), func, name, show_as, number_format });
    }

    // Calculated fields of the cache (each PivotTable on it can use them).
    for f in &cache.fields {
        if let Some(formula) = &f.formula
            && !pt.calculated_fields.iter().any(|c| same(&c.name, &f.name))
        {
            pt.calculated_fields.push(PivotCalcField { name: f.name.clone(), formula: formula.clone() });
        }
    }

    pt.anchor = loc.start;
    let above = if v.pages.is_empty() { 0 } else { u32::try_from(v.pages.len()).unwrap_or(u32::MAX).saturating_add(1) };
    pt.last_range = Some(RangeRef::new(CellRef::new(loc.start.row.saturating_sub(above), loc.start.col), loc.end));
    Some(pt)
}
