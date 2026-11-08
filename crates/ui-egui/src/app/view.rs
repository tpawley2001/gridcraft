use gridcraft_engine::core::{MAX_COLS, MAX_ROWS};

use super::{CellRef, SheetApp, SheetView};

impl SheetApp {
    /// The active workbook and sheet: what drafts, text boxes and picture targets belong to.
    pub fn view_key(&self) -> Option<(u64, usize)> {
        self.session.active().map(|d| (d.uid, d.wb.active_sheet()))
    }

    /// The active window and sheet: each window scrolls on its own.
    fn scroll_key(&self) -> Option<(u64, u64)> {
        Some((self.session.active_window()?, self.session.active()?.wb.active()?.uid.get()))
    }

    pub fn view(&self) -> SheetView {
        self.scroll_key().and_then(|k| self.views.get(&k).copied()).unwrap_or_else(|| self.saved_view())
    }

    pub fn view_mut(&mut self) -> Option<&mut SheetView> {
        let k = self.scroll_key()?;
        let saved = self.saved_view();
        Some(self.views.entry(k).or_insert(saved))
    }

    /// The view a sheet opens with: scrolled to the file's saved first visible cell (`topLeftCell`).
    fn saved_view(&self) -> SheetView {
        let Some(sh) = self.session.active().and_then(|d| d.wb.active()) else { return SheetView::default() };
        if sh.view != gridcraft_engine::model::SheetViewMode::Normal {
            return SheetView::default();
        }
        let (fr, fc) = sh.freeze.unwrap_or((0, 0));
        let top = sh.view_top_left;
        let y = sh.row_top(top.row.max(fr)) - sh.row_top(fr);
        let x = sh.col_left(top.col.max(fc)) - sh.col_left(fc);
        SheetView { scroll: egui::vec2(x.max(0.0) as f32, y.max(0.0) as f32) }
    }

    /// Hands the active document's scroll positions (first visible cell per sheet) and active
    /// cell to the engine so a save writes them (`sheetView topLeftCell`). Normal view only:
    /// Page Layout view scrolls over pages, not cells.
    pub fn store_view_state(&mut self) {
        let window = self.session.active_window();
        let Some(d) = self.session.active_mut() else { return };
        let mut tops = Vec::new();
        for (si, sh) in d.wb.sheets.iter().enumerate() {
            let Some(v) =
                window.and_then(|w| self.views.get(&(w, sh.uid.get()))).filter(|_| sh.view == gridcraft_engine::model::SheetViewMode::Normal)
            else {
                continue;
            };
            let (fr, fc) = sh.freeze.unwrap_or((0, 0));
            // A hair past the edge so a row that starts exactly at the scroll offset is the one found.
            let row = sh.row_at(sh.row_top(fr) + v.scroll.y.max(0.0) as f64 + 0.01);
            let col = sh.col_at(sh.col_left(fc) + v.scroll.x.max(0.0) as f64 + 0.01);
            tops.push((si, CellRef::new(row, col)));
        }
        d.store_view(&tops);
    }

    /// Drops the scroll positions of windows that have closed: window ids are never reused.
    pub(super) fn prune_views(&mut self) {
        let session = &self.session;
        self.views.retain(|&(w, _), _| session.window(w).is_some());
    }

    /// The active window's first scrolled (sheet uid, row, column), Normal view only.
    fn scroll_top(&self) -> Option<(u64, u32, u32)> {
        let sh = self.session.active()?.wb.active()?;
        if sh.view != gridcraft_engine::model::SheetViewMode::Normal {
            return None;
        }
        let v = self.view();
        let (fr, fc) = sh.freeze.unwrap_or((0, 0));
        // A hair past the edge so a row that starts exactly at the scroll offset is the one found.
        let row = sh.row_at(sh.row_top(fr) + f64::from(v.scroll.y.max(0.0)) + 0.01);
        let col = sh.col_at(sh.col_left(fc) + f64::from(v.scroll.x.max(0.0)) + 0.01);
        Some((sh.uid.get(), row, col))
    }

    /// Scrolls the active window so `row`, `col` come first (clamped to the frozen panes' edge).
    fn scroll_to_top(&mut self, row: u32, col: u32) {
        let Some(sh) = self.session.active().and_then(|d| d.wb.active()) else { return };
        let (fr, fc) = sh.freeze.unwrap_or((0, 0));
        let y = sh.row_top(row.max(fr)) - sh.row_top(fr);
        let x = sh.col_left(col.max(fc)) - sh.col_left(fc);
        if let Some(v) = self.view_mut() {
            v.scroll = egui::vec2(x.max(0.0) as f32, y.max(0.0) as f32);
        }
    }

    /// Synchronous Scrolling (View Side by Side): when the active window's first visible row or
    /// column moved since it last drew, on the same sheet, its partner scrolls by as many rows
    /// and columns (stopping at row 1 and column A).
    pub(crate) fn sync_scrolling(&mut self) {
        let Some(window) = self.session.active_window() else { return };
        let Some(now) = self.scroll_top() else {
            self.scroll_tops.remove(&window);
            return;
        };
        let before = self.scroll_tops.insert(window, now);
        let Some(partner) = self.session.scroll_partner(window) else { return };
        let Some((_, row, col)) = before.filter(|b| b.0 == now.0 && (b.1, b.2) != (now.1, now.2)) else { return };
        let (dr, dc) = (i64::from(now.1) - i64::from(row), i64::from(now.2) - i64::from(col));
        // The partner's sheet is in its workbook only while it is the window commands act on.
        self.session.show_window(partner);
        if let Some((_, prow, pcol)) = self.scroll_top() {
            let shift = |at: u32, by: i64, max: u32| u32::try_from((i64::from(at) + by).clamp(0, i64::from(max.saturating_sub(1)))).unwrap_or(at);
            self.scroll_to_top(shift(prow, dr, MAX_ROWS), shift(pcol, dc, MAX_COLS));
            // Recorded as moved, so the partner's own next frame doesn't scroll this window back.
            if let Some(moved) = self.scroll_top() {
                self.scroll_tops.insert(partner, moved);
            }
        }
        self.session.show_window(window);
    }
}
