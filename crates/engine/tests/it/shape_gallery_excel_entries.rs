//! Insert › Shapes: the gallery offers every preset of Excel's gallery that the renderer draws, in
//! Excel's category, including the Action Buttons category, the arrow and line callouts and the
//! snipped and rounded rectangles.

use gridcraft_engine::Session;
use gridcraft_model::{ShapeGeometry, preset_paths};
use serde_json::{Value as Json, json};

fn drawn(preset: &str) -> bool {
    preset_paths(&ShapeGeometry { preset: preset.into(), adjust: vec![], paths: vec![] }, 1000, 1000).is_some()
}

/// (category, preset, label prefix).
const EXPECTED: &[(&str, &str, &str)] = &[
    ("Rectangles", "snip2DiagRect", "Rectangle: "),
    ("Rectangles", "snipRoundRect", "Rectangle: "),
    ("Rectangles", "round2SameRect", "Rectangle: "),
    ("Rectangles", "round2DiagRect", "Rectangle: "),
    ("Block Arrows", "rightArrowCallout", "Callout: "),
    ("Block Arrows", "downArrowCallout", "Callout: "),
    ("Block Arrows", "leftArrowCallout", "Callout: "),
    ("Block Arrows", "upArrowCallout", "Callout: "),
    ("Block Arrows", "leftRightArrowCallout", "Callout: "),
    ("Block Arrows", "quadArrowCallout", "Callout: "),
    ("Block Arrows", "circularArrow", "Arrow: "),
    ("Callouts", "callout1", "Callout: "),
    ("Callouts", "callout2", "Callout: "),
    ("Callouts", "callout3", "Callout: "),
    ("Callouts", "accentCallout1", "Callout: "),
    ("Callouts", "accentCallout2", "Callout: "),
    ("Callouts", "accentCallout3", "Callout: "),
    ("Callouts", "accentBorderCallout1", "Callout: "),
    ("Callouts", "accentBorderCallout2", "Callout: "),
    ("Callouts", "accentBorderCallout3", "Callout: "),
    ("Action Buttons", "actionButtonBackPrevious", "Action Button: "),
    ("Action Buttons", "actionButtonForwardNext", "Action Button: "),
    ("Action Buttons", "actionButtonBeginning", "Action Button: "),
    ("Action Buttons", "actionButtonEnd", "Action Button: "),
    ("Action Buttons", "actionButtonHome", "Action Button: "),
    ("Action Buttons", "actionButtonInformation", "Action Button: "),
    ("Action Buttons", "actionButtonReturn", "Action Button: "),
    ("Action Buttons", "actionButtonMovie", "Action Button: "),
    ("Action Buttons", "actionButtonDocument", "Action Button: "),
    ("Action Buttons", "actionButtonSound", "Action Button: "),
    ("Action Buttons", "actionButtonHelp", "Action Button: "),
    ("Action Buttons", "actionButtonBlank", "Action Button: "),
];

/// (category, preset or kind, label) for every gallery entry, in order.
fn entries() -> Vec<(String, String, String)> {
    let mut s = Session::new();
    s.new_workbook();
    let g = s.execute("insert.shapeGallery", json!({})).unwrap();
    let mut out = Vec::new();
    for c in g["categories"].as_array().expect("categories array") {
        let name = c["name"].as_str().unwrap_or_default().to_string();
        for sh in c["shapes"].as_array().expect("shapes array") {
            let id = sh.get("preset").or_else(|| sh.get("kind")).and_then(Json::as_str).unwrap_or_default();
            out.push((name.clone(), id.to_string(), sh["label"].as_str().unwrap_or_default().to_string()));
        }
    }
    out
}

#[test]
fn every_drawn_excel_gallery_preset_is_offered_in_its_category() {
    let all = entries();
    let mut missing = Vec::new();
    for (cat, preset, prefix) in EXPECTED {
        let found: Vec<_> = all.iter().filter(|(_, id, _)| id == preset).collect();
        if !drawn(preset) {
            assert!(found.is_empty(), "{preset} is listed but not drawn");
            continue;
        }
        match found.as_slice() {
            [(c, _, label)] => {
                assert_eq!(c, cat, "{preset} category");
                assert!(label.starts_with(prefix), "{preset} label {label:?} should start with {prefix:?}");
            }
            [] => missing.push(*preset),
            _ => panic!("{preset} listed {} times", found.len()),
        }
    }
    assert!(missing.is_empty(), "drawn but not in the gallery: {missing:?}");
}

#[test]
fn rectangles_follow_excel_order() {
    let order = ["snip1Rect", "snip2SameRect", "snip2DiagRect", "snipRoundRect", "round1Rect", "round2SameRect", "round2DiagRect"];
    let listed: Vec<String> =
        entries().into_iter().filter(|(c, id, _)| c == "Rectangles" && order.contains(&id.as_str())).map(|(_, id, _)| id).collect();
    let want: Vec<String> = order.iter().filter(|p| drawn(p)).map(|p| p.to_string()).collect();
    assert_eq!(listed, want);
}

#[test]
fn action_buttons_are_the_last_category() {
    if !EXPECTED.iter().any(|(c, p, _)| *c == "Action Buttons" && drawn(p)) {
        return;
    }
    let all = entries();
    let last = all.last().map(|(c, _, _)| c.as_str());
    assert_eq!(last, Some("Action Buttons"));
    let first = all.iter().position(|(c, _, _)| c == "Action Buttons").unwrap();
    assert!(all[first..].iter().all(|(c, _, _)| c == "Action Buttons"), "Action Buttons entries are not contiguous");
}
