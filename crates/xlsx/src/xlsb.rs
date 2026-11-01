//! XLSB import of cell values and formulas, implemented from Microsoft's public MS-XLSB specification:
//! <https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-xlsb/acc8aa92-1f02-4167-99f5-84f9f676b95a>.
//! Cell formulas are decoded with the shared BIFF token decoder; those it can't represent keep
//! their saved result as a constant. Cell formatting (number formats, fonts, fills, borders,
//! alignment and protection), column widths, row heights, hidden and outlined rows/columns, frozen
//! panes, sheet view options and tab colours are imported; gradient fills, cell style inheritance,
//! differential formats, macros and other workbook features are not run or imported.

mod styles;

use std::sync::Arc;

use gridcraft_core::color::INDEXED;
use gridcraft_core::{CellError, CellRef, DateSystem, MAX_COLS, MAX_ROWS, RangeRef, Value};
use gridcraft_model::{Cell, Color, Formula, LineInfo, Sheet, StyleId, Visibility, Workbook};
use quick_xml::NsReader;
use quick_xml::XmlVersion;
use quick_xml::events::Event;
use quick_xml::name::ResolveResult;

use crate::package::{Package, Rel, normalize, resolve, split_dir};
use crate::tables::{col_width_to_px, pt_to_px};
use crate::xls::formula::decode_xlsb;
use crate::{IoError, ReadReport};
use gridcraft_formula::Expr;

const MAIN: &str = "application/vnd.ms-excel.main";
const BINARY_MAIN: &str = "application/vnd.ms-excel.sheet.binary.macroEnabled.main";
const WORKSHEET: &str = "application/vnd.ms-excel.worksheet";
const STYLES: &str = "application/vnd.ms-excel.styles";
const MAX_STYLE_RECORDS: usize = 65_536;
const STRINGS: &str = "application/vnd.ms-excel.sharedStrings";
const CT_NS: &[u8] = b"http://schemas.openxmlformats.org/package/2006/content-types";
const REL_NS: &[u8] = b"http://schemas.openxmlformats.org/package/2006/relationships";
const REL_TYPE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
const STRICT_REL_TYPE: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships/";
const MS_REL_TYPE: &str = "http://schemas.microsoft.com/office/2006/relationships/";
const MAX_META: u64 = 1024 * 1024;
const MAX_TEXT: usize = 64 * 1024 * 1024;
const MAX_CELLS: usize = 1_000_000;
const MAX_STRINGS: usize = 1_000_000;
const MAX_SHEETS: usize = 1024;
const MAX_RECORDS: usize = 5_000_000;
/// Most column-information cells one worksheet expands: overlapping BrtColInfo records repeat
/// work even when the column map stays small.
const MAX_COL_ASSIGNMENTS: u64 = 1_000_000;

fn invalid(msg: impl Into<String>) -> IoError {
    IoError::Format(format!("XLSB: {}", msg.into()))
}

#[derive(Default)]
struct Budget {
    cells: usize,
    text: usize,
    /// Formula cells kept as their saved result because their formula couldn't be decoded.
    formulas_kept: usize,
}
impl Budget {
    fn text(&mut self, len: usize) -> Result<(), IoError> {
        self.text = self.text.saturating_add(len);
        if self.text > MAX_TEXT {
            return Err(IoError::TooLarge("XLSB decoded text exceeds 64 MiB".into()));
        }
        Ok(())
    }
    fn cell(&mut self) -> Result<(), IoError> {
        self.cells += 1;
        if self.cells > MAX_CELLS {
            return Err(IoError::TooLarge("XLSB exceeds 1 million imported cells".into()));
        }
        Ok(())
    }
}

/// Imports worksheet scalar values, cell formulas (with their saved results) and cell formatting.
/// Dates retain the workbook's original date system. Inspect warnings for unsupported features.
pub fn read_xlsb(bytes: &[u8]) -> Result<(Workbook, ReadReport), IoError> {
    let mut package = Package::open(bytes)?;
    let types = content_types(&mut package)?;
    let roots = relationships(&mut package, "")?;
    let roots: Vec<_> = roots.iter().filter(|r| r.kind == "officeDocument").collect();
    if roots.len() != 1 {
        return Err(invalid("expected one workbook relationship"));
    }
    let root = roots.first().ok_or_else(|| invalid("missing workbook relationship"))?;
    if root.external {
        return Err(invalid("external workbook relationship"));
    }
    check_type(&types, &root.target, MAIN)?;
    let book = required(&mut package, &root.target)?;
    let mut wb = Workbook::new();
    wb.sheets.clear();
    let mut report = ReadReport::default();
    report.warn("XLSB import: charts, images, macros and other features are not imported. Save a new .xlsx file; XLSB saving is not supported.");
    let sheets = workbook(&book, &mut wb, &mut report)?;
    let rels = relationships(&mut package, &root.target)?;
    let mut budget = Budget::default();
    let styles = import_styles(&mut package, &types, &rels, &mut wb, &mut budget, &mut report);
    let sst_rels: Vec<_> = rels.iter().filter(|r| r.kind == "sharedStrings").collect();
    if sst_rels.len() > 1 {
        return Err(invalid("multiple shared-string relationships"));
    }
    let strings = if let Some(rel) = sst_rels.first() {
        if rel.external {
            return Err(invalid("external shared-string relationship"));
        }
        check_type(&types, &rel.target, STRINGS)?;
        shared_strings(&required(&mut package, &rel.target)?, &mut budget, &mut report)?
    } else {
        vec![]
    };
    for info in sheets {
        let Some(id) = info.rel else {
            if info.visibility != Visibility::VeryHidden {
                return Err(invalid("worksheet relationship is null"));
            }
            report.warn("XLSB module sheets were skipped.");
            continue;
        };
        let rel = rels.iter().find(|r| r.id == id).ok_or_else(|| invalid("worksheet relationship is missing"))?;
        if rel.external {
            return Err(invalid("external sheet relationship"));
        }
        if rel.kind != "worksheet" {
            if !matches!(rel.kind.as_str(), "chartsheet" | "dialogsheet" | "xlMacrosheet" | "xlIntlMacrosheet") {
                return Err(invalid("unknown sheet relationship type"));
            }
            report.warn("XLSB non-worksheet sheets were skipped.");
            continue;
        }
        check_type(&types, &rel.target, WORKSHEET)?;
        wb.check_sheet_name(&info.name, None).map_err(invalid)?;
        let mut sheet = worksheet(&required(&mut package, &rel.target)?, &info.name, &strings, &styles, &mut budget, &mut report)?;
        sheet.visibility = info.visibility;
        wb.sheets.push(Arc::new(sheet));
    }
    if wb.sheets.is_empty() {
        return Err(invalid("no supported worksheets"));
    }
    if budget.formulas_kept > 0 {
        report.warn(format!(
            "{} XLSB formula(s) use features that can't be imported yet (shared or array formulas, defined names, other sheets, array constants); they were kept as their saved results and will not recalculate.",
            budget.formulas_kept
        ));
    }
    wb.set_active_sheet(wb.sheets.iter().position(|s| s.visibility == Visibility::Visible).unwrap_or(0));
    Ok((wb, report))
}

pub(crate) fn is_xlsb(bytes: &[u8]) -> bool {
    let inspect = || -> Result<bool, IoError> {
        let mut package = Package::open(bytes)?;
        let types = content_types(&mut package)?;
        let roots = relationships(&mut package, "")?;
        let mut books = roots.iter().filter(|r| r.kind == "officeDocument");
        let Some(root) = books.next() else { return Ok(false) };
        Ok(books.next().is_none() && !root.external && check_type(&types, &root.target, MAIN).is_ok())
    };
    inspect().unwrap_or(false)
}

fn required(package: &mut Package<'_>, path: &str) -> Result<Vec<u8>, IoError> {
    package.read(path)?.ok_or_else(|| invalid(format!("missing part {path}")))
}

struct Entry {
    name: String,
    attrs: Vec<(String, String)>,
}
impl Entry {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }
}

/// The two package metadata parts are flat, namespace-qualified XML documents. Keep only
/// direct child entries, and reject truncated XML instead of guessing a partial relationship.
fn metadata(bytes: &[u8], ns: &[u8], root_name: &[u8]) -> Result<Vec<Entry>, IoError> {
    let mut reader = NsReader::from_reader(bytes);
    let mut depth = 0usize;
    let mut seen = false;
    let mut entries = vec![];
    loop {
        let event = reader.read_event().map_err(|e| IoError::Xml(e.to_string()))?;
        match &event {
            Event::Start(e) | Event::Empty(e) => {
                let (namespace, local) = reader.resolver_mut().resolve_element(e.name());
                let valid_ns = matches!(namespace, ResolveResult::Bound(n) if n.as_ref() == ns);
                if depth == 0 {
                    if seen || !valid_ns || local.as_ref() != root_name {
                        return Err(invalid("invalid package metadata root"));
                    }
                    seen = true;
                } else if depth == 1 && valid_ns {
                    let mut attrs = vec![];
                    for a in e.attributes() {
                        let a = a.map_err(|e| IoError::Xml(e.to_string()))?;
                        if !a.key.as_ref().contains(&b':') {
                            attrs.push((
                                String::from_utf8_lossy(a.key.as_ref()).into_owned(),
                                a.normalized_value(XmlVersion::Explicit1_0).map_err(|e| IoError::Xml(e.to_string()))?.into_owned(),
                            ));
                        }
                    }
                    entries.push(Entry { name: String::from_utf8_lossy(local.as_ref()).into_owned(), attrs });
                }
                if matches!(event, Event::Start(_)) {
                    depth += 1;
                    if depth > crate::xml::MAX_DEPTH {
                        return Err(invalid("package metadata is nested too deeply"));
                    }
                }
            }
            Event::End(_) => depth = depth.checked_sub(1).ok_or_else(|| invalid("unexpected metadata end"))?,
            Event::Text(t) if depth == 0 && !t.xml_content(XmlVersion::Explicit1_0).map_err(|e| IoError::Xml(e.to_string()))?.trim().is_empty() => {
                return Err(invalid("text outside package metadata"));
            }
            Event::DocType(_) => return Err(invalid("DOCTYPE in package metadata is unsupported")),
            Event::Eof => break,
            _ => {}
        }
    }
    if !seen || depth != 0 {
        return Err(invalid("incomplete package metadata"));
    }
    Ok(entries)
}

fn content_types(package: &mut Package<'_>) -> Result<Vec<Entry>, IoError> {
    let data = package.read_limited("[Content_Types].xml", MAX_META)?.ok_or_else(|| invalid("missing content types"))?;
    metadata(&data, CT_NS, b"Types")
}

fn check_type(types: &[Entry], path: &str, expected: &str) -> Result<(), IoError> {
    let found = types.iter().find(|e| e.name == "Override" && e.attr("PartName").is_some_and(|p| normalize(p) == normalize(path))).or_else(|| {
        types.iter().find(|e| {
            e.name == "Default" && e.attr("Extension").is_some_and(|ext| path.rsplit('.').next().is_some_and(|p| p.eq_ignore_ascii_case(ext)))
        })
    });
    let actual = found.and_then(|e| e.attr("ContentType"));
    if actual != Some(expected) && !(expected == MAIN && actual == Some(BINARY_MAIN)) {
        return Err(invalid(format!("wrong or missing content type for {path}")));
    }
    Ok(())
}

fn relationships(package: &mut Package<'_>, path: &str) -> Result<Vec<Rel>, IoError> {
    let (dir, file) = split_dir(path);
    let relpath = if dir.is_empty() { format!("_rels/{file}.rels") } else { format!("{dir}/_rels/{file}.rels") };
    let data = package.read_limited(&relpath, MAX_META)?.ok_or_else(|| invalid(format!("missing relationships for {path}")))?;
    let mut out: Vec<Rel> = vec![];
    for e in metadata(&data, REL_NS, b"Relationships")? {
        if e.name != "Relationship" {
            continue;
        }
        let id = e.attr("Id").filter(|s| !s.is_empty()).ok_or_else(|| invalid("relationship has no ID"))?;
        if out.iter().any(|r| r.id == id) {
            return Err(invalid("duplicate relationship ID"));
        }
        let ty = e.attr("Type").ok_or_else(|| invalid("relationship has no type"))?;
        let kind = ty
            .strip_prefix(REL_TYPE)
            .or_else(|| ty.strip_prefix(STRICT_REL_TYPE))
            .or_else(|| ty.strip_prefix(MS_REL_TYPE))
            .map(str::to_owned)
            .unwrap_or_else(|| format!("unsupported:{ty}"));
        let target = e.attr("Target").ok_or_else(|| invalid("relationship has no target"))?;
        let external = e.attr("TargetMode") == Some("External");
        out.push(Rel { id: id.into(), kind, target: if external { target.into() } else { resolve(dir, target) }, external });
    }
    Ok(out)
}

struct Bytes<'a> {
    rest: &'a [u8],
}
impl<'a> Bytes<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { rest: bytes }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], IoError> {
        let out = self.rest.get(..n).ok_or_else(|| invalid("truncated record or string"))?;
        self.rest = self.rest.get(n..).ok_or_else(|| invalid("truncated record"))?;
        Ok(out)
    }
    fn u8(&mut self) -> Result<u8, IoError> {
        self.take(1)?.first().copied().ok_or_else(|| invalid("truncated byte"))
    }
    fn u16(&mut self) -> Result<u16, IoError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().map_err(|_| invalid("truncated u16"))?))
    }
    fn u32(&mut self) -> Result<u32, IoError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().map_err(|_| invalid("truncated u32"))?))
    }
    fn f64(&mut self) -> Result<f64, IoError> {
        let n = f64::from_le_bytes(self.take(8)?.try_into().map_err(|_| invalid("truncated number"))?);
        if !n.is_finite() {
            return Err(invalid("nonfinite numeric value"));
        }
        Ok(n)
    }
    fn string(&mut self, max: u32, nullable: bool) -> Result<Option<String>, IoError> {
        let count = self.u32()?;
        if nullable && count == u32::MAX {
            return Ok(None);
        }
        if count > max {
            return Err(IoError::TooLarge("XLSB string length exceeds its limit".into()));
        }
        let size = (count as usize).checked_mul(2).ok_or_else(|| invalid("string length overflow"))?;
        let bytes = self.take(size)?;
        let mut text = String::new();
        for ch in char::decode_utf16(bytes.as_chunks::<2>().0.iter().copied().map(u16::from_le_bytes)) {
            text.push(ch.map_err(|_| invalid("invalid UTF-16 string"))?);
        }
        Ok(Some(text))
    }
    fn text(&mut self, max: u32) -> Result<String, IoError> {
        self.string(max, false)?.ok_or_else(|| invalid("null string"))
    }
    fn finish(self) -> Result<(), IoError> {
        if !self.rest.is_empty() {
            return Err(invalid("unexpected bytes after record payload"));
        }
        Ok(())
    }
}

struct Records<'a> {
    bytes: Bytes<'a>,
    count: usize,
    blocks: Vec<u16>,
}
impl<'a> Records<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes: Bytes::new(bytes), count: 0, blocks: vec![] }
    }
    fn next(&mut self, report: &mut ReadReport) -> Result<Option<(u16, &'a [u8])>, IoError> {
        while !self.bytes.rest.is_empty() {
            self.count += 1;
            if self.count > MAX_RECORDS {
                return Err(IoError::TooLarge("XLSB part contains too many records".into()));
            }
            let first = self.bytes.u8()?;
            let id = if first & 0x80 == 0 {
                first as u16
            } else {
                let id = u16::from(first & 0x7f) | (u16::from(self.bytes.u8()? & 0x7f) << 7);
                if id < 128 {
                    return Err(invalid("invalid two-byte record type"));
                }
                id
            };
            let mut size = 0usize;
            for shift in [0, 7, 14, 21] {
                let b = self.bytes.u8()?;
                size |= usize::from(b & 0x7f) << shift;
                // The high bit of the fourth length byte is ignored by MS-XLSB.
                if b & 0x80 == 0 || shift == 21 {
                    break;
                }
            }
            let payload = self.bytes.take(size)?;
            match id {
                35 | 37 => {
                    let mut b = Bytes::new(payload);
                    if id == 35 {
                        b.take(4)?;
                    } else {
                        let count = b.u16()?;
                        if count == 0 {
                            return Err(invalid("empty alternate-content version list"));
                        }
                        b.take(usize::from(count) * 4)?;
                    }
                    b.finish()?;
                    if self.blocks.len() >= 64 {
                        return Err(invalid("extension blocks nested too deeply"));
                    }
                    self.blocks.push(id + 1);
                    report.warn("Unsupported XLSB future-record and alternate-content blocks were skipped.");
                }
                36 | 38 => {
                    if self.blocks.pop() != Some(id) || !payload.is_empty() {
                        return Err(invalid("mismatched extension block end"));
                    }
                }
                _ if !self.blocks.is_empty() => {}
                _ => return Ok(Some((id, payload))),
            }
        }
        if !self.blocks.is_empty() {
            return Err(invalid("unterminated extension block"));
        }
        Ok(None)
    }
    fn first(&mut self, id: u16, report: &mut ReadReport) -> Result<(), IoError> {
        if self.next(report)? != Some((id, &[])) {
            return Err(invalid("missing part begin record"));
        }
        Ok(())
    }
    fn end(&mut self, payload: &[u8], report: &mut ReadReport) -> Result<(), IoError> {
        if !payload.is_empty() || self.next(report)?.is_some() {
            return Err(invalid("records after part end"));
        }
        Ok(())
    }
}

struct SheetInfo {
    name: String,
    rel: Option<String>,
    visibility: Visibility,
}
fn workbook(bytes: &[u8], wb: &mut Workbook, report: &mut ReadReport) -> Result<Vec<SheetInfo>, IoError> {
    let mut records = Records::new(bytes);
    records.first(131, report)?;
    let mut sheets = vec![];
    let mut bundle = false;
    let mut bundle_seen = false;
    while let Some((id, payload)) = records.next(report)? {
        let mut b = Bytes::new(payload);
        match id {
            132 => {
                if bundle || !bundle_seen {
                    return Err(invalid("incomplete worksheet list"));
                }
                records.end(payload, report)?;
                return Ok(sheets);
            }
            143 => {
                if bundle_seen {
                    return Err(invalid("duplicate worksheet list"));
                }
                bundle_seen = true;
                bundle = true;
                b.finish()?;
            }
            144 => {
                if !bundle {
                    return Err(invalid("unexpected worksheet list end"));
                }
                bundle = false;
                b.finish()?;
            }
            153 => {
                wb.date_system = if b.u32()? & 1 != 0 { DateSystem::D1904 } else { DateSystem::D1900 };
                b.u32()?;
                b.text(31)?;
                b.finish()?;
            }
            156 => {
                if !bundle {
                    return Err(invalid("worksheet outside the worksheet list"));
                }
                if sheets.len() >= MAX_SHEETS {
                    return Err(IoError::TooLarge("XLSB exceeds 1024 sheets".into()));
                }
                let visibility = match b.u32()? {
                    0 => Visibility::Visible,
                    1 => Visibility::Hidden,
                    2 => Visibility::VeryHidden,
                    _ => return Err(invalid("invalid sheet visibility")),
                };
                let sheet_id = b.u32()?;
                if !(1..=65535).contains(&sheet_id) {
                    return Err(invalid("invalid worksheet ID"));
                }
                let rel = b.string(255, true)?;
                let name = b.text(31)?;
                b.finish()?;
                sheets.push(SheetInfo { name, rel, visibility });
            }
            _ => {}
        }
    }
    Err(invalid("missing workbook end record"))
}

fn rich_string(b: &mut Bytes<'_>) -> Result<String, IoError> {
    let flags = b.u8()?;
    let text = b.text(32767)?;
    if flags & 1 != 0 {
        let count = b.u32()?;
        if count > 32767 {
            return Err(invalid("too many rich-text runs"));
        }
        b.take(count as usize * 4)?;
    }
    if flags & 2 != 0 {
        b.text(32767)?;
        let count = b.u32()?;
        if count > 32767 {
            return Err(invalid("too many phonetic runs"));
        }
        // [MS-XLSB] 2.5.103 PhRun: ichFirst, ichMom, cchMom, ifnt (u16 each)
        // then phType/alcH/unused1 (16 bits) = 10 bytes. A RichStr is always
        // the last field of its record (BrtSSTItem, BrtCellRichSt) and
        // phonetic data is not imported, so tolerate writers that pad runs:
        // require the documented size, then drop whatever follows.
        b.take(count as usize * 10)?;
        b.rest = &[];
    }
    Ok(text)
}

fn shared_strings(bytes: &[u8], budget: &mut Budget, report: &mut ReadReport) -> Result<Vec<Arc<str>>, IoError> {
    let mut records = Records::new(bytes);
    let Some((159, payload)) = records.next(report)? else {
        return Err(invalid("missing shared-string begin record"));
    };
    let mut header = Bytes::new(payload);
    let total = header.u32()?;
    let unique = header.u32()?;
    header.finish()?;
    if unique > total || unique as usize > MAX_STRINGS {
        return Err(IoError::TooLarge("invalid or oversized shared-string count".into()));
    }
    let mut strings = vec![];
    while let Some((id, payload)) = records.next(report)? {
        match id {
            160 => {
                records.end(payload, report)?;
                if strings.len() != unique as usize {
                    return Err(invalid("shared-string count does not match records"));
                }
                return Ok(strings);
            }
            19 => {
                if strings.len() >= unique as usize {
                    return Err(invalid("too many shared-string items"));
                }
                let mut b = Bytes::new(payload);
                let text = rich_string(&mut b)?;
                b.finish()?;
                budget.text(text.len())?;
                strings.push(Arc::from(text));
            }
            _ => return Err(invalid("unexpected shared-string record")),
        }
    }
    Err(invalid("missing shared-string end record"))
}

/// Optional formatting must not prevent recovery of worksheet values. Parse completely before
/// interning styles, and discard parser warnings and text charges if the styles part is damaged.
fn import_styles(
    package: &mut Package<'_>,
    types: &[Entry],
    rels: &[Rel],
    wb: &mut Workbook,
    budget: &mut Budget,
    report: &mut ReadReport,
) -> Vec<StyleId> {
    let mut rels = rels.iter().filter(|r| r.kind == "styles");
    let Some(rel) = rels.next() else { return vec![] };
    let text_before = budget.text;
    let mut parse = || -> Result<_, IoError> {
        if rel.external || rels.next().is_some() {
            return Err(invalid("external or multiple styles relationships"));
        }
        check_type(types, &rel.target, STYLES)?;
        styles::parse(&required(package, &rel.target)?, budget)
    };
    match parse() {
        Ok(parsed) => styles::build(parsed, wb),
        Err(e) => {
            budget.text = text_before;
            report.warn(format!("XLSB styles could not be imported; default styles were used: {e}"));
            vec![]
        }
    }
}

fn error(code: u8) -> Result<CellError, IoError> {
    Ok(match code {
        0x00 => CellError::Null,
        0x07 => CellError::Div0,
        0x0f => CellError::Value,
        0x17 => CellError::Ref,
        0x1d => CellError::Name,
        0x24 => CellError::Num,
        0x2a => CellError::NA,
        0x2b => CellError::GettingData,
        _ => return Err(invalid("unknown cell error code")),
    })
}
fn boolean(b: &mut Bytes<'_>) -> Result<bool, IoError> {
    match b.u8()? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(invalid("invalid boolean value")),
    }
}
/// The formula flags and `CellParsedFormula` after a formula cell's saved result: returns the
/// tokens (`rgce`); the extra data (`rgcb`) is only used by tokens the decoder refuses.
fn cached_formula<'a>(b: &mut Bytes<'a>) -> Result<&'a [u8], IoError> {
    b.take(2)?;
    let cce = b.u32()?;
    if !(1..=16384).contains(&cce) {
        return Err(invalid("invalid formula token length"));
    }
    let rgce = b.take(cce as usize)?;
    let extra = b.u32()?;
    b.take(extra as usize)?;
    Ok(rgce)
}
/// A cell's value and, for formula cells, the formula's tokens.
fn value<'a>(id: u16, b: &mut Bytes<'a>, strings: &[Arc<str>], budget: &mut Budget) -> Result<(Value, Option<&'a [u8]>), IoError> {
    let value = match id {
        1 => Value::Empty,
        2 => {
            let rk = b.u32()?;
            let n = if rk & 2 != 0 { ((rk as i32) >> 2) as f64 } else { f64::from_bits(u64::from(rk & !3) << 32) };
            let n = if rk & 1 != 0 { n / 100.0 } else { n };
            if !n.is_finite() {
                return Err(invalid("nonfinite RK number"));
            }
            Value::Number(n)
        }
        3 | 11 => Value::Error(error(b.u8()?)?),
        4 | 10 => Value::Bool(boolean(b)?),
        5 | 9 => Value::Number(b.f64()?),
        6 | 8 | 62 => {
            let s = if id == 62 { rich_string(b)? } else { b.text(32767)? };
            budget.text(s.len())?;
            Value::Text(Arc::from(s))
        }
        7 => Value::Text(strings.get(b.u32()? as usize).cloned().ok_or_else(|| invalid("shared-string index is out of bounds"))?),
        _ => return Err(invalid("unsupported cell record")),
    };
    let rgce = if (8..=11).contains(&id) { Some(cached_formula(b)?) } else { None };
    Ok((value, rgce))
}

/// The formula of `rgce` at `pos`: `Ok(None)` for a lone constant (`=1`), which calculates
/// nothing and is kept as its saved value; `Err` when it can't be represented.
fn formula(rgce: &[u8], pos: CellRef) -> Result<Option<Formula>, ()> {
    let e = decode_xlsb(rgce, pos).map_err(|_| ())?;
    if matches!(e, Expr::Number(_) | Expr::Bool(_) | Expr::Error(_) | Expr::Text(_)) {
        return Ok(None);
    }
    let f = Formula::new(&gridcraft_formula::print(&e));
    if f.expr.is_none() {
        return Err(());
    }
    Ok(Some(f))
}

fn worksheet(
    bytes: &[u8],
    name: &str,
    strings: &[Arc<str>],
    styles: &[StyleId],
    budget: &mut Budget,
    report: &mut ReadReport,
) -> Result<Sheet, IoError> {
    let mut records = Records::new(bytes);
    records.first(129, report)?;
    let mut sheet = Sheet::new(name);
    let mut data = false;
    let mut data_seen = false;
    let mut row = None;
    let mut col = None;
    let mut col_budget = MAX_COL_ASSIGNMENTS;
    let mut view_seen = false;
    let mut in_first_view = false;
    while let Some((id, payload)) = records.next(report)? {
        let mut b = Bytes::new(payload);
        match id {
            130 => {
                if data || !data_seen {
                    return Err(invalid("incomplete worksheet data"));
                }
                records.end(payload, report)?;
                return Ok(sheet);
            }
            145 => {
                if data_seen {
                    return Err(invalid("duplicate worksheet data"));
                }
                data_seen = true;
                data = true;
                b.finish()?;
            }
            146 => {
                if !data {
                    return Err(invalid("unexpected worksheet data end"));
                }
                data = false;
                b.finish()?;
            }
            0 => {
                if !data {
                    return Err(invalid("row outside worksheet data"));
                }
                let r = b.u32()?;
                if r >= MAX_ROWS || row.is_some_and(|previous| r <= previous) {
                    return Err(invalid("row is out of bounds or unordered"));
                }
                let ixfe = b.u32()?;
                let height = b.u16()?;
                let flags = b.u16()?;
                b.take(1)?;
                let spans = b.u32()?;
                if spans > 16 {
                    return Err(invalid("too many row spans"));
                }
                b.take(spans as usize * 8)?;
                b.finish()?;
                row_layout(&mut sheet, r, ixfe, height, flags, styles);
                row = Some(r);
                col = None;
            }
            1..=11 | 62 => {
                if !data {
                    return Err(invalid("cell outside worksheet data"));
                }
                let r = row.ok_or_else(|| invalid("cell before row header"))?;
                let c = b.u32()?;
                if c >= MAX_COLS || col.is_some_and(|previous| c <= previous) {
                    return Err(invalid("column is out of bounds or unordered"));
                }
                let style_ref = b.u32()? & 0x00ff_ffff; // Low 24 bits are iStyleRef.
                let style = styles.get(style_ref as usize).copied().unwrap_or(StyleId::DEFAULT);
                let (value, rgce) = value(id, &mut b, strings, budget)?;
                b.finish()?;
                let pos = CellRef::new(r, c);
                let formula = rgce.and_then(|rgce| {
                    formula(rgce, pos).unwrap_or_else(|()| {
                        budget.formulas_kept += 1;
                        None
                    })
                });
                if !value.is_empty() || formula.is_some() || style != StyleId::DEFAULT {
                    budget.cell()?;
                    let mut cell = Cell::value(value);
                    cell.formula = formula.map(Arc::new);
                    cell.style = style;
                    sheet.cells.set(pos, cell);
                }
                col = Some(c);
            }
            176 => {
                if data {
                    return Err(invalid("merge inside worksheet data"));
                }
                let r0 = b.u32()?;
                let r1 = b.u32()?;
                let c0 = b.u32()?;
                let c1 = b.u32()?;
                b.finish()?;
                if r0 > r1 || c0 > c1 || r1 >= MAX_ROWS || c1 >= MAX_COLS {
                    report.warn("Invalid XLSB merged ranges were skipped.");
                    continue;
                }
                if sheet.merges.len() >= 100_000 {
                    return Err(IoError::TooLarge("XLSB exceeds 100000 merges".into()));
                }
                sheet.merges.push(RangeRef::new(CellRef::new(r0, c0), CellRef::new(r1, c1)));
            }
            60 => {
                if col_info(&mut sheet, payload, styles, &mut col_budget).is_err() {
                    report.warn("Malformed XLSB column information records were skipped.");
                }
            }
            485 => {
                if sheet_format(&mut sheet, payload).is_err() {
                    report.warn("Malformed XLSB default row height and column width records were skipped.");
                }
            }
            147 => {
                if tab_color(&mut sheet, payload).is_err() {
                    report.warn("Malformed XLSB sheet property records were skipped.");
                }
            }
            137 => match sheet_view(&mut sheet, payload, view_seen) {
                Ok(first) => {
                    view_seen = true;
                    in_first_view = first;
                }
                Err(_) => report.warn("Malformed XLSB sheet view records were skipped."),
            },
            138 => in_first_view = false,
            151 if in_first_view && pane(&mut sheet, payload).is_err() => report.warn("Malformed XLSB pane records were skipped."),
            _ => {}
        }
    }
    Err(invalid("missing worksheet end record"))
}

/// Row height, visibility, outline and style from the BrtRowHdr fields after `rw`.
fn row_layout(sheet: &mut Sheet, r: u32, ixfe: u32, height: u16, flags: u16, styles: &[StyleId]) {
    let on = |bit: u16| flags & bit != 0;
    let mut info = LineInfo { custom: on(0x2000), hidden: on(0x1000), ..LineInfo::default() };
    let px = pt_to_px(f64::from(height) / 20.0);
    if px > 0.0 && (info.custom || info.hidden || (px - sheet.default_row_height).abs() > 0.5) {
        info.size = Some(px);
    }
    info.outline = ((flags >> 8) & 7) as u8;
    info.collapsed = on(0x0800);
    // fGhostDirty: the row carries its own format.
    if on(0x4000) {
        let style = styles.get(ixfe as usize).copied().unwrap_or(StyleId::DEFAULT);
        if style != StyleId::DEFAULT {
            info.style = Some(style);
        }
    }
    if info != LineInfo::default() {
        sheet.rows.insert(r, info);
    }
}

/// BrtColInfo: colFirst, colLast, coldx (1/256 characters), ixfe and flags.
fn col_info(sheet: &mut Sheet, payload: &[u8], styles: &[StyleId], budget: &mut u64) -> Result<(), IoError> {
    let mut b = Bytes::new(payload);
    let first = b.u32()?;
    let last = b.u32()?.min(MAX_COLS - 1);
    let width = b.u32()?;
    let ixfe = b.u32()?;
    let flags = b.u16()?;
    if first > last {
        return Err(invalid("invalid column range"));
    }
    let mut info = LineInfo::default();
    let px = col_width_to_px(f64::from(width) / 256.0);
    // fUserSet: a custom width equal to the default still pins the column.
    if flags & 0x0002 != 0 || px != sheet.default_col_width {
        info.size = Some(px);
    }
    info.hidden = flags & 0x0001 != 0;
    info.outline = ((flags >> 8) & 7) as u8;
    info.collapsed = flags & 0x1000 != 0;
    let style = styles.get(ixfe as usize).copied().unwrap_or(StyleId::DEFAULT);
    if style != StyleId::DEFAULT {
        info.style = Some(style);
    }
    if info == LineInfo::default() {
        return Ok(());
    }
    *budget = budget.checked_sub(u64::from(last - first) + 1).ok_or_else(|| invalid("too many column assignments"))?;
    for c in first..=last {
        sheet.cols.insert(c, info);
    }
    Ok(())
}

/// BrtWsFmtInfo: default column width (dxGCol, else cchDefColWidth) and default row height.
fn sheet_format(sheet: &mut Sheet, payload: &[u8]) -> Result<(), IoError> {
    let mut b = Bytes::new(payload);
    let width = b.u32()?;
    let chars = b.u16()?;
    let height = b.u16()?;
    let px = pt_to_px(f64::from(height) / 20.0);
    if px > 0.0 {
        sheet.default_row_height = px;
    }
    if width != u32::MAX {
        let px = col_width_to_px(f64::from(width) / 256.0);
        if px > 0.0 {
            sheet.default_col_width = px;
        }
    } else {
        // A base width in characters gets the cell padding, rounded up to whole 8 px steps.
        let px = ((f64::from(chars).min(255.0) * 7.0 + 5.0) / 8.0).ceil() * 8.0;
        sheet.default_col_width = px as f32;
    }
    Ok(())
}

/// BrtWsProp: the tab colour (BrtColor) follows three bytes of flags.
fn tab_color(sheet: &mut Sheet, payload: &[u8]) -> Result<(), IoError> {
    let mut b = Bytes::new(payload);
    b.take(3)?;
    let kind = b.u8()? >> 1;
    let index = b.u8()?;
    b.u16()?;
    let rgb = b.take(3)?;
    let rgb = rgb.iter().fold(0u32, |acc, &c| (acc << 8) | u32::from(c));
    sheet.tab_color = match kind {
        1 => INDEXED.get(usize::from(index)).map(|&c| Color::Rgb(c)),
        2 => Some(Color::Rgb(rgb)),
        _ => None,
    };
    Ok(())
}

/// BrtBeginWsView: display options of the first workbook view's sheet view. `Ok(true)` when
/// this view was applied (its pane applies too).
fn sheet_view(sheet: &mut Sheet, payload: &[u8], seen: bool) -> Result<bool, IoError> {
    let mut b = Bytes::new(payload);
    let flags = b.u16()?;
    b.take(16)?; // xlView, rwTop, colLeft, icvHdr and reserved fields.
    let zoom = b.u16()?;
    b.take(6)?; // wScaleNormal, wScaleSLV, wScalePLV.
    let book_view = b.u32()?;
    if seen || book_view != 0 {
        return Ok(false);
    }
    let on = |bit: u16| flags & bit != 0;
    sheet.show_formulas = on(0x0002);
    sheet.show_gridlines = on(0x0004);
    sheet.show_headings = on(0x0008);
    sheet.show_zeros = on(0x0010);
    sheet.right_to_left = on(0x0020);
    sheet.show_outline_symbols = on(0x0100);
    sheet.zoom = if zoom == 0 { 100 } else { zoom.clamp(10, 400) };
    Ok(true)
}

/// BrtPane: frozen panes keep their counts of frozen rows and columns; split panes are ignored.
fn pane(sheet: &mut Sheet, payload: &[u8]) -> Result<(), IoError> {
    let mut b = Bytes::new(payload);
    let x = b.f64()?;
    let y = b.f64()?;
    let top = b.u32()?;
    let left = b.u32()?;
    b.u32()?; // pnnAcct.
    let flags = b.u8()?;
    if flags & 0x01 == 0 {
        return Ok(());
    }
    let cols = x.clamp(0.0, f64::from(MAX_COLS - 1)) as u32;
    let rows = y.clamp(0.0, f64::from(MAX_ROWS - 1)) as u32;
    if rows > 0 || cols > 0 {
        sheet.freeze = Some((rows, cols));
        if top < MAX_ROWS && left < MAX_COLS {
            sheet.view_top_left = CellRef::new(top, left);
        }
    }
    Ok(())
}
