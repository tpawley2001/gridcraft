//! Insert tab: tables, charts, sparklines, pictures, shapes, hyperlinks, comments and notes,
//! conditional formatting (Home › Styles) and symbols.

use gridcraft_core::{CellRef, MAX_COLS, MAX_ROWS, RangeRef};
use gridcraft_formula::adjust::{Axis, Edit};
use gridcraft_model::{
    Anchor, BorderLine, BorderStyle, Borders, Cell, CellPicture, CfOperator, CfRule, CfValueKind, Chart, ChartKind, Color, Comment, CondFormat, Fill,
    Formula, Hyperlink, Image, LegendPos, ObjKind, ObjRef, Series, Shape, ShapeKind, Sheet, Sparkline, SparklineKind, Style, Table, TableColumn,
    TotalsFn, Underline, Workbook,
};
use serde_json::{Value as Json, json};

use super::sheet::{rewrite, shift_cell_features};
use super::{
    CommandSpec, EngineError, PROTECTED, Result, Session, always, anchor_mode_name, anchor_mode_param, bad, bool_param, cell_param, cell_param_on,
    cmd, edit, f64_param, has_doc, locked_cell_error, ok, split_sheet, str_param, target_range, target_ranges, target_sheet, u32_param,
};
use crate::selection::current_region;
mod chart;
mod cond_format;
mod links;
mod objects;
mod shape_gallery;
mod table;

pub(super) use chart::update_data_table;
pub use chart::{build_chart, chart_kind, recommend_charts, scatter_x_from_first, series_from_range};
use chart::{chart_delete, chart_set, chart_switch, insert_chart};
use cond_format::{add_cf, clear_cf, insert_icon, manage_cf};
use links::{comment, delete_comment, insert_link, open_link, reply_comment, resolve_comment, show_note};
use objects::{delete_object, insert_picture, insert_shape, insert_sparkline, move_object, set_anchor_mode, shape_set_text};
use table::{format_as_table, insert_table, table_convert, table_flag, table_rename, table_style, table_total_fn};
pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "insert.table",
            "Table",
            ["Insert", "Tables"],
            Some("Cmd+T"),
            "{range?, header?: true, style?: \"TableStyleMedium2\", name?}",
            has_doc,
            insert_table
        ),
        cmd!(
            "home.formatAsTable",
            "Format as Table",
            ["Home", "Styles"],
            None,
            "{range?, style: \"TableStyleMedium2\", header?} (inside a table: restyles it)",
            has_doc,
            format_as_table
        ),
        cmd!("table.totalRow", "Total Row", ["Table", "Table Style Options"], None, "{table?, on?: bool}", has_doc, |s, p| table_flag(
            s, p, "totals"
        )),
        cmd!("table.bandedRows", "Banded Rows", ["Table", "Table Style Options"], None, "{table?, on?}", has_doc, |s, p| table_flag(
            s,
            p,
            "bandedRows"
        )),
        cmd!("table.bandedColumns", "Banded Columns", ["Table", "Table Style Options"], None, "{table?, on?}", has_doc, |s, p| table_flag(
            s,
            p,
            "bandedCols"
        )),
        cmd!("table.headerRow", "Header Row", ["Table", "Table Style Options"], None, "{table?, on?}", has_doc, |s, p| table_flag(s, p, "header")),
        cmd!("table.firstColumn", "First Column", ["Table", "Table Style Options"], None, "{table?, on?}", has_doc, |s, p| table_flag(
            s, p, "firstCol"
        )),
        cmd!("table.lastColumn", "Last Column", ["Table", "Table Style Options"], None, "{table?, on?}", has_doc, |s, p| table_flag(s, p, "lastCol")),
        cmd!("table.style", "Table Styles", ["Table", "Table Styles"], None, "{table?, style}", has_doc, table_style),
        cmd!("table.rename", "Table Name", ["Table", "Properties"], None, "{table?, name}", has_doc, table_rename),
        cmd!("table.convertToRange", "Convert to Range", ["Table", "Tools"], None, "{table?}", has_doc, table_convert),
        cmd!(
            "table.totalFunction",
            "Totals Function",
            [],
            None,
            "{table?, column: \"Sales\", function: sum|average|count|countNums|max|min|stdDev|var|none}",
            has_doc,
            table_total_fn
        ),
        cmd!(
            "insert.chart",
            "Insert Chart",
            ["Insert", "Charts"],
            None,
            "{range?, type?: column|bar|line|pie|doughnut|area|scatter|bubble|radar|combo|histogram|waterfall|funnel|treemap|sunburst|boxWhisker|stock|surface|surfaceWireframe|contour|contourWireframe, subtype?: clustered|stacked|stacked100|markers|wireframe, title?, at?: \"H2\", width?, height?}",
            has_doc,
            insert_chart
        ),
        cmd!(
            "insert.recommendedCharts",
            "Recommended Charts",
            ["Insert", "Charts"],
            None,
            "{range?, type?, subtype?} (no type: the first of insert.chartRecommendations; the ribbon button opens the dialog)",
            has_doc,
            |s, p| insert_chart(s, &if p.get("type").is_some() { p.clone() } else { with(p, "type", json!("auto")) })
        ),
        cmd!(
            query "insert.chartRecommendations",
            "Chart Recommendations",
            [],
            None,
            "{range?} → [{type, subtype, label}] chart types that suit the data's shape, best first",
            has_doc,
            |s, p| Ok(Json::Array(recommend_charts(s, p)?.into_iter().map(|(t, sub, label)| json!({"type": t, "subtype": sub, "label": label})).collect()))
        ),
        cmd!(
            "chart.set",
            "Chart Properties",
            ["Chart Design"],
            None,
            "{chart?: id, type?, subtype?, title?, legend?: none|bottom|top|left|right, dataLabels?, gridlines?, dataTable?: bool, dataTableKeys?: bool, dataTableHorizontalBorder?: bool, dataTableVerticalBorder?: bool, dataTableOutline?: bool (data tables: column/bar/line/area and compatible combo charts), quartiles?: exclusive|inclusive, style?, holeSize?, firstSliceAngle?, xTitle?, yTitle?, at?, width?, height?, dx?, dy?}",
            has_doc,
            chart_set
        ),
        cmd!("chart.switchRowColumn", "Switch Row/Column", ["Chart Design", "Data"], None, "{chart?}", has_doc, chart_switch),
        cmd!("chart.delete", "Delete Chart", [], None, "{chart?}", has_doc, chart_delete),
        cmd!(
            "insert.sparkline",
            "Sparklines",
            ["Insert", "Sparklines"],
            None,
            "{range: \"B2:M2\", location: \"N2\", type?: line|column|winLoss, markers?}",
            has_doc,
            insert_sparkline
        ),
        cmd!(
            "insert.picture",
            "Picture",
            ["Insert", "Illustrations"],
            None,
            "{path? | base64?: \"...\", mime?, at?: \"B2\", width?, height?, alt?, mode?: moveOnly|moveAndSize|absolute, placement?: cell|overCells}",
            has_doc,
            insert_picture
        ),
        cmd!(
            "insert.shape",
            "Shapes",
            ["Insert", "Illustrations"],
            None,
            "{kind?: rectangle|roundedRectangle|ellipse|triangle|line|arrow|textBox, preset?: a DrawingML preset name the renderer draws (\"star5\", \"chevron\"; see insert.shapeGallery), not both, at?, width?, height?, text?, fill?, line?}",
            has_doc,
            insert_shape
        ),
        cmd!(
            query "insert.shapeGallery",
            "Shapes Gallery",
            ["Insert", "Illustrations"],
            None,
            "{} → {categories: [{name, shapes: [{kind | preset, label}]}]} every shape insert.shape inserts, by gallery category",
            always,
            shape_gallery::shape_gallery
        ),
        cmd!(
            "insert.icons",
            "Icons",
            ["Insert", "Illustrations"],
            None,
            "{name: chart|table|sum|filter|lock|comment|picture|book|folder|search|calc|check|… , at?, size?: 48, color?: hex}",
            has_doc,
            insert_icon
        ),
        cmd!("insert.textBox", "Text Box", ["Insert", "Text"], None, "{at?, width?, height?, text?}", has_doc, |s, p| insert_shape(
            s,
            &with(p, "kind", json!("textBox"))
        )),
        cmd!("shape.setText", "Edit Text Box", [], None, "{id: textBox id, text: string}", has_doc, shape_set_text),
        cmd!("object.delete", "Delete Object", [], None, "{kind: chart|image|shape|slicer|equation|control, id}", has_doc, delete_object),
        cmd!(
            "object.move",
            "Move Object",
            [],
            None,
            "{kind: chart|image|shape|slicer|equation|control, id, at?: \"C3\", dx?, dy?, width?, height? (each at most 10,000 points, or the object's current size when a file made it larger)}",
            has_doc,
            move_object
        ),
        cmd!(
            "image.setLockAspectRatio",
            "Lock aspect ratio",
            ["Picture Format", "Size"],
            None,
            "{id, locked: bool}",
            has_doc,
            image_set_lock_aspect_ratio
        ),
        cmd!(
            "image.setSize",
            "Size (Picture Format > Size)",
            ["Picture Format", "Size"],
            None,
            "{id, width?, height?} (points; one dimension keeps proportions when aspect ratio is locked; both dimensions override the lock)",
            has_doc,
            image_set_size
        ),
        cmd!(
            "object.setAnchorMode",
            "Object Properties",
            [],
            None,
            "{kind: chart|image|shape|slicer|equation|control, id, mode: moveAndSize|moveOnly|absolute}",
            has_doc,
            set_anchor_mode
        ),
        cmd!(
            "insert.link",
            "Link",
            ["Insert", "Links"],
            Some("Cmd+K"),
            "{cell?, target: \"https://…\" | \"Book.xlsx\" | \"Sheet2!A1\" | \"mailto:…\", text?, tooltip?, remove?: bool}",
            has_doc,
            insert_link
        ),
        cmd!(
            "link.open",
            "Open Hyperlink",
            [],
            None,
            "{url?: \"https://…\" | \"mailto:…\" | \"Sheet2!A1\", cell?} (default: the cell's link); a reference selects; a web or e-mail address is an error unless the host allows it: the user opens those by clicking the link and confirming; any scheme but http, https and mailto is always an error",
            has_doc,
            open_link
        ),
        cmd!("review.newNote", "New Note", ["Review", "Notes"], Some("Shift+F2"), "{cell?, text, author?}", has_doc, |s, p| comment(s, p, false)),
        cmd!("review.newComment", "New Comment", ["Review", "Comments"], Some("Cmd+Shift+F2"), "{cell?, text, author?}", has_doc, |s, p| comment(
            s, p, true
        )),
        cmd!("review.replyComment", "Reply", [], None, "{cell?, text, author?}", has_doc, reply_comment),
        cmd!("review.resolveComment", "Resolve Thread", [], None, "{cell?, resolved?: true}", has_doc, resolve_comment),
        cmd!("review.deleteComment", "Delete Comment", ["Review", "Comments"], None, "{cell?}", has_doc, delete_comment),
        cmd!("review.showNote", "Show/Hide Note", ["Review", "Notes"], None, "{cell?}", has_doc, show_note),
        cmd!(
            "insert.symbol",
            "Symbol",
            ["Insert", "Symbols"],
            None,
            "{char: \"€\"} (inserted at the end of the active cell's text)",
            has_doc,
            insert_symbol
        ),
        cmd!(
            "home.conditionalFormat",
            "Conditional Formatting",
            ["Home", "Styles"],
            None,
            "{range?, rule: {type: cellIs|expression|containsText|beginsWith|endsWith|blanks|noBlanks|errors|noErrors|duplicate|unique|top10|aboveAverage|timePeriod|colorScale|dataBar|iconSet, operator?, value?, value2?, formula?, text?, rank?, bottom?, percent?, below?, period?, colors?: [..], color?, set?, style?: {font?: {bold?, italic?, color?}, fill?: \"#FFC7CE\" | {pattern?, fg?, bg?}, borders?, num_fmt?} (colours \"#RRGGBB\") | preset?: lightRedFill|redText|yellowFill|greenFill|redBorder|lightRedFillDarkRedText…}; a text `value` is matched as text, or kept when already a quoted literal}",
            has_doc,
            add_cf
        ),
        cmd!("home.clearRules", "Clear Rules", ["Home", "Styles", "Conditional Formatting"], None, "{range?|sheet: true}", has_doc, clear_cf),
        cmd!(
            "home.manageRules",
            "Manage Rules…",
            ["Home", "Styles", "Conditional Formatting"],
            None,
            "{delete?: index, moveUp?: index, moveDown?: index}",
            has_doc,
            manage_cf
        ),
    ]
}

fn with(p: &Json, k: &str, v: Json) -> Json {
    let mut m = p.as_object().cloned().unwrap_or_default();
    m.insert(k.into(), v);
    Json::Object(m)
}

fn block(s: &Session, p: &Json) -> Result<RangeRef> {
    if p.get("range").is_some() {
        return target_range(s, p);
    }
    let d = s.doc()?;
    let sh = d.wb.active().ok_or(EngineError::NoDocument)?;
    let sel = d.selection.current();
    if !sel.is_single() {
        return Ok(sel);
    }
    Ok(current_region(sh, d.selection.active))
}
/// Largest offset (points) `object.move` accepts from the anchor cell.
const MAX_OBJECT_OFFSET: f64 = 100_000.0;

/// Largest width or height (points) `object.move` and `chart.set` give an object that is not
/// already larger: see [`size_cap`].
const MAX_OBJECT_SIZE: f64 = 10_000.0;

/// The most a command may make a dimension that is `current` now: [`MAX_OBJECT_SIZE`], or the
/// current size when a file made the object larger (the XLSX reader accepts anchors far past the
/// limit). Such an object keeps its size when it is moved and shrinks by what a resize asks, but
/// it never grows further, so hostile input is still capped. Never NaN: `f64::max` drops one.
fn size_cap(current: f32) -> f64 {
    MAX_OBJECT_SIZE.max(f64::from(current))
}

/// Resolve picture ids without letting an out-of-range integer name another picture.
fn image_param(s: &Session, p: &Json, cmd: &str) -> Result<(usize, u32)> {
    let id = p.get("id").and_then(Json::as_u64).and_then(|id| u32::try_from(id).ok()).ok_or_else(|| bad(cmd, "invalid picture `id`"))?;
    let d = s.doc()?;
    let sheet = d.wb.active_sheet();
    let sh = d.wb.sheet(sheet).ok_or_else(|| bad(cmd, "no such picture"))?;
    if !sh.images.iter().any(|image| image.id == id) {
        return Err(bad(cmd, "no such picture"));
    }
    if sh.is_protected() {
        return Err(EngineError::Other(PROTECTED.into()));
    }
    Ok((sheet, id))
}

fn image_set_lock_aspect_ratio(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "image.setLockAspectRatio";
    let (sheet, id) = image_param(s, p, CMD)?;
    let locked = bool_param(p, "locked").ok_or_else(|| bad(CMD, "`locked` must be a boolean"))?;
    edit(s, |cx| {
        let image = cx.sheet_mut(sheet)?.images.iter_mut().find(|image| image.id == id).ok_or_else(|| bad(CMD, "no such picture"))?;
        image.locks.get_or_insert_with(Default::default).no_change_aspect = locked;
        Ok(json!({"locked": locked}))
    })
}

fn image_set_size(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "image.setSize";
    let (sheet, id) = image_param(s, p, CMD)?;
    let dimension = |key| -> Result<Option<f64>> {
        match p.get(key) {
            None => Ok(None),
            Some(value) => value.as_f64().filter(|v| v.is_finite()).map(Some).ok_or_else(|| bad(CMD, format!("`{key}` must be a finite number"))),
        }
    };
    let width = dimension("width")?;
    let height = dimension("height")?;
    if width.is_none() && height.is_none() {
        return Err(bad(CMD, "provide `width` or `height`"));
    }
    edit(s, |cx| {
        let image = cx.sheet_mut(sheet)?.images.iter_mut().find(|image| image.id == id).ok_or_else(|| bad(CMD, "no such picture"))?;
        let old_w = f64::from(image.anchor.width);
        let old_h = f64::from(image.anchor.height);
        let cap_w = size_cap(image.anchor.width);
        let cap_h = size_cap(image.anchor.height);
        let mut w = width.map_or(old_w, |w| w.clamp(1.0, cap_w));
        let mut h = height.map_or(old_h, |h| h.clamp(0.0, cap_h));
        if image.aspect_locked() {
            match (width, height) {
                (Some(_), None) if old_w > 0.0 => h = old_h * (w / old_w),
                (None, Some(_)) if old_h > 0.0 => w = old_w * (h / old_h),
                _ => {}
            }
            // A proportional resize must shrink both dimensions when either reaches its cap.
            if width.is_some() != height.is_some() {
                let scale = (cap_w / w).min(cap_h / h).min(1.0);
                w *= scale;
                h *= scale;
            }
        }
        image.anchor.width = w.clamp(1.0, cap_w) as f32;
        image.anchor.height = h.clamp(0.0, cap_h) as f32;
        Ok(json!({"width": image.anchor.width, "height": image.anchor.height}))
    })
}

fn insert_symbol(s: &mut Session, p: &Json) -> Result<Json> {
    let ch = str_param(p, "char").unwrap_or("").to_string();
    let d = s.doc()?;
    let at = d.selection.active;
    let cur = d.wb.active().and_then(|sh| sh.cell(at)).map(|c| c.input_text()).unwrap_or_default();
    s.execute("cell.set", json!({"cell": at.a1(), "input": format!("{cur}{ch}")}))
}

#[cfg(test)]
mod tests {
    #[test]
    fn picture_size_limits_zero_dimensions_and_lock_undo() {
        use gridcraft_model::AnchorMode;
        use serde_json::json;

        for (old_w, old_h, params, expected) in [
            (80.0, 40.0, json!({"width": 9000}), (9000.0, 4500.0)),
            (40.0, 80.0, json!({"width": 9000}), (5000.0, 10000.0)),
            (20000.0, 10000.0, json!({"height": 20000}), (20000.0, 10000.0)),
            (80.0, 0.0, json!({"height": 60}), (80.0, 60.0)),
            (0.0, 40.0, json!({"width": 50}), (50.0, 40.0)),
            (80.0, 40.0, json!({"width": -10}), (1.0, 0.5)),
        ] {
            let mut s = super::Session::new();
            s.new_workbook();
            let d = s.doc_mut().unwrap();
            let wb = std::sync::Arc::make_mut(&mut d.wb);
            let id = wb.next_object_id();
            wb.sheet_mut(0).unwrap().images.push(gridcraft_model::Image {
                locks: None,
                hidden: false,
                no_print: false,
                hyperlink: None,
                name: None,
                id,
                anchor: gridcraft_model::Anchor {
                    cell: gridcraft_core::CellRef::parse("A1").unwrap(),
                    dx: 0.0,
                    dy: 0.0,
                    width: old_w,
                    height: old_h,
                    mode: AnchorMode::MoveAndSize,
                },
                data: Vec::new().into(),
                mime: "image/png".into(),
                alt: String::new(),
                link: None,
                blip_ext: Vec::new(),
                border: None,
                camera: None,
            });
            let mut params = params;
            params["id"] = json!(id);
            let result = s.execute("image.setSize", params).unwrap();
            assert_eq!((result["width"].as_f64().unwrap(), result["height"].as_f64().unwrap()), expected);
            assert!(s.execute("image.setSize", json!({"id": id, "width": "bad"})).is_err());
            s.execute("image.setLockAspectRatio", json!({"id": id, "locked": false})).unwrap();
            s.execute("edit.undo", json!({})).unwrap();
            assert!(s.doc().unwrap().wb.sheet(0).unwrap().images.first().unwrap().aspect_locked());
        }
    }

    use super::*;

    #[test]
    fn chart_from_whole_columns_uses_the_rows_in_use() {
        // #3114: selecting two whole columns of data refused the chart as "range too large".
        let mut s = Session::new();
        s.new_workbook();
        let values: Vec<Vec<String>> = (1..=5).map(|i| vec![i.to_string(), (i * 10).to_string()]).collect();
        s.execute("range.setValues", json!({"range": "A1:B5", "values": values})).unwrap();
        s.execute("selection.set", json!({"range": "A:B"})).unwrap();
        s.execute("insert.chart", json!({"type": "line"})).unwrap();
        let d = s.doc().unwrap();
        let chart = d.wb.active().unwrap().charts.last().unwrap();
        let values: Vec<&str> = chart.series.iter().map(|x| x.values.as_str()).collect();
        assert_eq!(values, ["Sheet1!$A$1:$A$5", "Sheet1!$B$1:$B$5"]);
        // A block that really is too large still is.
        s.execute("cell.set", json!({"cell": "B600000", "input": "1"})).unwrap();
        assert!(s.execute("insert.chart", json!({"range": "A:B", "type": "line"})).is_err());
        // A bounded block keeps its extent, even where it runs past the data.
        s.execute("insert.chart", json!({"range": "A1:A20", "type": "line"})).unwrap();
        let d = s.doc().unwrap();
        let chart = d.wb.active().unwrap().charts.last().unwrap();
        let values: Vec<&str> = chart.series.iter().map(|x| x.values.as_str()).collect();
        assert_eq!(values, ["Sheet1!$A$1:$A$20"]);
        // A bounded block entirely below the data keeps its extent.
        s.execute("insert.chart", json!({"range": "C700000:C700010", "type": "line"})).unwrap();
        let d = s.doc().unwrap();
        let chart = d.wb.active().unwrap().charts.last().unwrap();
        let values: Vec<&str> = chart.series.iter().map(|x| x.values.as_str()).collect();
        assert_eq!(values, ["Sheet1!$C$700000:$C$700010"]);
    }

    #[test]
    fn chart_data_table_commands_undo_and_follow_live_values() {
        let mut s = Session::new();
        s.new_workbook();
        s.execute("range.setValues", json!({"range": "A1", "values": [["Quarter", "Sales"], ["Q1", 10], ["Q2", 20]]})).unwrap();
        s.execute("insert.chart", json!({"range": "A1:B3", "type": "column"})).unwrap();
        s.execute("chart.addElement", json!({"element": "dataTable", "dataTableKeys": false})).unwrap();
        let get = |s: &Session| s.active().unwrap().wb.active().unwrap().charts[0].clone();
        let table = get(&s).data_table.unwrap();
        assert!(!table.show_keys && table.horizontal_border && table.vertical_border && table.outline);
        s.execute("edit.undo", json!({})).unwrap();
        assert_eq!(get(&s).data_table, None);
        s.execute("edit.redo", json!({})).unwrap();
        assert_eq!(get(&s).data_table, Some(table));
        s.execute("chart.set", json!({"dataTableKeys": true, "dataTableHorizontalBorder": false})).unwrap();
        assert!(get(&s).data_table.as_ref().unwrap().show_keys);
        assert!(!get(&s).data_table.as_ref().unwrap().horizontal_border);
        s.execute("cell.set", json!({"cell": "B2", "input": "=12*3"})).unwrap();
        let d = s.active().unwrap();
        assert_eq!(gridcraft_chart::resolve(&d.wb, 0, &get(&s)).series[0].values[0], Some(36.0));
        s.execute("chart.addElement", json!({"element": "dataTable", "on": false})).unwrap();
        assert_eq!(get(&s).data_table, None);
        // Old JSON documents have no table field.
        let mut json = serde_json::to_value(get(&s)).unwrap();
        json.as_object_mut().unwrap().remove("data_table");
        assert_eq!(serde_json::from_value::<Chart>(json).unwrap().data_table, None);
    }

    #[test]
    fn chart_data_table_refuses_unsupported_types_without_changing_history() {
        let mut s = Session::new();
        s.new_workbook();
        s.execute("range.setValues", json!({"range": "A1", "values": [["Q", "Sales"], ["Q1", 10], ["Q2", 20]]})).unwrap();
        s.execute("insert.chart", json!({"range": "A1:B3", "type": "pie"})).unwrap();
        let before = s.active().unwrap().wb.clone();
        let history = s.execute("history.list", json!({})).unwrap();
        for params in [json!({"dataTable": true}), json!({"dataTableKeys": true}), json!({"dataTable": "yes"})] {
            assert!(s.execute("chart.set", params).is_err());
            assert_eq!(s.active().unwrap().wb, before);
            assert_eq!(s.execute("history.list", json!({})).unwrap(), history);
        }
        assert!(s.execute("chart.addElement", json!({"element": "dataTable"})).is_err());
        s.execute("chart.set", json!({"type": "line", "dataTable": true})).unwrap();
        assert!(s.active().unwrap().wb.active().unwrap().charts[0].data_table.is_some());
        s.execute("chart.set", json!({"type": "scatter"})).unwrap();
        assert!(s.active().unwrap().wb.active().unwrap().charts[0].data_table.is_none());
        s.execute("edit.undo", json!({})).unwrap();
        assert!(s.active().unwrap().wb.active().unwrap().charts[0].data_table.is_some());
    }

    #[test]
    fn insert_submodules_are_compiled() {
        assert!(chart::compiled());
        assert!(cond_format::compiled());
        assert!(links::compiled());
        assert!(objects::compiled());
        assert!(table::compiled());
    }

    #[test]
    fn manage_rules_delete_then_move() {
        let mut s = Session::new();
        s.new_workbook();
        for v in ["1", "2"] {
            let rule = json!({"type": "cellIs", "operator": "greaterThan", "value": v, "preset": "lightRedFill"});
            s.execute("home.conditionalFormat", json!({"range": "A1:A3", "rule": rule})).unwrap();
        }
        // Deleting first left two rules' worth of indices for the move: it panicked.
        s.execute("home.manageRules", json!({"delete": 0, "moveUp": 1})).unwrap();
        s.execute("home.manageRules", json!({"delete": 0, "moveDown": 0})).unwrap();
        let rules = s.execute("home.manageRules", json!({})).unwrap();
        assert_eq!(rules["rules"].as_array().map(Vec::len), Some(0));
    }

    #[test]
    fn table_without_header_keeps_outside_references() {
        // #1030: the data moved down for the new header row, but formulas outside kept the old addresses.
        let mut s = Session::new();
        s.new_workbook();
        s.execute("home.insertSheet", json!({"name": "Two"})).unwrap();
        s.execute("sheet.activate", json!({"sheet": 0})).unwrap();
        s.execute("cell.set", json!({"cell": "A1", "input": "10"})).unwrap();
        s.execute("cell.set", json!({"cell": "A2", "input": "20"})).unwrap();
        s.execute("cell.set", json!({"cell": "B1", "input": "=SUM(A1:A2)"})).unwrap();
        s.execute("cell.set", json!({"cell": "C1", "input": "=A2"})).unwrap();
        s.execute("cell.set", json!({"sheet": "Two", "cell": "A1", "input": "=Sheet1!A2"})).unwrap();
        s.execute("insert.table", json!({"range": "A1:A2", "header": false})).unwrap();
        let get = |s: &mut Session, a: &str| s.execute("cell.get", json!({"cell": a})).unwrap();
        assert_eq!(get(&mut s, "A1")["value"], "Column1");
        assert_eq!(get(&mut s, "A2")["value"], json!(10.0));
        assert_eq!(get(&mut s, "B1")["formula"], "=SUM(A2:A3)");
        assert_eq!(get(&mut s, "B1")["value"], json!(30.0));
        assert_eq!(get(&mut s, "C1")["formula"], "=A3");
        assert_eq!(get(&mut s, "C1")["value"], json!(20.0));
        let other = s.execute("cell.get", json!({"sheet": "Two", "cell": "A1"})).unwrap();
        assert_eq!(other["formula"], "=Sheet1!A3");
        assert_eq!(other["value"], json!(20.0));
    }

    #[test]
    fn table_names_refuse_defined_names_and_r1c1() {
        let mut s = Session::new();
        s.new_workbook();
        s.execute("range.setValues", json!({"range": "A1:A2", "values": [["h"], ["1"]]})).unwrap();
        s.execute("formulas.defineName", json!({"name": "Rate", "refersTo": "=0.2"})).unwrap();
        s.execute("insert.table", json!({"range": "A1:A2"})).unwrap();
        let e = s.execute("table.rename", json!({"table": "Table1", "name": "Rate"})).unwrap_err();
        assert!(e.to_string().contains("already exists"), "{e}");
        let e = s.execute("table.rename", json!({"table": "Table1", "name": "rate"})).unwrap_err();
        assert!(e.to_string().contains("already exists"), "{e}");
        for n in ["R2C3", "c", "r", "RC", "C5"] {
            let e = s.execute("table.rename", json!({"table": "Table1", "name": n})).unwrap_err();
            assert!(e.to_string().contains("not valid"), "{n}: {e}");
            let e = s.execute("insert.table", json!({"range": "C1:C2", "name": n})).unwrap_err();
            assert!(e.to_string().contains("not valid"), "{n}: {e}");
        }
        s.execute("table.rename", json!({"table": "Table1", "name": "Table1"})).unwrap();
        s.execute("table.rename", json!({"table": "Table1", "name": "Rates"})).unwrap();
    }
}
