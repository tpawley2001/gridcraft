use super::controls::{act, anchor_keytip, control_keytip, invoke, popup_command, sep};
use super::{
    Icon, Sense, SheetApp, Stroke, StrokeKind, Tokens, Ui, big_button, big_split_button, icons, json, small_button, small_toggle_button,
    small_toggle_icon, vec2,
};

/// Draws the Insert tab: PivotTable, Table, Pictures, Shapes, Icons, charts, slicer, sparklines, form controls (button, slider, spin button), checkbox, links, comments, text box, header/footer and symbols.
pub(super) fn insert(app: &mut SheetApp, ui: &mut Ui) {
    let __k = big_button(ui, Icon::PivotTable, "PivotTable", "PivotTable", false);
    control_keytip(app, ui, &__k, "insert.pivotTable");
    if __k.clicked() {
        act(app, "insert.pivotTable", json!({}));
    }
    let __k = big_button(ui, Icon::Table, "Table", "Table", false);
    control_keytip(app, ui, &__k, "insert.table");
    if __k.clicked() {
        act(app, "insert.table", json!({}));
    }
    sep(ui);
    let pic = big_button(ui, Icon::Picture, "Pictures", "Insert a picture", true);
    anchor_keytip(app, ui, &pic, &["insert.picture"]);
    pic.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), "Pictures"));
    egui::Popup::menu(&pic).show(|ui| {
        for (label, placement) in [("Place in Cell", "cell"), ("Place over Cells", "overCells")] {
            if ui.button(label).clicked() {
                app.open_dialog("insertPicture", json!({"placement": placement}));
                ui.close();
            }
        }
    });
    let shapes = big_button(ui, Icon::Shapes, "Shapes", "Shapes", true);
    anchor_keytip(app, ui, &shapes, &["insert.shape"]);
    egui::Popup::menu(&shapes).show(|ui| shape_gallery(app, ui));
    let icn = big_button(ui, Icon::Theme, "Icons", "Insert an icon", true);
    anchor_keytip(app, ui, &icn, &["insert.icons"]);
    egui::Popup::menu(&icn).show(|ui| {
        ui.set_width(260.0);
        egui::Grid::new("icon_lib").show(ui, |ui| {
            for (i, (name, icon)) in icons::LIBRARY.iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, 4.0, Tokens::get(ui.ctx()).hover);
                }
                icons::paint(ui.painter(), r.shrink(6.0), *icon, Tokens::get(ui.ctx()).text);
                if resp.on_hover_text(*name).clicked() {
                    act(app, "insert.icons", json!({"name": name}));
                    ui.close();
                }
                if (i + 1) % 6 == 0 {
                    ui.end_row();
                }
            }
        });
    });
    sep(ui);
    let __k = big_button(ui, Icon::Chart, "Recommended\nCharts", "Recommended Charts", false);
    control_keytip(app, ui, &__k, "insert.recommendedCharts");
    if __k.clicked() {
        // Menu-style: with no chart type, run_or_alert opens the Insert Chart dialog.
        act(app, "insert.recommendedCharts", json!({}));
    }
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            for (icon, tip, cid, subs) in [
                (
                    Icon::ChartBar,
                    "Column or Bar",
                    "insert.chart.column",
                    vec![
                        ("Clustered Column", "column", ""),
                        ("Stacked Column", "column", "stacked"),
                        ("100% Stacked Column", "column", "stacked100"),
                        ("Clustered Bar", "bar", ""),
                        ("Stacked Bar", "bar", "stacked"),
                        ("100% Stacked Bar", "bar", "stacked100"),
                    ],
                ),
                (
                    Icon::ChartLine,
                    "Line or Area",
                    "insert.chart.line",
                    vec![
                        ("Line", "line", ""),
                        ("Line with Markers", "line", "markers"),
                        ("Stacked Line", "line", "stacked"),
                        ("Area", "area", ""),
                        ("Stacked Area", "area", "stacked"),
                        ("100% Stacked Area", "area", "stacked100"),
                    ],
                ),
                (
                    Icon::ChartPie,
                    "Pie or Doughnut",
                    "insert.chart.pie",
                    vec![("Pie", "pie", ""), ("Pie of Pie", "pie", "pieOfPie"), ("Bar of Pie", "pie", "barOfPie"), ("Doughnut", "doughnut", "")],
                ),
            ] {
                let r = small_button(ui, icon, "", tip, true);
                control_keytip(app, ui, &r, cid);
                let forced_c = invoke(app, cid);
                egui::Popup::menu(&r).open_memory(popup_command(forced_c, r.clicked())).show(|ui| {
                    for (label, k, s) in &subs {
                        if ui.button(*label).clicked() {
                            act(app, "insert.chart", json!({"type": k, "subtype": s}));
                        }
                    }
                });
            }
        });
        ui.horizontal(|ui| {
            for (icon, tip, cid, subs) in [
                (
                    Icon::ChartScatter,
                    "Scatter or Bubble",
                    "insert.chart.scatter",
                    vec![("Scatter", "scatter", ""), ("Scatter with Lines", "scatter", "lines"), ("Bubble", "bubble", "")],
                ),
                (Icon::Chart, "Statistic", "insert.chart.statistic", vec![("Histogram", "histogram", ""), ("Box & Whisker", "boxWhisker", "")]),
                (
                    Icon::Theme,
                    "Hierarchy & more",
                    "insert.chart.hierarchy",
                    vec![
                        ("Treemap", "treemap", ""),
                        ("Sunburst", "sunburst", ""),
                        ("Waterfall", "waterfall", ""),
                        ("Funnel", "funnel", ""),
                        ("Radar", "radar", ""),
                        ("Stock", "stock", ""),
                        ("3-D Surface", "surface", ""),
                        ("Wireframe 3-D Surface", "surfaceWireframe", ""),
                        ("Contour", "contour", ""),
                        ("Wireframe Contour", "contourWireframe", ""),
                        ("Combo", "combo", ""),
                    ],
                ),
            ] {
                let r = small_button(ui, icon, "", tip, true);
                control_keytip(app, ui, &r, cid);
                let forced_c = invoke(app, cid);
                egui::Popup::menu(&r).open_memory(popup_command(forced_c, r.clicked())).show(|ui| {
                    for (label, k, s) in &subs {
                        if ui.button(*label).clicked() {
                            act(app, "insert.chart", json!({"type": k, "subtype": s}));
                        }
                    }
                });
            }
        });
    });
    sep(ui);
    if big_button(ui, Icon::Filter, "Slicer", "Insert a slicer to filter a table", false).clicked() {
        act(app, "insert.slicer", json!({}));
    }
    sep(ui);
    let sp = big_button(ui, Icon::Sparkline, "Sparklines", "Sparklines", true);
    anchor_keytip(app, ui, &sp, &["dialog:sparkline"]);
    egui::Popup::menu(&sp).show(|ui| {
        for (label, kind) in [("Line", "line"), ("Column", "column"), ("Win/Loss", "winLoss")] {
            if ui.button(label).clicked() {
                app.open_dialog("sparkline", json!({"type": kind}));
            }
        }
    });
    sep(ui);
    // Form controls: draw a button, slider, spin button, check box or option button on the sheet
    // (a click places it at its default size). Their key tips run the command at the active cell.
    use gridcraft_engine::model::ControlKind;
    let placing = app.grid.placing;
    let controls = [
        (Icon::Button, ControlKind::Button, "Button", "Draw a button that runs a macro or a command", "insert.button"),
        (Icon::Slider, ControlKind::Slider, "Slider", "Draw a slider that sets a whole number in a linked cell", "insert.slider"),
        (Icon::Spin, ControlKind::Spin, "Spin Button", "Draw a spin button that steps a whole number in a linked cell", "insert.spinButton"),
        (Icon::Checkbox, ControlKind::CheckBox, "Check Box", "Draw a check box that writes TRUE or FALSE to a linked cell", "insert.formCheckBox"),
        (
            Icon::OptionButton,
            ControlKind::OptionButton,
            "Option Button",
            "Draw an option button; one of the sheet's option buttons is selected at a time",
            "insert.optionButton",
        ),
    ];
    // Three to a column, as the ribbon's small buttons stack; the second column shows icons only,
    // so the group stays narrow.
    for (n, column) in controls.chunks(3).enumerate() {
        ui.vertical(|ui| {
            for &(icon, kind, label, tip, id) in column {
                let on = placing == Some(kind);
                let r = if n == 0 { small_toggle_button(ui, icon, label, tip, on) } else { small_toggle_icon(ui, icon, label, tip, on) };
                control_keytip(app, ui, &r, id);
                if r.clicked() {
                    app.grid.placing = if placing == Some(kind) { None } else { Some(kind) };
                    app.selected_object = None;
                }
            }
        });
    }
    sep(ui);
    // Cell Controls.
    if big_button(ui, Icon::Checkbox, "Checkbox", "Insert checkboxes in the selected cells", false).clicked() {
        act(app, "insert.checkbox", json!({}));
    }
    sep(ui);
    let __k = big_button(ui, Icon::Link, "Link", "Insert Link", false);
    control_keytip(app, ui, &__k, "dialog:insertLink");
    if __k.clicked() {
        app.open_dialog("insertLink", json!({}));
    }
    let __k = big_button(ui, Icon::Comment, "Comment", "New Comment", false);
    control_keytip(app, ui, &__k, "dialog:comment");
    if __k.clicked() {
        app.open_dialog("comment", json!({"threaded": true}));
    }
    sep(ui);
    let __k = big_button(ui, Icon::TextBox, "Text\nBox", "Text Box", false);
    control_keytip(app, ui, &__k, "insert.textBox");
    if __k.clicked() {
        act(app, "insert.textBox", json!({}));
    }
    // Two lines like its neighbours, so the Insert tab stays as wide as before; the translated label is split, the catalog key is one line.
    let label = app.ui.language.tr("Decorative Text").replacen(' ', "\n", 1);
    let wa = big_button(ui, Icon::WordArt, &label, "Decorative Text", true);
    anchor_keytip(app, ui, &wa, &["insert.wordArt"]);
    // Menu invocation shows the style gallery; the command itself never opens a dialog.
    egui::Popup::menu(&wa).show(|ui| crate::wordart::gallery(app, ui));
    let __k = big_button(ui, Icon::PageLayout, "Header &\nFooter", "Header & Footer", false);
    control_keytip(app, ui, &__k, "dialog:headerFooter");
    if __k.clicked() {
        // As in Excel: Page Layout view, editing the header in place (the dialog is under Page Setup).
        crate::paper_view::start_editing(app);
    }
    // Symbols: Equation, then Symbol.
    sep(ui);
    let (eq, eq_menu) = big_split_button(ui, Icon::Equation, "Equation", "Insert an equation, or pick one from the gallery");
    control_keytip(app, ui, &eq, "dialog:equation");
    if eq.clicked() {
        app.open_dialog("equation", json!({}));
    }
    egui::Popup::menu(&eq_menu).show(|ui| {
        ui.set_width(300.0);
        if ui.add(egui::Button::new(app.ui.language.tr("Insert New Equation...")).frame(false).min_size(vec2(290.0, 24.0))).clicked() {
            app.open_dialog("equation", json!({}));
            ui.close();
        }
        ui.separator();
        if let Some(latex) = crate::equation::gallery(ui) {
            act(app, "insert.equation", json!({"latex": latex}));
            ui.close();
        }
    });
    let sym = big_button(ui, Icon::Symbol, "Symbol", "Symbol", true);
    control_keytip(app, ui, &sym, "insert.symbol");
    // Opened by its key tip too (Alt+I, S): the gallery picks the symbol, nothing is inserted before.
    let forced = invoke(app, "insert.symbol");
    egui::Popup::menu(&sym).open_memory(popup_command(forced, sym.clicked())).show(|ui| {
        ui.set_width(260.0);
        ui.horizontal_wrapped(|ui| {
            for ch in [
                "€", "£", "¥", "©", "®", "™", "±", "≠", "≤", "≥", "÷", "×", "∞", "µ", "α", "β", "π", "Σ", "Ω", "°", "•", "…", "→", "←", "↑", "↓",
                "✓", "★", "½", "¼",
            ] {
                // No bundled face has ✓: the button paints a code-drawn check instead of a tofu box.
                if crate::widgets::glyph_button(ui, ch.chars().next().unwrap_or('?'), &format!("Insert {ch}")).clicked() {
                    act(app, "insert.symbol", json!({"char": ch}));
                }
            }
        });
    });
}

pub(super) fn draw(app: &mut SheetApp, ui: &mut Ui) {
    let tool = app.session.draw_tool.clone();
    let __k = big_button(ui, Icon::Shapes, "Select\nObjects", "Select and move ink and shapes", false);
    control_keytip(app, ui, &__k, "draw.lasso");
    if __k.clicked() {
        act(app, "draw.lasso", json!({}));
    }
    sep(ui);
    for (label, color) in [("Pen\nBlue", "#1F5FC9"), ("Pen\nBlack", "#000000"), ("Pen\nRed", "#D13B2F"), ("Highlighter", "#F2E33A")] {
        let on = tool == "pen" && app.session.draw_color.eq_ignore_ascii_case(color);
        let r = big_button(ui, Icon::Pen, label, "Draw with ink", false);
        if on {
            ui.painter().rect_stroke(r.rect, 5.0, Stroke::new(2.0, Tokens::get(ui.ctx()).accent), StrokeKind::Inside);
        }
        if r.clicked() {
            let width = if label == "Highlighter" { 12.0 } else { 2.0 };
            act(app, "draw.pen", json!({"on": !on, "color": color, "width": width}));
        }
    }
    let er = big_button(ui, Icon::Eraser, "Eraser", "Erase ink strokes", false);
    if tool == "eraser" {
        ui.painter().rect_stroke(er.rect, 5.0, Stroke::new(2.0, Tokens::get(ui.ctx()).accent), StrokeKind::Inside);
    }
    if er.clicked() {
        act(app, "draw.eraser", json!({}));
    }
    sep(ui);
    let __k = big_button(ui, Icon::Shapes, "Ink to\nShape", "Convert the last ink stroke to a shape", false);
    control_keytip(app, ui, &__k, "draw.inkToShape");
    if __k.clicked() {
        act(app, "draw.inkToShape", json!({}));
    }
}

/// The Shapes popup: `insert.shapeGallery`'s categories, each a heading over a wrapping grid of
/// outline buttons; a click inserts the shape.
fn shape_gallery(app: &mut SheetApp, ui: &mut Ui) {
    let gallery = app.session.execute("insert.shapeGallery", json!({})).unwrap_or_default();
    let categories = gallery["categories"].as_array().cloned().unwrap_or_default();
    ui.set_width(300.0);
    egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
        for category in &categories {
            ui.label(egui::RichText::new(category["name"].as_str().unwrap_or_default()).small().strong());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
                for entry in category["shapes"].as_array().map(Vec::as_slice).unwrap_or_default() {
                    let label = entry["label"].as_str().unwrap_or_default();
                    let params = match (entry["kind"].as_str(), entry["preset"].as_str()) {
                        (Some(kind), _) => json!({"kind": kind}),
                        (None, Some(preset)) => json!({"preset": preset}),
                        (None, None) => continue,
                    };
                    let (r, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
                    let t = Tokens::get(ui.ctx());
                    if resp.hovered() {
                        ui.painter().rect_filled(r, 3.0, t.hover);
                    }
                    paint_gallery_shape(ui.painter(), r.shrink(5.0), &params, t.text);
                    if resp.on_hover_text(label).clicked() {
                        act(app, "insert.shape", params);
                        ui.close();
                    }
                }
            });
        }
    });
}

/// A gallery button's glyph: a preset's outline from the renderer's geometry, flattened like a
/// sheet shape, or a simple drawing of a kind.
fn paint_gallery_shape(p: &egui::Painter, r: egui::Rect, params: &serde_json::Value, color: egui::Color32) {
    let stroke = Stroke::new(1.0, color);
    if let Some(preset) = params["preset"].as_str() {
        let g = gridcraft_engine::model::ShapeGeometry { preset: preset.into(), adjust: vec![], paths: vec![] };
        // Hundredths of a pixel, so guide arithmetic keeps its precision at button size.
        let res = |v: f32| (v * 100.0).round() as i64;
        for o in crate::freeform::outlines(&gridcraft_engine::model::preset_paths(&g, res(r.width()), res(r.height())).unwrap_or_default(), r) {
            if o.stroke && o.points.len() >= 2 {
                p.add(if o.closed {
                    egui::epaint::PathShape::closed_line(o.points, stroke)
                } else {
                    egui::epaint::PathShape::line(o.points, stroke)
                });
            }
        }
        return;
    }
    match params["kind"].as_str().unwrap_or_default() {
        "line" => {
            p.line_segment([r.left_top(), r.right_bottom()], stroke);
        }
        "arrow" => {
            p.line_segment([r.left_top(), r.right_bottom()], stroke);
            let d = (r.right_bottom() - r.left_top()).normalized();
            let n = vec2(-d.y, d.x);
            let tip = r.right_bottom();
            p.add(egui::epaint::PathShape::convex_polygon(vec![tip, tip - d * 6.0 + n * 3.0, tip - d * 6.0 - n * 3.0], color, Stroke::NONE));
        }
        "roundedRectangle" => {
            p.rect_stroke(r, 4.0, stroke, StrokeKind::Inside);
        }
        "ellipse" => {
            p.add(egui::Shape::ellipse_stroke(r.center(), r.size() / 2.0, stroke));
        }
        "textBox" => {
            p.rect_stroke(r, 0.0, stroke, StrokeKind::Inside);
            for i in 1..=3 {
                let y = r.top() + r.height() * i as f32 / 4.0;
                p.line_segment([egui::pos2(r.left() + 3.0, y), egui::pos2(r.right() - 3.0, y)], Stroke::new(1.0, color.gamma_multiply(0.6)));
            }
        }
        _ => {
            p.rect_stroke(r, 0.0, stroke, StrokeKind::Inside);
        }
    }
}
