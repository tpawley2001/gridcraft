//! Insert › Shapes: `insert.shape {preset}` inserts any DrawingML preset (`a:prstGeom`, ECMA-376 Part 1
//! §20.1.9.18) the renderer draws, and `insert.shapeGallery` lists them by Excel's gallery categories.

use std::collections::HashSet;
use std::io::Read;

use gridcraft_engine::Session;
use gridcraft_model::{ShapeGeometry, ShapeKind, preset_paths};
use serde_json::{Value as Json, json};

fn session() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s
}

fn drawn(preset: &str) -> bool {
    preset_paths(&ShapeGeometry { preset: preset.into(), adjust: vec![], paths: vec![] }, 1000, 1000).is_some()
}

fn shape_count(s: &Session) -> usize {
    s.doc().unwrap().wb.active().unwrap().shapes.len()
}

fn gallery(s: &mut Session) -> Vec<Json> {
    let g = s.execute("insert.shapeGallery", json!({})).unwrap();
    g["categories"].as_array().expect("categories array").clone()
}

#[test]
fn preset_inserts_with_its_geometry() {
    let mut s = session();
    let r = s.execute("insert.shape", json!({"preset": "star5", "at": "C3"})).unwrap();
    assert!(r["shape"].is_u64(), "returns the new shape id: {r}");
    let sh = &s.doc().unwrap().wb.active().unwrap().shapes[0];
    assert_eq!(sh.geometry.as_ref().map(|g| g.preset.as_str()), Some("star5"));
    assert_eq!(sh.kind, ShapeKind::Rectangle);
    assert_eq!((sh.anchor.width, sh.anchor.height), (144.0, 96.0));
    s.execute("insert.shape", json!({"preset": "triangle"})).unwrap();
    assert_eq!(s.doc().unwrap().wb.active().unwrap().shapes[1].kind, ShapeKind::Triangle);
}

#[test]
fn preset_insert_is_one_undo_step() {
    let mut s = session();
    s.execute("insert.shape", json!({"preset": "diamond"})).unwrap();
    assert_eq!(shape_count(&s), 1);
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(shape_count(&s), 0);
}

#[test]
fn unknown_or_undrawn_preset_is_refused() {
    let mut s = session();
    for bad in ["notAShape", "", "rect\"/><x"] {
        assert!(s.execute("insert.shape", json!({"preset": bad})).is_err(), "{bad:?} must be refused");
    }
    assert!(s.execute("insert.shape", json!({"preset": "star5", "kind": "ellipse"})).is_err(), "kind + preset is refused");
    assert_eq!(shape_count(&s), 0);
}

#[test]
fn preset_survives_xlsx_round_trip() {
    let mut s = session();
    s.execute("insert.shape", json!({"preset": "chevron"})).unwrap();
    let bytes = gridcraft_engine::io::save_bytes(&s.doc().unwrap().wb, "g.xlsx").unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
    let mut xml = String::new();
    zip.by_name("xl/drawings/drawing1.xml").unwrap().read_to_string(&mut xml).unwrap();
    assert!(xml.contains(r#"<a:prstGeom prst="chevron">"#), "{xml}");
    let (wb, _) = gridcraft_engine::io::open_bytes("g.xlsx", &bytes).unwrap();
    let sh = &wb.active().unwrap().shapes[0];
    assert_eq!(sh.geometry.as_ref().map(|g| g.preset.as_str()), Some("chevron"));
}

#[test]
fn gallery_categories_follow_excel_order() {
    let mut s = session();
    let names: Vec<String> = gallery(&mut s).iter().map(|c| c["name"].as_str().unwrap().to_string()).collect();
    let order = ["Lines", "Rectangles", "Basic Shapes", "Block Arrows", "Equation Shapes", "Flowchart", "Stars and Banners", "Callouts"];
    let pos: Vec<usize> = names.iter().map(|n| order.iter().position(|o| o == n).unwrap_or_else(|| panic!("unknown category {n}"))).collect();
    assert!(pos.windows(2).all(|w| w[0] < w[1]), "categories out of order: {names:?}");
    // Presets drawn today, in the category the spec puts them.
    for (cat, preset) in [
        ("Rectangles", "snip1Rect"),
        ("Basic Shapes", "diamond"),
        ("Basic Shapes", "cube"),
        ("Block Arrows", "rightArrow"),
        ("Block Arrows", "chevron"),
        ("Stars and Banners", "star5"),
    ] {
        let c = gallery(&mut s).into_iter().find(|c| c["name"] == cat).unwrap_or_else(|| panic!("no {cat}"));
        assert!(c["shapes"].as_array().unwrap().iter().any(|e| e["preset"] == preset), "{preset} not in {cat}: {c}");
    }
    let lines = gallery(&mut s).into_iter().find(|c| c["name"] == "Lines").unwrap();
    assert_eq!(lines["shapes"][0], json!({"kind": "line", "label": "Line"}));
}

#[test]
fn every_gallery_entry_inserts_and_none_repeats() {
    let mut s = session();
    let mut seen = HashSet::new();
    let mut n = 0;
    for c in gallery(&mut s) {
        let shapes = c["shapes"].as_array().unwrap();
        assert!(!shapes.is_empty(), "empty category listed: {c}");
        for e in shapes {
            assert!(e["label"].as_str().is_some_and(|l| !l.is_empty()), "entry without a label: {e}");
            let p = match (e.get("preset"), e.get("kind")) {
                (Some(p), None) => {
                    let p = p.as_str().unwrap();
                    assert!(drawn(p), "gallery lists {p}, which is not drawn");
                    assert!(seen.insert(p.to_string()), "{p} listed twice");
                    json!({"preset": p})
                }
                (None, Some(k)) => json!({"kind": k}),
                _ => panic!("entry needs exactly one of kind/preset: {e}"),
            };
            s.execute("insert.shape", p).unwrap();
            n += 1;
        }
    }
    assert_eq!(shape_count(&s), n);
    assert!(n >= 30, "only {n} entries");
}

#[test]
fn gallery_is_read_only() {
    let mut s = session();
    gallery(&mut s);
    assert!(s.execute("edit.undo", json!({})).is_err() || shape_count(&s) == 0);
    assert_eq!(shape_count(&s), 0);
}
