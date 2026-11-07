//! Gradient fills in synthetic XLSB styles parts: BrtFill (45, [MS-XLSB] 2.4.681) with fls 0x28,
//! iGradientType, five doubles, cNumStop and the gradient stops (BrtColor + xnumPosition). They open
//! as a solid fill in the colour of the lowest-positioned stop, as the XLSX reader approximates
//! `gradientFill`.
use gridcraft_core::CellRef;
use gridcraft_model::{Color, Fill, Style, Workbook};

use super::xlsb_cell_style_tests::{PLAIN, package, rgb, xf};
use super::xlsb_tests::{font, record};
use crate::{read_xlsb, read_xlsx, write_xlsx};

const THEME4: [u8; 8] = [3 << 1, 4, 0, 0, 0, 0, 0, 0];
const GRADIENT: u32 = 0x28;

fn pattern_fill(fls: u32, fore: [u8; 8]) -> Vec<u8> {
    let mut d = fls.to_le_bytes().to_vec();
    d.extend(fore);
    d.extend(rgb(0xFFFFFF));
    d.extend([0; 4 + 5 * 8 + 4]);
    record(45, &d)
}
/// A gradient BrtFill declaring `count` stops and carrying the bytes of `stops` (colour, position).
fn gradient(kind: u32, degree: f64, count: u32, stops: &[([u8; 8], f64)]) -> Vec<u8> {
    let mut d = GRADIENT.to_le_bytes().to_vec();
    d.extend(PLAIN);
    d.extend(PLAIN);
    d.extend(kind.to_le_bytes());
    d.extend(degree.to_le_bytes());
    for v in [0.0f64, 0.0, 0.0, 0.0] {
        d.extend(v.to_le_bytes());
    }
    d.extend(count.to_le_bytes());
    for (color, pos) in stops {
        d.extend(color);
        d.extend(pos.to_le_bytes());
    }
    record(45, &d)
}
/// Fills [none, gray125, `a1_fill`, solid green]; cell XFs [0 default, 1 = fill 2 (A1), 2 = fill 3].
fn styles_part(a1_fill: Vec<u8>) -> Vec<u8> {
    let mut out = record(278, &[]);
    out.extend(record(611, &1u32.to_le_bytes()));
    out.extend(font(220, 0, 400, 0, 0, 2, 0, PLAIN, 2, "Calibri"));
    out.extend(record(612, &[]));
    out.extend(record(603, &4u32.to_le_bytes()));
    out.extend(pattern_fill(0, PLAIN));
    out.extend(pattern_fill(17, PLAIN));
    out.extend(a1_fill);
    out.extend(pattern_fill(1, rgb(0xC6EFCE)));
    out.extend(record(604, &[]));
    out.extend(record(617, &3u32.to_le_bytes()));
    out.extend(xf(0, 0, 0));
    out.extend(xf(0, 0, 2));
    out.extend(xf(0, 0, 3));
    out.extend(record(618, &[]));
    out.extend(record(279, &[]));
    out
}
fn a1(wb: &Workbook) -> Style {
    wb.styles.get(wb.sheets[0].style_id(CellRef::new(0, 0))).clone()
}
fn a1_fill(fill: Vec<u8>) -> Fill {
    let (wb, _) = read_xlsb(&package(&styles_part(fill))).unwrap();
    a1(&wb).fill
}

#[test]
fn xlsb_linear_gradient_opens_as_first_stop_colour() {
    let fill = gradient(0, 90.0, 2, &[(rgb(0x4472C4), 0.0), (rgb(0xFFFFFF), 1.0)]);
    assert_eq!(a1_fill(fill), Fill::solid(Color::Rgb(0x4472C4)));
}

#[test]
fn xlsb_path_gradient_with_theme_stop_opens_as_theme_colour() {
    let fill = gradient(1, 0.0, 2, &[(THEME4, 0.0), (rgb(0xFF0000), 1.0)]);
    assert_eq!(a1_fill(fill), Fill::solid(Color::Theme(4, 0)));
}

#[test]
fn xlsb_gradient_stops_out_of_order_use_the_lowest_position() {
    let fill = gradient(0, 0.0, 2, &[(rgb(0x00FF00), 1.0), (rgb(0x0000FF), 0.0)]);
    assert_eq!(a1_fill(fill), Fill::solid(Color::Rgb(0x0000FF)));
}

#[test]
fn xlsb_gradient_without_stops_is_no_fill() {
    assert_eq!(a1_fill(gradient(0, 0.0, 0, &[])), Fill::default());
}

#[test]
fn xlsb_gradient_claiming_more_stops_than_bytes_is_no_fill_and_opens() {
    let fill = gradient(0, 0.0, 1000, &[(rgb(0x4472C4), 0.0)]);
    assert_eq!(a1_fill(fill), Fill::default());
}

#[test]
fn xlsb_fill_after_a_gradient_keeps_its_own_colour() {
    let fill = gradient(0, 45.0, 3, &[(rgb(0x4472C4), 0.0), (rgb(0x123456), 0.5), (rgb(0xFFFFFF), 1.0)]);
    let (wb, _) = read_xlsb(&package(&styles_part(fill))).unwrap();
    let green = wb.styles.iter().any(|(_, s)| s.fill == Fill::solid(Color::Rgb(0xC6EFCE)));
    assert!(green, "the solid fill after the gradient was not read intact");
}

#[test]
fn xlsb_gradient_survives_save_as_xlsx() {
    let fill = gradient(0, 90.0, 2, &[(rgb(0x4472C4), 0.0), (rgb(0xFFFFFF), 1.0)]);
    let (wb, _) = read_xlsb(&package(&styles_part(fill))).unwrap();
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(a1(&back).fill, Fill::solid(Color::Rgb(0x4472C4)));
}
