//! XLSB links to other workbooks: the cached cells of the External Link part (BrtExternRowHdr,
//! BrtExternCell*) translated to `<sheetDataSet>`, PtgArea3d into a linked workbook, quoted linked
//! sheet names and a damaged second link, built record by record from the public [MS-XLSB] layout.
use gridcraft_core::{CellRef, Value};

use super::make_zip;
use super::xlsb_tests::{record, row, sst, wide, worksheet};
use crate::xls::formula::external_sheet_text;
use crate::{read_xlsb, read_xlsx, write_xlsx};

const ROOT: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="book" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.bin"/></Relationships>"#;
const EXT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLink";
const PATH_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLinkPath";

fn at(a: &str) -> CellRef {
    CellRef::parse(a).unwrap()
}

/// Sheet1, this workbook (BrtSupSelf), then `links` external books (BrtSupBookSrc `ext1`, `ext2`…).
/// XTI 0 = Sheet1; XTI k = external book k, sheet 0.
fn book(links: u32) -> Vec<u8> {
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
    out.extend(record(358, &[0, 0]));
    for k in 1..=links {
        out.extend(record(355, &wide(&format!("ext{k}"))));
    }
    let mut xti = (links + 1).to_le_bytes().to_vec();
    for sup in 0..=links {
        xti.extend(sup.to_le_bytes());
        xti.extend(0u32.to_le_bytes());
        xti.extend(0u32.to_le_bytes());
    }
    out.extend(record(362, &xti));
    out.extend(record(132, &[]));
    out
}

/// A link part for one sheet named `sheet`, its cached row 2 holding `cells`.
pub(super) fn link_part(sheet: &str, cells: &[u8]) -> Vec<u8> {
    let mut begin = 0u16.to_le_bytes().to_vec();
    begin.extend(wide("rId1"));
    begin.extend(u32::MAX.to_le_bytes());
    let mut out = record(360, &begin);
    let mut tabs = 1u32.to_le_bytes().to_vec();
    tabs.extend(wide(sheet));
    out.extend(record(359, &tabs));
    out.extend(record(363, &[0, 0, 0, 0, 1]));
    if !cells.is_empty() {
        out.extend(record(366, &1u32.to_le_bytes()));
        out.extend_from_slice(cells);
    }
    out.extend(record(364, &[]));
    out.extend(record(588, &[]));
    out
}

/// BrtExternCellReal, String, Bool, Error and Blank at B2..F2.
fn cached_cells() -> Vec<u8> {
    let mut out = vec![];
    let mut real = 1u32.to_le_bytes().to_vec();
    real.extend(42.5f64.to_le_bytes());
    out.extend(record(368, &real));
    let mut text = 2u32.to_le_bytes().to_vec();
    text.extend(wide("a<b"));
    out.extend(record(371, &text));
    let mut boolean = 3u32.to_le_bytes().to_vec();
    boolean.push(1);
    out.extend(record(369, &boolean));
    let mut err = 4u32.to_le_bytes().to_vec();
    err.push(0x2A);
    out.extend(record(370, &err));
    out.extend(record(367, &5u32.to_le_bytes()));
    out
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

/// PtgArea3d (reference class) over rows r0..r1, columns c0..c1, all relative.
fn ptg_area3d(ixti: u16, r0: u32, r1: u32, c0: u16, c1: u16) -> Vec<u8> {
    let mut out = vec![0x3B];
    out.extend(ixti.to_le_bytes());
    out.extend(r0.to_le_bytes());
    out.extend(r1.to_le_bytes());
    out.extend((c0 | 0xC000).to_le_bytes());
    out.extend((c1 | 0xC000).to_le_bytes());
    out
}

/// A1 = SUM(<link 1>!B2:C3) (cached 7); B1 = <link 1>!B2 (cached 42.5); C1 = <last link>!B2
/// (cached 3).
fn sheet(links: u16) -> Vec<u8> {
    let mut a1 = ptg_area3d(1, 1, 2, 1, 2);
    a1.extend([0x22, 1, 4, 0]);
    let mut cells = row(0);
    cells.extend(fmla_num(0, 7.0, &a1));
    cells.extend(fmla_num(1, 42.5, &ptg_ref3d(1, 1, 1)));
    cells.extend(fmla_num(2, 3.0, &ptg_ref3d(links, 1, 1)));
    worksheet(&cells, &[])
}

/// A package with one link part per entry of `parts`.
pub(super) fn file(parts: &[Vec<u8>]) -> Vec<u8> {
    let n = parts.len();
    let mut rels = String::from(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="one" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.bin"/><Relationship Id="strings" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.bin"/>"#,
    );
    let mut types = String::from(
        r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/xl/workbook.bin" ContentType="application/vnd.ms-excel.sheet.binary.macroEnabled.main"/><Override PartName="/xl/worksheets/sheet1.bin" ContentType="application/vnd.ms-excel.worksheet"/><Override PartName="/xl/sharedStrings.bin" ContentType="application/vnd.ms-excel.sharedStrings"/>"#,
    );
    let mut names = vec![];
    for k in 1..=n {
        rels.push_str(&format!(r#"<Relationship Id="ext{k}" Type="{EXT}" Target="externalLinks/externalLink{k}.bin"/>"#));
        types.push_str(&format!(
            r#"<Override PartName="/xl/externalLinks/externalLink{k}.bin" ContentType="application/vnd.ms-excel.externalLink"/>"#
        ));
        names.push((
            format!("xl/externalLinks/externalLink{k}.bin"),
            format!("xl/externalLinks/_rels/externalLink{k}.bin.rels"),
            format!(
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{PATH_REL}" Target="book{k}.xlsx" TargetMode="External"/></Relationships>"#
            ),
        ));
    }
    rels.push_str("</Relationships>");
    types.push_str("</Types>");
    let (book, sheet, strings) = (book(n as u32), sheet(n as u16), sst());
    let mut entries: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", ROOT.as_bytes()),
        ("xl/workbook.bin", &book),
        ("xl/_rels/workbook.bin.rels", rels.as_bytes()),
        ("xl/worksheets/sheet1.bin", &sheet),
        ("xl/sharedStrings.bin", &strings),
    ];
    for ((part, rels_name, rels_xml), data) in names.iter().zip(parts) {
        entries.push((part, data));
        entries.push((rels_name, rels_xml.as_bytes()));
    }
    make_zip(&entries)
}

fn formula(wb: &gridcraft_model::Workbook, a1: &str) -> Option<String> {
    wb.sheets[0].cell(at(a1)).and_then(|c| c.formula.as_ref()).map(|f| f.text.clone())
}

pub(super) fn saved_part(bytes: &[u8], name: &str) -> String {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut s = String::new();
    std::io::Read::read_to_string(&mut z.by_name(name).unwrap(), &mut s).unwrap();
    s
}

#[test]
fn area_into_a_linked_workbook_keeps_its_formula() {
    let (wb, report) = read_xlsb(&file(&[link_part("Data", &cached_cells())])).unwrap();
    assert_eq!(formula(&wb, "A1").as_deref(), Some("SUM([1]Data!B2:C3)"), "{:?}", report.warnings);
    assert_eq!(wb.sheets[0].cell(at("A1")).map(|c| c.value.clone()), Some(Value::Number(7.0)));
}

#[test]
fn cached_cells_are_saved_as_the_sheet_data_set() {
    let (wb, _) = read_xlsb(&file(&[link_part("Data", &cached_cells())])).unwrap();
    let saved = write_xlsx(&wb).unwrap();
    let link = saved_part(&saved, "xl/externalLinks/externalLink1.xml");
    assert!(link.contains(r#"<sheetData sheetId="0" refreshError="1"><row r="2">"#), "{link}");
    for cell in [
        r#"<cell r="B2"><v>42.5</v></cell>"#,
        r#"<cell r="C2" t="str"><v>a&lt;b</v></cell>"#,
        r#"<cell r="D2" t="b"><v>1</v></cell>"#,
        r##"<cell r="E2" t="e"><v>#N/A</v></cell>"##,
        r#"<cell r="F2"/>"#,
    ] {
        assert!(link.contains(cell), "{cell} in {link}");
    }
    let (back, report) = read_xlsx(&saved).unwrap();
    assert_eq!(formula(&back, "B1").as_deref(), Some("[1]Data!B2"));
    assert!(report.warnings.iter().any(|w| w.contains("external workbooks were kept")), "{:?}", report.warnings);
}

#[test]
fn linked_sheet_names_are_quoted_like_any_sheet_name() {
    let (wb, _) = read_xlsb(&file(&[link_part("My Data", &[])])).unwrap();
    assert_eq!(formula(&wb, "B1").as_deref(), Some("'[1]My Data'!B2"));
}

#[test]
fn second_link_is_numbered_by_its_position() {
    let (wb, _) = read_xlsb(&file(&[link_part("Data", &[]), link_part("Other", &[])])).unwrap();
    assert_eq!(formula(&wb, "B1").as_deref(), Some("[1]Data!B2"));
    assert_eq!(formula(&wb, "C1").as_deref(), Some("[2]Other!B2"));
    let saved = write_xlsx(&wb).unwrap();
    assert!(saved_part(&saved, "xl/externalLinks/_rels/externalLink2.xml.rels").contains(r#"Target="book2.xlsx""#));
}

#[test]
fn one_damaged_link_keeps_every_link_formula_as_its_value() {
    let mut damaged = link_part("Other", &[]);
    damaged.truncate(damaged.len() - 2);
    let (wb, report) = read_xlsb(&file(&[link_part("Data", &[]), damaged])).unwrap();
    assert_eq!(formula(&wb, "B1"), None);
    assert_eq!(formula(&wb, "C1"), None);
    assert_eq!(wb.sheets[0].cell(at("C1")).map(|c| c.value.clone()), Some(Value::Number(3.0)));
    assert!(report.warnings.iter().any(|w| w.contains("links to external workbooks were dropped")), "{:?}", report.warnings);
    assert!(!write_xlsx(&wb).map(|b| String::from_utf8_lossy(&b).contains("externalLink")).unwrap());
}

#[test]
fn cached_cell_outside_the_grid_drops_the_link() {
    let mut bad = 16_384u32.to_le_bytes().to_vec();
    bad.extend(1.0f64.to_le_bytes());
    let (wb, report) = read_xlsb(&file(&[link_part("Data", &record(368, &bad))])).unwrap();
    assert_eq!(formula(&wb, "B1"), None);
    assert!(report.warnings.iter().any(|w| w.contains("dropped")), "{:?}", report.warnings);
}

#[test]
fn external_sheet_text_leaves_string_constants_alone() {
    assert_eq!(external_sheet_text("'[1]Data'!B2&\"'[1]Data'!\""), ("[1]Data!B2&\"'[1]Data'!\"".to_string(), true));
    assert_eq!(external_sheet_text("'[2]Data:Totals'!B2"), ("[2]Data:Totals!B2".to_string(), true));
    assert_eq!(external_sheet_text("'[1]O''Neil'!B2"), ("'[1]O''Neil'!B2".to_string(), true));
    assert_eq!(external_sheet_text("'My Sheet'!A1"), ("'My Sheet'!A1".to_string(), false));
    assert_eq!(external_sheet_text("'unterminated"), ("'unterminated".to_string(), false));
}
