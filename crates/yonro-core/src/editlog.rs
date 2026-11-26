//! Operation undo for the page.
//!
//! A typing burst within 800ms is one step. A paste is one step. Deletes
//! remove whole graphemes, never a partial cluster. The TUI keeps its own
//! undo stack.

use std::time::{Duration, Instant};

use unicode_segmentation::UnicodeSegmentation;

use super::buffers::Buffer;
use super::format::{flat_index, flat_len, Format};
use super::prelude::Location;

/// How long successive character inserts stay one undo step.
const BURST: Duration = Duration::from_millis(800);

/// One recorded change. The deleted or inserted text is what undo restores.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditOp {
    /// One keyboard character.
    InsertChar {
        /// Grapheme location before the insert.
        at: Location,
        /// The character.
        text: char,
    },
    /// Several characters typed as one burst.
    InsertGroup {
        /// Location of the first character.
        start: Location,
        /// Characters in order, one line.
        text: String,
    },
    /// A paste or other multi-character insert, possibly with newlines.
    InsertText {
        /// Location before the insert.
        at: Location,
        /// Inserted text, newlines normalized to `\n`.
        text: String,
    },
    /// A grapheme span that was removed. `text` includes `\n` between lines.
    DeleteRange {
        /// Start of the removed span.
        start: Location,
        /// Exact text removed.
        text: String,
    },
    /// Enter.
    InsertNewLine {
        /// Caret before the split.
        at: Location,
        /// Grapheme count of that line before the split.
        grapheme_count_at_split: usize,
    },
    /// Backspace that joined two lines.
    DeleteNewLine {
        /// Line that kept the joined text.
        line_idx: usize,
        /// Grapheme where the next line was joined.
        split_at_grapheme: usize,
    },
}

/// Undo and redo stacks of [`EditOp`].
#[derive(Debug, Clone, Default)]
pub struct EditLog {
    undo: Vec<EditOp>,
    redo: Vec<EditOp>,
    last_insert: Option<Instant>,
    format: Format,
    undo_formats: Vec<Format>,
    redo_formats: Vec<Format>,
}

impl EditLog {
    /// Empty history.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether undo has a step.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether redo has a step.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Marks for the open scene.
    #[must_use]
    pub const fn format(&self) -> &Format {
        &self.format
    }

    /// Marks for the open scene.
    pub fn format_mut(&mut self) -> &mut Format {
        &mut self.format
    }

    /// Insert one character and record it. Returns the caret after the insert.
    pub fn insert_char(
        &mut self,
        buffer: &mut Buffer,
        at: Location,
        text: char,
        now: Instant,
    ) -> Location {
        let before = buffer.grapheme_count(at.line_idx);
        let flat = flat_index(buffer, at);
        buffer.insert_char(text, at);
        let after = buffer.grapheme_count(at.line_idx);
        if after == before {
            return at;
        }
        self.note_insert(at, text, now);
        self.format.shift_insert(flat, 1);
        Location {
            line_idx: at.line_idx,
            grapheme_idx: at.grapheme_idx.saturating_add(1),
        }
    }

    /// Insert a string as one undo step. `\r\n` and `\r` become `\n`.
    pub fn insert_text(
        &mut self,
        buffer: &mut Buffer,
        at: Location,
        text: &str,
        _now: Instant,
    ) -> Location {
        let text = normalize_newlines(text);
        if text.is_empty() {
            return at;
        }
        let flat = flat_index(buffer, at);
        let len = flat_len(&text);
        self.checkpoint();
        buffer.insert_str(at, &text);
        self.format.shift_insert(flat, len);
        let caret = end_after_insert(at, &text);
        self.redo.clear();
        self.undo.push(EditOp::InsertText { at, text });
        caret
    }

    /// Delete the half-open grapheme span `[start, end)`.
    ///
    /// Returns the caret, which is `start`. An empty span records nothing.
    pub fn delete_range(
        &mut self,
        buffer: &mut Buffer,
        start: Location,
        end: Location,
        _now: Instant,
    ) -> Location {
        let (start, end) = order_span(start, end);
        let text = range_text(buffer, start, end);
        if text.is_empty() {
            return start;
        }
        let flat = flat_index(buffer, start);
        let len = flat_len(&text);
        self.checkpoint();
        delete_span(buffer, start, end);
        self.format.shift_delete(flat, flat.saturating_add(len));
        self.redo.clear();
        self.undo.push(EditOp::DeleteRange { start, text });
        start
    }

    /// Split the line at `at`.
    pub fn insert_newline(&mut self, buffer: &mut Buffer, at: Location, _now: Instant) -> Location {
        let flat = flat_index(buffer, at);
        self.checkpoint();
        let grapheme_count_at_split = buffer.grapheme_count(at.line_idx);
        buffer.insert_newline(at);
        self.format.shift_insert(flat, 1);
        self.format.split_line(at.line_idx);
        self.redo.clear();
        self.undo.push(EditOp::InsertNewLine {
            at,
            grapheme_count_at_split,
        });
        Location {
            line_idx: at.line_idx.saturating_add(1),
            grapheme_idx: 0,
        }
    }

    /// Join `line_idx` with the next line. `split_at_grapheme` is where the
    /// next line's text begins after the join (usually the end of `line_idx`).
    pub fn delete_newline(
        &mut self,
        buffer: &mut Buffer,
        line_idx: usize,
        _now: Instant,
    ) -> Location {
        let split_at_grapheme = buffer.grapheme_count(line_idx);
        let flat = flat_index(
            buffer,
            Location {
                grapheme_idx: split_at_grapheme,
                line_idx,
            },
        );
        self.checkpoint();
        buffer.delete(Location {
            line_idx,
            grapheme_idx: split_at_grapheme,
        });
        self.format.shift_delete(flat, flat.saturating_add(1));
        self.format.join_line(line_idx);
        self.redo.clear();
        self.undo.push(EditOp::DeleteNewLine {
            line_idx,
            split_at_grapheme,
        });
        Location {
            line_idx,
            grapheme_idx: split_at_grapheme,
        }
    }

    /// Undo one step. Returns the caret to place, if a step existed.
    pub fn undo(&mut self, buffer: &mut Buffer) -> Option<Location> {
        let op = self.undo.pop()?;
        let before = self.undo_formats.pop().unwrap_or_default();
        self.redo_formats.push(self.format.clone());
        self.format = before;
        let caret = undo_op(buffer, &op);
        self.redo.push(op);
        self.last_insert = None;
        Some(caret)
    }

    /// Redo one step. Returns the caret to place, if a step existed.
    pub fn redo(&mut self, buffer: &mut Buffer) -> Option<Location> {
        let op = self.redo.pop()?;
        let after = self.redo_formats.pop().unwrap_or_default();
        self.undo_formats.push(self.format.clone());
        self.format = after;
        let caret = redo_op(buffer, &op);
        self.undo.push(op);
        self.last_insert = None;
        Some(caret)
    }

    fn note_insert(&mut self, at: Location, text: char, now: Instant) {
        self.redo.clear();
        self.redo_formats.clear();
        let burst = self
            .last_insert
            .is_some_and(|last| now.saturating_duration_since(last) <= BURST);
        if burst {
            if let Some(EditOp::InsertGroup { start, text: group }) = self.undo.last_mut() {
                let continues = at.line_idx == start.line_idx
                    && at.grapheme_idx
                        == start
                            .grapheme_idx
                            .saturating_add(group.graphemes(true).count());
                if continues {
                    group.push(text);
                    self.last_insert = Some(now);
                    return;
                }
            }
            if let Some(EditOp::InsertChar {
                at: prev,
                text: prev_text,
            }) = self.undo.last().cloned()
            {
                let continues = at.line_idx == prev.line_idx
                    && at.grapheme_idx == prev.grapheme_idx.saturating_add(1);
                if continues {
                    self.undo.pop();
                    let mut grouped = String::new();
                    grouped.push(prev_text);
                    grouped.push(text);
                    self.undo.push(EditOp::InsertGroup {
                        start: prev,
                        text: grouped,
                    });
                    self.last_insert = Some(now);
                    return;
                }
            }
        }
        self.undo_formats.push(self.format.clone());
        self.undo.push(EditOp::InsertChar { at, text });
        self.last_insert = Some(now);
    }

    fn checkpoint(&mut self) {
        self.break_burst();
        self.redo.clear();
        self.redo_formats.clear();
        self.undo_formats.push(self.format.clone());
    }

    fn break_burst(&mut self) {
        self.last_insert = None;
    }
}

fn undo_op(buffer: &mut Buffer, op: &EditOp) -> Location {
    match op {
        EditOp::InsertChar { at, .. } => {
            buffer.delete(*at);
            *at
        }
        EditOp::InsertGroup { start, text } => {
            let end = end_after_insert(*start, text);
            delete_span(buffer, *start, end);
            *start
        }
        EditOp::InsertText { at, text } => {
            let end = end_after_insert(*at, text);
            delete_span(buffer, *at, end);
            *at
        }
        EditOp::DeleteRange { start, text } => {
            buffer.insert_str(*start, text);
            end_after_insert(*start, text)
        }
        EditOp::InsertNewLine {
            at,
            grapheme_count_at_split,
        } => {
            buffer.delete(Location {
                line_idx: at.line_idx,
                grapheme_idx: *grapheme_count_at_split,
            });
            *at
        }
        EditOp::DeleteNewLine {
            line_idx,
            split_at_grapheme,
        } => {
            buffer.insert_newline(Location {
                line_idx: *line_idx,
                grapheme_idx: *split_at_grapheme,
            });
            Location {
                line_idx: line_idx.saturating_add(1),
                grapheme_idx: 0,
            }
        }
    }
}

fn redo_op(buffer: &mut Buffer, op: &EditOp) -> Location {
    match op {
        EditOp::InsertChar { at, text } => {
            buffer.insert_char(*text, *at);
            Location {
                line_idx: at.line_idx,
                grapheme_idx: at.grapheme_idx.saturating_add(1),
            }
        }
        EditOp::InsertGroup { start, text } | EditOp::InsertText { at: start, text } => {
            buffer.insert_str(*start, text);
            end_after_insert(*start, text)
        }
        EditOp::DeleteRange { start, text } => {
            let end = end_after_insert(*start, text);
            delete_span(buffer, *start, end);
            *start
        }
        EditOp::InsertNewLine { at, .. } => {
            buffer.insert_newline(*at);
            Location {
                line_idx: at.line_idx.saturating_add(1),
                grapheme_idx: 0,
            }
        }
        EditOp::DeleteNewLine {
            line_idx,
            split_at_grapheme,
        } => {
            buffer.delete(Location {
                line_idx: *line_idx,
                grapheme_idx: *split_at_grapheme,
            });
            Location {
                line_idx: *line_idx,
                grapheme_idx: *split_at_grapheme,
            }
        }
    }
}

fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn end_after_insert(start: Location, text: &str) -> Location {
    let mut line_idx = start.line_idx;
    let mut grapheme_idx = start.grapheme_idx;
    for (index, part) in text.split('\n').enumerate() {
        if index > 0 {
            line_idx = line_idx.saturating_add(1);
            grapheme_idx = 0;
        }
        grapheme_idx = grapheme_idx.saturating_add(part.graphemes(true).count());
    }
    Location {
        grapheme_idx,
        line_idx,
    }
}

fn order_span(start: Location, end: Location) -> (Location, Location) {
    if end.line_idx < start.line_idx
        || (end.line_idx == start.line_idx && end.grapheme_idx < start.grapheme_idx)
    {
        (end, start)
    } else {
        (start, end)
    }
}

fn range_text(buffer: &Buffer, start: Location, end: Location) -> String {
    let mut out = String::new();
    let mut line_idx = start.line_idx;
    let mut grapheme_idx = start.grapheme_idx;
    while line_idx < end.line_idx || (line_idx == end.line_idx && grapheme_idx < end.grapheme_idx) {
        let count = buffer.grapheme_count(line_idx);
        if grapheme_idx >= count {
            out.push('\n');
            line_idx = line_idx.saturating_add(1);
            grapheme_idx = 0;
            continue;
        }
        let stop = if line_idx == end.line_idx {
            end.grapheme_idx.min(count)
        } else {
            count
        };
        if let Some(line) = buffer.get_line(line_idx) {
            for (index, grapheme) in line.graphemes(true).enumerate() {
                if index >= grapheme_idx && index < stop {
                    out.push_str(grapheme);
                }
            }
        }
        if line_idx < end.line_idx {
            out.push('\n');
            line_idx = line_idx.saturating_add(1);
            grapheme_idx = 0;
        } else {
            break;
        }
    }
    out
}

fn delete_span(buffer: &mut Buffer, start: Location, end: Location) {
    let mut cursor = end;
    while cursor.line_idx > start.line_idx
        || (cursor.line_idx == start.line_idx && cursor.grapheme_idx > start.grapheme_idx)
    {
        if cursor.grapheme_idx == 0 {
            if cursor.line_idx == 0 {
                break;
            }
            let line_idx = cursor.line_idx.saturating_sub(1);
            let grapheme_idx = buffer.grapheme_count(line_idx);
            buffer.delete(Location {
                grapheme_idx,
                line_idx,
            });
            cursor = Location {
                grapheme_idx,
                line_idx,
            };
        } else {
            let grapheme_idx = cursor.grapheme_idx.saturating_sub(1);
            buffer.delete(Location {
                line_idx: cursor.line_idx,
                grapheme_idx,
            });
            cursor.grapheme_idx = grapheme_idx;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loc(line_idx: usize, grapheme_idx: usize) -> Location {
        Location {
            grapheme_idx,
            line_idx,
        }
    }

    #[test]
    fn typed_word_is_one_undo() {
        let mut buffer = Buffer::default();
        let mut log = EditLog::new();
        let t0 = Instant::now();
        let mut caret = loc(0, 0);
        for (index, ch) in "hello".chars().enumerate() {
            caret = log.insert_char(
                &mut buffer,
                caret,
                ch,
                t0 + Duration::from_millis(u64::try_from(index).unwrap_or(0).saturating_mul(20)),
            );
        }
        assert_eq!(buffer.text().trim_end_matches(['\n', '\r']), "hello");
        log.undo(&mut buffer);
        assert!(buffer.text().trim_end_matches(['\n', '\r']).is_empty());
    }

    #[test]
    fn newline_undo_merges_the_line() {
        let mut buffer = Buffer::default();
        let mut log = EditLog::new();
        let t0 = Instant::now();
        let caret = log.insert_text(&mut buffer, loc(0, 0), "hello", t0);
        log.insert_newline(&mut buffer, loc(0, 2), t0 + Duration::from_secs(2));
        assert_eq!(buffer.grapheme_count(0), 2);
        log.undo(&mut buffer);
        assert_eq!(buffer.grapheme_count(0), 5);
        let _ = caret;
    }

    #[test]
    fn paste_is_one_undo() {
        let mut buffer = Buffer::default();
        let mut log = EditLog::new();
        log.insert_text(&mut buffer, loc(0, 0), "one two", Instant::now());
        log.undo(&mut buffer);
        assert!(buffer.text().trim_end_matches(['\n', '\r']).is_empty());
        log.redo(&mut buffer);
        assert_eq!(buffer.text().trim_end_matches(['\n', '\r']), "one two");
    }

    #[test]
    fn delete_does_not_split_a_grapheme() {
        let mut buffer = Buffer::default();
        let mut log = EditLog::new();
        let cluster = "\u{65}\u{301}";
        log.insert_text(&mut buffer, loc(0, 0), cluster, Instant::now());
        assert_eq!(buffer.grapheme_count(0), 1);
        log.delete_range(
            &mut buffer,
            loc(0, 0),
            loc(0, 1),
            Instant::now() + Duration::from_secs(2),
        );
        assert_eq!(buffer.grapheme_count(0), 0);
        log.undo(&mut buffer);
        assert_eq!(buffer.grapheme_count(0), 1);
        assert_eq!(buffer.text().graphemes(true).next(), Some(cluster));
    }
}
