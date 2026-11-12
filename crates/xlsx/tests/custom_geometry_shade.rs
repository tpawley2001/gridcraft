//! Custom geometry path shading (`a:path/@fill`, ST_PathFillMode, ECMA-376 Part 1 §20.1.10.37):
//! a freeform with one path per mode keeps every mode through save and reopen.

use std::io::Read;
use std::sync::Arc;

use gridcraft_model::{Anchor, AnchorMode, Color, GeometryPath, PathCommand, PathShade, Shape, ShapeGeometry, ShapeKind, Sheet, Workbook};
use gridcraft_xlsx::{read_xlsx, write_xlsx};

/// (fill, shade) per path: `none`, `norm`, then the four shades.
const MODES: [(bool, PathShade); 6] = [
    (false, PathShade::Norm),
    (true, PathShade::Norm),
    (true, PathShade::Lighten),
    (true, PathShade::LightenLess),
    (true, PathShade::Darken),
    (true, PathShade::DarkenLess),
];

fn freeform() -> Shape {
    use PathCommand::*;
    let paths = MODES
        .iter()
        .enumerate()
        .map(|(i, &(fill, shade))| {
            let x = i64::try_from(i).unwrap() * 10;
            GeometryPath { w: 100, h: 50, fill, stroke: true, shade, commands: vec![MoveTo(x, 0), LineTo(x + 10, 0), LineTo(x + 10, 50), Close] }
        })
        .collect();
    Shape {
        hidden: false,
        no_print: false,
        unlocked: false,
        alt_title: String::new(),
        hyperlink: None,
        name: None,
        alt: String::new(),
        id: 1,
        kind: ShapeKind::Freeform,
        geometry: Some(ShapeGeometry { preset: String::new(), adjust: vec![], paths }),
        anchor: Anchor { width: 100.0, height: 50.0, mode: AnchorMode::Absolute, ..Default::default() },
        fill: Color::Rgb(0xC00000),
        line: Color::Rgb(0x000000),
        text: String::new(),
        format: Default::default(),
        math: Default::default(),
    }
}

fn modes(wb: &Workbook) -> Vec<(bool, PathShade)> {
    let shape = wb.sheets[0].shapes.iter().find(|s| s.kind == ShapeKind::Freeform).expect("the freeform is kept");
    shape.geometry.as_ref().expect("custom geometry").paths.iter().map(|p| (p.fill, p.shade)).collect()
}

#[test]
fn every_path_fill_mode_survives_save_and_reopen() {
    let mut sheet = Sheet::new("Shapes");
    sheet.shapes.push(freeform());
    let wb = Workbook { sheets: vec![Arc::new(sheet)], ..Default::default() };

    let bytes = write_xlsx(&wb).unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes.clone())).unwrap();
    let mut drawing = String::new();
    archive.by_name("xl/drawings/drawing1.xml").unwrap().read_to_string(&mut drawing).unwrap();
    for attr in ["fill=\"none\"", "fill=\"lighten\"", "fill=\"lightenLess\"", "fill=\"darken\"", "fill=\"darkenLess\""] {
        assert_eq!(drawing.matches(attr).count(), 1, "{attr} written once: {drawing}");
    }

    let read = read_xlsx(&bytes).unwrap().0;
    assert_eq!(modes(&read), MODES.to_vec());
    let again = read_xlsx(&write_xlsx(&read).unwrap()).unwrap().0;
    assert_eq!(modes(&again), MODES.to_vec());
}
