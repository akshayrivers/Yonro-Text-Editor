use crate::annotatedstring::AnnotatedString;
use crate::fileinfo::FileInfo;
use crate::highlighter::Highlighter;
use crate::line::Line;
use crate::prelude::*;
use ropey::Rope;
use std::fs::File;
use std::io::{BufReader, BufWriter, Error, Write};
use std::ops::Range;
use std::sync::Arc;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Default)]
pub struct Buffer {
    rope: Rope,
    file_info: FileInfo,
    dirty: bool,
}

impl Buffer {
    pub const fn is_dirty(&self) -> bool {
        self.dirty
    }
    pub const fn get_file_info(&self) -> &FileInfo {
        &self.file_info
    }

    /// Strip the single trailing line break that `ropey` includes in
    /// `Rope::line(idx)`. Old `Vec<Line>` storage never contained `\n`.
    fn strip_line_break(text: &str) -> &str {
        // CRLF must be checked before bare LF.
        if let Some(stripped) = text.strip_suffix("\r\n") {
            return stripped;
        }
        // `ropey` with default `unicode_lines` can break on these as well.
        for suffix in [
            "\n", "\r", "\u{85}", "\u{2028}", "\u{2029}", "\u{0B}", "\u{0C}",
        ] {
            if let Some(stripped) = text.strip_suffix(suffix) {
                return stripped;
            }
        }
        text
    }

    fn line_string(&self, idx: LineIdx) -> Option<String> {
        self.rope.get_line(idx).map(|slice| {
            let raw = slice.to_string();
            Self::strip_line_break(&raw).to_string()
        })
    }

    /// Materialize a `Line` (grapheme fragments + widths) for one rope line.
    /// Rendering / highlighting still owns grapheme logic; the rope owns storage.
    fn line_to_line(&self, idx: LineIdx) -> Option<Line> {
        self.line_string(idx).map(|s| Line::from(s.as_str()))
    }

    /// `Location{line, grapheme}` -> absolute `char_idx` ropey wants.
    /// Returns `None` if `line_idx` is out of range. Clamps `grapheme_idx`
    /// to end-of-line instead of panicking (fixes old `PLAN.md Phase 1.4` bug).
    fn location_to_char_idx(&self, at: Location) -> Option<usize> {
        if at.line_idx >= self.height() {
            return None;
        }
        let base = self.rope.line_to_char(at.line_idx);
        let text = self.line_string(at.line_idx).unwrap_or_default();
        let total_graphemes = text.graphemes(true).count();
        let clamped = at.grapheme_idx.min(total_graphemes);
        let mut intra: usize = 0;
        for (i, g) in text.graphemes(true).enumerate() {
            if i >= clamped {
                break;
            }
            intra = intra.saturating_add(g.chars().count());
        }
        Some(base.saturating_add(intra))
    }

    /// `char_idx` of the start of the *next* line (i.e. one past this line's
    /// break). Used to delete the newline itself when joining lines.
    fn next_line_start(&self, line_idx: LineIdx) -> Option<usize> {
        if line_idx.saturating_add(1) >= self.height() {
            return None;
        }
        Some(self.rope.line_to_char(line_idx.saturating_add(1)))
    }

    fn ends_with_newline(&self) -> bool {
        self.rope
            .get_char(self.rope.len_chars().saturating_sub(1))
            .is_some_and(|c| c == '\n' || c == '\r')
    }

    pub fn grapheme_count(&self, idx: LineIdx) -> GraphemeIdx {
        self.line_string(idx)
            .map_or(0, |s| s.graphemes(true).count())
    }
    pub fn width_until(&self, idx: LineIdx, until: GraphemeIdx) -> GraphemeIdx {
        self.line_to_line(idx)
            .map_or(0, |line| line.width_until(until))
    }
    /// Owned `Line` — the old `&Line` borrow cannot exist without a `Vec`.
    pub fn get_line(&self, idx: LineIdx) -> Option<Line> {
        self.line_to_line(idx)
    }
    pub fn lines_as_strings(&self) -> Vec<String> {
        (0..self.height())
            .filter_map(|idx| self.line_string(idx))
            .collect()
    }
    /// Column-range variant for soft-wrapped segments (`PLAN.md Phase 4.1`).
    /// `range` is display columns (wide graphemes occupy 2), matching
    /// `Line::get_annotated_visible_substr` semantics exactly.
    pub fn get_highlighted_column_range(
        &self,
        line_idx: LineIdx,
        range: Range<ColIdx>,
        highlighter: &Highlighter,
    ) -> Option<AnnotatedString> {
        self.line_to_line(line_idx).map(|line| {
            line.get_annotated_visible_substr(range, Some(&highlighter.get_annotations(line_idx)))
        })
    }
    pub fn highlight(&self, idx: LineIdx, highlighter: &mut Highlighter) {
        if let Some(line) = self.line_to_line(idx) {
            highlighter.highlight(idx, &line);
        }
    }
    pub fn load(file_name: &str) -> Result<Self, Error> {
        let file = File::open(file_name)?;
        let rope = Rope::from_reader(BufReader::new(file))?;
        Ok(Self {
            rope,
            file_info: FileInfo::from(file_name),
            dirty: false,
        })
    }
    pub fn search_forward(&self, query: &str, from: Location) -> Option<Location> {
        if query.is_empty() {
            return None;
        }
        let height = self.height();
        if height == 0 {
            return None;
        }
        let mut is_first = true;
        for offset in 0..height.saturating_add(1) {
            let line_idx = from.line_idx.saturating_add(offset) % height;
            // Only wrap once: stop after visiting every line.
            if offset > 0 && line_idx == from.line_idx && !is_first && offset >= height {
                break;
            }
            let Some(line) = self.line_to_line(line_idx) else {
                is_first = false;
                continue;
            };
            let from_grapheme_idx = if is_first {
                is_first = false;
                from.grapheme_idx
            } else {
                0
            };
            if let Some(grapheme_idx) = line.search_forward(query, from_grapheme_idx) {
                return Some(Location {
                    grapheme_idx,
                    line_idx,
                });
            }
            if offset.saturating_add(1) >= height {
                break;
            }
        }
        None
    }
    pub fn get_char_at(&self, at: Location) -> Option<char> {
        self.line_to_line(at.line_idx)?.get_char_at(at.grapheme_idx)
    }
    pub fn search_backward(&self, query: &str, from: Location) -> Option<Location> {
        if query.is_empty() {
            return None;
        }
        let height = self.height();
        if height == 0 {
            return None;
        }
        let mut is_first = true;
        for offset in 0..height {
            let line_idx = from.line_idx.saturating_add(height).saturating_sub(offset) % height;
            let Some(line) = self.line_to_line(line_idx) else {
                is_first = false;
                continue;
            };
            let from_grapheme_idx = if is_first {
                is_first = false;
                from.grapheme_idx
            } else {
                line.grapheme_count()
            };
            if let Some(grapheme_idx) = line.search_backward(query, from_grapheme_idx) {
                return Some(Location {
                    grapheme_idx,
                    line_idx,
                });
            }
            if offset.saturating_add(1) >= height {
                break;
            }
        }
        None
    }
    fn save_to_file(&self, file_info: &FileInfo) -> Result<(), Error> {
        if let Some(file_path) = &file_info.get_path() {
            // Atomic save (PLAN.md Phase 1.5): write tmp + sync + rename.
            let tmp_path = file_path.with_extension("tmp");
            {
                let file = File::create(&tmp_path)?;
                let mut writer = BufWriter::new(file);
                self.rope.write_to(&mut writer)?;
                writer.flush()?;
                writer.get_ref().sync_all()?;
            }
            std::fs::rename(&tmp_path, file_path)?;
        } else {
            #[cfg(debug_assertions)]
            {
                panic!("Attempting to save with no file path present");
            }
        }
        Ok(())
    }
    pub fn save_as(&mut self, file_name: &str) -> Result<(), Error> {
        let file_info = FileInfo::from(file_name);
        self.save_to_file(&file_info)?;
        self.file_info = file_info;
        self.dirty = false;
        Ok(())
    }
    pub fn save(&mut self) -> Result<(), Error> {
        // Borrow-checker: clone path info so `&self` + `&mut self` don't collide.
        let snapshot = self.file_info.get_path().map(|p| p.to_path_buf());
        if let Some(path) = snapshot {
            let info = FileInfo::from(path.to_str().unwrap_or_default());
            self.save_to_file(&info)?;
            self.dirty = false;
        } else {
            #[cfg(debug_assertions)]
            {
                panic!("Attempting to save with no file path present");
            }
        }
        Ok(())
    }
    pub fn is_empty(&self) -> bool {
        self.rope.len_chars() == 0
    }
    pub const fn is_file_loaded(&self) -> bool {
        self.file_info.has_path()
    }
    /// Replace the whole text (GUI whole-text sync, `PLAN.md Phase 5`).
    /// Marks the buffer dirty. Prefer granular ops when undo granularity
    /// matters — this is one atomic replacement.
    pub fn set_text(&mut self, text: &str) {
        self.rope = Rope::from(text);
        self.dirty = true;
    }
    /// Current full text.
    #[must_use]
    pub fn text(&self) -> String {
        self.rope.to_string()
    }
    pub fn height(&self) -> LineIdx {
        // Ropey semantics: trailing `\n` creates a real empty last line.
        // Old `Vec<Line>` (via `.lines()`) hid it; we keep the honest count
        // so in-memory edits and loaded files agree.
        self.rope.len_lines()
    }

    /// Grapheme-correct manuscript stats for status bar + WordCount pane.
    /// Per `AGENTS.md §2.1`: never treat byte/`char` as a visual character.
    /// - `graphemes`: `graphemes(true)` count (emoji ZWJ + combining safe).
    /// - `words`: `split_word_bounds` chunks containing an alphanumeric.
    /// - `lines`: rope line count (`0` when the buffer is empty).
    #[must_use]
    pub fn word_count_stats(&self) -> (usize, usize, usize) {
        if self.is_empty() {
            return (0, 0, 0);
        }
        let text = self.rope.to_string();
        let graphemes = text.graphemes(true).count();
        let mut words: usize = 0;
        for chunk in text.split_word_bounds() {
            if chunk.chars().any(char::is_alphanumeric) {
                words = words.saturating_add(1);
            }
        }
        (words, graphemes, self.height())
    }

    /// Get an immutable snapshot of the underlying rope as `Arc<Rope>`.
    /// This is O(1) — ropey uses copy-on-write, so cloning is just an Arc increment.
    pub fn rope(&self) -> Arc<Rope> {
        Arc::new(self.rope.clone())
    }

    pub fn insert_char(&mut self, character: char, at: Location) {
        if self.rope.len_chars() == 0 {
            debug_assert!(at.line_idx == 0);
            let mut text = String::new();
            text.push(character);
            self.rope.insert(0, &text);
            self.dirty = true;
            return;
        }
        if at.line_idx == self.height() {
            // Append a brand-new line past the end.
            let mut text = String::new();
            if !self.ends_with_newline() {
                text.push('\n');
            }
            text.push(character);
            let end = self.rope.len_chars();
            self.rope.insert(end, &text);
            self.dirty = true;
        } else if let Some(abs) = self.location_to_char_idx(at) {
            let mut text = String::new();
            text.push(character);
            self.rope.insert(abs, &text);
            self.dirty = true;
        }
    }
    pub fn delete(&mut self, at: Location) {
        if at.line_idx >= self.height() {
            return;
        }
        let count = self.grapheme_count(at.line_idx);
        if at.grapheme_idx >= count {
            // End of line: join with next line by removing the break.
            let Some(next_start) = self.next_line_start(at.line_idx) else {
                return;
            };
            let line_len_with_break = self.rope.line(at.line_idx).len_chars();
            let stripped_len = self
                .line_string(at.line_idx)
                .map_or(0, |s| s.chars().count());
            let break_len = line_len_with_break.saturating_sub(stripped_len);
            if break_len == 0 {
                return;
            }
            let break_start = next_start.saturating_sub(break_len);
            self.rope.remove(break_start..next_start);
            self.dirty = true;
        } else if let (Some(abs), Some(next_abs)) = (
            self.location_to_char_idx(at),
            self.location_to_char_idx(Location {
                line_idx: at.line_idx,
                grapheme_idx: at.grapheme_idx.saturating_add(1),
            }),
        ) {
            // Remove exactly one grapheme (1+ chars: ZWJ, combining marks).
            if abs < next_abs {
                self.rope.remove(abs..next_abs);
                self.dirty = true;
            }
        }
    }
    pub fn insert_newline(&mut self, at: Location) {
        if at.line_idx == self.height() {
            let end = self.rope.len_chars();
            self.rope.insert(end, "\n");
            self.dirty = true;
        } else if let Some(abs) = self.location_to_char_idx(at) {
            self.rope.insert(abs, "\n");
            self.dirty = true;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::Location;
    use std::fs;

    fn loc(line: usize, col: usize) -> Location {
        Location {
            line_idx: line,
            grapheme_idx: col,
        }
    }

    #[test]
    fn buffer_insert_char_basic() {
        let mut buffer = Buffer::default();

        buffer.insert_char('h', loc(0, 0));
        buffer.insert_char('i', loc(0, 1));

        assert_eq!(buffer.height(), 1);
        assert_eq!(buffer.grapheme_count(0), 2);
        assert!(buffer.is_dirty());
    }

    #[test]
    fn buffer_insert_newline() {
        let mut buffer = Buffer::default();

        buffer.insert_char('h', loc(0, 0));
        buffer.insert_char('i', loc(0, 1));

        buffer.insert_newline(loc(0, 2));

        assert_eq!(buffer.height(), 2);
        assert!(buffer.is_dirty());
    }

    #[test]
    fn buffer_insert_newline_middle() {
        let mut buffer = Buffer::default();

        buffer.insert_char('h', loc(0, 0));
        buffer.insert_char('e', loc(0, 1));
        buffer.insert_char('l', loc(0, 2));
        buffer.insert_char('l', loc(0, 3));
        buffer.insert_char('o', loc(0, 4));

        buffer.insert_newline(loc(0, 2));

        assert_eq!(buffer.height(), 2);
    }

    #[test]
    fn buffer_delete_char() {
        let mut buffer = Buffer::default();

        buffer.insert_char('h', loc(0, 0));
        buffer.insert_char('i', loc(0, 1));

        buffer.delete(loc(0, 1));

        assert_eq!(buffer.grapheme_count(0), 1);
    }

    #[test]
    fn buffer_delete_merge_lines() {
        let mut buffer = Buffer::default();

        buffer.insert_char('h', loc(0, 0));
        buffer.insert_newline(loc(0, 1));
        buffer.insert_char('i', loc(1, 0));

        buffer.delete(loc(0, 1));

        assert_eq!(buffer.height(), 1);
        assert_eq!(buffer.grapheme_count(0), 2);
    }

    #[test]
    fn buffer_search_forward_same_line() {
        let mut buffer = Buffer::default();

        for (i, c) in "hello world".chars().enumerate() {
            buffer.insert_char(c, loc(0, i));
        }

        let result = buffer.search_forward("world", loc(0, 0));

        assert!(result.is_some());
        let result = result.unwrap();

        assert_eq!(result.line_idx, 0);
        assert_eq!(result.grapheme_idx, 6);
    }

    #[test]
    fn buffer_search_forward_multiline() {
        let mut buffer = Buffer::default();

        for (i, c) in "hello".chars().enumerate() {
            buffer.insert_char(c, loc(0, i));
        }

        buffer.insert_newline(loc(0, 5));

        for (i, c) in "world".chars().enumerate() {
            buffer.insert_char(c, loc(1, i));
        }

        let result = buffer.search_forward("world", loc(0, 0));

        assert!(result.is_some());
        let result = result.unwrap();

        assert_eq!(result.line_idx, 1);
    }

    #[test]
    fn buffer_search_backward() {
        let mut buffer = Buffer::default();

        for (i, c) in "hello".chars().enumerate() {
            buffer.insert_char(c, loc(0, i));
        }

        buffer.insert_newline(loc(0, 5));

        for (i, c) in "hello".chars().enumerate() {
            buffer.insert_char(c, loc(1, i));
        }

        let result = buffer.search_backward("hello", loc(1, 5));

        assert!(result.is_some());
        let result = result.unwrap();

        assert_eq!(result.line_idx, 1);
    }

    #[test]
    fn buffer_height() {
        let mut buffer = Buffer::default();

        buffer.insert_char('a', loc(0, 0));
        buffer.insert_newline(loc(0, 1));
        buffer.insert_char('b', loc(1, 0));

        assert_eq!(buffer.height(), 2);
    }

    #[test]
    fn buffer_is_empty() {
        let buffer = Buffer::default();

        assert!(buffer.is_empty());
    }

    #[test]
    fn buffer_dirty_flag() {
        let mut buffer = Buffer::default();

        assert!(!buffer.is_dirty());

        buffer.insert_char('a', loc(0, 0));

        assert!(buffer.is_dirty());
    }

    #[test]
    fn buffer_save_and_load() {
        let file = "test_buffer_save.txt";

        let mut buffer = Buffer::default();

        buffer.insert_char('h', loc(0, 0));
        buffer.insert_char('i', loc(0, 1));

        buffer.save_as(file).unwrap();

        let loaded = Buffer::load(file).unwrap();

        assert_eq!(loaded.height(), 1);
        assert_eq!(loaded.grapheme_count(0), 2);

        fs::remove_file(file).unwrap();
    }

    #[test]
    fn buffer_is_file_loaded() {
        let mut buffer = Buffer::default();

        assert!(!buffer.is_file_loaded());

        buffer.save_as("test_file.txt").unwrap();

        assert!(buffer.is_file_loaded());

        fs::remove_file("test_file.txt").unwrap();
    }

    #[test]
    fn buffer_width_until() {
        let mut buffer = Buffer::default();

        buffer.insert_char('a', loc(0, 0));
        buffer.insert_char('b', loc(0, 1));
        buffer.insert_char('c', loc(0, 2));

        let width = buffer.width_until(0, 2);

        assert_eq!(width, 2);
    }

    #[test]
    fn buffer_multiple_newlines() {
        let mut buffer = Buffer::default();

        buffer.insert_char('a', loc(0, 0));
        buffer.insert_newline(loc(0, 1));
        buffer.insert_newline(loc(1, 0));
        buffer.insert_char('b', loc(2, 0));

        assert_eq!(buffer.height(), 3);
    }

    #[test]
    fn buffer_grapheme_emoji_single_delete() {
        // Family emoji is 7 chars but 1 grapheme — delete must remove it atomically.
        // Build via file load since `insert_char` takes a single `char`.
        let file = "test_buffer_emoji.txt";
        fs::write(file, "a👨‍👩‍👧‍👦b").unwrap();
        let mut buffer = Buffer::load(file).unwrap();
        assert_eq!(buffer.grapheme_count(0), 3);
        buffer.delete(loc(0, 1));
        assert_eq!(buffer.grapheme_count(0), 2);
        assert_eq!(buffer.lines_as_strings().join(""), "ab");
        fs::remove_file(file).unwrap();
    }
}
