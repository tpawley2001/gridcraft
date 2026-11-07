//! Named cell styles in synthetic XLSB styles parts: BrtStyle (48) in BrtBeginStyles (619)..BrtEndStyles
//! (620), pointing into the cell style XFs (BrtXF 47 in 626..627), from the public [MS-XLSB] layout.
use gridcraft_core::CellRef;
use gridcraft_model::{Color, Fill, Style, Workbook};

use super::make_zip;
use super::xlsb_tests::{RELS, ROOT, TYPES, book, font, record, row, sst, wide, worksheet};
use crate::{ReadReport, read_xlsb, read_xlsx, write_xlsx};

pub(super) const PLAIN: [u8; 8] = [3 << 1, 1, 0, 0, 0, 0, 0, 0];
pub(super) fn rgb(rgb: u32) -> [u8; 8] {
    let [_, r, g, b] = rgb.to_be_bytes();
    [1 | 2 << 1, 0, 0, 0, r, g, b, 255]
}
/// A BrtXF with the given number format, font and fill; bottom-aligned and locked as Excel writes it.
pub(super) fn xf(fmt: u16, font: u16, fill: u16) -> Vec<u8> {
    let mut d = 0u16.to_le_bytes().to_vec();
    for v in [fmt, font, fill, 0] {
        d.extend(v.to_le_bytes());
    }
    d.extend([0, 0]);
    d.extend((2u16 << 3 | 1 << 12).to_le_bytes());
    d.extend(0u16.to_le_bytes());
    record(47, &d)
}
fn fill(fls: u32, fore: [u8; 8]) -> Vec<u8> {
    let mut d = fls.to_le_bytes().to_vec();
    d.extend(fore);
    d.extend(rgb(0xFFFFFF));
    d.extend([0; 4 + 5 * 8 + 4]);
    record(45, &d)
}
/// BrtStyle: ixf, grbitObj1 (bit 0 fBuiltIn, bit 2 fCustom), iStyBuiltIn, iLevel, stName.
fn brt_style(ixf: u32, builtin: Option<u8>, name: &str) -> Vec<u8> {
    let mut d = ixf.to_le_bytes().to_vec();
    d.extend(u16::from(builtin.is_some()).to_le_bytes());
    d.extend([builtin.unwrap_or(0), 255]);
    d.extend(wide(name));
    record(48, &d)
}
/// Formats (164 `#,##0.00`), fonts (0 plain, 1 bold, 2 green), fills (0 none, 1 gray125, 2 solid
/// green), cell style XFs `style_xfs`, cell XFs [0 default, 1 bold] and the BrtStyle records.
fn styles_part(style_xfs: &[Vec<u8>], styles: &[Vec<u8>]) -> Vec<u8> {
    let mut out = record(278, &[]);
    out.extend(record(615, &1u32.to_le_bytes()));
    let mut fmt = 164u16.to_le_bytes().to_vec();
    fmt.extend(wide("#,##0.00"));
    out.extend(record(44, &fmt));
    out.extend(record(616, &[]));
    out.extend(record(611, &3u32.to_le_bytes()));
    out.extend(font(220, 0, 400, 0, 0, 2, 0, PLAIN, 2, "Calibri"));
    out.extend(font(220, 0, 700, 0, 0, 2, 0, PLAIN, 2, "Calibri"));
    out.extend(font(220, 0, 400, 0, 0, 2, 0, rgb(0x006100), 2, "Calibri"));
    out.extend(record(612, &[]));
    out.extend(record(603, &3u32.to_le_bytes()));
    out.extend(fill(0, PLAIN));
    out.extend(fill(17, PLAIN));
    out.extend(fill(1, rgb(0xC6EFCE)));
    out.extend(record(604, &[]));
    if !style_xfs.is_empty() {
        out.extend(record(626, &(style_xfs.len() as u32).to_le_bytes()));
        style_xfs.iter().for_each(|x| out.extend(x));
        out.extend(record(627, &[]));
    }
    out.extend(record(617, &2u32.to_le_bytes()));
    out.extend(xf(0, 0, 0));
    out.extend(xf(0, 1, 0));
    out.extend(record(618, &[]));
    if !styles.is_empty() {
        out.extend(record(619, &(styles.len() as u32).to_le_bytes()));
        styles.iter().for_each(|s| out.extend(s));
        out.extend(record(620, &[]));
    }
    out.extend(record(279, &[]));
    out
}
/// A1 uses cell XF 1.
pub(super) fn package(styles: &[u8]) -> Vec<u8> {
    let rels = RELS.replace(
        "</Relationships>",
        r#"<Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="../text/styles.bin"/></Relationships>"#,
    );
    let types = TYPES.replace("</Types>", r#"<Override PartName="/text/styles.bin" ContentType="application/vnd.ms-excel.styles"/></Types>"#);
    let mut data = row(0);
    let mut cell = 0u32.to_le_bytes().to_vec();
    cell.extend(1u32.to_le_bytes());
    cell.extend(5.0f64.to_le_bytes());
    data.extend(record(5, &cell));
    let (book, sheet, strings, two) = (book(false), worksheet(&data, &[]), sst(), worksheet(&[], &[]));
    make_zip(&[
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", ROOT.as_bytes()),
        ("records/book.bin", &book),
        ("records/_rels/book.bin.rels", rels.as_bytes()),
        ("data/one.bin", &sheet),
        ("data/two.bin", &two),
        ("text/strings.bin", &strings),
        ("text/styles.bin", styles),
    ])
}
fn read(styles: &[u8]) -> (Workbook, ReadReport) {
    read_xlsb(&package(styles)).unwrap()
}
fn money() -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    (vec![xf(0, 0, 0), xf(164, 1, 0)], vec![brt_style(0, Some(0), "Normal"), brt_style(1, None, "Money")])
}
fn cell_style_warnings(report: &ReadReport) -> usize {
    report.warnings.iter().filter(|w| w.contains("cell styles")).count()
}

#[test]
fn xlsb_user_cell_style_is_imported_and_normal_skipped() {
    let (xfs, styles) = money();
    let (wb, report) = read(&styles_part(&xfs, &styles));
    assert_eq!(cell_style_warnings(&report), 0, "{:?}", report.warnings);
    assert_eq!(wb.cell_styles.len(), 1);
    let (name, style) = &wb.cell_styles[0];
    assert_eq!(name, "Money");
    assert!(style.font.bold);
    assert_eq!(style.num_fmt.as_str(), "#,##0.00");
    assert_eq!(style.fill, Fill::default());
}

#[test]
fn xlsb_builtin_cell_style_keeps_font_colour_and_fill() {
    let xfs = vec![xf(0, 0, 0), xf(0, 2, 2)];
    let (wb, _) = read(&styles_part(&xfs, &[brt_style(0, Some(0), "Normal"), brt_style(1, Some(26), "Good")]));
    assert_eq!(wb.cell_styles.len(), 1);
    let (name, style) = &wb.cell_styles[0];
    assert_eq!(name, "Good");
    assert_eq!(style.font.color, Color::Rgb(0x006100));
    assert_eq!(style.fill, Fill::solid(Color::Rgb(0xC6EFCE)));
}

#[test]
fn xlsb_cell_style_past_the_style_xfs_is_skipped_with_one_warning() {
    let (xfs, mut styles) = money();
    styles.insert(1, brt_style(99, None, "Broken"));
    styles.push(brt_style(u32::MAX, None, "Also broken"));
    let (wb, report) = read(&styles_part(&xfs, &styles));
    let names: Vec<_> = wb.cell_styles.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["Money"]);
    assert_eq!(cell_style_warnings(&report), 1, "{:?}", report.warnings);
}

#[test]
fn xlsb_without_style_records_has_no_cell_styles() {
    let (wb, report) = read(&styles_part(&[xf(0, 0, 0)], &[]));
    assert!(wb.cell_styles.is_empty());
    assert_eq!(cell_style_warnings(&report), 0, "{:?}", report.warnings);
    assert!(!report.warnings.iter().any(|w| w.to_lowercase().contains("styles")), "{:?}", report.warnings);
}

#[test]
fn xlsb_cell_style_xfs_do_not_shift_cell_xfs() {
    let a1 = |wb: &Workbook| -> Style { wb.styles.get(wb.sheets[0].style_id(CellRef::new(0, 0))).clone() };
    let (xfs, styles) = money();
    let (with, _) = read(&styles_part(&xfs, &styles));
    let (without, _) = read(&styles_part(&[], &[]));
    assert!(a1(&with).font.bold);
    assert_eq!(a1(&with).num_fmt.as_str(), "General");
    assert_eq!(a1(&with), a1(&without));
}

#[test]
fn xlsb_cell_styles_survive_saving_as_xlsx() {
    let (xfs, styles) = money();
    let (wb, _) = read(&styles_part(&xfs, &styles));
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(back.cell_styles, wb.cell_styles);
}
