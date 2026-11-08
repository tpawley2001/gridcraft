//! Form controls on the sheet: drawing buttons, sliders, spin buttons, check boxes, option
//! buttons, list boxes, drop-downs, group boxes and labels, and working them with
//! the pointer and the keyboard. Clicking a list box's item, or an item of a drop-down's open
//! list, selects it (`control.setValue`). A click operates a control; Ctrl or Cmd+click, or a right-click,
//! selects it instead (to move, resize or format it). Values change through `control.setValue`,
//! and a whole drag or a held arrow is one undo step.
//!
//! Positions are logical (see `Geo::rtl`): on a right-to-left sheet a control moves as a whole
//! but is not mirrored, so a point inside it is reflected back with [`inner`] first.

use egui::epaint::PathShape;
use egui::{Painter, Pos2, Rect, Stroke, StrokeKind, pos2, vec2};
use gridcraft_engine::model::{ControlKind, FormControl, ObjKind, ObjRef, Sheet};
use serde_json::json;

use crate::SheetApp;
use crate::grid::Geo;
use crate::theme::{self, Tokens};

/// How long an arrow or the track waits before it repeats, and then between repeats (seconds).
const REPEAT_DELAY: f64 = 0.4;
const REPEAT_EVERY: f64 = 0.06;

/// The part of a control under the pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Button,
    /// A spin button's arrow that adds a step.
    Up,
    /// A spin button's arrow that takes a step away.
    Down,
    Thumb,
    /// The track on the minimum's side of the thumb (a page down).
    TrackBefore,
    /// The track on the maximum's side of the thumb (a page up).
    TrackAfter,
    /// A list box's items.
    List,
    /// A drop-down's box, which opens its list.
    Drop,
}

/// The height of one item row of a list box or drop-down, unzoomed.
const ROW: f32 = 16.0;

/// The 0-based first item shown in `rows` rows: the top, scrolled down so the selected item shows.
fn first_shown(value: u32, rows: usize) -> usize {
    (value as usize).saturating_sub(rows.max(1))
}

/// How many item rows fit in a list box drawn in `r`.
fn list_rows(r: Rect, z: f32) -> usize {
    (((r.height() - 2.0) / (ROW * z)).floor().max(1.0)) as usize
}

/// The rectangle of a drop-down's open list, below the drop-down drawn in `r`, of `drop_lines`
/// rows (fewer when there are fewer items).
pub fn drop_list_rect(r: Rect, c: &FormControl, z: f32, items: usize) -> Rect {
    let rows = items.clamp(1, c.drop_lines.clamp(1, 64) as usize);
    Rect::from_min_size(pos2(r.left(), r.bottom()), vec2(r.width(), rows as f32 * ROW * z + 2.0))
}

/// The 1-based item at `p` among `items` items drawn from the top of `area` in `rows` rows, with
/// `value` selected.
fn item_in(area: Rect, value: u32, rows: usize, items: usize, z: f32, p: Pos2) -> Option<u32> {
    if !area.contains(p) {
        return None;
    }
    let row = ((p.y - area.top() - 1.0) / (ROW * z)).floor();
    if !(0.0..rows as f32).contains(&row) {
        return None;
    }
    let i = first_shown(value, rows).saturating_add(row as usize);
    (i < items).then(|| u32::try_from(i + 1).ok()).flatten()
}

/// The 1-based item of a list box drawn in `r` at `p`.
pub fn list_item_at(r: Rect, c: &FormControl, z: f32, items: usize, p: Pos2) -> Option<u32> {
    item_in(r, c.value, list_rows(r, z), items, z, p)
}

/// The 1-based item of a drop-down's open list (the drop-down drawn in `r`) at `p`.
pub fn drop_item_at(r: Rect, c: &FormControl, z: f32, items: usize, p: Pos2) -> Option<u32> {
    let area = drop_list_rect(r, c, z, items);
    let rows = items.clamp(1, c.drop_lines.clamp(1, 64) as usize);
    item_in(area, c.value, rows, items, z, p)
}

/// Paints `rows` item rows of `items` from the top of `area`, the selected one (`value`, 1-based)
/// highlighted.
fn paint_items(p: &Painter, area: Rect, value: u32, rows: usize, items: &[String], z: f32) {
    let t = Tokens::get(p.ctx());
    let clip = p.with_clip_rect(area.shrink(1.0).intersect(p.clip_rect()));
    let first = first_shown(value, rows);
    for (k, text) in items.iter().enumerate().skip(first).take(rows) {
        let top = area.top() + 1.0 + (k - first) as f32 * ROW * z;
        let row = Rect::from_min_size(pos2(area.left() + 1.0, top), vec2(area.width() - 2.0, ROW * z));
        let selected = k + 1 == value as usize;
        if selected {
            clip.rect_filled(row, 0.0, t.sheet_fill);
        }
        let ink = if selected { t.sheet_control_bg } else { t.sheet_control_text };
        let g = clip.layout_no_wrap(text.clone(), theme::ui_font(12.0 * z), ink);
        clip.galley(pos2(row.left() + 3.0 * z, row.center().y - g.size().y / 2.0), g, ink);
    }
}

/// Paints a drop-down's open list below it (the drop-down drawn in `r`).
pub fn paint_drop_list(p: &Painter, r: Rect, c: &FormControl, z: f32, items: &[String]) {
    let t = Tokens::get(p.ctx());
    let area = drop_list_rect(r, c, z, items.len());
    p.rect_filled(area, 0.0, t.sheet_control_bg);
    p.rect_stroke(area, 0.0, Stroke::new(1.0, t.sheet_control_border), StrokeKind::Inside);
    let rows = items.len().clamp(1, c.drop_lines.clamp(1, 64) as usize);
    paint_items(p, area, c.value, rows, items, z);
}

/// A control being worked with the pointer.
#[derive(Clone, Debug, PartialEq)]
pub struct Gesture {
    pub id: u32,
    pub part: Part,
    /// Thumb drags: where along the track the pointer grabbed the thumb, from its centre.
    pub grab: f32,
    /// The undo stack's length when the gesture began: everything it changes is one step.
    pub undo_start: usize,
    /// When a held arrow or track repeats next (egui time, seconds).
    pub next_repeat: f64,
}

/// What a control looks like this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Look {
    pub hover: Option<Part>,
    pub pressed: Option<Part>,
    pub focused: bool,
    /// Show the value next to the thumb (while dragging).
    pub show_value: bool,
}

fn fraction(c: &FormControl) -> f32 {
    let span = c.max.saturating_sub(c.min).max(1) as f32;
    (c.value.saturating_sub(c.min) as f32 / span).clamp(0.0, 1.0)
}

/// A slider's track and thumb in `r`. A horizontal slider has its minimum on the left; a vertical
/// one has it at the top (as Excel's scroll bars do).
pub fn slider_parts(r: Rect, c: &FormControl, z: f32) -> (Rect, Rect) {
    let f = fraction(c);
    if c.horizontal {
        let knob = (r.height() - 4.0 * z).clamp(6.0 * z, 16.0 * z).min(r.width() / 2.0).max(1.0);
        let track = Rect::from_center_size(r.center(), vec2((r.width() - knob).max(1.0), (4.0 * z).min(r.height())));
        let x = track.left() + f * track.width();
        (track, Rect::from_center_size(pos2(x, r.center().y), vec2(knob, (r.height() - 4.0 * z).max(4.0))))
    } else {
        let knob = (r.width() - 4.0 * z).clamp(6.0 * z, 16.0 * z).min(r.height() / 2.0).max(1.0);
        let track = Rect::from_center_size(r.center(), vec2((4.0 * z).min(r.width()), (r.height() - knob).max(1.0)));
        let y = track.top() + f * track.height();
        (track, Rect::from_center_size(pos2(r.center().x, y), vec2((r.width() - 4.0 * z).max(4.0), knob)))
    }
}

/// A spin button's halves: (the arrow that adds, the arrow that subtracts). Tall spin buttons
/// stack them (up on top); wide ones put them side by side (up on the right).
pub fn spin_parts(r: Rect) -> (Rect, Rect) {
    if r.height() >= r.width() {
        let (top, bottom) = r.split_top_bottom_at_fraction(0.5);
        (top, bottom)
    } else {
        let (left, right) = r.split_left_right_at_fraction(0.5);
        (right, left)
    }
}

/// A logical point `p` inside control rect `r`, as the control is drawn: on a right-to-left
/// sheet the control moves as a whole without mirroring, so `p` is reflected within `r`.
pub fn inner(geo: &Geo, r: Rect, p: Pos2) -> Pos2 {
    if geo.rtl { pos2(r.left() + r.right() - p.x, p.y) } else { p }
}

/// The part of control `c` (drawn in `r`) under `p`.
pub fn part_at(r: Rect, c: &FormControl, z: f32, p: Pos2) -> Option<Part> {
    if !r.contains(p) {
        return None;
    }
    Some(match c.kind {
        ControlKind::Button | ControlKind::CheckBox | ControlKind::OptionButton => Part::Button,
        ControlKind::ListBox => Part::List,
        ControlKind::DropDown => Part::Drop,
        // Group boxes and labels only show their caption; a click on one reaches the grid.
        ControlKind::GroupBox | ControlKind::Label => return None,
        ControlKind::Spin => {
            let (up, _) = spin_parts(r);
            if up.contains(p) { Part::Up } else { Part::Down }
        }
        ControlKind::Slider => {
            let (_, thumb) = slider_parts(r, c, z);
            if thumb.expand(2.0).contains(p) {
                Part::Thumb
            } else {
                let before = if c.horizontal { p.x < thumb.center().x } else { p.y < thumb.center().y };
                if before { Part::TrackBefore } else { Part::TrackAfter }
            }
        }
    })
}

/// The value a thumb dragged to `p` stands for (`grab`: the offset from the thumb's centre where
/// it was grabbed).
pub fn value_at(r: Rect, c: &FormControl, z: f32, p: Pos2, grab: f32) -> f64 {
    let (track, _) = slider_parts(r, c, z);
    let f = if c.horizontal { (p.x - grab - track.left()) / track.width().max(1.0) } else { (p.y - grab - track.top()) / track.height().max(1.0) };
    c.min as f64 + f.clamp(0.0, 1.0) as f64 * c.max.saturating_sub(c.min) as f64
}

/// Paints control `c` in `r`, in the sheet's control colours. `items` are a list box's or
/// drop-down's items (empty for the other kinds).
pub fn paint(p: &Painter, r: Rect, c: &FormControl, z: f32, look: Look, items: &[String]) {
    let t = Tokens::get(p.ctx());
    let border = Stroke::new(1.0, t.sheet_control_border);
    let bg = |part: Part| {
        if look.pressed == Some(part) {
            t.sheet_control_pressed
        } else if look.hover == Some(part) {
            t.sheet_control_hover
        } else {
            t.sheet_control_bg
        }
    };
    let clip = p.with_clip_rect(r.expand(2.0 * z.max(1.0) + 3.0).intersect(p.clip_rect()));
    match c.kind {
        ControlKind::CheckBox | ControlKind::OptionButton => {
            // A small box or circle on the left, the caption to its right; 3-D adds a shade.
            let side = (13.0 * z).min(r.height() - 2.0).max(4.0);
            let mark = Rect::from_min_size(pos2(r.left() + 3.0 * z, r.center().y - side / 2.0), vec2(side, side));
            let fill = if look.pressed == Some(Part::Button) { t.sheet_control_pressed } else { t.sheet_control_bg };
            let ink = t.sheet_control_text;
            if c.kind == ControlKind::CheckBox {
                clip.rect_filled(mark, 1.0, fill);
                clip.rect_stroke(mark, 1.0, border, StrokeKind::Inside);
                if c.three_d {
                    clip.rect_stroke(mark.shrink(1.0), 0.0, Stroke::new(1.0, t.sheet_control_hover), StrokeKind::Inside);
                }
                match c.value {
                    1 => {
                        let (l, w) = (mark.left(), mark.width());
                        let tick = vec![
                            pos2(l + w * 0.2, mark.top() + w * 0.52),
                            pos2(l + w * 0.42, mark.top() + w * 0.74),
                            pos2(l + w * 0.8, mark.top() + w * 0.28),
                        ];
                        clip.add(PathShape::line(tick, Stroke::new((1.8 * z).max(1.0), ink)));
                    }
                    2 => {
                        clip.rect_filled(mark.shrink(side * 0.25), 0.0, ink);
                    }
                    _ => {}
                }
            } else {
                let radius = side / 2.0;
                clip.circle_filled(mark.center(), radius, fill);
                clip.circle_stroke(mark.center(), radius - 0.5, border);
                if c.three_d {
                    clip.circle_stroke(mark.center(), (radius - 1.5).max(0.5), Stroke::new(1.0, t.sheet_control_hover));
                }
                if c.value == 1 {
                    clip.circle_filled(mark.center(), radius * 0.45, ink);
                }
            }
            let left = mark.right() + 4.0 * z;
            let g = clip.layout_no_wrap(c.text.clone(), theme::ui_font(12.0 * z), ink);
            let at = pos2(left, r.center().y - g.size().y / 2.0);
            clip.with_clip_rect(r.intersect(clip.clip_rect())).galley(at, g, ink);
        }
        ControlKind::Button => {
            let fill = bg(Part::Button);
            clip.rect_filled(r, 3.0 * z, fill);
            clip.rect_stroke(r, 3.0 * z, border, StrokeKind::Inside);
            let shift = if look.pressed == Some(Part::Button) { vec2(0.0, 1.0) } else { vec2(0.0, 0.0) };
            let g = clip.layout(c.text.clone(), theme::ui_font(12.0 * z), t.sheet_control_text, (r.width() - 8.0 * z).max(1.0));
            clip.with_clip_rect(r.shrink(1.0).intersect(clip.clip_rect())).galley(r.center() - g.size() / 2.0 + shift, g, t.sheet_control_text);
        }
        ControlKind::Slider => {
            clip.rect_filled(r, 2.0 * z, t.sheet_control_bg);
            clip.rect_stroke(r, 2.0 * z, border, StrokeKind::Inside);
            let (track, thumb) = slider_parts(r, c, z);
            clip.rect_filled(track, 2.0 * z, t.sheet_track);
            let filled = if c.horizontal {
                Rect::from_min_max(track.min, pos2(thumb.center().x, track.max.y))
            } else {
                Rect::from_min_max(track.min, pos2(track.max.x, thumb.center().y))
            };
            clip.rect_filled(filled, 2.0 * z, t.sheet_fill);
            let knob = if look.pressed == Some(Part::Thumb) || look.hover == Some(Part::Thumb) { t.sheet_control_text } else { t.sheet_thumb };
            clip.rect_filled(thumb, 3.0 * z, knob);
            clip.rect_stroke(thumb, 3.0 * z, Stroke::new(1.0, t.sheet_control_bg), StrokeKind::Inside);
            if look.show_value {
                let g = clip.layout_no_wrap(c.value.to_string(), theme::ui_font(11.0 * z), t.sheet_control_text);
                let at = if c.horizontal {
                    pos2(thumb.center().x - g.size().x / 2.0, r.top() - g.size().y - 4.0 * z)
                } else {
                    pos2(r.right() + 4.0 * z, thumb.center().y - g.size().y / 2.0)
                };
                let card = Rect::from_min_size(at, g.size()).expand(2.0 * z);
                p.rect_filled(card, 2.0 * z, t.sheet_control_bg);
                p.rect_stroke(card, 2.0 * z, border, StrokeKind::Inside);
                p.galley(at, g, t.sheet_control_text);
            }
        }
        ControlKind::ListBox => {
            clip.rect_filled(r, 0.0, t.sheet_control_bg);
            clip.rect_stroke(r, 0.0, border, StrokeKind::Inside);
            paint_items(&clip, r, c.value, list_rows(r, z), items, z);
        }
        ControlKind::DropDown => {
            clip.rect_filled(r, 0.0, t.sheet_control_bg);
            clip.rect_stroke(r, 0.0, border, StrokeKind::Inside);
            // The arrow button on the right, a square as tall as the box.
            let side = r.height().min(r.width() / 2.0).max(1.0);
            let arrow = Rect::from_min_max(pos2(r.right() - side, r.top()), r.max);
            clip.rect_filled(arrow.shrink(1.0), 0.0, bg(Part::Drop));
            clip.rect_stroke(arrow, 0.0, border, StrokeKind::Inside);
            let (m, s) = (arrow.center(), (side * 0.2).max(1.5));
            let tri = vec![pos2(m.x - s, m.y - s * 0.5), pos2(m.x + s, m.y - s * 0.5), pos2(m.x, m.y + s * 0.6)];
            clip.add(PathShape::convex_polygon(tri, t.sheet_control_text, Stroke::NONE));
            let chosen = (c.value as usize).checked_sub(1).and_then(|i| items.get(i));
            if let Some(text) = chosen {
                let g = clip.layout_no_wrap(text.clone(), theme::ui_font(12.0 * z), t.sheet_control_text);
                let at = pos2(r.left() + 3.0 * z, r.center().y - g.size().y / 2.0);
                let text_area = Rect::from_min_max(r.min, pos2(arrow.left(), r.bottom())).shrink(1.0);
                clip.with_clip_rect(text_area.intersect(clip.clip_rect())).galley(at, g, t.sheet_control_text);
            }
        }
        ControlKind::GroupBox => {
            // A frame whose top edge breaks for the caption.
            let g = clip.layout_no_wrap(c.text.clone(), theme::ui_font(12.0 * z), t.sheet_control_text);
            let frame = Rect::from_min_max(pos2(r.left(), r.top() + g.size().y / 2.0), r.max);
            clip.rect_stroke(frame, 0.0, border, StrokeKind::Inside);
            if !c.text.is_empty() {
                let at = pos2(r.left() + 8.0 * z, r.top());
                clip.rect_filled(Rect::from_min_size(at - vec2(2.0 * z, 0.0), g.size() + vec2(4.0 * z, 0.0)), 0.0, t.grid_bg);
                clip.with_clip_rect(r.intersect(clip.clip_rect())).galley(at, g, t.sheet_control_text);
            }
        }
        ControlKind::Label => {
            let g = clip.layout(c.text.clone(), theme::ui_font(12.0 * z), t.sheet_control_text, r.width().max(1.0));
            clip.with_clip_rect(r.intersect(clip.clip_rect())).galley(r.min, g, t.sheet_control_text);
        }
        ControlKind::Spin => {
            let (up, down) = spin_parts(r);
            let tall = r.height() >= r.width();
            for (half, part) in [(up, Part::Up), (down, Part::Down)] {
                clip.rect_filled(half, 2.0 * z, bg(part));
                clip.rect_stroke(half, 2.0 * z, border, StrokeKind::Inside);
                let s = (half.width().min(half.height()) * 0.28).max(1.5);
                let m = half.center();
                let tri = match (tall, part) {
                    (true, Part::Up) => vec![pos2(m.x, m.y - s * 0.6), pos2(m.x - s, m.y + s * 0.5), pos2(m.x + s, m.y + s * 0.5)],
                    (true, _) => vec![pos2(m.x, m.y + s * 0.6), pos2(m.x + s, m.y - s * 0.5), pos2(m.x - s, m.y - s * 0.5)],
                    (false, Part::Up) => vec![pos2(m.x + s * 0.6, m.y), pos2(m.x - s * 0.5, m.y + s), pos2(m.x - s * 0.5, m.y - s)],
                    (false, _) => vec![pos2(m.x - s * 0.6, m.y), pos2(m.x + s * 0.5, m.y - s), pos2(m.x + s * 0.5, m.y + s)],
                };
                clip.add(PathShape::convex_polygon(tri, t.sheet_control_text, Stroke::NONE));
            }
        }
    }
    if look.focused {
        p.rect_stroke(r.expand(2.0), 3.0 * z, Stroke::new(2.0, t.sheet_focus), StrokeKind::Outside);
    }
}

/// The control with `id` on `sh`.
pub fn find(sh: &Sheet, id: u32) -> Option<&FormControl> {
    sh.controls.iter().find(|c| c.id == id)
}

/// How control `c` looks this frame, from the gesture, the focus and the pointer (`hover`, a
/// logical position).
pub fn look(app: &SheetApp, geo: &Geo, c: &FormControl, r: Rect, hover: Option<Pos2>) -> Look {
    let gesture = app.grid.control.as_ref().filter(|g| g.id == c.id);
    let selected = app.selected_object == Some(ObjRef::new(ObjKind::Control, c.id));
    let hover = if selected { None } else { hover.and_then(|p| part_at(r, c, geo.z, inner(geo, r, p))) };
    let pressed = gesture.map(|g| g.part).filter(|part| *part == Part::Thumb || hover == Some(*part));
    Look { hover, pressed, focused: app.grid.focused_control == Some(c.id) && !selected, show_value: gesture.is_some_and(|g| g.part == Part::Thumb) }
}

impl SheetApp {
    /// Sets a slider or spin button's value; inside a gesture, keeps it one undo step.
    fn control_set(&mut self, id: u32, value: f64) -> bool {
        match self.session.run("control.setValue", json!({"id": id, "value": value})) {
            Ok(_) => {
                if let Some(start) = self.grid.control.as_ref().map(|g| g.undo_start)
                    && let Some(d) = self.session.active_mut()
                    && d.undo.len() > start + 1
                {
                    d.undo.truncate(start + 1);
                }
                true
            }
            Err(e) => {
                self.grid.control = None;
                self.message = Some(("Form Control".into(), crate::clean_error(&e)));
                false
            }
        }
    }

    /// Moves a slider or spin button by `steps` steps (negative: toward the minimum), or by pages
    /// when `page` is set.
    pub fn control_nudge(&mut self, id: u32, steps: i64, page: bool) -> bool {
        let Some(c) = self.session.active().and_then(|d| d.wb.active()).and_then(|sh| find(sh, id)).cloned() else { return false };
        if !c.kind.is_number() {
            return false;
        }
        let by = if page && c.kind == ControlKind::Slider { c.page } else { c.step } as f64;
        self.control_set(id, c.value as f64 + steps as f64 * by)
    }

    /// Selects item `item` (1-based) of a list box or drop-down and writes it to the linked cell.
    pub fn control_choose(&mut self, id: u32, item: u32) -> bool {
        match self.session.run("control.setValue", json!({"id": id, "value": item})) {
            Ok(_) => true,
            Err(e) => {
                self.message = Some(("Form Control".into(), crate::clean_error(&e)));
                false
            }
        }
    }

    /// Runs a button's action. Questions (approve it, make room for a macro's output) become a
    /// prompt.
    pub fn press_button(&mut self, id: u32) {
        crate::control_ui::press(self, json!({"id": id}));
    }
}

/// Pointer handling for controls on the grid. Returns true when it used the pointer this frame
/// (so the grid neither selects cells nor objects).
pub fn interact(app: &mut SheetApp, ctx: &egui::Context, resp: &egui::Response, geo: &Geo, sh: &Sheet) -> bool {
    let (pointer, mods, time) = ctx.input(|i| (i.pointer.clone(), i.modifiers, i.time));
    let pos = pointer.interact_pos().or(pointer.hover_pos()).map(|p| geo.mirror_pos(p));
    // A control that no longer exists loses its gesture and focus.
    if let Some(id) = app.grid.focused_control
        && find(sh, id).is_none()
    {
        app.grid.focused_control = None;
    }
    // An open drop-down list takes the next press: on an item it chooses it; anywhere it closes.
    if let Some(id) = app.grid.drop_open {
        let Some(c) = find(sh, id).cloned() else {
            app.grid.drop_open = None;
            return false;
        };
        if pointer.primary_pressed() || pointer.secondary_pressed() {
            app.grid.drop_open = None;
            let r = crate::chartview::anchor_rect(geo, sh, &c.anchor);
            let at = pointer.press_origin().map(|p| geo.mirror_pos(p)).or(pos);
            if let Some(p) = at.filter(|_| pointer.primary_pressed()) {
                let items = items_of(app, &c);
                let list = drop_list_rect(r, &c, geo.z, items.len());
                if list.contains(p) {
                    if let Some(n) = drop_item_at(r, &c, geo.z, items.len(), p) {
                        app.control_choose(c.id, n);
                    }
                    return true;
                }
                if r.contains(p) {
                    return true;
                }
            }
        }
    }
    if let Some(g) = app.grid.control.clone() {
        let Some(c) = find(sh, g.id).cloned() else {
            app.grid.control = None;
            return true;
        };
        let r = crate::chartview::anchor_rect(geo, sh, &c.anchor);
        if pointer.primary_down() {
            match g.part {
                Part::Thumb => {
                    if let Some(p) = pos {
                        let v = c.snap(value_at(r, &c, geo.z, inner(geo, r, p), g.grab));
                        if v != c.value {
                            app.control_set(c.id, v as f64);
                        }
                    }
                }
                Part::Button | Part::List | Part::Drop => {}
                part => {
                    // Held arrows and track repeat while the pointer stays on them.
                    let over = pos.and_then(|p| part_at(r, &c, geo.z, inner(geo, r, p))) == Some(part);
                    if over && time >= g.next_repeat {
                        step_part(app, &c, part);
                        if let Some(gg) = app.grid.control.as_mut() {
                            gg.next_repeat = time + REPEAT_EVERY;
                        }
                    }
                    ctx.request_repaint_after(std::time::Duration::from_millis(30));
                }
            }
            return true;
        }
        // Released: a button runs when the pointer is still on it.
        app.grid.control = None;
        if g.part == Part::Button && pos.is_some_and(|p| r.contains(p)) {
            app.press_button(c.id);
        }
        resp.request_focus();
        return true;
    }
    if !pointer.primary_pressed() || !resp.contains_pointer() {
        // Any other press, on the grid (a right-click on a cell) or off it, moves the focus off
        // the control, so a later Enter or Space doesn't work it.
        if pointer.any_pressed() && app.grid.focused_control.is_some() {
            app.grid.focused_control = None;
        }
        return false;
    }
    let Some(p) = pointer.press_origin().map(|p| geo.mirror_pos(p)).or(pos) else { return false };
    // The selected object's handles reach past its frame and win over a control beneath them.
    if let Some(r) = app.selected_object.and_then(|o| crate::chartview::object_rect(geo, sh, o))
        && crate::objects::handle_at(r, p).is_some()
    {
        return false;
    }
    let hit = crate::chartview::hit(app, geo, sh, p).filter(|(o, _)| o.kind == ObjKind::Control);
    // Ctrl or Cmd+click selects; a selected control moves and resizes instead of working.
    let Some((o, r)) = hit.filter(|(o, _)| !mods.command && !mods.ctrl && app.selected_object != Some(*o)) else {
        app.grid.focused_control = None;
        return false;
    };
    let Some(c) = find(sh, o.id).cloned() else { return false };
    let p = inner(geo, r, p);
    let Some(part) = part_at(r, &c, geo.z, p) else { return false };
    app.selected_object = None;
    app.grid.focused_control = Some(c.id);
    match part {
        Part::Drop => {
            app.grid.drop_open = Some(c.id);
            resp.request_focus();
            return true;
        }
        Part::List => {
            // Only a single-selection list box changes on a click (multi-selection isn't edited).
            if c.selection == gridcraft_engine::model::ListSelection::Single {
                let items = items_of(app, &c).len();
                if let Some(n) = list_item_at(r, &c, geo.z, items, p).filter(|n| *n != c.value) {
                    app.control_choose(c.id, n);
                }
            }
            resp.request_focus();
            return true;
        }
        _ => {}
    }
    let undo_start = app.session.active().map_or(0, |d| d.undo.len());
    let grab = if part == Part::Thumb {
        let (_, thumb) = slider_parts(r, &c, geo.z);
        if c.horizontal { p.x - thumb.center().x } else { p.y - thumb.center().y }
    } else {
        0.0
    };
    app.grid.control = Some(Gesture { id: c.id, part, grab, undo_start, next_repeat: time + REPEAT_DELAY });
    if !matches!(part, Part::Button | Part::Thumb) {
        step_part(app, &c, part);
    }
    resp.request_focus();
    true
}

/// One click on an arrow or the track.
fn step_part(app: &mut SheetApp, c: &FormControl, part: Part) {
    match part {
        Part::Up => app.control_nudge(c.id, 1, false),
        Part::Down => app.control_nudge(c.id, -1, false),
        Part::TrackBefore => app.control_nudge(c.id, -1, true),
        Part::TrackAfter => app.control_nudge(c.id, 1, true),
        Part::Button | Part::Thumb | Part::List | Part::Drop => false,
    };
}

/// The items of list box or drop-down `c` on sheet `si` (none for the other kinds).
pub fn items(wb: &gridcraft_engine::model::Workbook, si: usize, c: &FormControl) -> Vec<String> {
    if c.kind.is_list() { gridcraft_engine::cmd::controls::list_items(wb, si, c) } else { Vec::new() }
}

/// The items of list box or drop-down `c` on the active sheet.
fn items_of(app: &SheetApp, c: &FormControl) -> Vec<String> {
    app.session.active().map(|d| items(&d.wb, d.wb.active_sheet(), c)).unwrap_or_default()
}

/// Whether the focused control is a button, check box or option button. Space presses one, so
/// the space it types must not start a cell edit; a slider or spin button doesn't take Space, and
/// its space reaches the grid like any other character.
pub fn focused_button(app: &SheetApp, sh: &Sheet) -> bool {
    app.grid
        .focused_control
        .and_then(|id| find(sh, id))
        .is_some_and(|c| matches!(c.kind, ControlKind::Button | ControlKind::CheckBox | ControlKind::OptionButton))
}

/// The grid takes a key while a control is focused: a button gives the keyboard back, so a later
/// Enter or Space doesn't press it again and Space starts a cell edit; a slider or spin button
/// keeps it, as the control last worked, which the arrows still step.
pub fn key_to_grid(app: &mut SheetApp, sh: &Sheet) {
    if focused_button(app, sh) {
        app.grid.focused_control = None;
    }
}

/// Keys for the focused control: arrows move by a step, Page Up and Page Down by a page, Home and
/// End to the ends; Space or Enter presses a button; Space clicks a check box or option button. True when the key was used. `repeat` is a key
/// held down: it steps a slider or spin button again, but a button runs once per press.
pub fn key(app: &mut SheetApp, sh: &Sheet, key: egui::Key, repeat: bool) -> bool {
    use egui::Key;
    let Some(c) = app.grid.focused_control.and_then(|id| find(sh, id)).cloned() else { return false };
    if app.selected_object.is_some() {
        return false;
    }
    match (c.kind, key) {
        (_, Key::Escape) => {
            app.grid.focused_control = None;
            app.grid.drop_open = None;
            true
        }
        (ControlKind::Button, Key::Space | Key::Enter) => {
            if !repeat {
                app.press_button(c.id);
            }
            true
        }
        (ControlKind::CheckBox | ControlKind::OptionButton, Key::Space) => {
            if !repeat {
                app.press_button(c.id);
            }
            true
        }
        (ControlKind::Button | ControlKind::CheckBox | ControlKind::OptionButton, _) => false,
        (ControlKind::ListBox | ControlKind::DropDown | ControlKind::GroupBox | ControlKind::Label, _) => false,
        // A spin button's up arrow adds; a slider's minimum is at its left or top.
        (ControlKind::Spin, Key::ArrowUp | Key::ArrowRight) | (ControlKind::Slider, Key::ArrowDown | Key::ArrowRight) => {
            app.control_nudge(c.id, 1, false)
        }
        (ControlKind::Spin, Key::ArrowDown | Key::ArrowLeft) | (ControlKind::Slider, Key::ArrowUp | Key::ArrowLeft) => {
            app.control_nudge(c.id, -1, false)
        }
        (ControlKind::Spin, Key::PageUp) | (ControlKind::Slider, Key::PageDown) => app.control_nudge(c.id, 1, true),
        (ControlKind::Spin, Key::PageDown) | (ControlKind::Slider, Key::PageUp) => app.control_nudge(c.id, -1, true),
        (_, Key::Home) => app.control_set(c.id, c.min as f64),
        (_, Key::End) => app.control_set(c.id, c.max as f64),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slider(value: u32, horizontal: bool) -> FormControl {
        FormControl { kind: ControlKind::Slider, min: 0, max: 100, value, horizontal, ..Default::default() }
    }

    #[test]
    fn the_thumb_sits_at_the_value_and_dragging_reads_it_back() {
        let r = Rect::from_min_size(pos2(10.0, 10.0), vec2(200.0, 20.0));
        for v in [0, 25, 50, 100] {
            let c = slider(v, true);
            let (_, thumb) = slider_parts(r, &c, 1.0);
            assert!((value_at(r, &c, 1.0, thumb.center(), 0.0) - v as f64).abs() < 0.5, "{v}");
            assert_eq!(part_at(r, &c, 1.0, thumb.center()), Some(Part::Thumb));
        }
        let c = slider(50, true);
        assert_eq!(part_at(r, &c, 1.0, pos2(15.0, 20.0)), Some(Part::TrackBefore));
        assert_eq!(part_at(r, &c, 1.0, pos2(205.0, 20.0)), Some(Part::TrackAfter));
        assert_eq!(part_at(r, &c, 1.0, pos2(5.0, 20.0)), None);
        // Past the ends clamps.
        assert_eq!(value_at(r, &c, 1.0, pos2(-500.0, 20.0), 0.0), 0.0);
        assert_eq!(value_at(r, &c, 1.0, pos2(900.0, 20.0), 0.0), 100.0);
        // Vertical: the minimum is at the top.
        let r = Rect::from_min_size(pos2(0.0, 0.0), vec2(20.0, 200.0));
        let c = slider(0, false);
        let (_, thumb) = slider_parts(r, &c, 1.0);
        assert!(thumb.center().y < 20.0);
        assert_eq!(part_at(r, &slider(50, false), 1.0, pos2(10.0, 190.0)), Some(Part::TrackAfter));
    }

    #[test]
    fn list_items_are_hit_by_row_and_scroll_to_the_selection() {
        let r = Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 50.0));
        let mut c = FormControl { kind: ControlKind::ListBox, ..Default::default() };
        // Three rows of 16 fit; the first row is item 1.
        assert_eq!(list_item_at(r, &c, 1.0, 5, pos2(10.0, 5.0)), Some(1));
        assert_eq!(list_item_at(r, &c, 1.0, 5, pos2(10.0, 40.0)), Some(3));
        assert_eq!(list_item_at(r, &c, 1.0, 2, pos2(10.0, 40.0)), None, "past the last item");
        c.value = 5;
        assert_eq!(list_item_at(r, &c, 1.0, 5, pos2(10.0, 40.0)), Some(5), "scrolled to the selection");
        let d = FormControl { kind: ControlKind::DropDown, drop_lines: 2, ..Default::default() };
        let r = Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 20.0));
        assert_eq!(drop_list_rect(r, &d, 1.0, 9).height(), 34.0);
        assert_eq!(drop_item_at(r, &d, 1.0, 9, pos2(10.0, 40.0)), Some(2));
        assert_eq!(drop_item_at(r, &d, 1.0, 9, pos2(10.0, 10.0)), None, "the box itself");
        assert_eq!(part_at(r, &d, 1.0, pos2(10.0, 10.0)), Some(Part::Drop));
        let g = FormControl { kind: ControlKind::GroupBox, ..Default::default() };
        assert_eq!(part_at(r, &g, 1.0, pos2(10.0, 10.0)), None);
    }

    #[test]
    fn spin_halves() {
        let tall = Rect::from_min_size(pos2(0.0, 0.0), vec2(18.0, 36.0));
        let c = FormControl { kind: ControlKind::Spin, ..Default::default() };
        assert_eq!(part_at(tall, &c, 1.0, pos2(9.0, 5.0)), Some(Part::Up));
        assert_eq!(part_at(tall, &c, 1.0, pos2(9.0, 30.0)), Some(Part::Down));
        let wide = Rect::from_min_size(pos2(0.0, 0.0), vec2(40.0, 18.0));
        assert_eq!(part_at(wide, &c, 1.0, pos2(35.0, 9.0)), Some(Part::Up));
        assert_eq!(part_at(wide, &c, 1.0, pos2(5.0, 9.0)), Some(Part::Down));
    }

    #[test]
    fn degenerate_ranges_do_not_divide_by_zero() {
        let r = Rect::from_min_size(pos2(0.0, 0.0), vec2(1.0, 1.0));
        let c = FormControl { kind: ControlKind::Slider, min: 5, max: 5, value: 5, ..Default::default() };
        let (track, thumb) = slider_parts(r, &c, 1.0);
        assert!(track.width().is_finite() && thumb.center().x.is_finite());
        assert!(value_at(r, &c, 1.0, pos2(0.5, 0.5), 0.0).is_finite());
    }
}
