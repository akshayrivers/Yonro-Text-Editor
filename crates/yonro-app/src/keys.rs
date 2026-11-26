//! Keystrokes and the platform text input handler.
//!
//! The stored caret is a grapheme `Location`. UTF-16 exists only in the
//! methods GPUI calls.

use std::ops::Range;
use std::time::Instant;

use gpui::{
    point, px, Bounds, Context, EntityInputHandler, KeyDownEvent, Pixels, UTF16Selection, Window,
};
use yonro_core::{
    location_to_point, location_to_utf16, point_to_location, prelude::Location, utf16_to_location,
    word_bounds_at,
};

use crate::document::{Page, COLUMNS};

const PAGE_ROWS: usize = 20;

impl Page {
    pub(crate) fn on_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mods = &event.keystroke.modifiers;
        if self.command(event, window, cx) {
            cx.notify();
            return;
        }
        if self.mention_key(event) {
            cx.notify();
            return;
        }
        let shift = mods.shift;
        let word = mods.platform || mods.control;
        match event.keystroke.key.as_str() {
            "left" => self.place(self.step_left(word), shift),
            "right" => self.place(self.step_right(word), shift),
            "up" => self.place(self.step_rows(1, true), shift),
            "down" => self.place(self.step_rows(1, false), shift),
            "home" => self.place(self.row_edge(true), shift),
            "end" => self.place(self.row_edge(false), shift),
            "pageup" => self.place(self.step_rows(PAGE_ROWS, true), shift),
            "pagedown" => self.place(self.step_rows(PAGE_ROWS, false), shift),
            "backspace" => self.backspace(),
            "delete" => self.delete_forward(),
            "enter" => self.enter(),
            _ => {
                if mods.platform || mods.control || mods.function {
                    return;
                }
                if let Some(text) = event.keystroke.key_char.as_deref() {
                    if printable(text) {
                        self.type_text(text);
                    }
                }
            }
        }
        self.refresh_mentions();
        cx.notify();
    }

    fn command(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let mods = &event.keystroke.modifiers;
        if !mods.platform && !mods.control {
            return false;
        }
        match event.keystroke.key.as_str() {
            "z" if mods.shift => self.redo(),
            "z" => self.undo(),
            "y" => self.redo(),
            "a" => self.select_all(),
            "c" => self.copy(cx),
            "x" if mods.shift => self.toggle_kind(yonro_core::MarkKind::Strike),
            "x" => self.cut(cx),
            "b" => self.toggle_kind(yonro_core::MarkKind::Bold),
            "i" => self.toggle_kind(yonro_core::MarkKind::Italic),
            "u" => self.toggle_kind(yonro_core::MarkKind::Underline),
            "h" if mods.shift => self.toggle_kind(yonro_core::MarkKind::Highlight { swatch: 1 }),
            "e" if mods.shift => self.toggle_kind(yonro_core::MarkKind::Color { swatch: 1 }),
            "=" => self.toggle_kind(yonro_core::MarkKind::Size { points: 24 }),
            "k" => self.toggle_kind(yonro_core::MarkKind::Link {
                href: "https://".to_string(),
            }),
            "\\" => self.clear_format(),
            "0" if mods.alt => self.set_style(yonro_core::ParaStyle::Normal),
            "1" if mods.alt => self.set_style(yonro_core::ParaStyle::H1),
            "2" if mods.alt => self.set_style(yonro_core::ParaStyle::H2),
            "3" if mods.alt => self.set_style(yonro_core::ParaStyle::H3),
            "l" if mods.shift => self.cycle_align(),
            "7" if mods.shift => self.toggle_list(yonro_core::ListKind::Bullet),
            "8" if mods.shift => self.toggle_list(yonro_core::ListKind::Number),
            "]" => self.indent_lines(false),
            "[" => self.indent_lines(true),
            "s" if mods.alt => self.cycle_spacing(),
            "f" if mods.alt => self.cycle_face(),
            "v" => self.paste(cx),
            "s" => self.save(),
            "f" => self.open_find(window),
            _ => return false,
        }
        true
    }

    fn undo(&mut self) {
        if let Some(caret) = self.log.undo(&mut self.buffer) {
            self.caret = caret;
            self.anchor = None;
        }
    }

    fn redo(&mut self) {
        if let Some(caret) = self.log.redo(&mut self.buffer) {
            self.caret = caret;
            self.anchor = None;
        }
    }

    fn select_all(&mut self) {
        let line_idx = self.buffer.height().saturating_sub(1);
        self.anchor = Some(Location::default());
        self.caret = Location {
            grapheme_idx: self.buffer.grapheme_count(line_idx),
            line_idx,
        };
    }

    fn copy(&self, cx: &mut Context<Self>) {
        let text = self.selected_text();
        if text.is_empty() {
            return;
        }
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
    }

    fn cut(&mut self, cx: &mut Context<Self>) {
        self.copy(cx);
        self.take_selection();
    }

    fn paste(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        if text.is_empty() {
            return;
        }
        self.type_text(&text);
        self.echo = None;
    }

    fn save(&mut self) {
        let result = if self.buffer.is_file_loaded() {
            self.buffer.save()
        } else if let Some(path) = self.path.clone() {
            self.buffer.save_as(&path.to_string_lossy())
        } else {
            eprintln!("yonro: no file to save");
            return;
        };
        if let Err(err) = result {
            eprintln!("yonro: could not save ({err})");
        }
    }

    fn selected_text(&self) -> String {
        let (start, end, _) = self.selection_utf16();
        if start == end {
            return String::new();
        }
        let units: Vec<u16> = self.buffer.text().encode_utf16().collect();
        let end = end.min(units.len());
        let start = start.min(end);
        String::from_utf16(&units[start..end]).unwrap_or_default()
    }

    fn place(&mut self, next: Location, shift: bool) {
        if shift {
            if self.anchor.is_none() {
                self.anchor = Some(self.caret);
            }
        } else {
            self.anchor = None;
        }
        self.caret = next;
        self.refresh_mentions();
    }

    fn step_left(&self, word: bool) -> Location {
        if word {
            word_left(&self.buffer, self.caret)
        } else {
            grapheme_left(&self.buffer, self.caret)
        }
    }

    fn step_right(&self, word: bool) -> Location {
        if word {
            word_right(&self.buffer, self.caret)
        } else {
            grapheme_right(&self.buffer, self.caret)
        }
    }

    fn step_rows(&self, rows: usize, up: bool) -> Location {
        let (row, column) = location_to_point(&self.buffer, COLUMNS, self.caret);
        let target = if up {
            row.saturating_sub(rows)
        } else {
            row.saturating_add(rows)
        };
        point_to_location(&self.buffer, COLUMNS, target, column)
    }

    fn row_edge(&self, home: bool) -> Location {
        let (row_index, _) = location_to_point(&self.buffer, COLUMNS, self.caret);
        let rows = yonro_core::visual_rows(&self.buffer, COLUMNS);
        let Some(row) = rows.get(row_index) else {
            return self.caret;
        };
        let grapheme_idx = if home {
            row.start
        } else if rows
            .get(row_index.saturating_add(1))
            .is_some_and(|next| next.line_idx == row.line_idx)
        {
            row.end
        } else {
            self.buffer.grapheme_count(row.line_idx)
        };
        Location {
            grapheme_idx,
            line_idx: row.line_idx,
        }
    }

    fn backspace(&mut self) {
        if self.take_selection() {
            return;
        }
        let now = Instant::now();
        if self.caret.grapheme_idx == 0 {
            if self.caret.line_idx > 0 {
                self.caret = self.log.delete_newline(
                    &mut self.buffer,
                    self.caret.line_idx.saturating_sub(1),
                    now,
                );
            }
            return;
        }
        let start = Location {
            grapheme_idx: self.caret.grapheme_idx.saturating_sub(1),
            line_idx: self.caret.line_idx,
        };
        self.caret = self
            .log
            .delete_range(&mut self.buffer, start, self.caret, now);
    }

    fn delete_forward(&mut self) {
        if self.take_selection() {
            return;
        }
        let now = Instant::now();
        let count = self.buffer.grapheme_count(self.caret.line_idx);
        if self.caret.grapheme_idx < count {
            let end = Location {
                grapheme_idx: self.caret.grapheme_idx.saturating_add(1),
                line_idx: self.caret.line_idx,
            };
            self.caret = self
                .log
                .delete_range(&mut self.buffer, self.caret, end, now);
            return;
        }
        if self.caret.line_idx.saturating_add(1) < self.buffer.height() {
            self.caret = self
                .log
                .delete_newline(&mut self.buffer, self.caret.line_idx, now);
        }
    }

    fn enter(&mut self) {
        self.take_selection();
        self.caret = self
            .log
            .insert_newline(&mut self.buffer, self.caret, Instant::now());
        self.anchor = None;
    }

    pub(crate) fn type_text(&mut self, text: &str) {
        if self.echo.as_deref() == Some(text) {
            self.echo = None;
            return;
        }
        self.take_selection();
        let now = Instant::now();
        let before = self.caret;
        let single = text.chars().nth(1).is_none() && !text.contains('\n') && !text.contains('\r');
        self.caret = if single {
            let Some(ch) = text.chars().next() else {
                return;
            };
            self.log.insert_char(&mut self.buffer, self.caret, ch, now)
        } else {
            self.log
                .insert_text(&mut self.buffer, self.caret, text, now)
        };
        self.anchor = None;
        self.echo = Some(text.to_string());
        self.apply_sticky(before);
        self.refresh_mentions();
    }

    fn take_selection(&mut self) -> bool {
        let Some(anchor) = self.anchor else {
            return false;
        };
        if anchor.line_idx == self.caret.line_idx && anchor.grapheme_idx == self.caret.grapheme_idx
        {
            self.anchor = None;
            return false;
        }
        self.caret = self
            .log
            .delete_range(&mut self.buffer, anchor, self.caret, Instant::now());
        self.anchor = None;
        true
    }

    fn selection_utf16(&self) -> (usize, usize, bool) {
        let head = location_to_utf16(&self.buffer, self.caret);
        let tail = self
            .anchor
            .map(|mark| location_to_utf16(&self.buffer, mark))
            .unwrap_or(head);
        if tail <= head {
            (tail, head, false)
        } else {
            (head, tail, true)
        }
    }
}

impl EntityInputHandler for Page {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let units: Vec<u16> = self.buffer.text().encode_utf16().collect();
        let start = range.start.min(units.len());
        let end = range.end.min(units.len()).max(start);
        *adjusted_range = Some(start..end);
        String::from_utf16(&units[start..end]).ok()
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let (start, end, reversed) = self.selection_utf16();
        Some(UTF16Selection {
            range: start..end,
            reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        let (start, end) = self.marked?;
        let from = location_to_utf16(&self.buffer, start);
        let to = location_to_utf16(&self.buffer, end);
        Some(from.min(to)..from.max(to))
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(range) = range {
            let start = utf16_to_location(&self.buffer, range.start);
            let end = utf16_to_location(&self.buffer, range.end);
            self.caret = start;
            if end.line_idx != start.line_idx || end.grapheme_idx != start.grapheme_idx {
                self.anchor = Some(end);
            }
        }
        self.type_text(text);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let start = self.caret;
        self.replace_text_in_range(range, new_text, window, cx);
        self.marked = Some((start, self.caret));
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let loc = utf16_to_location(&self.buffer, range_utf16.start);
        let (row, column) = location_to_point(&self.buffer, COLUMNS, loc);
        Some(Bounds {
            origin: point(
                element_bounds.origin.x + advance(column),
                element_bounds.origin.y + advance_rows(row),
            ),
            size: gpui::size(px(1.0), px(34.0)),
        })
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let row = rows_between(self.origin.y, point.y);
        let column = columns_between(self.origin.x, point.x);
        let loc = point_to_location(&self.buffer, COLUMNS, row, column);
        Some(location_to_utf16(&self.buffer, loc))
    }
}

fn printable(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|ch| !ch.is_control())
}

fn grapheme_left(buffer: &yonro_core::Buffer, at: Location) -> Location {
    if at.grapheme_idx > 0 {
        return Location {
            grapheme_idx: at.grapheme_idx.saturating_sub(1),
            line_idx: at.line_idx,
        };
    }
    if at.line_idx == 0 {
        return at;
    }
    let line_idx = at.line_idx.saturating_sub(1);
    Location {
        grapheme_idx: buffer.grapheme_count(line_idx),
        line_idx,
    }
}

fn grapheme_right(buffer: &yonro_core::Buffer, at: Location) -> Location {
    let count = buffer.grapheme_count(at.line_idx);
    if at.grapheme_idx < count {
        return Location {
            grapheme_idx: at.grapheme_idx.saturating_add(1),
            line_idx: at.line_idx,
        };
    }
    if at.line_idx.saturating_add(1) >= buffer.height() {
        return at;
    }
    Location {
        grapheme_idx: 0,
        line_idx: at.line_idx.saturating_add(1),
    }
}

fn word_left(buffer: &yonro_core::Buffer, at: Location) -> Location {
    if at.grapheme_idx == 0 {
        return grapheme_left(buffer, at);
    }
    let (start, _) = word_bounds_at(buffer, at);
    if start.grapheme_idx < at.grapheme_idx {
        return start;
    }
    let previous = grapheme_left(buffer, at);
    word_bounds_at(buffer, previous).0
}

fn word_right(buffer: &yonro_core::Buffer, at: Location) -> Location {
    let (_, end) = word_bounds_at(buffer, at);
    if end.grapheme_idx > at.grapheme_idx || end.line_idx > at.line_idx {
        return end;
    }
    let next = grapheme_right(buffer, at);
    word_bounds_at(buffer, next).1
}

fn advance(column: usize) -> Pixels {
    let mut x = px(0.0);
    let step = px(10.0);
    for _ in 0..column {
        x += step;
    }
    x
}

fn advance_rows(row: usize) -> Pixels {
    let mut y = px(0.0);
    let step = px(34.0);
    for _ in 0..row {
        y += step;
    }
    y
}

fn rows_between(origin: Pixels, point: Pixels) -> usize {
    let step = px(34.0);
    let mut y = origin;
    let mut row = 0usize;
    if point < origin {
        return 0;
    }
    while y + step <= point {
        y += step;
        row = row.saturating_add(1);
    }
    row
}

fn columns_between(origin: Pixels, point: Pixels) -> usize {
    let step = px(10.0);
    let mut x = origin;
    let mut column = 0usize;
    if point < origin {
        return 0;
    }
    while x + step <= point {
        x += step;
        column = column.saturating_add(1);
    }
    column
}
