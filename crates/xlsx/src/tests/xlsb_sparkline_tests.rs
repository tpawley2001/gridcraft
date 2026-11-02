//! XLSB sparklines: the Excel 2010 sparkline group records of a worksheet's future-record block,
//! assembled in Rust from the public [MS-XLSB] layout and compared with the `.xlsx` reader.
use gridcraft_core::{CellRef, Value};
use gridcraft_model::{Color, Sparkline, SparklineKind};

use super::xlsb_tests::{cell, record, row, sst, wide, worksheet};
use super::{make_zip, minimal};
use crate::{read_xlsb, read_xlsx, write_xlsx};

const ROOT: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="book" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="records/book.bin"/></Relationships>"#;
const RELS: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="one" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="../data/one.bin"/><Relationship Id="two" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="../data/two.bin"/><Relationship Id="strings" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="../text/strings.bin"/></Relationships>"#;
const TYPES: &str = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/records/book.bin" ContentType="application/vnd.ms-excel.sheet.binary.macroEnabled.main"/><Override PartName="/data/one.bin" ContentType="application/vnd.ms-excel.worksheet"/><Override PartName="/data/two.bin" ContentType="application/vnd.ms-excel.worksheet"/><Override PartName="/text/strings.bin" ContentType="application/vnd.ms-excel.sharedStrings"/></Types>"#;

fn at(a: &str) -> CellRef {
    CellRef::parse(a).unwrap()
}

/// Sheets "Sheet1" and "Other", with this workbook as supporting link 0 and XTIs 0 and 1 for
/// each sheet, so 3-D references can name them.
fn book() -> Vec<u8> {
    let mut out = record(131, &[]);
    let mut props = 0u32.to_le_bytes().to_vec();
    props.extend(0u32.to_le_bytes());
    props.extend(wide(""));
    out.extend(record(153, &props));
    out.extend(record(143, &[]));
    for (id, rel, name) in [(1u32, "one", "Sheet1"), (2, "two", "Other")] {
        let mut data = 0u32.to_le_bytes().to_vec();
        data.extend(id.to_le_bytes());
        data.extend(wide(rel));
        data.extend(wide(name));
        out.extend(record(156, &data));
    }
    out.extend(record(144, &[]));
    out.extend(record(358, &[0, 0])); // BrtSupSelf
    let mut xti = 2u32.to_le_bytes().to_vec();
    for sheet in [0u32, 1] {
        xti.extend(0u32.to_le_bytes());
        xti.extend(sheet.to_le_bytes());
        xti.extend(sheet.to_le_bytes());
    }
    out.extend(record(362, &xti));
    out.extend(record(132, &[]));
    out
}

fn file(sheet: &[u8]) -> Vec<u8> {
    let (book, strings, two) = (book(), sst(), worksheet(&[], &[]));
    make_zip(&[
        ("[Content_Types].xml", TYPES.as_bytes()),
        ("_rels/.rels", ROOT.as_bytes()),
        ("records/book.bin", &book),
        ("records/_rels/book.bin.rels", RELS.as_bytes()),
        ("data/one.bin", sheet),
        ("data/two.bin", &two),
        ("text/strings.bin", &strings),
    ])
}

/// A BrtColor: RGB, or a theme colour index.
fn rgb(v: u32) -> [u8; 8] {
    let [_, r, g, b] = v.to_be_bytes();
    [2 << 1, 0, 0, 0, r, g, b, 0xff]
}
fn theme(index: u8) -> [u8; 8] {
    [3 << 1, index, 0, 0, 0, 0, 0, 0]
}

/// BrtBeginSparklineGroup with an empty FRTHeader.
fn group(kind: u32, series: [u8; 8], markers: bool) -> Vec<u8> {
    let mut out = 0u32.to_le_bytes().to_vec();
    out.extend(if markers { 0x000Bu16 } else { 0x0002 }.to_le_bytes());
    out.extend(series);
    for _ in 0..7 {
        out.extend(rgb(0xD00000));
    }
    out.extend(0f64.to_le_bytes());
    out.extend(0f64.to_le_bytes());
    out.extend(0.75f64.to_le_bytes());
    out.extend(kind.to_le_bytes());
    record(1041, &out)
}

/// BrtSparkline at `cell`, plotting row `r` columns A:D of the sheet behind XTI `ixti`.
fn sparkline(cell: &str, ixti: u16, r: u32) -> Vec<u8> {
    let c = at(cell);
    let mut out = 0b110u32.to_le_bytes().to_vec();
    out.extend(1u32.to_le_bytes()); // csqref
    out.extend(0b10u32.to_le_bytes()); // fDoAdjust
    out.extend(1u32.to_le_bytes()); // crfx
    for n in [c.row, c.row, c.col, c.col] {
        out.extend(n.to_le_bytes());
    }
    // PtgArea3d with relative rows and columns.
    let mut rgce = vec![0x3B];
    rgce.extend(ixti.to_le_bytes());
    rgce.extend(r.to_le_bytes());
    rgce.extend(r.to_le_bytes());
    rgce.extend(0xC000u16.to_le_bytes());
    rgce.extend(0xC003u16.to_le_bytes());
    out.extend(1u32.to_le_bytes()); // cformula
    out.extend(0b10u32.to_le_bytes());
    out.extend((rgce.len() as u32).to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend(rgce);
    record(1043, &out)
}

/// One group record and its sparklines, wrapped in BrtBeginSparklineGroup .. BrtEndSparklineGroup.
fn wrapped(group: Vec<u8>, sparklines: &[Vec<u8>]) -> Vec<u8> {
    let mut out = group;
    out.extend(record(1056, &[]));
    for s in sparklines {
        out.extend(s);
    }
    out.extend(record(1057, &[]));
    out.extend(record(1042, &[]));
    out
}

/// The SPARKLINEGROUPS future-record block.
fn groups(groups: &[Vec<u8>]) -> Vec<u8> {
    let mut out = record(35, &[0x0e, 0, 0, 0]);
    out.extend(record(1058, &[]));
    for g in groups {
        out.extend(g);
    }
    out.extend(record(1059, &[]));
    out.extend(record(36, &[]));
    out
}

fn sheet(tail: &[u8]) -> Vec<u8> {
    let mut data = row(1);
    data.extend(cell(5, 0, &7.0f64.to_le_bytes()));
    worksheet(&data, tail)
}

fn line_group() -> Vec<u8> {
    wrapped(group(0, rgb(0x376092), true), &[sparkline("E2", 0, 1), sparkline("E3", 0, 2), sparkline("E4", 0, 3)])
}

#[test]
fn line_group_becomes_three_sparklines() {
    let (wb, _) = read_xlsb(&file(&sheet(&groups(&[line_group()])))).unwrap();
    let got = &wb.sheets[0].sparklines;
    let want: Vec<_> = (2..=4)
        .map(|r| Sparkline {
            cell: at(&format!("E{r}")),
            source: format!("Sheet1!A{r}:D{r}"),
            kind: SparklineKind::Line,
            color: Color::Rgb(0x376092),
            markers: true,
        })
        .collect();
    assert_eq!(got, &want);
}

#[test]
fn column_and_win_loss_groups_and_theme_colours() {
    let tail =
        groups(&[wrapped(group(1, theme(4), false), &[sparkline("F2", 1, 0)]), wrapped(group(2, rgb(0x00B050), true), &[sparkline("G2", 0, 1)])]);
    let (wb, _) = read_xlsb(&file(&sheet(&tail))).unwrap();
    let s = &wb.sheets[0].sparklines;
    assert_eq!(s.len(), 2);
    assert_eq!((s[0].kind, s[0].markers, s[0].source.as_str()), (SparklineKind::Column, false, "Other!A1:D1"));
    assert_eq!((s[1].kind, s[1].markers, s[1].color), (SparklineKind::WinLoss, true, Color::Rgb(0x00B050)));
    // The same group in `.xlsx` resolves its theme colour identically.
    let xml = r#"<sheetData/><extLst><ext uri="{05C60535-1F16-4fd2-B633-F4F36F0B64E0}"><x14:sparklineGroups xmlns:x14="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main" xmlns:xm="http://schemas.microsoft.com/office/excel/2006/main"><x14:sparklineGroup type="column"><x14:colorSeries theme="4"/><x14:sparklines><x14:sparkline><xm:f>Other!A1:D1</xm:f><xm:sqref>F2</xm:sqref></x14:sparkline></x14:sparklines></x14:sparklineGroup></x14:sparklineGroups></ext></extLst>"#;
    let (x, _) = read_xlsx(&minimal(xml, &[], "", "")).unwrap();
    assert_eq!(x.sheets[0].sparklines.first(), s.first());
}

#[test]
fn truncated_group_is_skipped_without_losing_the_sheet() {
    let short = record(1041, &[0, 0, 0, 0, 0x08, 0, 4, 0]);
    let bad = wrapped(short, &[sparkline("E2", 0, 1)]);
    let mut lone = record(1043, &[0b10, 0, 0, 0, 1]);
    lone.extend(record(1043, &[]));
    let (wb, report) = read_xlsb(&file(&sheet(&groups(&[bad, lone, line_group()])))).unwrap();
    let sh = &wb.sheets[0];
    assert_eq!(sh.cells.get(at("A2")).map(|c| c.value.clone()), Some(Value::Number(7.0)));
    // The damaged group's sparkline is dropped; the intact group still loads.
    assert_eq!(sh.sparklines.iter().map(|s| s.cell).collect::<Vec<_>>(), [at("E2"), at("E3"), at("E4")]);
    assert_eq!(report.warnings.iter().filter(|w| w.contains("sparklines")).count(), 1);

    // A damaged sparkline inside a good group, and garbage of every length, never panic.
    let mut broken = line_group();
    for n in 0..broken.len() {
        broken.truncate(n);
        let _ = read_xlsb(&file(&sheet(&groups(&[broken.clone()]))));
        broken = line_group();
    }
}

#[test]
fn sparklines_survive_saving_as_xlsx() {
    let (wb, _) = read_xlsb(&file(&sheet(&groups(&[line_group()])))).unwrap();
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(back.sheets[0].sparklines, wb.sheets[0].sparklines);
    assert_eq!(back.sheets[0].sparklines.len(), 3);
}
