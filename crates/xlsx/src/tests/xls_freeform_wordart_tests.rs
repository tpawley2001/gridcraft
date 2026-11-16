//! `.xls` freeform shapes, BIFF8 polygons and WordArt ([MS-XLS] 2.4.181 Obj, [MS-ODRAW] 2.2.9
//! OfficeArtFOPT, 2.3.6 geometry properties, 2.3.22 geometry text): a shape of type 0 with its own
//! vertices and segments opens as a freeform, a plain-text WordArt (type 136) with its text, size
//! and font; both survive Save As .xlsx.

use gridcraft_model::{PathCommand, Shape, ShapeKind, Workbook};

use crate::{read_xls, read_xlsx, write_xlsx};

use super::xls_chart_tests::{cat, obj, rec, u16s};
use super::xls_shapes_tests::{txo, xls};

const OT_POLYGON: u16 = 0x0009;
const OT_OFFICE_ART: u16 = 0x001E;
const SPT_FREEFORM: u16 = 0;
const SPT_TEXT_PLAIN_TEXT: u16 = 136;

const GEO_LEFT: u16 = 0x0140;
const GEO_TOP: u16 = 0x0141;
const GEO_RIGHT: u16 = 0x0142;
const GEO_BOTTOM: u16 = 0x0143;
const P_VERTICES: u16 = 0x0145;
const P_SEGMENT_INFO: u16 = 0x0146;
const GTEXT_UNICODE: u16 = 0x00C0;
const GTEXT_SIZE: u16 = 0x00C3;
const GTEXT_FONT: u16 = 0x00C5;
const COMPLEX: u16 = 0x8000;

const SHAPES_WARNING: &str = "Shapes in .xls files are not imported yet";

// MSOPATHINFO segments.
const MOVE_TO: u16 = 0x4000;
const END: u16 = 0x8000;
const CLOSE_1: u16 = 0x6001;
const AUTO_LINE: u16 = 0xAC00; // escape 0x0C (an editing hint), no vertices
fn line_to(n: u16) -> u16 {
    n
}
fn curve_to(n: u16) -> u16 {
    0x2000 | n
}

fn art(ver: u16, inst: u16, ty: u16, b: &[u8]) -> Vec<u8> {
    cat(&[&u16s(&[ver | (inst << 4), ty]), &(b.len() as u32).to_le_bytes(), b])
}

/// A FOPT property: simple `(id, value)` or complex `(id, data)`.
enum Prop {
    Simple(u16, u32),
    Complex(u16, Vec<u8>, u32),
}

/// An IMsoArray of (x, y) points with 4-byte elements (cbElem 0xFFF0). `op_counts_header` picks
/// how its `op` is written: real Excel files leave the 6-byte header out of `op`.
fn vertices(points: &[(i16, i16)], op_counts_header: bool) -> Prop {
    let n = points.len() as u16;
    let mut d = u16s(&[n, n, 0xFFF0]);
    for (x, y) in points {
        d.extend(x.to_le_bytes());
        d.extend(y.to_le_bytes());
    }
    let op = if op_counts_header { d.len() } else { d.len() - 6 } as u32;
    Prop::Complex(P_VERTICES, d, op)
}

fn segments(segs: &[u16], op_counts_header: bool) -> Prop {
    let n = segs.len() as u16;
    let d = cat(&[&u16s(&[n, n, 2]), &u16s(segs)]);
    let op = if op_counts_header { d.len() } else { d.len() - 6 } as u32;
    Prop::Complex(P_SEGMENT_INFO, d, op)
}

fn utf16z(s: &str) -> Vec<u8> {
    s.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect()
}

fn complex_text(id: u16, s: &str) -> Prop {
    let d = utf16z(s);
    let op = d.len() as u32;
    Prop::Complex(id, d, op)
}

/// One MsoDrawing shape (type `spt`, cells B2:E6) with these FOPT properties, then its Obj.
fn shape_with(spt: u16, props: &[Prop], ot: u16) -> Vec<u8> {
    let sp = art(2, spt, 0xF00A, &cat(&[&1025u32.to_le_bytes(), &0x0A00u32.to_le_bytes()]));
    let (mut head, mut tail) = (vec![], vec![]);
    for p in props {
        match p {
            Prop::Simple(id, v) => {
                head.extend(u16s(&[*id]));
                head.extend(v.to_le_bytes());
            }
            Prop::Complex(id, d, op) => {
                head.extend(u16s(&[*id | COMPLEX]));
                head.extend(op.to_le_bytes());
                tail.extend(d);
            }
        }
    }
    let fopt = art(3, props.len() as u16, 0xF00B, &cat(&[&head, &tail]));
    let anchor = art(0, 0, 0xF010, &u16s(&[0, 1, 0, 1, 0, 4, 0, 5, 0]));
    let data = art(0, 0, 0xF011, &[]);
    cat(&[&rec(0x00EC, &art(15, 0, 0xF004, &cat(&[&sp, &fopt, &anchor, &data]))), &obj(ot)])
}

/// The zigzag measured on a real file: nine points in a 672 x 96 box, drawn as a moveTo and
/// eight lineTo segments each preceded by an AutoLine editing hint, then end.
const ZIGZAG: [(i16, i16); 9] = [(0, 48), (48, 0), (144, 96), (240, 0), (336, 96), (432, 0), (528, 96), (624, 0), (672, 48)];

fn zigzag_segments() -> Vec<u16> {
    let mut s = vec![MOVE_TO];
    for _ in 0..8 {
        s.extend([AUTO_LINE, line_to(1)]);
    }
    s.push(END);
    s
}

fn zigzag(ot: u16, op_counts_header: bool) -> Vec<u8> {
    shape_with(
        SPT_FREEFORM,
        &[
            Prop::Simple(GEO_RIGHT, 672),
            Prop::Simple(GEO_BOTTOM, 96),
            vertices(&ZIGZAG, op_counts_header),
            segments(&zigzag_segments(), op_counts_header),
        ],
        ot,
    )
}

fn open(records: &[u8]) -> (Workbook, Vec<String>) {
    let (wb, report) = read_xls(&xls(records)).unwrap();
    (wb, report.warnings.clone())
}

fn only_shape(wb: &Workbook) -> &Shape {
    let shapes = &wb.sheet(0).unwrap().shapes;
    assert_eq!(shapes.len(), 1, "{shapes:?}");
    &shapes[0]
}

fn zigzag_commands() -> Vec<PathCommand> {
    ZIGZAG
        .iter()
        .enumerate()
        .map(|(i, &(x, y))| {
            let (x, y) = (i64::from(x), i64::from(y));
            if i == 0 { PathCommand::MoveTo(x, y) } else { PathCommand::LineTo(x, y) }
        })
        .collect()
}

fn assert_zigzag(shape: &Shape) {
    assert_eq!(shape.kind, ShapeKind::Freeform);
    let g = shape.geometry.as_ref().expect("a freeform has its geometry");
    assert_eq!(g.paths.len(), 1);
    assert_eq!((g.paths[0].w, g.paths[0].h), (672, 96));
    assert_eq!(g.paths[0].commands, zigzag_commands());
}

fn no_shapes_warning(warnings: &[String]) {
    assert!(!warnings.iter().any(|w| w.contains(SHAPES_WARNING)), "{warnings:?}");
}

#[test]
fn an_office_art_freeform_opens_with_its_path() {
    let (wb, warnings) = open(&zigzag(OT_OFFICE_ART, false));
    assert_zigzag(only_shape(&wb));
    no_shapes_warning(&warnings);
}

#[test]
fn an_array_whose_op_counts_its_header_reads_the_same() {
    let (wb, warnings) = open(&zigzag(OT_OFFICE_ART, true));
    assert_zigzag(only_shape(&wb));
    no_shapes_warning(&warnings);
}

#[test]
fn a_biff8_polygon_obj_opens_as_a_freeform() {
    let (wb, warnings) = open(&zigzag(OT_POLYGON, false));
    assert_zigzag(only_shape(&wb));
    no_shapes_warning(&warnings);
}

#[test]
fn a_closed_path_with_curves_keeps_its_close_and_beziers() {
    // moveTo (0,0), one curve (3 points), lineTo 1, close, end; box offset by geoLeft/geoTop.
    let pts = [(100, 200), (110, 200), (120, 210), (130, 230), (100, 230)];
    let segs = [MOVE_TO, curve_to(1), line_to(1), CLOSE_1, END];
    let records = shape_with(
        SPT_FREEFORM,
        &[
            Prop::Simple(GEO_LEFT, 100),
            Prop::Simple(GEO_TOP, 200),
            Prop::Simple(GEO_RIGHT, 140),
            Prop::Simple(GEO_BOTTOM, 240),
            vertices(&pts, false),
            segments(&segs, false),
        ],
        OT_OFFICE_ART,
    );
    let (wb, warnings) = open(&records);
    let s = only_shape(&wb);
    assert_eq!(s.kind, ShapeKind::Freeform);
    let g = s.geometry.as_ref().unwrap();
    assert_eq!((g.paths[0].w, g.paths[0].h), (40, 40));
    assert_eq!(
        g.paths[0].commands,
        [PathCommand::MoveTo(0, 0), PathCommand::CubicBezTo([(10, 0), (20, 10), (30, 30)]), PathCommand::LineTo(0, 30), PathCommand::Close,]
    );
    no_shapes_warning(&warnings);
}

#[test]
fn without_segment_info_the_points_are_one_open_polyline() {
    let records = shape_with(SPT_FREEFORM, &[Prop::Simple(GEO_RIGHT, 672), Prop::Simple(GEO_BOTTOM, 96), vertices(&ZIGZAG, false)], OT_OFFICE_ART);
    let (wb, _) = open(&records);
    assert_zigzag(only_shape(&wb));
}

#[test]
fn a_freeform_with_a_truncated_vertex_array_is_skipped_and_reported() {
    // nElems says 9 but the data holds 2 points: no panic, nothing half-read.
    let mut d = u16s(&[9, 9, 0xFFF0]);
    d.extend(cat(&[&u16s(&[0, 48]), &u16s(&[48, 0])]));
    let records = shape_with(SPT_FREEFORM, &[Prop::Simple(GEO_RIGHT, 672), Prop::Complex(P_VERTICES, d, 36)], OT_OFFICE_ART);
    let (wb, warnings) = open(&records);
    assert!(wb.sheet(0).unwrap().shapes.is_empty());
    assert!(warnings.iter().any(|w| w.contains(SHAPES_WARNING)), "{warnings:?}");
}

#[test]
fn a_freeform_keeps_the_text_of_its_txo() {
    let (wb, _) = open(&cat(&[&zigzag(OT_OFFICE_ART, false), &txo("Flow")]));
    assert_eq!(only_shape(&wb).text, "Flow");
}

fn word_art() -> Vec<u8> {
    shape_with(
        SPT_TEXT_PLAIN_TEXT,
        &[complex_text(GTEXT_UNICODE, "Reservoir"), Prop::Simple(GTEXT_SIZE, 28 << 16), complex_text(GTEXT_FONT, "Arial Black")],
        OT_OFFICE_ART,
    )
}

#[test]
fn plain_text_word_art_opens_with_its_text_size_and_font() {
    let (wb, warnings) = open(&word_art());
    let s = only_shape(&wb);
    assert_eq!(s.kind, ShapeKind::WordArt);
    assert_eq!(s.text, "Reservoir");
    assert_eq!(s.format.font_size, Some(28.0));
    assert_eq!(s.format.font.as_deref(), Some("Arial Black"));
    no_shapes_warning(&warnings);
}

#[test]
fn freeform_and_word_art_survive_save_as_xlsx() {
    let (wb, _) = open(&cat(&[&zigzag(OT_OFFICE_ART, false), &word_art()]));
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    let shapes = &back.sheet(0).unwrap().shapes;
    assert_eq!(shapes.len(), 2, "{shapes:?}");
    let ff = shapes.iter().find(|s| s.kind == ShapeKind::Freeform).expect("the freeform is saved");
    assert_eq!(ff.geometry.as_ref().unwrap().paths[0].commands, zigzag_commands());
    let wa = shapes.iter().find(|s| s.kind == ShapeKind::WordArt).expect("the WordArt is saved");
    assert_eq!(wa.text, "Reservoir");
    assert_eq!(wa.format.font_size, Some(28.0));
}
