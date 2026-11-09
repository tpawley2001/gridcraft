use gridcraft_model::{PathCommand, ShapeGeometry, preset_paths};

#[test]
fn star5_and_snip1rect_pin_adjustments() {
    for preset in ["star5", "snip1Rect"] {
        let geometry = |value: &str| ShapeGeometry { preset: preset.into(), adjust: vec![("adj".into(), format!("val {value}"))], paths: vec![] };
        for (outside, bound) in [("-9223372036854775808", "0"), ("9223372036854775807", "50000")] {
            let actual = preset_paths(&geometry(outside), 101, 203).expect("supported preset");
            let expected = preset_paths(&geometry(bound), 101, 203).expect("supported preset");
            assert_eq!(actual, expected, "{preset}: adj={outside}");
        }
    }
}

#[test]
fn star5_and_snip1rect_vertices_stay_in_hostile_boxes() {
    for preset in ["star5", "snip1Rect"] {
        for (w, h) in [(1, 1), (1, i64::MAX), (i64::MAX, 1), (i64::MAX, i64::MAX)] {
            for scale in [i64::MIN, 105146, i64::MAX] {
                let geometry = ShapeGeometry {
                    preset: preset.into(),
                    adjust: vec![("adj".into(), "val 50000".into()), ("hf".into(), format!("val {scale}")), ("vf".into(), format!("val {scale}"))],
                    paths: vec![],
                };
                let paths = preset_paths(&geometry, w, h).expect("supported preset");
                for path in paths {
                    assert!(path.fill && path.stroke);
                    assert_eq!(path.commands.last(), Some(&PathCommand::Close));
                    for command in path.commands {
                        if let PathCommand::MoveTo(x, y) | PathCommand::LineTo(x, y) = command {
                            assert!((0..=w).contains(&x) && (0..=h).contains(&y), "{preset}: {w}x{h}, scale={scale}: ({x}, {y})");
                        }
                    }
                }
            }
        }
    }
}
