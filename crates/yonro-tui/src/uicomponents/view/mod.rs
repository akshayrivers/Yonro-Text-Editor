use super::UIComponent;
use crate::prelude::*;
use crate::terminal::Terminal;
use std::time::Instant;
use std::{cmp::min, io::Error};
use yonro_core::command::{Edit, Move};
use yonro_core::highlighter::Highlighter;
use yonro_core::{Buffer, DocumentStatus, Line};
mod searchdirection;
use searchdirection::SearchDirection;

mod searchinfo;
use searchinfo::SearchInfo;

#[derive(Clone, Debug)]
pub enum EditOperation {
    // NOTE: text is char (single Unicode scalar value) not a grapheme cluster.
    // This is safe for keyboard input since the OS composes grapheme clusters
    // before delivery. If paste support is added, this must change to String.
    InsertChar {
        at: Location,
        text: char,
    },
    DeleteChar {
        at: Location,
        text: char,
    },
    InsertNewLine {
        at: Location,
        grapheme_count_at_split: usize,
    },
    DeleteNewLine {
        line_idx: usize,
        split_at_grapheme: usize,
    },
    InsertGroup {
        start: Location,
        chars: Vec<char>,
    },
}
pub struct View {
    id: usize,
    buffer_id: usize,
    is_active: bool,
    needs_redraw: bool,
    // always starting at (0,0)and the size will dietermine the visible area
    rect: Rect,
    text_location: Location,
    scroll_offset: Position,
    search_info: Option<SearchInfo>,
    undo_stack: Vec<EditOperation>,
    redo_stack: Vec<EditOperation>,
    // timestamp of the last insert (helps us in grouping the steps)
    last_insert_time: Option<Instant>,
    // tracks where the cursor was after the last insert
    // used alongside last_insert_time to detect non-contiguous typing
    last_insert_location: Option<Location>,
    /// Soft word-wrap (`PLAN.md Phase 4.1`): long buffer lines render as
    /// multiple visual rows at word boundaries. No hard newlines inserted.
    pub word_wrap: bool,
}

impl Default for View {
    fn default() -> Self {
        Self {
            id: 0,
            buffer_id: 0,
            is_active: false,
            needs_redraw: false,
            rect: Rect::default(),
            text_location: Location::default(),
            scroll_offset: Position::default(),
            search_info: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            last_insert_time: None,
            last_insert_location: None,
            word_wrap: true,
        }
    }
}
/// How long a gap between keystrokes before we start a new undo group.
const GROUP_TIMEOUT_MS: u128 = 800;
impl View {
    pub fn get_status(&self, buffer: &Buffer) -> DocumentStatus {
        let file_info = buffer.get_file_info();
        let (words, graphemes, _) = buffer.word_count_stats();
        DocumentStatus {
            file_name: format!("{file_info}"),
            total_lines: buffer.height(),
            current_line_idx: self.text_location.line_idx,
            is_modified: buffer.is_dirty(),
            file_type: file_info.get_file_type(),
            word_count: words,
            char_count: graphemes,
        }
    }
    pub fn set_id(&mut self, id: usize) {
        self.id = id;
    }
    pub fn buffer_id(&self) -> usize {
        self.buffer_id
    }
    /// Current cursor location (for `@mention` query extraction).
    pub const fn location(&self) -> Location {
        self.text_location
    }
    /// Delete `count` graphemes backward, stopping at document start.
    pub fn delete_backward_chars(&mut self, buffer: &mut Buffer, count: usize) {
        for _ in 0..count {
            let before = self.text_location;
            self.delete_backward(buffer);
            if self.text_location == before {
                break;
            }
        }
    }
    /// Insert literal text (`\n` splits lines, `\r` dropped for CRLF pastes).
    pub fn insert_text(&mut self, buffer: &mut Buffer, text: &str) {
        for ch in text.chars() {
            if ch == '\n' {
                self.insert_newline(buffer);
            } else if ch != '\r' {
                self.insert_char(ch, buffer);
            }
        }
        self.scroll_text_location_into_view(buffer);
        self.mark_redraw(true);
    }
    pub fn set_buffer_id(&mut self, id: usize) {
        self.buffer_id = id;
        self.text_location = Location::default();
        self.scroll_offset = Position::default();
        self.mark_redraw(true);
    }
    pub fn set_active(&mut self, active: bool) {
        if self.is_active != active {
            self.is_active = active;
            self.mark_redraw(true);
        }
    }
    pub fn handle_edit_command(
        &mut self,
        command: Edit,
        buffer: &mut Buffer,
        clipboard: &mut dyn crate::clipboard::Clipboard,
    ) {
        match command {
            Edit::Insert(character) => self.insert_char(character, buffer),
            Edit::Delete => self.delete(buffer),
            Edit::DeleteBackward => self.delete_backward(buffer),
            Edit::InsertNewLine => self.insert_newline(buffer),
            Edit::Copy => self.copy_current_line(buffer, clipboard),
            Edit::Cut => self.cut_current_line(buffer, clipboard),
            Edit::Paste => {
                self.paste(buffer, clipboard);
            }
        }
    }

    /// Copy the current line (text only, no newline) to the clipboard.
    /// Line-based: the editor has no visual selection model yet.
    fn copy_current_line(
        &mut self,
        buffer: &Buffer,
        clipboard: &mut dyn crate::clipboard::Clipboard,
    ) {
        let text = buffer
            .get_line(self.text_location.line_idx)
            .map(|line| line.to_string())
            .unwrap_or_default();
        clipboard.set(text);
        self.mark_redraw(true);
    }

    /// Cut the current line: copy it, then delete its content and join with
    /// the next line (unless it is the last). Undoable, one op per grapheme.
    fn cut_current_line(
        &mut self,
        buffer: &mut Buffer,
        clipboard: &mut dyn crate::clipboard::Clipboard,
    ) {
        self.copy_current_line(buffer, clipboard);
        let line_idx = self.text_location.line_idx;
        self.text_location = Location {
            line_idx,
            grapheme_idx: 0,
        };
        let count = buffer.grapheme_count(line_idx);
        for _ in 0..count {
            self.delete(buffer);
        }
        // Remove the emptied line itself (join), unless it is the last one.
        self.text_location = Location {
            line_idx,
            grapheme_idx: 0,
        };
        self.delete(buffer);
        self.scroll_text_location_into_view(buffer);
        self.mark_redraw(true);
    }

    /// Insert clipboard contents at the cursor (`\n` splits lines, `\r`
    /// dropped so CRLF pastes cleanly). Returns false when empty.
    fn paste(
        &mut self,
        buffer: &mut Buffer,
        clipboard: &mut dyn crate::clipboard::Clipboard,
    ) -> bool {
        let Some(text) = clipboard.get() else {
            return false;
        };
        if text.is_empty() {
            return false;
        }
        self.insert_text(buffer, &text);
        true
    }
    pub fn handle_move_command(&mut self, command: Move, buffer: &Buffer) {
        // This match moves the positon, but does not check for all boundaries.
        // The final boundarline checking happens after the match statement.
        let height = self.rect.size.height.saturating_sub(2);
        match command {
            Move::Up => self.move_up(1, buffer),
            Move::Down => self.move_down(1, buffer),
            Move::Left => self.move_left(buffer),
            Move::Right => self.move_right(buffer),
            Move::PageUp => self.move_up(height.saturating_sub(1), buffer),
            Move::PageDown => self.move_down(height.saturating_sub(1), buffer),
            Move::StartOfLine => self.move_to_start_of_line(),
            Move::EndOfLine => self.move_to_end_of_line(buffer),
        }
        self.scroll_text_location_into_view(buffer);
    }

    fn insert_char(&mut self, character: char, buffer: &mut Buffer) {
        let old_len = buffer.grapheme_count(self.text_location.line_idx);
        buffer.insert_char(character, self.text_location);
        self.redo_stack.clear();

        let now = Instant::now();
        let should_group = self
            .last_insert_time
            .map(|t| now.duration_since(t).as_millis() < GROUP_TIMEOUT_MS)
            .unwrap_or(false)
            && self
                .last_insert_location
                .map(|loc| {
                    loc.line_idx == self.text_location.line_idx
                        && loc.grapheme_idx == self.text_location.grapheme_idx
                })
                .unwrap_or(false);

        if should_group {
            // Try to merge into the last op on the undo stack
            if let Some(EditOperation::InsertGroup { chars, .. }) = self.undo_stack.last_mut() {
                chars.push(character);
                self.last_insert_time = Some(now);
                // Still need to move cursor
                let new_len = buffer.grapheme_count(self.text_location.line_idx);
                if new_len.saturating_sub(old_len) > 0 {
                    self.handle_move_command(Move::Right, buffer);
                }
                self.mark_redraw(true);
                return;
            }
            // Last op was InsertChar (single) — upgrade it to a group
            if let Some(EditOperation::InsertChar { at, text }) = self.undo_stack.pop() {
                self.undo_stack.push(EditOperation::InsertGroup {
                    start: at,
                    chars: vec![text, character],
                });
                self.last_insert_time = Some(now);
                let new_len = buffer.grapheme_count(self.text_location.line_idx);
                if new_len.saturating_sub(old_len) > 0 {
                    self.handle_move_command(Move::Right, buffer);
                }
                self.mark_redraw(true);
                return;
            }
        }

        // No grouping: push a fresh InsertChar
        self.undo_stack.push(EditOperation::InsertChar {
            at: self.text_location,
            text: character,
        });
        self.last_insert_time = Some(now);

        let new_len = buffer.grapheme_count(self.text_location.line_idx);
        if new_len.saturating_sub(old_len) > 0 {
            self.handle_move_command(Move::Right, buffer);
        }
        self.mark_redraw(true);
    }
    fn insert_newline(&mut self, buffer: &mut Buffer) {
        self.last_insert_time = None;
        self.last_insert_location = None;
        let grapheme_count_at_split = buffer.grapheme_count(self.text_location.line_idx);
        self.undo_stack.push(EditOperation::InsertNewLine {
            at: self.text_location,
            grapheme_count_at_split,
        });
        self.redo_stack.clear();
        buffer.insert_newline(self.text_location);
        self.handle_move_command(Move::Right, buffer);
        self.mark_redraw(true);
    }
    fn delete_backward(&mut self, buffer: &mut Buffer) {
        self.last_insert_time = None;
        self.last_insert_location = None;
        // Recording before moving, so we know the true deletion location
        if self.text_location.line_idx == 0 && self.text_location.grapheme_idx == 0 {
            return; // Nothing to do at start of document
        }

        if self.text_location.grapheme_idx == 0 {
            // Backspace at line start = merge this line onto previous
            // The "newline" being deleted is at the end of line_idx - 1
            let prev_line_idx = self.text_location.line_idx.saturating_sub(1);
            let split_at = buffer.grapheme_count(prev_line_idx);
            self.undo_stack.push(EditOperation::DeleteNewLine {
                line_idx: prev_line_idx,
                split_at_grapheme: split_at,
            });
            self.redo_stack.clear();
            // Move left (to end of previous line), then delete the newline via buffer.delete
            self.handle_move_command(Move::Left, buffer);
            buffer.delete(self.text_location); // deletes end-of-line = merges lines
        } else {
            self.handle_move_command(Move::Left, buffer);
            // Now at is the grapheme we want to delete
            if let Some(ch) = buffer.get_char_at(self.text_location) {
                self.undo_stack.push(EditOperation::DeleteChar {
                    at: self.text_location,
                    text: ch,
                });
                self.redo_stack.clear();
                buffer.delete(self.text_location);
            }
        }
        self.mark_redraw(true);
    }
    fn delete(&mut self, buffer: &mut Buffer) {
        self.last_insert_time = None;
        self.last_insert_location = None;
        let at = self.text_location;
        let grapheme_count = buffer.grapheme_count(at.line_idx);

        if at.grapheme_idx >= grapheme_count {
            // Delete at end-of-line = merge next line onto this one
            if at.line_idx.saturating_add(1) < buffer.height() {
                self.undo_stack.push(EditOperation::DeleteNewLine {
                    line_idx: at.line_idx,
                    split_at_grapheme: grapheme_count,
                });
                self.redo_stack.clear();
                buffer.delete(at);
            }
            // else: at end of last line, nothing to do
        } else if let Some(ch) = buffer.get_char_at(at) {
            self.undo_stack
                .push(EditOperation::DeleteChar { at, text: ch });
            self.redo_stack.clear();
            buffer.delete(at);
        }
        self.mark_redraw(true);
    }

    fn render_line(rect: Rect, row_offset: RowIdx, line_text: &str) -> Result<(), Error> {
        Terminal::print_rect(rect, row_offset, line_text)
    }
    fn build_welcome_message(width: usize) -> String {
        if width == 0 {
            return String::new();
        }
        let welcome_message = format!("{NAME} editor -- version {VERSION}");
        let len = welcome_message.len();
        let remaining_width = width.saturating_sub(1);
        if remaining_width < len {
            return "~".to_string();
        }
        format!("{:<1}{:^remaining_width$}", "~", welcome_message)
    }
    // hmm not so very simple undo redo anymore ig
    pub fn undo(&mut self, buffer: &mut Buffer) {
        if let Some(op) = self.undo_stack.pop() {
            self.apply_undo(&op, buffer);
            self.redo_stack.push(op);
            self.scroll_text_location_into_view(buffer);
            self.mark_redraw(true);
        }
    }

    pub fn redo(&mut self, buffer: &mut Buffer) {
        if let Some(op) = self.redo_stack.pop() {
            self.apply_redo(&op, buffer);
            self.undo_stack.push(op);
            self.scroll_text_location_into_view(buffer);
            self.mark_redraw(true);
        }
    }
    fn apply_undo(&mut self, op: &EditOperation, buffer: &mut Buffer) {
        match op {
            EditOperation::InsertChar { at, .. } => {
                // Undo an insert = delete the character that was inserted
                buffer.delete(*at);
                self.text_location = *at;
            }
            EditOperation::DeleteChar { at, text } => {
                // Undo a delete = re-insert the character
                buffer.insert_char(*text, *at);
                self.text_location = *at;
            }
            EditOperation::InsertNewLine {
                at,
                grapheme_count_at_split,
            } => {
                // Undo an Enter = merge the line that was split back together.
                // After insert_newline at `at`, the cursor moved to {line_idx+1, grapheme_idx:0}.
                // The split point is at `at.grapheme_idx` on line `at.line_idx`.
                // To undo: delete the newline = buffer.delete at end of at.line_idx
                buffer.delete(Location {
                    line_idx: at.line_idx,
                    grapheme_idx: *grapheme_count_at_split,
                });
                self.text_location = *at;
            }
            EditOperation::DeleteNewLine {
                line_idx,
                split_at_grapheme,
            } => {
                // Undo a newline deletion = re-split the merged line
                buffer.insert_newline(Location {
                    line_idx: *line_idx,
                    grapheme_idx: *split_at_grapheme,
                });
                self.text_location = Location {
                    line_idx: line_idx.saturating_add(1),
                    grapheme_idx: 0,
                };
            }
            EditOperation::InsertGroup { start, chars } => {
                // Delete all chars in the group, working backwards from the last
                // inserted position so byte indices stay valid.
                // The chars were inserted left-to-right starting at `start`.
                // After inserting N chars, the last char is at start.grapheme_idx + N - 1.
                let end_grapheme = start.grapheme_idx.saturating_add(chars.len());
                for grapheme_idx in (start.grapheme_idx..end_grapheme).rev() {
                    buffer.delete(Location {
                        line_idx: start.line_idx,
                        grapheme_idx,
                    });
                }
                self.text_location = *start;
            }
        }
    }
    fn apply_redo(&mut self, op: &EditOperation, buffer: &mut Buffer) {
        match op {
            EditOperation::InsertChar { at, text } => {
                buffer.insert_char(*text, *at);
                self.text_location = Location {
                    line_idx: at.line_idx,
                    grapheme_idx: at.grapheme_idx.saturating_add(1),
                };
            }
            EditOperation::DeleteChar { at, .. } => {
                buffer.delete(*at);
                self.text_location = *at;
            }
            EditOperation::InsertNewLine { at, .. } => {
                buffer.insert_newline(*at);
                self.text_location = Location {
                    line_idx: at.line_idx.saturating_add(1),
                    grapheme_idx: 0,
                };
            }
            EditOperation::DeleteNewLine {
                line_idx,
                split_at_grapheme,
            } => {
                // Redo the merge: delete the newline at end of line_idx
                buffer.delete(Location {
                    line_idx: *line_idx,
                    grapheme_idx: *split_at_grapheme,
                });
                self.text_location = Location {
                    line_idx: *line_idx,
                    grapheme_idx: *split_at_grapheme,
                };
            }
            EditOperation::InsertGroup { start, chars } => {
                // Re-insert chars left-to-right
                let mut current = *start;
                for &ch in chars {
                    buffer.insert_char(ch, current);
                    current.grapheme_idx = current.grapheme_idx.saturating_add(1);
                }
                self.text_location = current;
            }
        }
    }
    // SCROLLING
    fn scroll_vertically(&mut self, to: RowIdx) {
        let height = self.rect.size.height.saturating_sub(2); // new borders man
        if height == 0 {
            return;
        }
        let offset_changed = if to < self.scroll_offset.row {
            self.scroll_offset.row = to;
            true
        } else if to >= self.scroll_offset.row.saturating_add(height) {
            self.scroll_offset.row = to.saturating_sub(height).saturating_add(1);
            true
        } else {
            false
        };
        if offset_changed {
            self.mark_redraw(true);
        }
    }
    fn scroll_horizontally(&mut self, to: ColIdx) {
        // Wrapped text never scrolls horizontally: every visual column fits.
        if self.word_wrap && self.wrap_width() > 0 {
            if self.scroll_offset.col != 0 {
                self.scroll_offset.col = 0;
                self.mark_redraw(true);
            }
            return;
        }
        let width = self.rect.size.width.saturating_sub(2); // same for the width part
        let offset_changed = if to < self.scroll_offset.col {
            self.scroll_offset.col = to;
            true
        } else if to >= self.scroll_offset.col.saturating_add(width) {
            self.scroll_offset.col = to.saturating_sub(width).saturating_add(1);
            true
        } else {
            false
        };
        if offset_changed {
            self.mark_redraw(true);
        }
    }
    fn center_text_location(&mut self, buffer: &Buffer) {
        let height = self.rect.size.height.saturating_sub(2);
        let width = self.rect.size.width.saturating_sub(2);
        let Position { row, col } = self.text_location_to_position(buffer);
        let vertical_mid = height.div_ceil(2);
        let horizontal_mid = width.div_ceil(2);
        self.scroll_offset.row = row.saturating_sub(vertical_mid);
        // Wrapped text never scrolls horizontally (see `scroll_horizontally`).
        self.scroll_offset.col = if self.word_wrap && self.wrap_width() > 0 {
            0
        } else {
            col.saturating_sub(horizontal_mid)
        };
        self.mark_redraw(true);
    }
    fn scroll_text_location_into_view(&mut self, buffer: &Buffer) {
        let Position { row, col } = self.text_location_to_position(buffer);
        self.scroll_vertically(row);
        self.scroll_horizontally(col);
    }
    /// Pin the cursor's visual row to the vertical center of the content
    /// area (typewriter scrolling, `PLAN.md Phase 4.2`). Idempotent: marks
    /// redraw only when the offset actually moves.
    pub fn apply_typewriter(&mut self, buffer: &Buffer) {
        let height = self.rect.size.height.saturating_sub(2);
        if height == 0 {
            return;
        }
        let width = self.wrap_width();
        let (visual_row, _) = if !self.word_wrap || width == 0 {
            (self.text_location.line_idx, 0)
        } else {
            self.location_to_visual(buffer, self.text_location, width)
        };
        let target = visual_row.saturating_sub(height / 2);
        if target != self.scroll_offset.row {
            self.scroll_offset.row = target;
            self.mark_redraw(true);
        }
    }
    pub fn caret_position(&self, buffer: &Buffer) -> Position {
        let Position { col, row } = self.text_location_to_position(buffer);
        let relative_row = row.saturating_sub(self.scroll_offset.row);
        let relative_col = col.saturating_sub(self.scroll_offset.col);

        let max_row = self.rect.size.height.saturating_sub(3);
        let max_col = self.rect.size.width.saturating_sub(3);

        let clamped_row = min(relative_row, max_row);
        let clamped_col = min(relative_col, max_col);
        Position {
            col: clamped_col
                .saturating_add(self.rect.position.col)
                .saturating_add(1),
            row: clamped_row
                .saturating_add(self.rect.position.row)
                .saturating_add(1),
        }
    }
    fn text_location_to_position(&self, buffer: &Buffer) -> Position {
        let width = self.wrap_width();
        if !self.word_wrap || width == 0 {
            // Unwrapped (or sizeless, e.g. unit tests): 1 line == 1 row.
            let row = self.text_location.line_idx;
            debug_assert!(row.saturating_sub(1) <= buffer.height());
            let col = buffer.width_until(row, self.text_location.grapheme_idx);
            return Position { col, row };
        }
        let (row, col) = self.location_to_visual(buffer, self.text_location, width);
        Position { col, row }
    }

    /// Display-column budget for one visual row (pane width minus borders).
    /// `0` when the view has no size yet — callers treat that as unwrapped.
    fn wrap_width(&self) -> usize {
        self.rect.size.width.saturating_sub(2)
    }

    /// Wrap segments for one buffer line (single whole-line segment when
    /// wrapping is off or the view is sizeless).
    fn segments_for_line(
        &self,
        buffer: &Buffer,
        line_idx: usize,
        width: usize,
    ) -> Vec<std::ops::Range<usize>> {
        let Some(line) = buffer.get_line(line_idx) else {
            return vec![0..0];
        };
        if !self.word_wrap || width == 0 {
            return vec![0..line.grapheme_count()];
        }
        line.wrap_segments(width)
    }

    /// Buffer location → (visual row, visual column).
    fn location_to_visual(
        &self,
        buffer: &Buffer,
        location: Location,
        width: usize,
    ) -> (usize, usize) {
        let mut visual_row: usize = 0;
        for line_idx in 0..location.line_idx {
            visual_row = visual_row
                .saturating_add(self.segments_for_line(buffer, line_idx, width).len().max(1));
        }
        let mut seg_idx: usize = 0;
        let mut seg_start_col = 0;
        for (i, seg) in self
            .segments_for_line(buffer, location.line_idx, width)
            .iter()
            .enumerate()
        {
            if location.grapheme_idx >= seg.start && location.grapheme_idx <= seg.end {
                seg_idx = i;
                seg_start_col = buffer.width_until(location.line_idx, seg.start);
                // Caret at EOL belongs to the last segment covering it.
                if location.grapheme_idx == seg.end {
                    continue;
                }
                break;
            }
        }
        let col = buffer
            .width_until(location.line_idx, location.grapheme_idx)
            .saturating_sub(seg_start_col);
        (visual_row.saturating_add(seg_idx), col)
    }

    /// (visual row, visual column) → buffer location, clamped to content.
    fn visual_to_location(
        &self,
        buffer: &Buffer,
        visual_row: usize,
        visual_col: usize,
        width: usize,
    ) -> Location {
        let mut remaining = visual_row;
        let height = buffer.height();
        for line_idx in 0..height {
            let segments = self.segments_for_line(buffer, line_idx, width);
            let segs = segments.len().max(1);
            if remaining < segs {
                let seg = segments.get(remaining).cloned().unwrap_or(0..0);
                let seg_start_col = buffer.width_until(line_idx, seg.start);
                let target_col = seg_start_col.saturating_add(visual_col);
                // First grapheme whose column reaches the target (snap right).
                let mut grapheme_idx = seg.end;
                let mut g = seg.start;
                while g < seg.end {
                    if buffer.width_until(line_idx, g.saturating_add(1)) >= target_col {
                        grapheme_idx = g.saturating_add(1).min(seg.end);
                        // Prefer landing ON the target column: step back when the
                        // grapheme overshoots a wide char boundary.
                        if buffer.width_until(line_idx, grapheme_idx) > target_col
                            && grapheme_idx > seg.start
                        {
                            grapheme_idx = grapheme_idx.saturating_sub(1);
                        }
                        break;
                    }
                    g = g.saturating_add(1);
                }
                return Location {
                    line_idx,
                    grapheme_idx,
                };
            }
            remaining = remaining.saturating_sub(segs);
        }
        // Past EOF: end of last line.
        let last = height.saturating_sub(1);
        Location {
            line_idx: last,
            grapheme_idx: buffer.grapheme_count(last),
        }
    }

    /// Visual rows in `[start, start + count)` as (line, grapheme-range).
    fn visual_rows_for_window(
        &self,
        buffer: &Buffer,
        width: usize,
        start: usize,
        count: usize,
    ) -> Vec<(usize, std::ops::Range<usize>)> {
        let mut rows = Vec::new();
        if count == 0 {
            return rows;
        }
        let mut visual_row = 0;
        for line_idx in 0..buffer.height() {
            for seg in self.segments_for_line(buffer, line_idx, width) {
                if visual_row >= start && rows.len() < count {
                    rows.push((line_idx, seg));
                }
                visual_row = visual_row.saturating_add(1);
                if rows.len() >= count {
                    return rows;
                }
            }
        }
        rows
    }

    fn move_up(&mut self, step: usize, buffer: &Buffer) {
        let width = self.wrap_width();
        if !self.word_wrap || width == 0 {
            self.text_location.line_idx = self.text_location.line_idx.saturating_sub(step);
            self.snap_to_valid_grapheme(buffer);
            return;
        }
        let (visual_row, visual_col) = self.location_to_visual(buffer, self.text_location, width);
        self.text_location =
            self.visual_to_location(buffer, visual_row.saturating_sub(step), visual_col, width);
        self.snap_to_valid_grapheme(buffer);
    }
    fn move_down(&mut self, step: usize, buffer: &Buffer) {
        let width = self.wrap_width();
        if !self.word_wrap || width == 0 {
            self.text_location.line_idx = self.text_location.line_idx.saturating_add(step);
            self.snap_to_valid_line(buffer);
            self.snap_to_valid_grapheme(buffer);
            return;
        }
        let (visual_row, visual_col) = self.location_to_visual(buffer, self.text_location, width);
        self.text_location =
            self.visual_to_location(buffer, visual_row.saturating_add(step), visual_col, width);
        self.snap_to_valid_line(buffer);
        self.snap_to_valid_grapheme(buffer);
    }
    // clippy::arithmetic_side_effects: This function performs arithmetic calculations
    // after explicitly checking that the target value will be within bounds.
    #[allow(clippy::arithmetic_side_effects)]
    fn move_right(&mut self, buffer: &Buffer) {
        let grapheme_count = buffer.grapheme_count(self.text_location.line_idx);
        if self.text_location.grapheme_idx < grapheme_count {
            self.text_location.grapheme_idx += 1;
        } else {
            self.move_to_start_of_line();
            self.move_down(1, buffer);
        }
    }
    #[allow(clippy::arithmetic_side_effects)]
    fn move_left(&mut self, buffer: &Buffer) {
        if self.text_location.grapheme_idx > 0 {
            self.text_location.grapheme_idx -= 1;
        } else if self.text_location.line_idx > 0 {
            self.move_up(1, buffer);
            self.move_to_end_of_line(buffer);
        }
    }

    fn move_to_start_of_line(&mut self) {
        self.text_location.grapheme_idx = 0;
    }
    fn move_to_end_of_line(&mut self, buffer: &Buffer) {
        self.text_location.grapheme_idx = buffer.grapheme_count(self.text_location.line_idx);
    }

    // Ensures self.location.grapheme_idx points to a valid grapheme index by snapping it to the left most grapheme if appropriate.
    // Doesn't trigger scrolling.
    fn snap_to_valid_grapheme(&mut self, buffer: &Buffer) {
        self.text_location.grapheme_idx = min(
            self.text_location.grapheme_idx,
            buffer.grapheme_count(self.text_location.line_idx),
        )
    }
    // Ensures self.location.line_idx points to a valid line index by snapping it to the bottom most line if appropriate.
    // Doesn't trigger scrolling.
    // Valid lines are `0..height` (i.e. max `height - 1`); `height` itself
    // is one past EOF and must never be a resting cursor position.
    fn snap_to_valid_line(&mut self, buffer: &Buffer) {
        let height = buffer.height();
        if height == 0 {
            self.text_location.line_idx = 0;
        } else {
            self.text_location.line_idx =
                min(self.text_location.line_idx, height.saturating_sub(1));
        }
    }
    // region : Search
    pub fn enter_search(&mut self) {
        self.search_info = Some(SearchInfo {
            prev_location: self.text_location,
            prev_scroll_offset: self.scroll_offset,
            query: None,
        });
    }
    pub fn exit_search(&mut self) {
        self.search_info = None;
        self.mark_redraw(true);
    }
    pub fn dismiss_search(&mut self, buffer: &Buffer) {
        if let Some(search_info) = &self.search_info {
            self.text_location = search_info.prev_location;
            self.scroll_offset = search_info.prev_scroll_offset;
            self.scroll_text_location_into_view(buffer);
        }
        self.exit_search();
    }
    pub fn search(&mut self, query: &str, buffer: &Buffer) {
        if let Some(search_info) = &mut self.search_info {
            search_info.query = Some(Line::from(query));
        }
        self.search_in_direction(self.text_location, SearchDirection::default(), buffer);
    }
    fn get_search_query(&self) -> Option<&Line> {
        let query = self
            .search_info
            .as_ref()
            .and_then(|search_info| search_info.query.as_ref());

        debug_assert!(
            query.is_some(),
            "Attempting to search with malformed searchinfo present"
        );
        query
    }
    fn search_in_direction(&mut self, from: Location, direction: SearchDirection, buffer: &Buffer) {
        if let Some(location) = self.get_search_query().and_then(|query| {
            if query.is_empty() {
                None
            } else if direction == SearchDirection::Forward {
                buffer.search_forward(query, from)
            } else {
                buffer.search_backward(query, from)
            }
        }) {
            self.text_location = location;
            self.center_text_location(buffer);
        };
        self.mark_redraw(true);
    }
    pub fn search_next(&mut self, buffer: &Buffer) {
        let step_right = self
            .get_search_query()
            .map_or(1, |query| min(query.grapheme_count(), 1));
        let location = Location {
            line_idx: self.text_location.line_idx,
            grapheme_idx: self.text_location.grapheme_idx.saturating_add(step_right),
        };
        self.search_in_direction(location, SearchDirection::Forward, buffer);
    }
    pub fn search_prev(&mut self, buffer: &Buffer) {
        self.search_in_direction(self.text_location, SearchDirection::Backward, buffer);
    }
    // endregion

    pub fn draw_content_with_buffer(&mut self, rect: Rect, buffer: &Buffer) -> Result<(), Error> {
        let Size { height, width } = rect.size;

        if height == 0 || width == 0 {
            return Ok(());
        }

        let _origin_row = rect.position.row;

        let top_third = height.div_ceil(3);

        let scroll_top = self.scroll_offset.row;

        let query = self
            .search_info
            .as_ref()
            .and_then(|search_info| search_info.query.as_deref());

        let selected_match = query.is_some().then_some(self.text_location);

        let mut highlighter = Highlighter::new(
            query,
            selected_match,
            buffer.get_file_info().get_file_type(),
        );

        // Highlight from line 0: `Highlighter` requires sequential
        // `highlight()` calls per line index (`debug_assert_eq!` in core).
        // `height` visual rows touch at most `height` buffer lines.
        let visual_rows = self.visual_rows_for_window(buffer, width, scroll_top, height);
        let end_line_idx = visual_rows
            .last()
            .map_or(0, |(line_idx, _)| line_idx.saturating_add(1));
        for current_row in 0..end_line_idx {
            buffer.highlight(current_row, &mut highlighter);
        }

        // render inside content area (one wrapped segment per screen row)
        for screen_row in 0..height {
            let Some((line_idx, seg)) = visual_rows.get(screen_row) else {
                // Past content: welcome message once on empty docs, else `~`.
                if screen_row == top_third && buffer.is_empty() {
                    Self::render_line(rect, screen_row, &Self::build_welcome_message(width))?;
                } else {
                    Self::render_line(rect, screen_row, "~")?;
                }
                continue;
            };
            let line_idx = *line_idx;
            let seg_start_col = buffer.width_until(line_idx, seg.start);
            let seg_end_col = buffer.width_until(line_idx, seg.end);
            // Unwrapped: apply the horizontal scroll window, as before.
            let (left, right) = if self.word_wrap {
                (seg_start_col, seg_end_col)
            } else {
                let left = self.scroll_offset.col;
                (
                    seg_start_col.max(left),
                    seg_end_col.min(left.saturating_add(width)),
                )
            };

            if left < right {
                if let Some(annotated_string) =
                    buffer.get_highlighted_column_range(line_idx, left..right, &highlighter)
                {
                    Terminal::print_annotated_rect(rect, screen_row, &annotated_string)?;
                    continue;
                }
            }
            // Empty segment (blank buffer line) renders as a cleared row.
            Self::render_line(rect, screen_row, "")?;
        }

        Ok(())
    }
}

impl UIComponent for View {
    fn mark_redraw(&mut self, value: bool) {
        self.needs_redraw = value;
    }

    fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }

    fn set_size(&mut self, rect: Rect) {
        self.rect = rect;
        // self.scroll_text_location_into_view(); // Needs buffer
    }

    fn rect(&self) -> Rect {
        self.rect
    }

    fn draw(&mut self) -> Result<(), Error> {
        // This method cannot be implemented without a Buffer anymore.
        // We should probably remove View's UIComponent impl or change it.
        // For now, it will be a no-op and we call draw_with_buffer manually.
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use yonro_core::command::Edit;

    // Helper: build a View and a Buffer
    fn setup_view_and_buffer(text: &str) -> (View, Buffer) {
        let mut view = View::default();
        let mut buffer = Buffer::default();
        view.set_size(Rect {
            position: Position { row: 0, col: 0 },
            size: Size {
                height: 24,
                width: 80,
            },
        });
        for ch in text.chars() {
            if ch == '\n' {
                view.handle_edit_command(
                    Edit::InsertNewLine,
                    &mut buffer,
                    &mut crate::clipboard::MemClipboard::default(),
                );
            } else {
                view.handle_edit_command(
                    Edit::Insert(ch),
                    &mut buffer,
                    &mut crate::clipboard::MemClipboard::default(),
                );
            }
        }
        view.undo_stack.clear();
        view.redo_stack.clear();
        view.text_location = Location {
            line_idx: 0,
            grapheme_idx: 0,
        };
        (view, buffer)
    }

    #[test]
    fn undo_single_insert() {
        let mut view = View::default();
        let mut buffer = Buffer::default();
        view.handle_edit_command(
            Edit::Insert('a'),
            &mut buffer,
            &mut crate::clipboard::MemClipboard::default(),
        );
        assert_eq!(buffer.grapheme_count(0), 1);

        view.undo(&mut buffer);
        assert_eq!(buffer.grapheme_count(0), 0);
    }

    #[test]
    fn redo_single_insert() {
        let mut view = View::default();
        let mut buffer = Buffer::default();
        view.handle_edit_command(
            Edit::Insert('a'),
            &mut buffer,
            &mut crate::clipboard::MemClipboard::default(),
        );
        view.undo(&mut buffer);
        view.redo(&mut buffer);
        assert_eq!(buffer.grapheme_count(0), 1);
    }

    #[test]
    fn undo_multiple_inserts() {
        let mut view = View::default();
        let mut buffer = Buffer::default();
        for ch in "hello".chars() {
            view.handle_edit_command(
                Edit::Insert(ch),
                &mut buffer,
                &mut crate::clipboard::MemClipboard::default(),
            );
        }
        assert_eq!(buffer.grapheme_count(0), 5);

        for _ in 0..5 {
            view.undo(&mut buffer);
        }
        assert_eq!(buffer.grapheme_count(0), 0);
    }

    #[test]
    fn undo_newline_merges_lines() {
        let mut view = View::default();
        let mut buffer = Buffer::default();
        for ch in "hello".chars() {
            view.handle_edit_command(
                Edit::Insert(ch),
                &mut buffer,
                &mut crate::clipboard::MemClipboard::default(),
            );
        }
        view.handle_edit_command(
            Edit::InsertNewLine,
            &mut buffer,
            &mut crate::clipboard::MemClipboard::default(),
        );
        assert_eq!(buffer.height(), 2);

        view.undo(&mut buffer);
        assert_eq!(buffer.height(), 1);
        assert_eq!(buffer.grapheme_count(0), 5);
    }

    #[test]
    fn delete_undo_redo() {
        let (mut view, mut buffer) = setup_view_and_buffer("hi");
        view.handle_edit_command(
            Edit::Delete,
            &mut buffer,
            &mut crate::clipboard::MemClipboard::default(),
        );
        assert_eq!(buffer.grapheme_count(0), 1);

        view.undo(&mut buffer);
        assert_eq!(buffer.grapheme_count(0), 2);

        view.redo(&mut buffer);
        assert_eq!(buffer.grapheme_count(0), 1);
    }

    #[test]
    fn rapid_inserts_form_a_group() {
        let mut view = View::default();
        let mut buffer = Buffer::default();
        for ch in "hello".chars() {
            view.handle_edit_command(
                Edit::Insert(ch),
                &mut buffer,
                &mut crate::clipboard::MemClipboard::default(),
            );
            view.last_insert_time = Some(std::time::Instant::now());
            view.last_insert_location = Some(view.text_location);
        }
        assert_eq!(view.undo_stack.len(), 1);
        view.undo(&mut buffer);
        assert_eq!(buffer.grapheme_count(0), 0);
    }

    #[test]
    fn move_left_wrap_around() {
        let (mut view, buffer) = setup_view_and_buffer("a\nb");
        // Position at start of second line
        view.text_location = Location {
            line_idx: 1,
            grapheme_idx: 0,
        };
        view.handle_move_command(Move::Left, &buffer);
        assert_eq!(view.text_location.line_idx, 0);
        assert_eq!(view.text_location.grapheme_idx, 1);
    }

    #[test]
    fn move_down_at_eof_stays_on_last_line() {
        let (mut view, buffer) = setup_view_and_buffer("a\nb");
        assert_eq!(buffer.height(), 2);
        view.text_location = Location {
            line_idx: 1,
            grapheme_idx: 0,
        };
        view.handle_move_command(Move::Down, &buffer);
        assert_eq!(view.text_location.line_idx, 1);
        // Pushing far past EOF must also clamp, never rest on the phantom line.
        view.text_location = Location {
            line_idx: 99,
            grapheme_idx: 0,
        };
        view.handle_move_command(Move::Down, &buffer);
        assert_eq!(view.text_location.line_idx, 1);
    }

    // Soft-wrap tests (PLAN.md Phase 4.1). Narrow the view to force wraps.
    fn setup_wrapped_view(text: &str, wrap_cols: usize) -> (View, Buffer) {
        let (mut view, buffer) = setup_view_and_buffer(text);
        view.set_size(Rect {
            position: Position { row: 0, col: 0 },
            size: Size {
                height: 24,
                width: wrap_cols.saturating_add(2),
            },
        });
        assert!(view.word_wrap);
        (view, buffer)
    }

    #[test]
    fn wrap_location_to_visual_row() {
        // "hello world foo" @ wrap width 10 → rows [0..6][6..12][12..15].
        let (view, buffer) = setup_wrapped_view("hello world foo", 8);
        let loc = |grapheme_idx: usize| Location {
            line_idx: 0,
            grapheme_idx,
        };
        assert_eq!(view.location_to_visual(&buffer, loc(0), 8), (0, 0));
        assert_eq!(view.location_to_visual(&buffer, loc(5), 8), (0, 5));
        assert_eq!(view.location_to_visual(&buffer, loc(6), 8), (1, 0));
        assert_eq!(view.location_to_visual(&buffer, loc(11), 8), (1, 5));
        assert_eq!(view.location_to_visual(&buffer, loc(14), 8), (2, 2));
    }

    #[test]
    fn wrap_move_down_steps_visual_rows() {
        let (mut view, buffer) = setup_wrapped_view("hello world foo", 8);
        view.text_location = Location {
            line_idx: 0,
            grapheme_idx: 0,
        };
        view.handle_move_command(Move::Down, &buffer);
        assert_eq!(view.text_location.line_idx, 0);
        assert_eq!(view.text_location.grapheme_idx, 6);
        view.handle_move_command(Move::Down, &buffer);
        assert_eq!(view.text_location.grapheme_idx, 12);
    }

    #[test]
    fn wrap_move_up_from_second_row() {
        let (mut view, buffer) = setup_wrapped_view("hello world foo", 8);
        view.text_location = Location {
            line_idx: 0,
            grapheme_idx: 8,
        };
        view.handle_move_command(Move::Up, &buffer);
        assert_eq!(view.text_location.line_idx, 0);
        assert_eq!(view.text_location.grapheme_idx, 2);
    }

    #[test]
    fn wrap_caret_follows_visual_position() {
        let (mut view, buffer) = setup_wrapped_view("hello world foo", 8);
        view.text_location = Location {
            line_idx: 0,
            grapheme_idx: 7,
        };
        // Visual row 1, visual col 1, plus pane offsets (+1) each.
        let caret = view.caret_position(&buffer);
        assert_eq!(caret.row, 2);
        assert_eq!(caret.col, 2);
    }

    #[test]
    fn wrap_off_preserves_line_mapping() {
        let (mut view, buffer) = setup_wrapped_view("hello world foo", 8);
        view.word_wrap = false;
        view.text_location = Location {
            line_idx: 0,
            grapheme_idx: 7,
        };
        let caret = view.caret_position(&buffer);
        assert_eq!(caret.row, 1);
        assert_eq!(caret.col, 8);
    }

    #[test]
    fn wrap_window_lists_segments_in_order() {
        let (view, buffer) = setup_wrapped_view("hello world foo\nxy", 8);
        let rows = view.visual_rows_for_window(&buffer, 8, 0, 4);
        assert_eq!(rows, vec![(0, 0..6), (0, 6..12), (0, 12..15), (1, 0..2)]);
        // Window starting mid-line skips earlier segments.
        let rows = view.visual_rows_for_window(&buffer, 8, 2, 2);
        assert_eq!(rows, vec![(0, 12..15), (1, 0..2)]);
    }

    #[test]
    fn typewriter_pins_cursor_to_vertical_center() {
        // 30 short lines; cursor on line 20, 24-row view → content height 22.
        let text = (0..30).map(|_| "x").collect::<Vec<_>>().join("\n");
        let (mut view, buffer) = setup_view_and_buffer(&text);
        view.text_location = Location {
            line_idx: 20,
            grapheme_idx: 0,
        };
        view.apply_typewriter(&buffer);
        assert_eq!(view.scroll_offset.row, 20 - 22 / 2);
        // Idempotent: second call moves nothing and marks nothing.
        view.mark_redraw(false);
        view.apply_typewriter(&buffer);
        assert_eq!(view.scroll_offset.row, 9);
        assert!(!view.needs_redraw());
    }
    #[test]
    fn typewriter_near_top_clamps_to_zero() {
        let text = (0..30).map(|_| "x").collect::<Vec<_>>().join("\n");
        let (mut view, buffer) = setup_view_and_buffer(&text);
        view.text_location = Location {
            line_idx: 3,
            grapheme_idx: 0,
        };
        view.apply_typewriter(&buffer);
        assert_eq!(view.scroll_offset.row, 0);
    }

    // Clipboard tests (`PLAN.md Phase 4.4`, line-based: no selection model).
    #[test]
    fn copy_line_preserves_buffer() {
        let (mut view, mut buffer) = setup_view_and_buffer("hello\nworld");
        let mut clip = crate::clipboard::MemClipboard::default();
        view.text_location = Location {
            line_idx: 1,
            grapheme_idx: 0,
        };
        view.handle_edit_command(Edit::Copy, &mut buffer, &mut clip);
        assert_eq!(clip.text(), Some("world"));
        assert_eq!(buffer.height(), 2);
    }

    #[test]
    fn cut_first_line_joins_next() {
        let (mut view, mut buffer) = setup_view_and_buffer("hello\nworld");
        let mut clip = crate::clipboard::MemClipboard::default();
        view.text_location = Location {
            line_idx: 0,
            grapheme_idx: 2,
        };
        view.handle_edit_command(Edit::Cut, &mut buffer, &mut clip);
        assert_eq!(clip.text(), Some("hello"));
        assert_eq!(buffer.height(), 1);
        assert_eq!(buffer.get_line(0).unwrap().to_string(), "world");
    }

    #[test]
    fn cut_last_line_leaves_empty_line() {
        let (mut view, mut buffer) = setup_view_and_buffer("hello\nworld");
        let mut clip = crate::clipboard::MemClipboard::default();
        view.text_location = Location {
            line_idx: 1,
            grapheme_idx: 0,
        };
        view.handle_edit_command(Edit::Cut, &mut buffer, &mut clip);
        assert_eq!(clip.text(), Some("world"));
        assert_eq!(buffer.height(), 2);
        assert_eq!(buffer.get_line(1).unwrap().to_string(), "");
    }

    #[test]
    fn paste_inserts_inline() {
        let (mut view, mut buffer) = setup_view_and_buffer("abcd");
        let mut clip = crate::clipboard::MemClipboard::with_text("XY");
        view.text_location = Location {
            line_idx: 0,
            grapheme_idx: 1,
        };
        view.handle_edit_command(Edit::Paste, &mut buffer, &mut clip);
        assert_eq!(buffer.get_line(0).unwrap().to_string(), "aXYbcd");
    }

    #[test]
    fn paste_splits_lines() {
        let (mut view, mut buffer) = setup_view_and_buffer("abcd");
        let mut clip = crate::clipboard::MemClipboard::with_text("X\nY");
        view.text_location = Location {
            line_idx: 0,
            grapheme_idx: 1,
        };
        view.handle_edit_command(Edit::Paste, &mut buffer, &mut clip);
        assert_eq!(buffer.height(), 2);
        assert_eq!(buffer.get_line(0).unwrap().to_string(), "aX");
        assert_eq!(buffer.get_line(1).unwrap().to_string(), "Ybcd");
    }

    #[test]
    fn paste_empty_clipboard_is_noop() {
        let (mut view, mut buffer) = setup_view_and_buffer("abcd");
        let mut clip = crate::clipboard::MemClipboard::default();
        assert!(!view.paste(&mut buffer, &mut clip));
        assert_eq!(buffer.get_line(0).unwrap().to_string(), "abcd");
    }
}
