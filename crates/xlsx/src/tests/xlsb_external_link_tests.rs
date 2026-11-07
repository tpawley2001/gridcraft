//! XLSB links to other workbooks: BrtSupBookSrc in the workbook part, the binary external link
//! part (BrtBeginSupBook, BrtSupTabs, BrtExternTableStart/End, BrtEndSupBook) and PtgRef3d into
//! it, assembled from the public [MS-XLSB] layout and compared with the `.xlsx` reader.
use gridcraft_core::{CellRef, Value};

use super::make_zip;
use super::xlsb_tests::{record, row, sst, wide, worksheet};
use crate::{read_xlsb, read_xlsx, write_xlsx};

const ROOT: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="book" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="records/book.bin"/></Relationships>"#;
const RELS: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="one" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="../data/one.bin"/><Relationship Id="strings" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="../text/strings.bin"/><Relationship Id="ext1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLink" Target="../links/externalLink1.bin"/></Relationships>"#;
const TYPES: &str = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/records/book.bin" ContentType="application/vnd.ms-excel.sheet.binary.macroEnabled.main"/><Override PartName="/data/one.bin" ContentType="application/vnd.ms-excel.worksheet"/><Override PartName="/text/strings.bin" ContentType="application/vnd.ms-excel.sharedStrings"/><Override PartName="/links/externalLink1.bin" ContentType="application/vnd.ms-excel.externalLink"/></Types>"#;
const PATH_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLinkPath";
const MISSING_REL: &str = "http://schemas.microsoft.com/office/2006/relationships/xlExternalLinkPath/xlPathMissing";

fn at(a: &str) -> CellRef {
    CellRef::parse(a).unwrap()
}

/// Sheet "Sheet1"; supporting link 0 is this workbook (BrtSupSelf), link 1 the external book
/// (BrtSupBookSrc, relationship `ext1`). XTI 0 = Sheet1, XTI 1 = external sheet 0, XTI 2 =
/// external sheets 0..1.
fn book() -> Vec<u8> {
    let mut out = record(131, &[]);
    let mut props = 0u32.to_le_bytes().to_vec();
    props.extend(0u32.to_le_bytes());
    props.extend(wide(""));
    out.extend(record(153, &props));
    out.extend(record(143, &[]));
    let mut data = 0u32.to_le_bytes().to_vec();
    data.extend(1u32.to_le_bytes());
    data.extend(wide("one"));
    data.extend(wide("Sheet1"));
    out.extend(record(156, &data));
    out.extend(record(144, &[]));
    out.extend(record(358, &[0, 0])); // BrtSupSelf
    out.extend(record(355, &wide("ext1"))); // BrtSupBookSrc
    let mut xti = 3u32.to_le_bytes().to_vec();
    for (sup, first, last) in [(0u32, 0u32, 0u32), (1, 0, 0), (1, 0, 1)] {
        xti.extend(sup.to_le_bytes());
        xti.extend(first.to_le_bytes());
        xti.extend(last.to_le_bytes());
    }
    out.extend(record(362, &xti));
    out.extend(record(132, &[]));
    out
}

/// The external link part as Excel writes it (layout measured on a real file): BrtBeginSupBook
/// (sbt 0, the path relationship id, a null string), BrtSupTabs, one empty cached table per sheet,
/// BrtEndSupBook.
fn link_part(sheets: &[&str]) -> Vec<u8> {
    let mut begin = 0u16.to_le_bytes().to_vec();
    begin.extend(wide("rId1"));
    begin.extend(u32::MAX.to_le_bytes());
    let mut out = record(360, &begin);
    let mut tabs = (sheets.len() as u32).to_le_bytes().to_vec();
    for s in sheets {
        tabs.extend(wide(s));
    }
    out.extend(record(359, &tabs));
    for i in 0..sheets.len() as u32 {
        let mut start = i.to_le_bytes().to_vec();
        start.push(0);
        out.extend(record(363, &start));
        out.extend(record(364, &[]));
    }
    out.extend(record(588, &[]));
    out
}

fn link_rels(rel_type: &str, target: &str) -> String {
    format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{rel_type}" Target="{target}" TargetMode="External"/></Relationships>"#
    )
}

/// BrtFmlaNum at `col` with a cached number and the given formula tokens.
fn fmla_num(col: u32, cached: f64, rgce: &[u8]) -> Vec<u8> {
    let mut data = col.to_le_bytes().to_vec();
    data.extend([0; 4]);
    data.extend(cached.to_le_bytes());
    data.extend([0; 2]);
    data.extend((rgce.len() as u32).to_le_bytes());
    data.extend(rgce);
    data.extend(0u32.to_le_bytes());
    record(9, &data)
}

/// PtgRef3d (reference class), relative row and column.
fn ptg_ref3d(ixti: u16, row: u32, col: u16) -> Vec<u8> {
    let mut out = vec![0x3A];
    out.extend(ixti.to_le_bytes());
    out.extend(row.to_le_bytes());
    out.extend((col | 0xC000).to_le_bytes());
    out
}

/// A1 = [1]Data!B2*2 (cached 84); B1 = Sheet1!C5 (internal, cached 0); C1 = SUM([1]Data:Totals!B2)
/// (cached 50).
fn sheet() -> Vec<u8> {
    let mut a1 = ptg_ref3d(1, 1, 1);
    a1.extend([0x1E, 2, 0, 0x05]); // PtgInt 2, PtgMul
    let b1 = ptg_ref3d(0, 4, 2);
    let mut c1 = ptg_ref3d(2, 1, 1);
    c1.extend([0x22, 1, 4, 0]); // PtgFuncVar SUM, 1 argument
    let mut cells = row(0);
    cells.extend(fmla_num(0, 84.0, &a1));
    cells.extend(fmla_num(1, 0.0, &b1));
    cells.extend(fmla_num(2, 50.0, &c1));
    worksheet(&cells, &[])
}

fn file(rel_type: &str, target: &str) -> Vec<u8> {
    let (book, sheet, strings, link) = (book(), sheet(), sst(), link_part(&["Data", "Totals"]));
    let rels = link_rels(rel_type, target);
    make_zip(&[
        ("[Content_Types].xml", TYPES.as_bytes()),
        ("_rels/.rels", ROOT.as_bytes()),
        ("records/book.bin", &book),
        ("records/_rels/book.bin.rels", RELS.as_bytes()),
        ("data/one.bin", &sheet),
        ("text/strings.bin", &strings),
        ("links/externalLink1.bin", &link),
        ("links/_rels/externalLink1.bin.rels", rels.as_bytes()),
    ])
}

fn formula(wb: &gridcraft_model::Workbook, a1: &str) -> Option<String> {
    wb.sheets[0].cell(at(a1)).and_then(|c| c.formula.as_ref()).map(|f| f.text.clone())
}

fn saved_text(bytes: &[u8]) -> String {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut all = String::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let mut s = String::new();
        if std::io::Read::read_to_string(&mut f, &mut s).is_ok() {
            all.push_str(f.name());
            all.push('\n');
            all.push_str(&s);
            all.push('\n');
        }
    }
    all
}

#[test]
fn external_reference_keeps_its_formula_and_cached_value() {
    let (wb, report) = read_xlsb(&file(PATH_REL, "other.xlsx")).unwrap();
    assert_eq!(formula(&wb, "A1").as_deref(), Some("[1]Data!B2*2"), "{:?}", report.warnings);
    assert_eq!(wb.sheets[0].cell(at("A1")).map(|c| c.value.clone()), Some(Value::Number(84.0)));
    assert!(
        !report.warnings.iter().any(|w| w.contains("can't be imported yet")),
        "external-link formulas are imported, not kept as values: {:?}",
        report.warnings
    );
}

#[test]
fn external_sheet_span_keeps_its_formula() {
    let (wb, _) = read_xlsb(&file(PATH_REL, "other.xlsx")).unwrap();
    assert_eq!(formula(&wb, "C1").as_deref(), Some("SUM([1]Data:Totals!B2)"));
}

#[test]
fn internal_reference_next_to_an_external_link_is_unchanged() {
    let (wb, _) = read_xlsb(&file(PATH_REL, "other.xlsx")).unwrap();
    assert_eq!(formula(&wb, "B1").as_deref(), Some("Sheet1!C5"));
}

#[test]
fn link_survives_save_as_xlsx_like_the_xlsx_reader() {
    let (wb, _) = read_xlsb(&file(PATH_REL, "other.xlsx")).unwrap();
    let saved = write_xlsx(&wb).unwrap();
    let text = saved_text(&saved);
    assert!(text.contains("externalReference"), "workbook.xml lists the link");
    assert!(text.contains(r#"<sheetName val="Data"/>"#) && text.contains(r#"<sheetName val="Totals"/>"#), "{text}");
    assert!(text.contains(PATH_REL) && text.contains(r#"Target="other.xlsx""#), "{text}");
    let (back, report) = read_xlsx(&saved).unwrap();
    assert_eq!(formula(&back, "A1").as_deref(), Some("[1]Data!B2*2"));
    assert_eq!(formula(&back, "C1").as_deref(), Some("SUM([1]Data:Totals!B2)"));
    assert!(report.warnings.iter().any(|w| w.contains("external workbooks were kept")), "{:?}", report.warnings);
}

#[test]
fn missing_path_relationship_is_kept_verbatim() {
    let (wb, _) = read_xlsb(&file(MISSING_REL, "ab")).unwrap();
    assert_eq!(formula(&wb, "A1").as_deref(), Some("[1]Data!B2*2"));
    let text = saved_text(&write_xlsx(&wb).unwrap());
    assert!(text.contains(MISSING_REL) && text.contains(r#"Target="ab""#), "{text}");
}

#[test]
fn damaged_link_part_keeps_the_formulas_as_values() {
    let mut bytes_link = link_part(&["Data", "Totals"]);
    bytes_link.truncate(9);
    let (book, sheet, strings) = (book(), sheet(), sst());
    let rels = link_rels(PATH_REL, "other.xlsx");
    let bytes = make_zip(&[
        ("[Content_Types].xml", TYPES.as_bytes()),
        ("_rels/.rels", ROOT.as_bytes()),
        ("records/book.bin", &book),
        ("records/_rels/book.bin.rels", RELS.as_bytes()),
        ("data/one.bin", &sheet),
        ("text/strings.bin", &strings),
        ("links/externalLink1.bin", &bytes_link),
        ("links/_rels/externalLink1.bin.rels", rels.as_bytes()),
    ]);
    let (wb, _) = read_xlsb(&bytes).unwrap();
    assert_eq!(wb.sheets[0].cell(at("A1")).map(|c| c.value.clone()), Some(Value::Number(84.0)));
    assert_eq!(formula(&wb, "B1").as_deref(), Some("Sheet1!C5"));
}
