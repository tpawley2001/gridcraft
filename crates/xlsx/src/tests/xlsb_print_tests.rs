//! XLSB print settings: original BIFF12 records assembled in Rust from [MS-XLSB] 2.4, compared
//! with the model the `.xlsx` reader builds from the matching SpreadsheetML elements.
use gridcraft_model::Orientation;

use super::make_zip;
use super::minimal;
use crate::{read_xlsb, read_xlsx};

const ROOT: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="book" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="records/book.bin"/></Relationships>"#;
const RELS: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="one" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="../data/one.bin"/></Relationships>"#;
const TYPES: &str = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/records/book.bin" ContentType="application/vnd.ms-excel.sheet.binary.macroEnabled.main"/><Override PartName="/data/one.bin" ContentType="application/vnd.ms-excel.worksheet"/></Types>"#;

pub(super) fn record(id: u16, bytes: &[u8]) -> Vec<u8> {
    let mut out = vec![];
    if id < 128 {
        out.push(id as u8);
    } else {
        out.extend([(id as u8 & 0x7f) | 0x80, (id >> 7) as u8]);
    }
    let mut len = bytes.len();
    loop {
        let byte = (len & 0x7f) as u8;
        len >>= 7;
        out.push(if len == 0 { byte } else { byte | 0x80 });
        if len == 0 {
            break;
        }
    }
    out.extend(bytes);
    out
}

/// An `XLNullableWideString` ([MS-XLSB] 2.5.167); `None` is the null string.
fn wide(text: Option<&str>) -> Vec<u8> {
    let Some(text) = text else { return u32::MAX.to_le_bytes().to_vec() };
    let units: Vec<_> = text.encode_utf16().collect();
    let mut out = (units.len() as u32).to_le_bytes().to_vec();
    for unit in units {
        out.extend(unit.to_le_bytes());
    }
    out
}

fn book() -> Vec<u8> {
    let mut out = record(131, &[]);
    let mut props = 0u32.to_le_bytes().to_vec();
    props.extend(0u32.to_le_bytes());
    props.extend(wide(Some("")));
    out.extend(record(153, &props));
    out.extend(record(143, &[]));
    let mut sheet = 0u32.to_le_bytes().to_vec();
    sheet.extend(1u32.to_le_bytes());
    sheet.extend(wide(Some("one")));
    sheet.extend(wide(Some("Data")));
    out.extend(record(156, &sheet));
    out.extend(record(144, &[]));
    out.extend(record(132, &[]));
    out
}

/// A worksheet part whose body holds `body` outside the cell table, so print records read like
/// the ones Excel writes after `BrtEndSheetData`.
pub(super) fn worksheet(body: &[u8]) -> Vec<u8> {
    let mut out = record(129, &[]);
    out.extend(record(145, &[]));
    out.extend(record(146, &[]));
    out.extend(body);
    out.extend(record(130, &[]));
    out
}

pub(super) fn package(sheet: &[u8]) -> Vec<u8> {
    make_zip(&[
        ("[Content_Types].xml", TYPES.as_bytes()),
        ("_rels/.rels", ROOT.as_bytes()),
        ("records/book.bin", &book()),
        ("records/_rels/book.bin.rels", RELS.as_bytes()),
        ("data/one.bin", sheet),
    ])
}

fn margins(left: f64, right: f64, top: f64, bottom: f64, header: f64, footer: f64) -> Vec<u8> {
    let mut out = vec![];
    for value in [left, right, top, bottom, header, footer] {
        out.extend(value.to_le_bytes());
    }
    record(476, &out)
}

/// `BrtPageSetup`: paper, scale, resolution, copies, first page, fit width/height, flags, printer
/// settings relationship.
pub(super) fn page_setup(paper: u32, scale: u32, first_page: u32, fit: (u32, u32), flags: u16) -> Vec<u8> {
    let mut out = paper.to_le_bytes().to_vec();
    for value in [scale, 600, 600, 1, first_page, fit.0, fit.1] {
        out.extend(value.to_le_bytes());
    }
    out.extend(flags.to_le_bytes());
    out.extend(wide(None));
    record(478, &out)
}

fn print_options(flags: u16) -> Vec<u8> {
    record(477, &flags.to_le_bytes())
}

fn header_footer(flags: u16, texts: [Option<&str>; 6]) -> Vec<u8> {
    let mut out = flags.to_le_bytes().to_vec();
    for text in texts {
        out.extend(wide(text));
    }
    record(479, &out)
}

fn ws_prop(flags: u16) -> Vec<u8> {
    let mut out = flags.to_le_bytes().to_vec();
    out.extend([0; 8]); // brtcolorTab
    out.extend(u32::MAX.to_le_bytes()); // rwSync
    out.extend(u32::MAX.to_le_bytes()); // colSync
    out.extend(wide(Some(""))); // strName
    record(147, &out)
}

fn brk(row: u32) -> Vec<u8> {
    let mut out = row.to_le_bytes().to_vec();
    out.extend(0u32.to_le_bytes());
    out.extend(16383u32.to_le_bytes());
    out.extend(1u32.to_le_bytes()); // fMan
    out.extend(0u32.to_le_bytes()); // fPivot
    record(396, &out)
}

fn row_breaks(breaks: &[u32]) -> Vec<u8> {
    let mut out = record(392, &[breaks.len() as u8, 0, 0, 0, breaks.len() as u8, 0, 0, 0]);
    for at in breaks {
        out.extend(brk(*at));
    }
    out.extend(record(393, &[]));
    out
}

fn col_breaks(breaks: &[u32]) -> Vec<u8> {
    let mut out = record(394, &[breaks.len() as u8, 0, 0, 0, breaks.len() as u8, 0, 0, 0]);
    for at in breaks {
        out.extend(brk(*at));
    }
    out.extend(record(395, &[]));
    out
}

#[test]
fn xlsb_landscape_a4_scale_and_margins_match_the_xlsx_reader() {
    let body = [
        margins(0.5, 0.5, 1.0, 1.0, 0.3, 0.3),
        page_setup(9, 75, 0, (1, 1), 0b0000_0010), // fLandscape
    ]
    .concat();
    let (wb, _) = read_xlsb(&package(&worksheet(&body))).unwrap();
    let print = &wb.sheet(0).unwrap().print;

    let xml = r#"<sheetData/><pageMargins left="0.5" right="0.5" top="1" bottom="1" header="0.3" footer="0.3"/><pageSetup paperSize="9" scale="75" orientation="landscape"/>"#;
    let (xlsx, _) = read_xlsx(&minimal(xml, &[], "", "")).unwrap();
    let expected = &xlsx.sheet(0).unwrap().print;

    assert_eq!(print.orientation, Orientation::Landscape);
    assert_eq!(print.paper, "A4");
    assert_eq!(print.scale, 75);
    assert_eq!(print.margins, [0.5, 0.5, 1.0, 1.0, 0.3, 0.3]);
    assert_eq!(print.orientation, expected.orientation);
    assert_eq!(print.paper, expected.paper);
    assert_eq!(print.scale, expected.scale);
    assert_eq!(print.margins, expected.margins);
}

#[test]
fn xlsb_fit_to_one_page_wide_needs_the_ws_prop_flag() {
    let body = [ws_prop(0x0100), page_setup(9, 0, 0, (1, 0), 0)].concat();
    let (wb, _) = read_xlsb(&package(&worksheet(&body))).unwrap();
    assert_eq!(wb.sheet(0).unwrap().print.fit_to, Some((1, 0)));
    // A zero scale is unstated, as in the `.xlsx` reader.
    assert_eq!(wb.sheet(0).unwrap().print.scale, 100);

    let without = page_setup(9, 0, 0, (1, 0), 0);
    let (wb, _) = read_xlsb(&package(&worksheet(&without))).unwrap();
    assert_eq!(wb.sheet(0).unwrap().print.fit_to, None);

    // Another BrtWsProp bit (fColSumsRight, bit 7) must not turn fitting on.
    let other_bit = [ws_prop(0x0080), page_setup(9, 0, 0, (1, 0), 0)].concat();
    let (wb, _) = read_xlsb(&package(&worksheet(&other_bit))).unwrap();
    assert_eq!(wb.sheet(0).unwrap().print.fit_to, None);
}

#[test]
fn xlsb_gridlines_headings_and_horizontal_centring_are_printed() {
    // fHCenter, fPrintHeaders and fPrintGrid; vertical centring stays off.
    let body = print_options(0b1101);
    let (wb, _) = read_xlsb(&package(&worksheet(&body))).unwrap();
    let print = &wb.sheet(0).unwrap().print;
    assert!(print.gridlines && print.headings && print.center_h);
    assert!(!print.center_v);

    let (xlsx, _) = read_xlsx(&minimal(r#"<printOptions gridLines="1" headings="1" horizontalCentered="1"/>"#, &[], "", "")).unwrap();
    assert_eq!(print.gridlines, xlsx.sheet(0).unwrap().print.gridlines);
    assert_eq!(print.headings, xlsx.sheet(0).unwrap().print.headings);
    assert_eq!(print.center_h, xlsx.sheet(0).unwrap().print.center_h);
}

#[test]
fn xlsb_header_footer_codes_and_first_page_variants_survive() {
    let header = r#"&L&"Arial,Bold"Left&CPage &P of &N&RRight"#;
    // fHFDiffFirst with scale-with-document and align-with-margins left on.
    let body = header_footer(0b0000_1110, [Some(header), Some("&CPage &P"), None, None, None, Some("&CFirst footer")]);
    let (wb, report) = read_xlsb(&package(&worksheet(&body))).unwrap();
    let print = &wb.sheet(0).unwrap().print;
    assert_eq!(print.header, header);
    assert_eq!(print.footer, "&CPage &P");
    assert!(print.different_first && !print.different_odd_even);
    assert_eq!(print.first_footer, "&CFirst footer");
    assert_eq!(print.first_header, "");
    assert!(print.hf_scale_with_doc && print.hf_align_with_margins);
    assert!(!report.warnings.iter().any(|w| w.contains("HeaderFooter")), "{:?}", report.warnings);
}

#[test]
fn xlsb_manual_row_and_column_breaks_are_imported() {
    let body = [row_breaks(&[10, 20]), col_breaks(&[4])].concat();
    let (wb, _) = read_xlsb(&package(&worksheet(&body))).unwrap();
    let print = &wb.sheet(0).unwrap().print;
    assert_eq!(print.row_breaks, vec![10, 20]);
    assert_eq!(print.col_breaks, vec![4]);

    let (xlsx, _) = read_xlsx(&minimal(
        r#"<rowBreaks count="2" manualBreakCount="2"><brk id="10"/><brk id="20"/></rowBreaks><colBreaks count="1"><brk id="4"/></colBreaks>"#,
        &[],
        "",
        "",
    ))
    .unwrap();
    assert_eq!(print.row_breaks, xlsx.sheet(0).unwrap().print.row_breaks);
    assert_eq!(print.col_breaks, xlsx.sheet(0).unwrap().print.col_breaks);
}

#[test]
fn xlsb_truncated_page_setup_warns_and_keeps_the_other_print_settings() {
    let body = [
        margins(0.5, 0.25, 1.0, 1.0, 0.3, 0.3),
        record(478, &[9, 0, 0, 0, 75, 0, 0]), // A BrtPageSetup cut off inside iScale.
        print_options(0b1001),
    ]
    .concat();
    let (wb, report) = read_xlsb(&package(&worksheet(&body))).unwrap();
    let print = &wb.sheet(0).unwrap().print;
    assert!(report.warnings.iter().any(|w| w.contains("BrtPageSetup")), "{:?}", report.warnings);
    assert_eq!(report.warnings.iter().filter(|w| w.contains("BrtPageSetup")).count(), 1);
    assert_eq!(print.margins, [0.5, 0.25, 1.0, 1.0, 0.3, 0.3]);
    assert!(print.gridlines && print.center_h);
    assert_eq!(print.paper, "Letter");
    assert_eq!(print.scale, 100);
}

#[test]
fn xlsb_out_of_range_and_damaged_breaks_are_skipped_with_one_warning_each() {
    let body = [row_breaks(&[0, 1_048_576, 20]), brk(7), col_breaks(&[16_384, 4])].concat();
    let (wb, report) = read_xlsb(&package(&worksheet(&body))).unwrap();
    let print = &wb.sheet(0).unwrap().print;
    assert_eq!(print.row_breaks, vec![20]);
    assert_eq!(print.col_breaks, vec![4]);
    assert_eq!(report.warnings.iter().filter(|w| w.contains("BrtBrk")).count(), 1);
}
