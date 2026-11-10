//! DrawingML charts (`xl/charts/chartN.xml`): reading the common 2-D/3-D chart groups into the
//! model, and writing model charts (with cached series values) from scratch.

use std::fmt::Write as _;

use gridcraft_core::{RangeRef, Value, category_levels};
use gridcraft_formula::{Expr, SheetSel};
use gridcraft_model::{
    AxisCrosses, AxisScale, Chart, Chart3d, ChartKind, ChartScale, Color, DefinedName, DisplayUnit, DisplayUnits, LegendPos, LineDash, OfPie, OfPieSplit,
    Series, Trendline, TrendlineKind, Workbook,
};

use crate::drawing::{dml_color, srgb};
use crate::xml::{El, esc, num};

// ---------------------------------------------------------------- reading

fn group_kind(g: &El) -> Option<ChartKind> {
    let grouping = g.child_val("grouping").unwrap_or("standard");
    let stacked = grouping == "stacked";
    let pct = grouping == "percentStacked";
    Some(match g.name.as_str() {
        "barChart" | "bar3DChart" => {
            let bar = g.child_val("barDir") == Some("bar");
            match (bar, stacked, pct) {
                (false, true, _) => ChartKind::ColumnStacked,
                (false, _, true) => ChartKind::ColumnStacked100,
                (false, _, _) => ChartKind::ColumnClustered,
                (true, true, _) => ChartKind::BarStacked,
                (true, _, true) => ChartKind::BarStacked100,
                (true, _, _) => ChartKind::BarClustered,
            }
        }
        "lineChart" | "line3DChart" => {
            if stacked || pct {
                ChartKind::LineStacked
            } else if g.name == "line3DChart" {
                // A 3-D line is a ribbon: it has no markers.
                ChartKind::Line
            } else {
                let group_markers = g.child_val("marker").is_none_or(|v| v == "1" || v == "true");
                let all_none = g.kids("ser").all(|s| s.path(&["marker", "symbol"]).and_then(|m| m.attr("val")) == Some("none"));
                if !group_markers || (all_none && g.kids("ser").next().is_some()) { ChartKind::Line } else { ChartKind::LineMarkers }
            }
        }
        "pieChart" | "pie3DChart" => ChartKind::Pie,
        // §21.2.2.128: a missing `ofPieType` is a pie of pie.
        "ofPieChart" => {
            if g.child_val("ofPieType") == Some("bar") {
                ChartKind::BarOfPie
            } else {
                ChartKind::PieOfPie
            }
        }
        "doughnutChart" => ChartKind::Doughnut,
        "areaChart" | "area3DChart" => {
            if pct {
                ChartKind::AreaStacked100
            } else if stacked {
                ChartKind::AreaStacked
            } else {
                ChartKind::Area
            }
        }
        "scatterChart" => {
            let no_lines = g.kids("ser").all(|s| s.path(&["spPr", "ln"]).is_some_and(|l| l.child("noFill").is_some()));
            if g.child_val("scatterStyle") == Some("marker") || (no_lines && g.kids("ser").next().is_some()) {
                ChartKind::Scatter
            } else {
                ChartKind::ScatterLines
            }
        }
        "bubbleChart" => ChartKind::Bubble,
        "radarChart" => ChartKind::Radar,
        "stockChart" => ChartKind::Stock,
        // A missing `c:wireframe` means filled (CT_SurfaceChart, CT_Surface3DChart).
        "surface3DChart" if bool_val(g, "wireframe") => ChartKind::SurfaceWireframe,
        "surface3DChart" => ChartKind::Surface,
        "surfaceChart" if bool_val(g, "wireframe") => ChartKind::ContourWireframe,
        "surfaceChart" => ChartKind::Contour,
        _ => return None,
    })
}

/// Excel's file notation for "this workbook" in front of a workbook-scoped defined name.
const THIS_BOOK: &str = "[0]!";

/// A chart reference (`<c:f>`) read from a file → model text: drops a leading `=` and the
/// `[0]!` in front of workbook-scoped names (`[0]!Sales` → `Sales`), which the formula parser
/// would take for a structured reference. External books (`[1]Sheet1!$A$1`), quoted sheet
/// names and string literals are kept as they are.
pub(crate) fn chart_ref_from_file(text: &str) -> String {
    let t = text.trim();
    let t = t.strip_prefix('=').unwrap_or(t);
    let mut out = String::with_capacity(t.len());
    let mut quote: Option<char> = None;
    // The last character kept: `[0]!` only counts at the start or after an operator,
    // never inside a structured reference (`T[[0]]`) or a longer name.
    let mut prev: Option<char> = None;
    let mut i = 0;
    while let Some(c) = t.get(i..).and_then(|r| r.chars().next()) {
        let rest = t.get(i..).unwrap_or("");
        if quote.is_none()
            && rest.starts_with(THIS_BOOK)
            && prev.is_none_or(|p| !(p.is_alphanumeric() || "_.\\[]'\"!$#".contains(p)))
            && rest.get(THIS_BOOK.len()..).and_then(|n| n.chars().next()).is_some_and(|n| n.is_alphabetic() || n == '_' || n == '\\')
        {
            i += THIS_BOOK.len();
            continue;
        }
        match (c, quote) {
            ('"' | '\'', None) => quote = Some(c),
            (q, Some(open)) if q == open => quote = None,
            _ => {}
        }
        out.push(c);
        prev = Some(c);
        i += c.len_utf8();
    }
    out
}

fn data_ref(e: Option<&El>) -> Option<String> {
    let e = e?;
    for k in ["numRef", "strRef", "multiLvlStrRef"] {
        if let Some(f) = e.child(k).and_then(|r| r.child("f")) {
            return Some(chart_ref_from_file(&f.text));
        }
    }
    // Literal values become an array constant.
    for k in ["numLit", "strLit"] {
        if let Some(l) = e.child(k) {
            let pts: Vec<String> = l
                .kids("pt")
                .filter_map(|p| p.child("v"))
                .map(|v| if k == "numLit" { v.text.trim().to_string() } else { format!("\"{}\"", v.text.replace('"', "\"\"")) })
                .collect();
            return Some(format!("{{{}}}", pts.join(",")));
        }
    }
    None
}

fn read_series(s: &El) -> Series {
    let name = s
        .child("tx")
        .and_then(|tx| tx.path(&["strRef", "f"]).map(|f| chart_ref_from_file(&f.text)).or_else(|| tx.child("v").map(|v| v.text.clone())));
    let sp = s.child("spPr");
    let color = sp
        .and_then(|p| p.child("solidFill"))
        .and_then(dml_color)
        .or_else(|| sp.and_then(|p| p.path(&["ln", "solidFill"])).and_then(dml_color))
        .or_else(|| s.path(&["marker", "spPr", "solidFill"]).and_then(dml_color));
    Series {
        name,
        categories: data_ref(s.child("cat")).or_else(|| data_ref(s.child("xVal"))),
        values: data_ref(s.child("val")).or_else(|| data_ref(s.child("yVal"))).unwrap_or_default(),
        bubble_sizes: data_ref(s.child("bubbleSize")),
        color,
        secondary: false,
        kind: None,
        quartiles: Default::default(),
        smooth: bool_val(s, "smooth"),
        trendlines: s.kids("trendline").filter_map(read_trendline).collect(),
        explosion: explosion_val(s).unwrap_or(0),
        point_explosions: s
            .kids("dPt")
            .filter_map(|d| Some((d.child_val("idx")?.trim().parse::<u32>().ok()?, explosion_val(d)?)))
            .take(MAX_POINT_EXPLOSIONS)
            .collect(),
    }
}

/// Exploded slices kept per series; a hostile file cannot make the list grow without bound.
const MAX_POINT_EXPLOSIONS: usize = 4096;

/// `c:explosion` (ECMA-376 Part 1, 21.2.2.61): percent of the radius, clamped to the UI maximum.
fn explosion_val(e: &El) -> Option<u32> {
    let v = e.child_val("explosion")?.trim();
    // Digits beyond u32 still mean "very far out": clamp rather than drop.
    let n = v.parse::<u64>().ok().or_else(|| (!v.is_empty() && v.bytes().all(|b| b.is_ascii_digit())).then_some(u64::MAX))?;
    Some(u32::try_from(n.min(u64::from(Series::MAX_EXPLOSION))).unwrap_or(Series::MAX_EXPLOSION))
}

const DASHES: [(LineDash, &str); 11] = [
    (LineDash::Solid, "solid"),
    (LineDash::Dot, "dot"),
    (LineDash::Dash, "dash"),
    (LineDash::LgDash, "lgDash"),
    (LineDash::DashDot, "dashDot"),
    (LineDash::LgDashDot, "lgDashDot"),
    (LineDash::LgDashDotDot, "lgDashDotDot"),
    (LineDash::SysDash, "sysDash"),
    (LineDash::SysDot, "sysDot"),
    (LineDash::SysDashDot, "sysDashDot"),
    (LineDash::SysDashDotDot, "sysDashDotDot"),
];

const EMU_PER_PT: f64 = 12_700.0;

/// A `CT_Boolean` child (`val` defaults to true).
fn bool_val(e: &El, tag: &str) -> bool {
    e.child(tag).is_some_and(|c| c.flag("val", true))
}

fn f64_val(e: &El, tag: &str) -> Option<f64> {
    e.child(tag).and_then(|c| c.attr_f64("val"))
}

/// `<c:trendline>` (ECMA-376 Part 1, 21.2.2.211). Unknown types are dropped.
fn read_trendline(t: &El) -> Option<Trendline> {
    let kind = match t.child_val("trendlineType").unwrap_or("linear") {
        "linear" => TrendlineKind::Linear,
        "exp" => TrendlineKind::Exponential,
        "log" => TrendlineKind::Logarithmic,
        "power" => TrendlineKind::Power,
        "poly" => TrendlineKind::Polynomial(t.child_val("order").and_then(|v| v.trim().parse::<u8>().ok()).unwrap_or(2).clamp(2, 6)),
        "movingAvg" => TrendlineKind::MovingAverage(t.child_val("period").and_then(|v| v.trim().parse::<u32>().ok()).unwrap_or(2).clamp(2, 255)),
        _ => return None,
    };
    let ln = t.path(&["spPr", "ln"]);
    Some(Trendline {
        kind,
        name: t.child("name").map(|n| n.text.clone()),
        forward: Trendline::clamp_forecast(f64_val(t, "forward").unwrap_or(0.0)),
        backward: Trendline::clamp_forecast(f64_val(t, "backward").unwrap_or(0.0)),
        intercept: f64_val(t, "intercept"),
        display_equation: bool_val(t, "dispEq"),
        display_r_squared: bool_val(t, "dispRSqr"),
        color: ln.and_then(|l| l.child("solidFill")).and_then(dml_color),
        width: ln.and_then(|l| l.attr_f64("w")).map(|w| (w / EMU_PER_PT) as f32).filter(|w| w.is_finite() && *w >= 0.0),
        dash: ln.and_then(|l| l.child("prstDash")).and_then(|d| d.attr("val")).and_then(|v| DASHES.iter().find(|(_, n)| *n == v).map(|(d, _)| *d)),
    })
}

fn rich_title(t: &El) -> Option<String> {
    let tx = t.child("tx")?;
    if let Some(rich) = tx.child("rich") {
        let paras: Vec<String> =
            rich.kids("p").map(|p| p.kids("r").filter_map(|r| r.child("t")).map(|t| t.text.as_str()).collect::<String>()).collect();
        let s = paras.join("\n");
        return if s.is_empty() { None } else { Some(s) };
    }
    if let Some(sr) = tx.child("strRef") {
        if let Some(v) = sr.path(&["strCache", "pt", "v"]) {
            return Some(v.text.clone());
        }
        return sr.child("f").map(|f| f.text.trim().to_string());
    }
    None
}

/// The 3-D settings of a single `c:*3DChart` group: the chart's `c:view3D` (each value kept only
/// when present and inside its schema range), the group's `gapDepth` and `shape`, whether the
/// series stand in depth (a real third axis id with a `c:serAx`).
fn read_3d(chart: &El, plot: &El, g: &El) -> Option<Chart3d> {
    if !matches!(g.name.as_str(), "bar3DChart" | "line3DChart" | "area3DChart" | "pie3DChart") {
        return None;
    }
    let view = chart.child("view3D");
    let int = |tag: &str| view.and_then(|v| v.child_val(tag)).and_then(|v| v.trim().trim_end_matches('%').parse::<i64>().ok());
    let in_range = |tag: &str, lo: i64, hi: i64| int(tag).filter(|v| (lo..=hi).contains(v));
    let third_axis = axis_ids(g).get(2).is_some_and(|id| id.trim() != "0");
    let series_in_depth = match g.name.as_str() {
        "line3DChart" => true,
        "bar3DChart" | "area3DChart" => third_axis && plot.child("serAx").is_some() && g.child_val("grouping").is_none_or(|v| v == "standard"),
        _ => false,
    };
    Some(Chart3d {
        rot_x: in_range("rotX", -90, 90).and_then(|v| i16::try_from(v).ok()),
        h_percent: in_range("hPercent", 5, 500).and_then(|v| u16::try_from(v).ok()),
        rot_y: in_range("rotY", 0, 360).and_then(|v| u16::try_from(v).ok()),
        depth_percent: in_range("depthPercent", 20, 2000).and_then(|v| u16::try_from(v).ok()),
        right_angle_axes: view.and_then(|v| v.child("rAngAx")).map(|r| r.flag("val", true)),
        perspective: in_range("perspective", 0, 240).and_then(|v| u8::try_from(v).ok()),
        gap_depth: g.child_val("gapDepth").and_then(|v| v.trim().trim_end_matches('%').parse::<u16>().ok()).filter(|v| *v <= 500),
        shape: g.child_val("shape").filter(|v| SHAPES.contains(v)).map(str::to_string),
        series_in_depth,
    })
}

/// `ST_Shape` values.
const SHAPES: [&str; 6] = ["box", "cylinder", "cone", "coneToMax", "pyramid", "pyramidToMax"];

fn axis_ids(g: &El) -> Vec<String> {
    g.kids("axId").filter_map(|a| a.attr("val")).map(str::to_string).collect()
}

pub fn read_chart(root: &El) -> Option<Chart> {
    let chart = root.child("chart")?;
    let plot = chart.child("plotArea")?;
    let groups: Vec<(&El, ChartKind)> = plot.children.iter().filter_map(|g| group_kind(g).map(|k| (g, k))).collect();
    let (first, first_kind) = *groups.first()?;
    let first_axes = axis_ids(first);
    let mut series = vec![];
    for (gi, (g, k)) in groups.iter().enumerate() {
        for s in g.kids("ser") {
            let mut ser = read_series(s);
            if groups.len() > 1 {
                // Every series of a combo chart keeps its own group's kind, the first group's included.
                ser.kind = Some(*k);
            }
            if gi > 0 {
                ser.secondary = axis_ids(g) != first_axes;
            }
            series.push(ser);
        }
    }
    let kind = if groups.len() > 1 { ChartKind::Combo } else { first_kind };
    let show_val = |e: &El| e.child("dLbls").is_some_and(|d| bool_val(d, "showVal"));
    let data_labels = groups.iter().any(|(g, _)| show_val(g) || g.kids("ser").any(show_val));
    let val_axes: Vec<&El> = plot.kids("valAx").collect();
    let cat_ax = plot.child("catAx").or_else(|| plot.child("dateAx"));
    let scatter = matches!(first_kind, ChartKind::Scatter | ChartKind::ScatterLines | ChartKind::Bubble);
    let (x_ax, y_ax) = if scatter {
        let x = val_axes.iter().find(|a| matches!(a.child_val("axPos"), Some("b") | Some("t"))).copied();
        let y = val_axes.iter().find(|a| matches!(a.child_val("axPos"), Some("l") | Some("r"))).copied();
        (x, y)
    } else {
        (cat_ax, val_axes.first().copied())
    };
    let legend = match chart.child("legend") {
        None => LegendPos::None,
        Some(l) => match l.child_val("legendPos").unwrap_or("r") {
            "b" => LegendPos::Bottom,
            "t" => LegendPos::Top,
            "l" => LegendPos::Left,
            _ => LegendPos::Right,
        },
    };
    let doughnut = plot.child("doughnutChart");
    let hole_size = doughnut.and_then(|g| g.child_val("holeSize")).and_then(|v| v.trim().parse::<u8>().ok()).unwrap_or(75).clamp(10, 90);
    let first_slice_angle =
        plot.child("pieChart").or(doughnut).and_then(|g| g.child_val("firstSliceAng")).and_then(|v| v.trim().parse::<u16>().ok()).unwrap_or(0) % 360;
    Some(Chart {
        alt: String::new(),
        theme_override: None,
        hidden: false,
        id: 0,
        kind,
        anchor: Default::default(),
        title: chart.child("title").and_then(rich_title),
        series,
        legend,
        data_labels,
        data_table: plot.child("dTable").map(|t| gridcraft_model::ChartDataTable {
            show_keys: bool_val(t, "showKeys"),
            horizontal_border: bool_val(t, "showHorzBorder"),
            vertical_border: bool_val(t, "showVertBorder"),
            outline: bool_val(t, "showOutline"),
        }),
        gridlines: val_axes.iter().any(|a| a.child("majorGridlines").is_some()),
        style: root.child_val("style").and_then(|v| v.parse().ok()).unwrap_or(2),
        hole_size,
        first_slice_angle,
        x_title: x_ax.and_then(|a| a.child("title")).and_then(rich_title),
        y_title: y_ax.and_then(|a| a.child("title")).and_then(rich_title),
        source: None,
        by_rows: false,
        pivot: root.path(&["pivotSource", "name"]).map(|n| strip_book(n.text.trim()).to_string()).filter(|n| !n.is_empty()),
        of_pie: plot.child("ofPieChart").map(read_of_pie),
        scale: ChartScale {
            x: if scatter { x_ax.map(read_scale).unwrap_or_default() } else { AxisScale::default() },
            y: y_ax.map(read_scale).unwrap_or_default(),
            cat_crosses: if scatter { AxisCrosses::AutoZero } else { cat_ax.map(read_crosses).unwrap_or_default() },
        },
        view_3d: if groups.len() == 1 { read_3d(chart, plot, first) } else { None },
    })
}

/// Most points a custom split keeps (a hostile file could list millions).
const MAX_CUSTOM_SPLIT: usize = 4096;

/// The second plot of a `c:ofPieChart` (§21.2.2.126): gap width, split, second plot size and
/// series lines. Values outside the schema's ranges are clamped; a split without a usable
/// `splitPos` is automatic.
fn read_of_pie(g: &El) -> OfPie {
    let int = |name: &str| g.child_val(name).and_then(|v| v.trim().parse::<u32>().ok());
    let pos = g.child_val("splitPos").and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite());
    let split = match (g.child_val("splitType"), pos) {
        (Some("cust"), _) => OfPieSplit::Custom(
            g.child("custSplit")
                .map(|c| c.kids("secondPiePt").filter_map(|p| p.attr("val")?.trim().parse::<u32>().ok()).take(MAX_CUSTOM_SPLIT).collect())
                .unwrap_or_default(),
        ),
        (Some("pos"), Some(v)) => OfPieSplit::Position(v),
        (Some("val"), Some(v)) => OfPieSplit::Value(v),
        (Some("percent"), Some(v)) => OfPieSplit::Percent(v),
        _ => OfPieSplit::Auto,
    };
    OfPie {
        gap_width: int("gapWidth").unwrap_or(100).min(500) as u16,
        split,
        second_size: int("secondPieSize").unwrap_or(75).clamp(5, 200) as u16,
        series_lines: g.child("serLines").is_some(),
    }
}

/// A value axis's bounds, units, log base and orientation. Values that aren't finite, a log base
/// outside 2..=1000 and a minimum not below the maximum are dropped (left automatic).
fn read_scale(ax: &El) -> AxisScale {
    let num = |e: Option<&El>| e.and_then(|e| e.attr("val")).and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite());
    let scaling = ax.child("scaling");
    let mut s = AxisScale {
        min: num(scaling.and_then(|x| x.child("min"))),
        max: num(scaling.and_then(|x| x.child("max"))),
        major_unit: num(ax.child("majorUnit")).filter(|u| *u > 0.0),
        minor_unit: num(ax.child("minorUnit")).filter(|u| *u > 0.0),
        log_base: num(scaling.and_then(|x| x.child("logBase"))).filter(|b| (2.0..=1000.0).contains(b)),
        reverse: scaling.and_then(|x| x.child_val("orientation")) == Some("maxMin"),
        display_units: ax.child("dispUnits").and_then(read_display_units),
        crosses: read_crosses(ax),
    };
    if let (Some(lo), Some(hi)) = (s.min, s.max)
        && lo >= hi
    {
        s.min = None;
        s.max = None;
    }
    s
}

/// Where an axis element crosses its `c:crossAx` (§21.2.2 `crosses` / `crossesAt`), in that
/// axis's units. A crossing that isn't a finite number is automatic.
fn read_crosses(ax: &El) -> AxisCrosses {
    if let Some(v) = ax.child("crossesAt").and_then(|e| e.attr("val")) {
        return v.trim().parse::<f64>().ok().filter(|v| v.is_finite()).map_or(AxisCrosses::AutoZero, AxisCrosses::At);
    }
    match ax.child_val("crosses") {
        Some("min") => AxisCrosses::Min,
        Some("max") => AxisCrosses::Max,
        _ => AxisCrosses::AutoZero,
    }
}

/// `c:dispUnits` (CT_DispUnits): a built-in unit (attribute default `thousands`) or a custom one
/// (dropped unless finite and positive); `c:dispUnitsLbl` shows the label.
fn read_display_units(d: &El) -> Option<DisplayUnits> {
    let unit = if let Some(b) = d.child("builtInUnit") {
        let name = b.attr("val").unwrap_or("thousands");
        DisplayUnit::BUILT_IN.iter().find(|(_, n)| *n == name).map(|(u, _)| *u)?
    } else {
        let f = d.child_val("custUnit")?.trim().parse::<f64>().ok().filter(|f| f.is_finite() && *f > 0.0)?;
        DisplayUnit::Custom(f)
    };
    Some(DisplayUnits { unit, show_label: d.child("dispUnitsLbl").is_some() })
}

// ---------------------------------------------------------------- writing

#[derive(Clone, Copy, PartialEq)]
enum G {
    Bar {
        bar: bool,
        grouping: &'static str,
    },
    Line {
        stacked: bool,
        markers: bool,
    },
    Pie,
    /// `c:ofPieChart`: bar of pie when `bar`, else pie of pie.
    OfPie {
        bar: bool,
    },
    Doughnut,
    Area {
        grouping: &'static str,
    },
    Scatter {
        lines: bool,
    },
    Bubble,
    Radar,
    Stock,
    /// `c:surface3DChart` (`three_d`) or `c:surfaceChart` (contour).
    Surface {
        three_d: bool,
        wireframe: bool,
    },
}

fn group_for(k: ChartKind) -> G {
    match k {
        ChartKind::ColumnStacked => G::Bar { bar: false, grouping: "stacked" },
        ChartKind::ColumnStacked100 => G::Bar { bar: false, grouping: "percentStacked" },
        ChartKind::BarClustered => G::Bar { bar: true, grouping: "clustered" },
        ChartKind::BarStacked => G::Bar { bar: true, grouping: "stacked" },
        ChartKind::BarStacked100 => G::Bar { bar: true, grouping: "percentStacked" },
        ChartKind::Line => G::Line { stacked: false, markers: false },
        ChartKind::LineMarkers => G::Line { stacked: false, markers: true },
        ChartKind::LineStacked => G::Line { stacked: true, markers: false },
        ChartKind::Pie => G::Pie,
        ChartKind::PieOfPie => G::OfPie { bar: false },
        ChartKind::BarOfPie => G::OfPie { bar: true },
        ChartKind::Doughnut => G::Doughnut,
        ChartKind::Area => G::Area { grouping: "standard" },
        ChartKind::AreaStacked => G::Area { grouping: "stacked" },
        ChartKind::AreaStacked100 => G::Area { grouping: "percentStacked" },
        ChartKind::Scatter => G::Scatter { lines: false },
        ChartKind::ScatterLines => G::Scatter { lines: true },
        ChartKind::Bubble => G::Bubble,
        ChartKind::Radar => G::Radar,
        ChartKind::Stock => G::Stock,
        ChartKind::Surface => G::Surface { three_d: true, wireframe: false },
        ChartKind::SurfaceWireframe => G::Surface { three_d: true, wireframe: true },
        ChartKind::Contour => G::Surface { three_d: false, wireframe: false },
        ChartKind::ContourWireframe => G::Surface { three_d: false, wireframe: true },
        // Combo charts start from clustered columns; chartex kinds have their own part.
        _ => G::Bar { bar: false, grouping: "clustered" },
    }
}

const MAX_CACHE: u64 = 100_000;

/// The sheet and range a same-workbook reference (`Sheet1!$B$2:$B$9`) names.
fn ref_target(wb: &Workbook, sheet_idx: usize, f: &str) -> Option<(usize, RangeRef)> {
    let e = gridcraft_formula::parse(f).ok()?;
    let Expr::Ref(r) = e else { return None };
    let si = match &r.sheet {
        SheetSel::Current => sheet_idx,
        SheetSel::Named(n) => wb.sheet_index(n)?,
        SheetSel::Span(..) => return None,
    };
    Some((si, r.range()))
}

/// Values of a same-workbook reference (`Sheet1!$B$2:$B$9`), row-major.
pub(crate) fn ref_values(wb: &Workbook, sheet_idx: usize, f: &str) -> Option<Vec<Value>> {
    let (si, mut range) = ref_target(wb, sheet_idx, f)?;
    let sheet = wb.sheet(si)?;
    // Whole columns/rows: cut the far end at the used range. The start stays put, so point indices keep matching the reference.
    if let Some(u) = sheet.used_range() {
        if range.is_full_cols() {
            range.end.row = range.end.row.min(u.end.row.max(range.start.row));
        }
        if range.is_full_rows() {
            range.end.col = range.end.col.min(u.end.col.max(range.start.col));
        }
    }
    if range.count() > MAX_CACHE {
        return None;
    }
    Some(range.iter().map(|c| sheet.value(c)).collect())
}

fn is_ref(f: &str) -> bool {
    matches!(gridcraft_formula::parse(f), Ok(Expr::Ref(_)))
}

/// A chart reference that is a name: its sheet when qualified (`Sheet1!Sales`), the bare
/// name, and the definition formulas would find from sheet `si` (none when undefined). `None`
/// when it isn't a name, or names a sheet the workbook doesn't have.
fn chart_name<'a>(wb: &'a Workbook, si: usize, f: &str) -> Option<(Option<usize>, String, Option<&'a DefinedName>)> {
    let Ok(Expr::Name(n)) = gridcraft_formula::parse(f.trim()) else { return None };
    // The parser has already unquoted the sheet (`'Bob''s'!Name` → `Bob's!Name`, #2012): split
    // at the last `!` (`'a!b'!Name`) and keep the name as is (`' Sales '` keeps its spaces).
    match n.rsplit_once('!') {
        Some((s, bare)) => {
            let i = wb.sheet_index(s)?;
            Some((Some(i), bare.to_string(), wb.name(bare, i)))
        }
        None => {
            let def = wb.name(&n, si);
            Some((None, n, def))
        }
    }
}

/// A chart reference for `<c:f>`. Excel needs a name qualified there: with a bare `Name` it
/// draws the whole chart blank, with no repair prompt (observed in Excel for Windows,
/// Microsoft 365). A workbook-scoped name is written `[0]!Name`, as Excel writes it, a
/// sheet-scoped one `Sheet!Name`; an undefined name gets `[0]!`, as the file it came from had.
fn chart_ref_to_file(wb: &Workbook, si: usize, f: &str) -> String {
    let on_sheet = |i: usize, n: &str| wb.sheet(i).map(|sh| format!("{}!{n}", gridcraft_formula::quote_sheet(&sh.name)));
    match chart_name(wb, si, f) {
        Some((Some(i), n, _)) => on_sheet(i, &n).unwrap_or_else(|| crate::fmla::text_to_file(f)),
        Some((None, n, def)) => def.and_then(|d| d.scope).and_then(|i| on_sheet(i, &n)).unwrap_or_else(|| format!("{THIS_BOOK}{n}")),
        None => crate::fmla::text_to_file(f),
    }
}

/// A series name that refers to a defined name (as opposed to literal text that parses as one).
fn is_defined_name(wb: &Workbook, si: usize, f: &str) -> bool {
    chart_name(wb, si, f).is_some_and(|(_, _, d)| d.is_some())
}

fn num_data(tag: &str, f: Option<&str>, wb: &Workbook, si: usize) -> String {
    let f = f.unwrap_or("").trim();
    if f.is_empty() {
        return format!("<c:{tag}><c:numLit><c:ptCount val=\"0\"/></c:numLit></c:{tag}>");
    }
    if f.starts_with('{') {
        let vals: Vec<&str> = f.trim_matches(|c| c == '{' || c == '}').split([',', ';']).collect();
        let mut s = format!("<c:{tag}><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val=\"{}\"/>", vals.len());
        for (i, v) in vals.iter().enumerate() {
            if let Ok(n) = v.trim().parse::<f64>() {
                let _ = write!(s, "<c:pt idx=\"{i}\"><c:v>{}</c:v></c:pt>", num(n));
            }
        }
        let _ = write!(s, "</c:numLit></c:{tag}>");
        return s;
    }
    let mut s = format!("<c:{tag}><c:numRef><c:f>{}</c:f>", esc(&chart_ref_to_file(wb, si, f)));
    if let Some(vals) = ref_values(wb, si, f) {
        let _ = write!(s, "<c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"{}\"/>", vals.len());
        for (i, v) in vals.iter().enumerate() {
            if let Value::Number(n) = v {
                let _ = write!(s, "<c:pt idx=\"{i}\"><c:v>{}</c:v></c:pt>", num(*n));
            }
        }
        s.push_str("</c:numCache>");
    }
    let _ = write!(s, "</c:numRef></c:{tag}>");
    s
}

/// Splits the inside of an array constant on `,` and `;` outside double quotes; inside quotes `""` is a literal quote.
/// Unquoted items are trimmed; quoted ones keep the text between the quotes and drop the spaces around them.
fn split_array_items(inner: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut in_quote = false;
    let mut chars = inner.chars().peekable();
    let finish = |cur: &mut String, quoted: &mut bool| {
        let text = std::mem::take(cur);
        let item = if *quoted { text } else { text.trim().to_string() };
        *quoted = false;
        item
    };
    while let Some(c) = chars.next() {
        match c {
            '"' if in_quote && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => {
                if !quoted && cur.trim().is_empty() {
                    cur.clear();
                }
                in_quote = !in_quote;
                quoted = true;
            }
            ',' | ';' if !in_quote => items.push(finish(&mut cur, &mut quoted)),
            c if quoted && !in_quote && c.is_whitespace() => {}
            _ => cur.push(c),
        }
    }
    items.push(finish(&mut cur, &mut quoted));
    items
}

fn str_data(tag: &str, f: &str, wb: &Workbook, si: usize) -> String {
    let f = f.trim();
    if f.starts_with('{') {
        let vals = split_array_items(f.trim_matches(|c| c == '{' || c == '}'));
        let mut s = format!("<c:{tag}><c:strLit><c:ptCount val=\"{}\"/>", vals.len());
        for (i, v) in vals.iter().enumerate() {
            let _ = write!(s, "<c:pt idx=\"{i}\"><c:v>{}</c:v></c:pt>", esc(v));
        }
        let _ = write!(s, "</c:strLit></c:{tag}>");
        return s;
    }
    let mut s = format!("<c:{tag}><c:strRef><c:f>{}</c:f>", esc(&chart_ref_to_file(wb, si, f)));
    if let Some(vals) = ref_values(wb, si, f) {
        let _ = write!(s, "<c:strCache><c:ptCount val=\"{}\"/>", vals.len());
        for (i, v) in vals.iter().enumerate() {
            if !v.is_empty() {
                let _ = write!(s, "<c:pt idx=\"{i}\"><c:v>{}</c:v></c:pt>", esc(&v.display()));
            }
        }
        s.push_str("</c:strCache>");
    }
    let _ = write!(s, "</c:strRef></c:{tag}>");
    s
}

/// `<c:cat>` of a series: a string reference, or a multi-level one (`multiLvlStrRef`) when the
/// category range has more columns than a column of values (or more rows than a row of values:
/// year beside month). Its cache lists the innermost level first, then each outer level with
/// only the cells that start a group, as Excel writes it.
fn cat_data(f: &str, values: &str, wb: &Workbook, si: usize) -> String {
    multi_level_cat(f.trim(), values, wb, si).unwrap_or_else(|| str_data("cat", f, wb, si))
}

fn multi_level_cat(f: &str, values: &str, wb: &Workbook, si: usize) -> Option<String> {
    let (sheet, range) = ref_target(wb, si, f)?;
    let value_range = ref_target(wb, si, values).map(|(_, r)| r);
    let levels = category_levels(&range, value_range.as_ref())?;
    if range.count() > MAX_CACHE {
        return None;
    }
    let sheet = wb.sheet(sheet)?;
    let (w, h) = (range.width() as usize, range.height() as usize);
    let text = |row: usize, col: usize| -> String {
        let v = range.start.offset(row as i64, col as i64).map(|c| sheet.value(c));
        v.filter(|v| !v.is_empty()).map(|v| v.display()).unwrap_or_default()
    };
    // The innermost level (nearest the values), then the outer ones.
    let (n, lvls): (usize, Vec<Vec<String>>) = if levels.by_columns {
        (h, (0..=levels.outer as usize).filter_map(|k| (w - 1).checked_sub(k)).map(|c| (0..h).map(|r| text(r, c)).collect()).collect())
    } else {
        (w, (0..=levels.outer as usize).filter_map(|k| (h - 1).checked_sub(k)).map(|r| (0..w).map(|c| text(r, c)).collect()).collect())
    };
    let mut s = format!("<c:cat><c:multiLvlStrRef><c:f>{}</c:f><c:multiLvlStrCache><c:ptCount val=\"{n}\"/>", esc(&crate::fmla::text_to_file(f)));
    for lvl in &lvls {
        s.push_str("<c:lvl>");
        for (i, v) in lvl.iter().enumerate() {
            if !v.is_empty() {
                let _ = write!(s, "<c:pt idx=\"{i}\"><c:v>{}</c:v></c:pt>", esc(v));
            }
        }
        s.push_str("</c:lvl>");
    }
    s.push_str("</c:multiLvlStrCache></c:multiLvlStrRef></c:cat>");
    Some(s)
}

fn series_xml(s: &Series, idx: usize, g: G, wb: &Workbook, si: usize) -> String {
    let mut x = format!("<c:ser><c:idx val=\"{idx}\"/><c:order val=\"{idx}\"/>");
    if let Some(n) = &s.name {
        if is_ref(n) || is_defined_name(wb, si, n) {
            x.push_str(&str_data("tx", n, wb, si));
        } else {
            let lit = n.trim().strip_prefix('"').and_then(|t| t.strip_suffix('"')).unwrap_or(n);
            let _ = write!(x, "<c:tx><c:v>{}</c:v></c:tx>", esc(lit));
        }
    }
    let color = s.color.as_ref().and_then(|c: &Color| srgb(c, &wb.theme));
    let line_like = matches!(g, G::Line { .. } | G::Radar | G::Scatter { lines: true });
    match (&color, line_like) {
        (Some(c), true) => {
            let _ = write!(x, "<c:spPr><a:ln w=\"28575\" cap=\"rnd\"><a:solidFill>{c}</a:solidFill><a:round/></a:ln></c:spPr>");
        }
        (Some(c), false) if g == (G::Scatter { lines: false }) => {
            let _ = write!(
                x,
                "<c:spPr><a:ln w=\"19050\"><a:noFill/></a:ln></c:spPr><c:marker><c:symbol val=\"circle\"/><c:size val=\"5\"/><c:spPr><a:solidFill>{c}</a:solidFill></c:spPr></c:marker>"
            );
        }
        (Some(c), false) => {
            let _ = write!(x, "<c:spPr><a:solidFill>{c}</a:solidFill></c:spPr>");
        }
        (None, _) if g == (G::Scatter { lines: false }) || g == G::Stock => x.push_str("<c:spPr><a:ln w=\"19050\"><a:noFill/></a:ln></c:spPr>"),
        _ => {}
    }
    match g {
        G::Bar { .. } => x.push_str("<c:invertIfNegative val=\"0\"/>"),
        G::Line { markers: false, .. } | G::Scatter { lines: true } | G::Stock => x.push_str("<c:marker><c:symbol val=\"none\"/></c:marker>"),
        G::Bubble => x.push_str("<c:invertIfNegative val=\"0\"/>"),
        _ => {}
    }
    // CT_PieSer: explosion, then one dPt (idx, explosion) per pulled-out slice, before dLbls/cat.
    if matches!(g, G::Pie | G::Doughnut) {
        let e = s.explosion.min(Series::MAX_EXPLOSION);
        if e > 0 {
            let _ = write!(x, "<c:explosion val=\"{e}\"/>");
        }
        let mut points: Vec<(u32, u32)> = Vec::new();
        for &(i, pe) in &s.point_explosions {
            match points.iter_mut().find(|(j, _)| *j == i) {
                Some(p) => p.1 = pe,
                None => points.push((i, pe)),
            }
        }
        points.sort_by_key(|(i, _)| *i);
        for (i, pe) in points {
            let _ = write!(x, "<c:dPt><c:idx val=\"{i}\"/><c:explosion val=\"{}\"/></c:dPt>", pe.min(Series::MAX_EXPLOSION));
        }
    }
    // Trendlines sit between dLbls and the data in CT_LineSer, CT_BarSer, CT_AreaSer,
    // CT_ScatterSer and CT_BubbleSer; pie, radar and surface series have none.
    if !matches!(g, G::Pie | G::OfPie { .. } | G::Doughnut | G::Radar | G::Surface { .. }) {
        for t in &s.trendlines {
            x.push_str(&trendline_xml(t, wb));
        }
    }
    match g {
        G::Scatter { .. } | G::Bubble => {
            if let Some(c) = &s.categories {
                x.push_str(&num_data("xVal", Some(c), wb, si));
            }
            x.push_str(&num_data("yVal", Some(&s.values), wb, si));
            if g == G::Bubble {
                x.push_str(&num_data("bubbleSize", s.bubble_sizes.as_deref().or(Some(&s.values)), wb, si));
                x.push_str("<c:bubble3D val=\"0\"/>");
            } else {
                let _ = write!(x, "<c:smooth val=\"{}\"/>", s.smooth as u8);
            }
        }
        _ => {
            if let Some(c) = &s.categories {
                x.push_str(&cat_data(c, &s.values, wb, si));
            }
            x.push_str(&num_data("val", Some(&s.values), wb, si));
            if matches!(g, G::Line { .. }) {
                let _ = write!(x, "<c:smooth val=\"{}\"/>", s.smooth as u8);
            }
        }
    }
    x.push_str("</c:ser>");
    x
}

/// `<c:trendline>` with children in schema order: name, spPr, trendlineType, order, period,
/// forward, backward, intercept, dispRSqr, dispEq, trendlineLbl.
fn trendline_xml(t: &Trendline, wb: &Workbook) -> String {
    let mut x = String::from("<c:trendline>");
    if let Some(n) = &t.name {
        let _ = write!(x, "<c:name>{}</c:name>", esc(n));
    }
    let color = t.color.as_ref().and_then(|c| srgb(c, &wb.theme));
    if color.is_some() || t.width.is_some() || t.dash.is_some() {
        x.push_str("<c:spPr><a:ln");
        if let Some(w) = t.width.filter(|w| w.is_finite()) {
            // ST_LineWidth: 0..=20116800 EMU.
            let _ = write!(x, " w=\"{}\"", (f64::from(w) * EMU_PER_PT).round().clamp(0.0, 20_116_800.0) as i64);
        }
        x.push_str(" cap=\"rnd\">");
        if let Some(c) = &color {
            let _ = write!(x, "<a:solidFill>{c}</a:solidFill>");
        }
        if let Some(d) = t.dash.and_then(|d| DASHES.iter().find(|(k, _)| *k == d)) {
            let _ = write!(x, "<a:prstDash val=\"{}\"/>", d.1);
        }
        x.push_str("</a:ln></c:spPr>");
    }
    let ty = match t.kind {
        TrendlineKind::Linear => "linear",
        TrendlineKind::Exponential => "exp",
        TrendlineKind::Logarithmic => "log",
        TrendlineKind::Polynomial(_) => "poly",
        TrendlineKind::Power => "power",
        TrendlineKind::MovingAverage(_) => "movingAvg",
    };
    let _ = write!(x, "<c:trendlineType val=\"{ty}\"/>");
    match t.kind {
        TrendlineKind::Polynomial(o) => {
            let _ = write!(x, "<c:order val=\"{}\"/>", o.clamp(2, 6));
        }
        TrendlineKind::MovingAverage(p) => {
            let _ = write!(x, "<c:period val=\"{}\"/>", p.clamp(2, 255));
        }
        _ => {}
    }
    for (tag, v) in [("forward", t.forward), ("backward", t.backward)] {
        if v.is_finite() && v > 0.0 {
            let _ = write!(x, "<c:{tag} val=\"{}\"/>", num(v));
        }
    }
    if let Some(i) = t.intercept.filter(|i| i.is_finite()) {
        let _ = write!(x, "<c:intercept val=\"{}\"/>", num(i));
    }
    let _ = write!(x, "<c:dispRSqr val=\"{}\"/><c:dispEq val=\"{}\"/>", t.display_r_squared as u8, t.display_equation as u8);
    if t.display_r_squared || t.display_equation {
        x.push_str("<c:trendlineLbl><c:numFmt formatCode=\"General\" sourceLinked=\"0\"/></c:trendlineLbl>");
    }
    x.push_str("</c:trendline>");
    x
}

fn dlbls(show: bool) -> String {
    format!(
        "<c:dLbls><c:showLegendKey val=\"0\"/><c:showVal val=\"{}\"/><c:showCatName val=\"0\"/><c:showSerName val=\"0\"/><c:showPercent val=\"0\"/><c:showBubbleSize val=\"0\"/></c:dLbls>",
        show as u8
    )
}

fn group_xml(g: G, series: &[(usize, &Series)], ch: &Chart, axes: (u32, u32), wb: &Workbook, si: usize) -> String {
    let sers: String = series.iter().map(|(i, s)| series_xml(s, *i, g, wb, si)).collect();
    let dl = dlbls(ch.data_labels);
    let ax = format!("<c:axId val=\"{}\"/><c:axId val=\"{}\"/>", axes.0, axes.1);
    match g {
        G::Bar { bar, grouping } => {
            let overlap = if grouping == "clustered" { String::new() } else { "<c:overlap val=\"100\"/>".into() };
            format!(
                "<c:barChart><c:barDir val=\"{}\"/><c:grouping val=\"{grouping}\"/><c:varyColors val=\"0\"/>{sers}{dl}<c:gapWidth val=\"150\"/>{overlap}{ax}</c:barChart>",
                if bar { "bar" } else { "col" }
            )
        }
        G::Line { stacked, .. } => format!(
            "<c:lineChart><c:grouping val=\"{}\"/><c:varyColors val=\"0\"/>{sers}{dl}<c:marker val=\"1\"/>{ax}</c:lineChart>",
            if stacked { "stacked" } else { "standard" }
        ),
        G::Pie => format!("<c:pieChart><c:varyColors val=\"1\"/>{sers}{dl}<c:firstSliceAng val=\"{}\"/></c:pieChart>", ch.first_slice_angle % 360),
        G::OfPie { bar } => of_pie_xml(bar, &sers, &dl, &ch.of_pie.clone().unwrap_or_default()),
        G::Doughnut => format!(
            "<c:doughnutChart><c:varyColors val=\"1\"/>{sers}{dl}<c:firstSliceAng val=\"{}\"/><c:holeSize val=\"{}\"/></c:doughnutChart>",
            ch.first_slice_angle % 360,
            ch.hole_size.clamp(10, 90)
        ),
        G::Area { grouping } => format!("<c:areaChart><c:grouping val=\"{grouping}\"/><c:varyColors val=\"0\"/>{sers}{dl}{ax}</c:areaChart>"),
        G::Scatter { .. } => format!("<c:scatterChart><c:scatterStyle val=\"lineMarker\"/><c:varyColors val=\"0\"/>{sers}{dl}{ax}</c:scatterChart>"),
        G::Bubble => {
            format!("<c:bubbleChart><c:varyColors val=\"0\"/>{sers}{dl}<c:bubbleScale val=\"100\"/><c:showNegBubbles val=\"0\"/>{ax}</c:bubbleChart>")
        }
        G::Radar => format!("<c:radarChart><c:radarStyle val=\"marker\"/><c:varyColors val=\"0\"/>{sers}{dl}{ax}</c:radarChart>"),
        G::Stock => format!("<c:stockChart>{sers}{dl}<c:hiLowLines/>{ax}</c:stockChart>"),
        // CT_SurfaceChart / CT_Surface3DChart: wireframe, ser*, bandFmts?, then the category,
        // value and series axis ids; no varyColors and no data labels.
        G::Surface { three_d, wireframe } => {
            let tag = if three_d { "surface3DChart" } else { "surfaceChart" };
            format!("<c:{tag}><c:wireframe val=\"{}\"/>{sers}{ax}<c:axId val=\"{SER_AX}\"/></c:{tag}>", wireframe as u8)
        }
    }
}

/// The series (depth) axis id of a surface chart; the category and value axes are 1 and 2.
const SER_AX: u32 = 5;

/// `c:view3D` as Excel writes it for each surface type: rotX, rotY, rAngAx, perspective.
fn surface_view_xml(three_d: bool) -> String {
    let (rx, ry, persp) = if three_d { (15, 20, 30) } else { (90, 0, 0) };
    format!("<c:view3D><c:rotX val=\"{rx}\"/><c:rotY val=\"{ry}\"/><c:rAngAx val=\"0\"/><c:perspective val=\"{persp}\"/></c:view3D>")
}

/// CT_SerAx: the depth axis of a surface chart, one tick per series.
fn ser_axis_xml() -> String {
    format!(
        "<c:serAx><c:axId val=\"{SER_AX}\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"b\"/><c:majorTickMark val=\"out\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"2\"/><c:crosses val=\"autoZero\"/></c:serAx>"
    )
}

/// `c:ofPieChart` in CT_OfPieChart order: ofPieType, varyColors, ser*, dLbls, gapWidth,
/// splitType, splitPos, custSplit, secondPieSize, serLines. The split is written only when it
/// isn't automatic.
fn of_pie_xml(bar: bool, sers: &str, dl: &str, o: &OfPie) -> String {
    let mut x = format!(
        "<c:ofPieChart><c:ofPieType val=\"{}\"/><c:varyColors val=\"1\"/>{sers}{dl}<c:gapWidth val=\"{}\"/>",
        if bar { "bar" } else { "pie" },
        o.gap_width.min(500)
    );
    let at = |x: &mut String, ty: &str, v: f64| {
        let v = if v.is_finite() { v } else { 0.0 };
        let _ = write!(x, "<c:splitType val=\"{ty}\"/><c:splitPos val=\"{}\"/>", num(v));
    };
    match &o.split {
        OfPieSplit::Auto => {}
        OfPieSplit::Position(v) => at(&mut x, "pos", *v),
        OfPieSplit::Value(v) => at(&mut x, "val", *v),
        OfPieSplit::Percent(v) => at(&mut x, "percent", *v),
        OfPieSplit::Custom(pts) => {
            x.push_str("<c:splitType val=\"cust\"/><c:custSplit>");
            for p in pts.iter().take(MAX_CUSTOM_SPLIT) {
                let _ = write!(x, "<c:secondPiePt val=\"{p}\"/>");
            }
            x.push_str("</c:custSplit>");
        }
    }
    let _ = write!(x, "<c:secondPieSize val=\"{}\"/>", o.second_size.clamp(5, 200));
    if o.series_lines {
        x.push_str("<c:serLines/>");
    }
    x.push_str("</c:ofPieChart>");
    x
}

/// Whether a chart of group `g` with 3-D settings `v` has its series in depth (a series axis).
/// A 3-D line always has one; columns, bars and areas only in standard grouping.
fn in_depth(g: G, v: &Chart3d) -> bool {
    match g {
        G::Line { .. } => true,
        G::Bar { grouping, .. } => v.series_in_depth && grouping == "clustered",
        G::Area { grouping } => v.series_in_depth && grouping == "standard",
        _ => false,
    }
}

/// A 3-D group (`c:bar3DChart`, `c:line3DChart`, `c:area3DChart`, `c:pie3DChart`) with its children
/// in schema order; `None` for groups that have no 3-D form here. A third axis id of `0` means
/// "no series axis"; the series axis is id 3.
fn group_3d_xml(g: G, v: &Chart3d, series: &[(usize, &Series)], ch: &Chart, wb: &Workbook, si: usize) -> Option<String> {
    let sers: String = series.iter().map(|(i, s)| series_xml(s, *i, g, wb, si)).collect();
    let dl = dlbls(ch.data_labels);
    let depth = in_depth(g, v);
    let ax = format!("<c:axId val=\"1\"/><c:axId val=\"2\"/><c:axId val=\"{}\"/>", if depth { 3 } else { 0 });
    let gap_depth = v.gap_depth.filter(|d| *d <= 500).map(|d| format!("<c:gapDepth val=\"{d}\"/>")).unwrap_or_default();
    Some(match g {
        G::Bar { bar, grouping } => {
            let shape = v.shape.as_deref().filter(|s| SHAPES.contains(s)).map(|s| format!("<c:shape val=\"{s}\"/>")).unwrap_or_default();
            format!(
                "<c:bar3DChart><c:barDir val=\"{}\"/><c:grouping val=\"{}\"/><c:varyColors val=\"0\"/>{sers}{dl}<c:gapWidth val=\"150\"/>{gap_depth}{shape}{ax}</c:bar3DChart>",
                if bar { "bar" } else { "col" },
                if depth { "standard" } else { grouping }
            )
        }
        G::Line { stacked, .. } => format!(
            "<c:line3DChart><c:grouping val=\"{}\"/><c:varyColors val=\"0\"/>{sers}{dl}{gap_depth}{ax}</c:line3DChart>",
            if stacked { "stacked" } else { "standard" }
        ),
        G::Area { grouping } => {
            format!("<c:area3DChart><c:grouping val=\"{grouping}\"/><c:varyColors val=\"0\"/>{sers}{dl}{gap_depth}{ax}</c:area3DChart>")
        }
        G::Pie => format!("<c:pie3DChart><c:varyColors val=\"1\"/>{sers}{dl}</c:pie3DChart>"),
        _ => return None,
    })
}

/// `c:view3D` with exactly the values the chart has, in schema order (rotX, hPercent, rotY,
/// depthPercent, rAngAx, perspective), then a floor and walls without thickness.
fn view_3d_xml(v: &Chart3d) -> String {
    let mut s = String::from("<c:view3D>");
    if let Some(x) = v.rot_x.filter(|x| (-90..=90).contains(x)) {
        let _ = write!(s, "<c:rotX val=\"{x}\"/>");
    }
    if let Some(h) = v.h_percent.filter(|h| (5..=500).contains(h)) {
        let _ = write!(s, "<c:hPercent val=\"{h}\"/>");
    }
    if let Some(y) = v.rot_y.filter(|y| *y <= 360) {
        let _ = write!(s, "<c:rotY val=\"{y}\"/>");
    }
    if let Some(d) = v.depth_percent.filter(|d| (20..=2000).contains(d)) {
        let _ = write!(s, "<c:depthPercent val=\"{d}\"/>");
    }
    if let Some(r) = v.right_angle_axes {
        let _ = write!(s, "<c:rAngAx val=\"{}\"/>", r as u8);
    }
    if let Some(p) = v.perspective.filter(|p| *p <= 240) {
        let _ = write!(s, "<c:perspective val=\"{p}\"/>");
    }
    s.push_str("</c:view3D>");
    for wall in ["floor", "sideWall", "backWall"] {
        let _ = write!(s, "<c:{wall}><c:thickness val=\"0\"/></c:{wall}>");
    }
    s
}

/// The series axis of a chart whose series stand in depth: id 3, crossing the value axis (id 2).
const SER_AX_3D: &str = "<c:serAx><c:axId val=\"3\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"b\"/><c:majorTickMark val=\"out\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"2\"/><c:crosses val=\"autoZero\"/></c:serAx>";

fn title_xml(t: &str) -> String {
    let mut s = String::from("<c:title><c:tx><c:rich><a:bodyPr/><a:lstStyle/>");
    for line in t.split('\n') {
        let _ = write!(s, "<a:p><a:pPr><a:defRPr/></a:pPr><a:r><a:t>{}</a:t></a:r></a:p>", esc(line));
    }
    s.push_str("</c:rich></c:tx><c:overlay val=\"0\"/></c:title>");
    s
}

struct Ax<'a> {
    cat: bool,
    id: u32,
    cross: u32,
    pos: &'a str,
    delete: bool,
    grid: bool,
    title: Option<&'a str>,
    crosses_max: bool,
    /// Where this axis crosses `cross` (a category axis's; a value axis uses `scale.crosses`).
    crosses: AxisCrosses,
    scale: AxisScale,
    /// `c:crossBetween val="midCat"` (surface charts) instead of `between`.
    mid_cat: bool,
}

fn axis_xml(a: &Ax<'_>) -> String {
    let mut s = format!(
        "<c:{0}><c:axId val=\"{1}\"/><c:scaling>{4}</c:scaling><c:delete val=\"{2}\"/><c:axPos val=\"{3}\"/>",
        if a.cat { "catAx" } else { "valAx" },
        a.id,
        a.delete as u8,
        a.pos,
        scaling_xml(&a.scale, a.cat),
    );
    if a.grid {
        s.push_str("<c:majorGridlines/>");
    }
    if let Some(t) = a.title {
        s.push_str(&title_xml(t));
    }
    let _ = write!(
        s,
        "<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/><c:majorTickMark val=\"out\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"{}\"/>",
        a.cross,
    );
    // Exactly one of crosses / crossesAt; a secondary axis always crosses at the maximum.
    let crosses = if a.cat { a.crosses } else { a.scale.crosses };
    match crosses {
        _ if a.crosses_max => s.push_str("<c:crosses val=\"max\"/>"),
        AxisCrosses::At(v) if v.is_finite() => {
            let _ = write!(s, "<c:crossesAt val=\"{}\"/>", crate::xml::num(v));
        }
        AxisCrosses::Min => s.push_str("<c:crosses val=\"min\"/>"),
        AxisCrosses::Max => s.push_str("<c:crosses val=\"max\"/>"),
        AxisCrosses::AutoZero | AxisCrosses::At(_) => s.push_str("<c:crosses val=\"autoZero\"/>"),
    }
    if a.cat {
        s.push_str("<c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/><c:noMultiLvlLbl val=\"0\"/></c:catAx>");
    } else {
        let _ = write!(s, "<c:crossBetween val=\"{}\"/>", if a.mid_cat { "midCat" } else { "between" });
        // Schema order: crossBetween, majorUnit, minorUnit.
        for (tag, v) in [("majorUnit", a.scale.major_unit), ("minorUnit", a.scale.minor_unit)] {
            if let Some(v) = v.filter(|v| v.is_finite() && *v > 0.0) {
                let _ = write!(s, "<c:{tag} val=\"{}\"/>", crate::xml::num(v));
            }
        }
        if let Some(d) = &a.scale.display_units {
            s.push_str(&display_units_xml(d));
        }
        s.push_str("</c:valAx>");
    }
    s
}

/// `c:dispUnits` (after `c:minorUnit` in CT_ValAx); empty for an unusable custom unit.
fn display_units_xml(d: &DisplayUnits) -> String {
    let unit = match d.unit {
        DisplayUnit::Custom(f) if f.is_finite() && f > 0.0 => format!("<c:custUnit val=\"{}\"/>", crate::xml::num(f)),
        DisplayUnit::Custom(_) => return String::new(),
        u => match DisplayUnit::BUILT_IN.iter().find(|(b, _)| *b == u) {
            Some((_, name)) => format!("<c:builtInUnit val=\"{name}\"/>"),
            None => return String::new(),
        },
    };
    format!("<c:dispUnits>{unit}{}</c:dispUnits>", if d.show_label { "<c:dispUnitsLbl/>" } else { "" })
}

/// `c:scaling`'s children in schema order: logBase, orientation, max, min. A category axis
/// keeps only its orientation.
fn scaling_xml(sc: &AxisScale, cat: bool) -> String {
    let mut s = String::new();
    let finite = |v: Option<f64>| v.filter(|v| v.is_finite());
    if !cat && let Some(b) = finite(sc.log_base).filter(|b| (2.0..=1000.0).contains(b)) {
        let _ = write!(s, "<c:logBase val=\"{}\"/>", crate::xml::num(b));
    }
    let _ = write!(s, "<c:orientation val=\"{}\"/>", if sc.reverse { "maxMin" } else { "minMax" });
    if !cat {
        for (tag, v) in [("max", finite(sc.max)), ("min", finite(sc.min))] {
            if let Some(v) = v {
                let _ = write!(s, "<c:{tag} val=\"{}\"/>", crate::xml::num(v));
            }
        }
    }
    s
}

/// A PivotChart link without the workbook in brackets: `[Book.xlsx]Sheet2!PT` → `Sheet2!PT`.
fn strip_book(link: &str) -> &str {
    link.strip_prefix('[').and_then(|r| r.split_once(']')).map_or(link, |(_, rest)| rest)
}

/// Chart part XML for a model chart on sheet `si`.
pub fn write_chart(wb: &Workbook, si: usize, ch: &Chart, book_name: &str) -> String {
    let mut s = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><c:roundedCorners val=\"0\"/>",
    );
    if (1..=48).contains(&ch.style) {
        let _ = write!(s, "<c:style val=\"{}\"/>", ch.style);
    }
    // A PivotChart names its PivotTable behind the workbook in brackets (`[Book.xlsx]Sheet2!PivotTable1`);
    // without the brackets Excel shows an ordinary chart.
    if let Some(link) = ch.pivot.as_deref().map(str::trim).filter(|l| !l.is_empty()) {
        let link = strip_book(link);
        let _ = write!(s, "<c:pivotSource><c:name>[{}]{}</c:name><c:fmtId val=\"0\"/></c:pivotSource>", esc(book_name), esc(link));
    }
    s.push_str("<c:chart>");
    match &ch.title {
        Some(t) if !t.is_empty() => {
            s.push_str(&title_xml(t));
            s.push_str("<c:autoTitleDeleted val=\"0\"/>");
        }
        _ => s.push_str("<c:autoTitleDeleted val=\"1\"/>"),
    }
    // `c:view3D` sits in `c:chart` before `c:plotArea` (CT_Chart).
    let surface = match group_for(ch.kind) {
        G::Surface { three_d, .. } if ch.kind != ChartKind::Combo => Some(three_d),
        _ => None,
    };
    if let Some(three_d) = surface {
        s.push_str(&surface_view_xml(three_d));
    }
    // A 3-D chart: one 3-D group of a kind that has a 3-D form (combo charts stay flat).
    let three_d = ch
        .view_3d
        .as_ref()
        .filter(|_| ch.kind != ChartKind::Combo)
        .filter(|_| matches!(group_for(ch.kind), G::Bar { .. } | G::Line { .. } | G::Area { .. } | G::Pie));
    if let Some(v) = three_d {
        s.push_str(&view_3d_xml(v));
    }
    s.push_str("<c:plotArea><c:layout/>");
    let all: Vec<(usize, &Series)> = ch.series.iter().enumerate().collect();
    let primary_g;
    let mut secondary: Option<G> = None;
    if ch.kind == ChartKind::Combo {
        // Series without a kind of their own: the first is a column, the others lines (as drawn).
        let kind = |i: usize, x: &Series| x.kind.filter(|k| *k != ChartKind::Combo).or((i > 0).then_some(ChartKind::Line));
        let is_line = |k: Option<ChartKind>| matches!(k.map(group_for), Some(G::Line { .. } | G::Area { .. } | G::Radar | G::Scatter { .. }));
        let bars: Vec<(usize, &Series)> = all.iter().filter(|(i, x)| !x.secondary && !is_line(kind(*i, x))).copied().collect();
        let lines: Vec<(usize, &Series)> = all.iter().filter(|(i, x)| !x.secondary && is_line(kind(*i, x))).copied().collect();
        let sec: Vec<(usize, &Series)> = all.iter().filter(|(_, x)| x.secondary).copied().collect();
        // Pie and doughnut groups only: one axis-less group per kind, in the order the kinds first appear.
        let round = |i: usize, x: &Series| kind(i, x).map(group_for).filter(|g| matches!(g, G::Pie | G::Doughnut));
        let mut rounds: Vec<G> = Vec::new();
        for (i, x) in &all {
            if let Some(g) = round(*i, x)
                && !rounds.contains(&g)
            {
                rounds.push(g);
            }
        }
        let all_round = !all.is_empty() && sec.is_empty() && all.iter().all(|(i, x)| round(*i, x).is_some());
        primary_g = match rounds.first() {
            Some(g) if all_round => *g,
            _ => G::Bar { bar: false, grouping: "clustered" },
        };
        if all_round {
            for g in &rounds {
                let group: Vec<(usize, &Series)> = all.iter().filter(|(i, x)| round(*i, x) == Some(*g)).copied().collect();
                s.push_str(&group_xml(*g, &group, ch, (1, 2), wb, si));
            }
        } else if !bars.is_empty() || (lines.is_empty() && sec.is_empty()) {
            s.push_str(&group_xml(primary_g, &bars, ch, (1, 2), wb, si));
        }
        if !lines.is_empty() {
            s.push_str(&group_xml(G::Line { stacked: false, markers: false }, &lines, ch, (1, 2), wb, si));
        }
        if !sec.is_empty() {
            let g = match sec.first().and_then(|(_, x)| x.kind).map(group_for) {
                Some(b @ G::Bar { .. }) => b,
                _ => G::Line { stacked: false, markers: false },
            };
            s.push_str(&group_xml(g, &sec, ch, (3, 4), wb, si));
            secondary = Some(g);
        }
    } else {
        primary_g = match group_for(ch.kind) {
            // A stock chart has 3 or 4 series (high-low-close, open-high-low-close).
            G::Stock if !(3..=4).contains(&ch.series.len()) => G::Line { stacked: false, markers: false },
            g => g,
        };
        match three_d.and_then(|v| group_3d_xml(primary_g, v, &all, ch, wb, si)) {
            Some(x) => s.push_str(&x),
            None => s.push_str(&group_xml(primary_g, &all, ch, (1, 2), wb, si)),
        }
    }
    match primary_g {
        G::Pie | G::OfPie { .. } | G::Doughnut => {}
        G::Scatter { .. } | G::Bubble => {
            s.push_str(&axis_xml(&Ax {
                cat: false,
                id: 1,
                cross: 2,
                pos: "b",
                delete: false,
                grid: false,
                title: ch.x_title.as_deref(),
                crosses_max: false,
                crosses: AxisCrosses::AutoZero,
                scale: ch.scale.x,
                mid_cat: false,
            }));
            s.push_str(&axis_xml(&Ax {
                cat: false,
                id: 2,
                cross: 1,
                pos: "l",
                delete: false,
                grid: ch.gridlines,
                title: ch.y_title.as_deref(),
                crosses_max: false,
                crosses: AxisCrosses::AutoZero,
                scale: ch.scale.y,
                mid_cat: false,
            }));
        }
        G::Bar { bar: true, .. } => {
            s.push_str(&axis_xml(&Ax {
                cat: true,
                id: 1,
                cross: 2,
                pos: "l",
                delete: false,
                grid: false,
                title: ch.x_title.as_deref(),
                crosses_max: false,
                crosses: ch.scale.cat_crosses,
                scale: AxisScale::default(),
                mid_cat: false,
            }));
            s.push_str(&axis_xml(&Ax {
                cat: false,
                id: 2,
                cross: 1,
                pos: "b",
                delete: false,
                grid: ch.gridlines,
                title: ch.y_title.as_deref(),
                crosses_max: false,
                crosses: AxisCrosses::AutoZero,
                scale: ch.scale.y,
                mid_cat: false,
            }));
        }
        _ => {
            s.push_str(&axis_xml(&Ax {
                cat: true,
                id: 1,
                cross: 2,
                pos: "b",
                delete: false,
                grid: false,
                title: ch.x_title.as_deref(),
                crosses_max: false,
                crosses: ch.scale.cat_crosses,
                scale: AxisScale::default(),
                mid_cat: false,
            }));
            s.push_str(&axis_xml(&Ax {
                cat: false,
                id: 2,
                cross: 1,
                pos: "l",
                delete: false,
                grid: ch.gridlines,
                title: ch.y_title.as_deref(),
                crosses_max: false,
                crosses: AxisCrosses::AutoZero,
                scale: ch.scale.y,
                mid_cat: surface.is_some(),
            }));
            if surface.is_some() {
                s.push_str(&ser_axis_xml());
            }
        }
    }
    if three_d.is_some_and(|v| in_depth(primary_g, v)) {
        s.push_str(SER_AX_3D);
    }
    if secondary.is_some() {
        s.push_str(&axis_xml(&Ax {
            cat: true,
            id: 3,
            cross: 4,
            pos: "b",
            delete: true,
            grid: false,
            title: None,
            crosses_max: false,
            crosses: AxisCrosses::AutoZero,
            scale: AxisScale::default(),
            mid_cat: false,
        }));
        s.push_str(&axis_xml(&Ax {
            cat: false,
            id: 4,
            cross: 3,
            pos: "r",
            delete: false,
            grid: false,
            title: None,
            crosses_max: true,
            crosses: AxisCrosses::AutoZero,
            scale: AxisScale::default(),
            mid_cat: false,
        }));
    }
    if let Some(t) = &ch.data_table {
        let _ = write!(
            s,
            "<c:dTable><c:showHorzBorder val=\"{}\"/><c:showVertBorder val=\"{}\"/><c:showOutline val=\"{}\"/><c:showKeys val=\"{}\"/></c:dTable>",
            u8::from(t.horizontal_border),
            u8::from(t.vertical_border),
            u8::from(t.outline),
            u8::from(t.show_keys)
        );
    }
    s.push_str("</c:plotArea>");
    let pos = match ch.legend {
        LegendPos::None => None,
        LegendPos::Bottom => Some("b"),
        LegendPos::Top => Some("t"),
        LegendPos::Left => Some("l"),
        LegendPos::Right => Some("r"),
    };
    if let Some(p) = pos {
        let _ = write!(s, "<c:legend><c:legendPos val=\"{p}\"/><c:overlay val=\"0\"/></c:legend>");
    }
    s.push_str("<c:plotVisOnly val=\"1\"/><c:dispBlanksAs val=\"gap\"/></c:chart></c:chartSpace>");
    s
}
