//! Insert › Shapes gallery: every shape `insert.shape` can insert, by gallery category.
//!
//! An entry is an `insert.shape` kind or a DrawingML preset (ST_ShapeType, ECMA-376 Part 1
//! §20.1.10.56). A preset is listed only while the renderer draws it, so presets drawn later join
//! the gallery with no change here.

use gridcraft_model::{ShapeGeometry, preset_paths};
use serde_json::{Value as Json, json};

use super::{Result, Session};

/// One gallery entry: an existing `insert.shape` kind or a preset name, and its label.
#[derive(Clone, Copy)]
enum Entry {
    Kind(&'static str, &'static str),
    Preset(&'static str, &'static str),
}
use Entry::{Kind as K, Preset as P};

/// The gallery, category by category, in the gallery's order.
const GALLERY: &[(&str, &[Entry])] = &[
    ("Lines", &[K("line", "Line"), K("arrow", "Line Arrow")]),
    (
        "Rectangles",
        &[
            K("rectangle", "Rectangle"),
            K("roundedRectangle", "Rectangle: Rounded Corners"),
            P("snip1Rect", "Rectangle: Single Corner Snipped"),
            P("snip2SameRect", "Rectangle: Top Corners Snipped"),
            P("round1Rect", "Rectangle: Single Corner Rounded"),
        ],
    ),
    (
        "Basic Shapes",
        &[
            K("textBox", "Text Box"),
            K("ellipse", "Oval"),
            P("triangle", "Isosceles Triangle"),
            P("rtTriangle", "Right Triangle"),
            P("parallelogram", "Parallelogram"),
            P("trapezoid", "Trapezoid"),
            P("diamond", "Diamond"),
            P("pentagon", "Regular Pentagon"),
            P("hexagon", "Hexagon"),
            P("heptagon", "Heptagon"),
            P("octagon", "Octagon"),
            P("decagon", "Decagon"),
            P("dodecagon", "Dodecagon"),
            P("pie", "Partial Circle"),
            P("chord", "Chord"),
            P("teardrop", "Teardrop"),
            P("frame", "Frame"),
            P("halfFrame", "Half Frame"),
            P("corner", "L-Shape"),
            P("diagStripe", "Diagonal Stripe"),
            P("plus", "Cross"),
            P("plaque", "Plaque"),
            P("can", "Cylinder"),
            P("cube", "Cube"),
            P("bevel", "Bevel"),
            P("donut", "Donut"),
            P("noSmoking", "\"No\" Symbol"),
            P("blockArc", "Block Arc"),
            P("foldedCorner", "Folded Corner"),
            P("smileyFace", "Smiley Face"),
            P("heart", "Heart"),
            P("lightningBolt", "Lightning Bolt"),
            P("sun", "Sun"),
            P("moon", "Moon"),
            P("cloud", "Cloud"),
            P("arc", "Arc"),
            P("bracketPair", "Double Bracket"),
            P("bracePair", "Double Brace"),
            P("leftBracket", "Left Bracket"),
            P("rightBracket", "Right Bracket"),
            P("leftBrace", "Left Brace"),
            P("rightBrace", "Right Brace"),
        ],
    ),
    (
        "Block Arrows",
        &[
            P("rightArrow", "Arrow: Right"),
            P("leftArrow", "Arrow: Left"),
            P("upArrow", "Arrow: Up"),
            P("downArrow", "Arrow: Down"),
            P("leftRightArrow", "Arrow: Left-Right"),
            P("upDownArrow", "Arrow: Up-Down"),
            P("quadArrow", "Arrow: Quad"),
            P("leftRightUpArrow", "Arrow: Left-Right-Up"),
            P("bentArrow", "Arrow: Bent"),
            P("uturnArrow", "Arrow: U-Turn"),
            P("leftUpArrow", "Arrow: Left-Up"),
            P("bentUpArrow", "Arrow: Bent-Up"),
            P("curvedRightArrow", "Arrow: Curved Right"),
            P("curvedLeftArrow", "Arrow: Curved Left"),
            P("curvedUpArrow", "Arrow: Curved Up"),
            P("curvedDownArrow", "Arrow: Curved Down"),
            P("stripedRightArrow", "Arrow: Striped Right"),
            P("notchedRightArrow", "Arrow: Notched Right"),
            P("homePlate", "Arrow: Pentagon"),
            P("chevron", "Arrow: Chevron"),
        ],
    ),
    (
        "Equation Shapes",
        &[
            P("mathPlus", "Plus Sign"),
            P("mathMinus", "Minus Sign"),
            P("mathMultiply", "Multiplication Sign"),
            P("mathDivide", "Division Sign"),
            P("mathEqual", "Equals"),
            P("mathNotEqual", "Not Equal"),
        ],
    ),
    (
        "Flowchart",
        &[
            P("flowChartProcess", "Flowchart: Process"),
            P("flowChartAlternateProcess", "Flowchart: Alternate Process"),
            P("flowChartDecision", "Flowchart: Decision"),
            P("flowChartInputOutput", "Flowchart: Data"),
            P("flowChartPredefinedProcess", "Flowchart: Predefined Process"),
            P("flowChartInternalStorage", "Flowchart: Internal Storage"),
            P("flowChartDocument", "Flowchart: Document"),
            P("flowChartMultidocument", "Flowchart: Multidocument"),
            P("flowChartTerminator", "Flowchart: Terminator"),
            P("flowChartPreparation", "Flowchart: Preparation"),
            P("flowChartManualInput", "Flowchart: Manual Input"),
            P("flowChartManualOperation", "Flowchart: Manual Operation"),
            P("flowChartConnector", "Flowchart: Connector"),
            P("flowChartOffpageConnector", "Flowchart: Off-page Connector"),
            P("flowChartPunchedCard", "Flowchart: Card"),
            P("flowChartPunchedTape", "Flowchart: Punched Tape"),
            P("flowChartSummingJunction", "Flowchart: Summing Junction"),
            P("flowChartOr", "Flowchart: Or"),
            P("flowChartCollate", "Flowchart: Collate"),
            P("flowChartSort", "Flowchart: Sort"),
            P("flowChartExtract", "Flowchart: Extract"),
            P("flowChartMerge", "Flowchart: Merge"),
            P("flowChartOnlineStorage", "Flowchart: Stored Data"),
            P("flowChartDelay", "Flowchart: Delay"),
            P("flowChartMagneticTape", "Flowchart: Sequential Access Storage"),
            P("flowChartMagneticDisk", "Flowchart: Magnetic Disk"),
            P("flowChartMagneticDrum", "Flowchart: Direct Access Storage"),
            P("flowChartDisplay", "Flowchart: Display"),
        ],
    ),
    (
        "Stars and Banners",
        &[
            P("irregularSeal1", "Explosion: 8 Points"),
            P("irregularSeal2", "Explosion: 14 Points"),
            P("star4", "Star: 4 Points"),
            P("star5", "Star: 5 Points"),
            P("star6", "Star: 6 Points"),
            P("star7", "Star: 7 Points"),
            P("star8", "Star: 8 Points"),
            P("star10", "Star: 10 Points"),
            P("star12", "Star: 12 Points"),
            P("star16", "Star: 16 Points"),
            P("star24", "Star: 24 Points"),
            P("star32", "Star: 32 Points"),
            P("ribbon2", "Ribbon: Tilted Up"),
            P("ribbon", "Ribbon: Tilted Down"),
            P("ellipseRibbon2", "Ribbon: Curved and Tilted Up"),
            P("ellipseRibbon", "Ribbon: Curved and Tilted Down"),
            P("verticalScroll", "Scroll: Vertical"),
            P("horizontalScroll", "Scroll: Horizontal"),
            P("wave", "Wave"),
            P("doubleWave", "Double Wave"),
        ],
    ),
    (
        "Callouts",
        &[
            P("wedgeRectCallout", "Speech Bubble: Rectangle"),
            P("wedgeRoundRectCallout", "Speech Bubble: Rectangle with Corners Rounded"),
            P("wedgeEllipseCallout", "Speech Bubble: Oval"),
            P("cloudCallout", "Thought Bubble: Cloud"),
            P("borderCallout1", "Callout: Line"),
            P("borderCallout2", "Callout: Bent Line"),
            P("borderCallout3", "Callout: Double Bent Line"),
            P("accentCallout1", "Callout: Line with Accent Bar"),
            P("accentCallout2", "Callout: Bent Line with Accent Bar"),
            P("accentCallout3", "Callout: Double Bent Line with Accent Bar"),
            P("callout1", "Callout: Line with No Border"),
            P("callout2", "Callout: Bent Line with No Border"),
            P("callout3", "Callout: Double Bent Line with No Border"),
            P("accentBorderCallout1", "Callout: Line with Border and Accent Bar"),
            P("accentBorderCallout2", "Callout: Bent Line with Border and Accent Bar"),
            P("accentBorderCallout3", "Callout: Double Bent Line with Border and Accent Bar"),
        ],
    ),
];

/// The preset's geometry, as `insert.shape` stores it.
pub(super) fn preset_geometry(preset: &str) -> ShapeGeometry {
    ShapeGeometry { preset: preset.into(), adjust: vec![], paths: vec![] }
}

/// Whether the renderer draws `preset`, i.e. whether `insert.shape` accepts it.
pub(super) fn drawn(preset: &str) -> bool {
    preset_paths(&preset_geometry(preset), 1000, 1000).is_some()
}

/// `insert.shapeGallery`: `{categories: [{name, shapes: [{kind | preset, label}]}]}`.
pub(super) fn shape_gallery(_: &mut Session, _: &Json) -> Result<Json> {
    let categories: Vec<Json> = GALLERY
        .iter()
        .filter_map(|(name, entries)| {
            let shapes: Vec<Json> = entries
                .iter()
                .filter_map(|e| match *e {
                    K(kind, label) => Some(json!({"kind": kind, "label": label})),
                    P(preset, label) => drawn(preset).then(|| json!({"preset": preset, "label": label})),
                })
                .collect();
            (!shapes.is_empty()).then(|| json!({"name": name, "shapes": shapes}))
        })
        .collect();
    Ok(json!({"categories": categories}))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn no_preset_is_listed_twice() {
        let mut seen = HashSet::new();
        for (_, entries) in GALLERY {
            for e in *entries {
                if let P(preset, _) = e {
                    assert!(seen.insert(*preset), "{preset} listed twice");
                }
            }
        }
    }

    #[test]
    fn unknown_preset_is_not_drawn() {
        assert!(!drawn("notAShape"));
        assert!(!drawn(""));
        assert!(drawn("star5"));
    }
}
