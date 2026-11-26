//! Pointer placement. The shaper names the glyph. Core names the grapheme.

use gpui::{Context, MouseDownEvent, Pixels, Point, Window};
use yonro_core::{prelude::Location, word_bounds_at};

use crate::document::{location_at, Page, PaintedRow};

impl Page {
    pub(crate) fn mouse_down(
        &mut self,
        event: &MouseDownEvent,
        rows: &[PaintedRow],
        origin: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus.focus(window);
        self.origin = origin;
        let (caret, row_index) = location_at(window, rows, origin, event.position);
        match event.click_count {
            2 => self.select_word(caret),
            3 => self.select_row(rows.get(row_index)),
            _ => self.place_click(caret, event.modifiers.shift),
        }
        cx.notify();
    }

    pub(crate) fn mouse_move(
        &mut self,
        rows: &[PaintedRow],
        origin: Point<Pixels>,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.dragging {
            return;
        }
        self.caret = location_at(window, rows, origin, position).0;
        cx.notify();
    }

    pub(crate) fn mouse_up(&mut self, cx: &mut Context<Self>) {
        if self.anchor == Some(self.caret) {
            self.anchor = None;
        }
        self.dragging = false;
        cx.notify();
    }

    fn place_click(&mut self, caret: Location, shift: bool) {
        if shift {
            if self.anchor.is_none() {
                self.anchor = Some(self.caret);
            }
        } else {
            self.anchor = Some(caret);
        }
        self.caret = caret;
        self.dragging = true;
    }

    fn select_word(&mut self, caret: Location) {
        let (start, end) = word_bounds_at(&self.buffer, caret);
        self.anchor = Some(start);
        self.caret = end;
        self.dragging = false;
    }

    fn select_row(&mut self, row: Option<&PaintedRow>) {
        let Some(row) = row else {
            return;
        };
        self.anchor = Some(Location {
            grapheme_idx: row.start,
            line_idx: row.line_idx,
        });
        self.caret = Location {
            grapheme_idx: row.end,
            line_idx: row.line_idx,
        };
        self.dragging = false;
    }
}
