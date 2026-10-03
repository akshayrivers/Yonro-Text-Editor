use crate::prelude::*;
use std::{
    cmp::min,
    fmt::{self, Display},
    ops::{Deref, Range},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;
pub mod graphemewidth;
pub mod textfragment;
pub use graphemewidth::GraphemeWidth;
pub use textfragment::TextFragment;

use super::AnnotatedString;
use super::Annotation;

#[derive(Default, Clone)]
pub struct Line {
    fragments: Vec<TextFragment>,
    string: String,
}

impl Line {
    pub fn from(line_str: &str) -> Self {
        debug_assert!(line_str.is_empty() || line_str.lines().count() == 1);
        let fragments = Self::str_to_fragments(line_str);
        Self {
            fragments,
            string: String::from(line_str),
        }
    }

    fn str_to_fragments(line_str: &str) -> Vec<TextFragment> {
        line_str
            .grapheme_indices(true)
            .map(|(byte_idx, grapheme)| {
                let (replacement, rendered_width) = Self::get_replacement_character(grapheme)
                    .map_or_else(
                        || {
                            let unicode_width = grapheme.width();
                            let rendered_width = match unicode_width {
                                0 | 1 => GraphemeWidth::Half,
                                _ => GraphemeWidth::Full,
                            };
                            (None, rendered_width)
                        },
                        |replacement| (Some(replacement), GraphemeWidth::Half),
                    );

                TextFragment {
                    grapheme: grapheme.to_string(),
                    rendered_width,
                    replacement,
                    start_byte_idx: byte_idx,
                }
            })
            .collect()
    }

    fn rebuild_fragments(&mut self) {
        self.fragments = Self::str_to_fragments(&self.string);
    }

    fn get_replacement_character(for_str: &str) -> Option<char> {
        let width = for_str.width();
        match for_str {
            " " => None,
            "\t" => Some(' '),
            _ if width > 0 && for_str.trim().is_empty() => Some('␣'),
            _ if width == 0 => {
                let mut chars = for_str.chars();
                if let Some(ch) = chars.next() {
                    if ch.is_control() && chars.next().is_none() {
                        return Some('▯');
                    }
                }
                Some('·')
            }
            _ => None,
        }
    }
    // Gets the visible graphemes in the given column index.
    // Note that the column index is not the same as the grapheme index:
    // A grapheme can have a width of 2 columns.
    pub fn get_visible_graphemes(&self, range: Range<ColIdx>) -> String {
        self.get_annotated_visible_substr(range, None).to_string()
    }

    // Gets the annotated string in the given column index.
    // Note that the column index is not the same as the grapheme index:
    // A grapheme can have a width of 2 columns.
    // Parameters:
    // - range: The range of columns to get the annotated string from.
    // - query: The query to highlight in the annotated string.
    // - selected_match: The selected match to highlight in the annotated string. This is only applied if the query is not empty.
    pub fn get_annotated_visible_substr(
        &self,
        range: Range<ColIdx>,
        annotations: Option<&Vec<Annotation>>,
    ) -> AnnotatedString {
        if range.start >= range.end {
            return AnnotatedString::default();
        }
        // Create a new annotated string
        let mut result = AnnotatedString::from(&self.string);

        if let Some(annotations) = annotations {
            for annotation in annotations {
                result.add_annotation(
                    annotation.annotation_type,
                    annotation.start_byte_idx,
                    annotation.end_byte_idx,
                );
            }
        }

        // Insert replacement characters, and truncate if needed.
        // We do this backwards, otherwise the byte indices would be off in case a replacement character has a different width than the original character.

        let mut fragment_start = self.width();
        for fragment in self.fragments.iter().rev() {
            let fragment_end = fragment_start;
            fragment_start = fragment_start.saturating_sub(fragment.rendered_width.into());

            if fragment_start > range.end {
                continue; // No  processing needed if we haven't reached the visible range yet.
            }

            // clip right if the fragment is partially visible
            if fragment_start < range.end && fragment_end > range.end {
                result.replace(fragment.start_byte_idx, self.string.len(), "⋯");
                continue;
            } else if fragment_start == range.end {
                // Truncate right if we've reached the end of the visible range
                result.truncate_right_from(fragment.start_byte_idx);
                continue;
            }

            // Fragment ends at the start of the range: Remove the entire left side of the string (if not already at start of string)
            if fragment_end <= range.start {
                result.truncate_left_until(
                    fragment
                        .start_byte_idx
                        .saturating_add(fragment.grapheme.len()),
                );
                break; //End processing since all remaining fragments will be invisible.
            } else if fragment_start < range.start && fragment_end > range.start {
                // Fragment overlaps with the start of range: Remove the left side of the string and add an ellipsis
                result.replace(
                    0,
                    fragment
                        .start_byte_idx
                        .saturating_add(fragment.grapheme.len()),
                    "⋯",
                );
                break; //End processing since all remaining fragments will be invisible.
            }

            // Fragment is fully within range: Apply replacement characters if appropriate
            if fragment_start >= range.start && fragment_end <= range.end {
                if let Some(replacement) = fragment.replacement {
                    let start_byte_idx = fragment.start_byte_idx;
                    let end_byte_idx = start_byte_idx.saturating_add(fragment.grapheme.len());
                    result.replace(start_byte_idx, end_byte_idx, &replacement.to_string());
                }
            }
        }

        result
    }
    pub fn get_char_at(&self, idx: GraphemeIdx) -> Option<char> {
        self.fragments
            .get(idx)
            .and_then(|frag| frag.grapheme.chars().next())
    }
    pub fn grapheme_count(&self) -> GraphemeIdx {
        self.fragments.len()
    }
    pub fn width_until(&self, grapheme_idx: GraphemeIdx) -> ColIdx {
        self.fragments
            .iter()
            .take(grapheme_idx)
            .map(|fragment| match fragment.rendered_width {
                GraphemeWidth::Half => 1,
                GraphemeWidth::Full => 2,
            })
            .sum()
    }
    pub fn width(&self) -> ColIdx {
        self.width_until(self.grapheme_count())
    }
    /// Split this line into visual segments fitting `width` display columns
    /// (soft word-wrap; `PLAN.md Phase 4.1`). Returns grapheme-index ranges.
    /// Breaks after spaces; over-long words (or width < grapheme width) fall
    /// back to hard breaks. Never splits a grapheme cluster (`AGENTS.md §2.1`).
    /// An empty line yields one empty segment; `width == 0` yields the whole
    /// line unwrapped.
    #[must_use]
    pub fn wrap_segments(&self, width: ColIdx) -> Vec<Range<GraphemeIdx>> {
        let count = self.grapheme_count();
        if count == 0 {
            return vec![0..0];
        }
        if width == 0 {
            return vec![0..count];
        }
        // Prefix display widths: cum[i] = columns of fragments[0..i].
        let mut cum: Vec<ColIdx> = Vec::with_capacity(count.saturating_add(1));
        cum.push(0);
        for fragment in &self.fragments {
            let w: ColIdx = fragment.rendered_width.into();
            cum.push(cum[cum.len().saturating_sub(1)].saturating_add(w));
        }
        let is_breakable = |idx: GraphemeIdx| -> bool {
            self.fragments
                .get(idx)
                .is_some_and(|f| f.grapheme == " " || f.grapheme == "\t")
        };
        let mut segments = Vec::new();
        let mut start: GraphemeIdx = 0;
        while start < count {
            // Extend j while fragments[start..=j] fit.
            let mut j = start;
            let mut last_space: Option<GraphemeIdx> = None;
            while j < count && cum[j.saturating_add(1)].saturating_sub(cum[start]) <= width {
                if is_breakable(j) {
                    last_space = Some(j);
                }
                j = j.saturating_add(1);
            }
            if j >= count {
                segments.push(start..count);
                break;
            }
            match last_space {
                // Break after the last space (space stays at line end).
                Some(sp) if sp >= start => {
                    segments.push(start..sp.saturating_add(1));
                    start = sp.saturating_add(1);
                }
                // No space in range: hard break before the overflowing grapheme…
                _ if j > start => {
                    segments.push(start..j);
                    start = j;
                }
                // …unless a single grapheme alone overflows: it takes its own row.
                _ => {
                    segments.push(start..start.saturating_add(1));
                    start = start.saturating_add(1);
                }
            }
        }
        segments
    }
    // Inserts a character into the line, or appends it at the end if at == grapheme_count + 1
    pub fn insert_char(&mut self, character: char, at: GraphemeIdx) {
        debug_assert!(at.saturating_sub(1) <= self.grapheme_count());
        if let Some(fragment) = self.fragments.get(at) {
            self.string.insert(fragment.start_byte_idx, character);
        } else {
            self.string.push(character);
        }
        self.rebuild_fragments();
    }
    pub fn append_char(&mut self, character: char) {
        self.insert_char(character, self.grapheme_count());
    }
    pub fn delete(&mut self, at: GraphemeIdx) {
        debug_assert!(at <= self.grapheme_count());
        if let Some(fragment) = self.fragments.get(at) {
            let start = fragment.start_byte_idx;
            let end = fragment
                .start_byte_idx
                .saturating_add(fragment.grapheme.len());
            self.string.drain(start..end);
            self.rebuild_fragments();
        }
    }

    pub fn delete_last(&mut self) {
        self.delete(self.grapheme_count().saturating_sub(1));
    }

    pub fn append(&mut self, other: &Self) {
        self.string.push_str(&other.string);
        self.rebuild_fragments();
    }

    pub fn split(&mut self, at: GraphemeIdx) -> Self {
        if let Some(fragment) = self.fragments.get(at) {
            let remainder = self.string.split_off(fragment.start_byte_idx);
            self.rebuild_fragments();
            Self::from(&remainder)
        } else {
            Self::default()
        }
    }
    fn byte_idx_to_grapheme_idx(&self, byte_idx: ByteIdx) -> Option<GraphemeIdx> {
        if byte_idx > self.string.len() {
            return None;
        }
        self.fragments
            .iter()
            .position(|fragment| fragment.start_byte_idx >= byte_idx)
    }
    fn grapheme_idx_to_byte_idx(&self, grapheme_idx: GraphemeIdx) -> ByteIdx {
        if grapheme_idx == 0 || self.grapheme_count() == 0 {
            return 0;
        }
        // End-of-line is valid (cursor rests here constantly) — return one-past-end,
        // never panic. Covers `PLAN.md Phase 1.4`.
        if grapheme_idx >= self.grapheme_count() {
            return self.string.len();
        }
        self.fragments
            .get(grapheme_idx)
            .map_or(self.string.len(), |fragment| fragment.start_byte_idx)
    }
    pub fn search_forward(
        &self,
        query: &str,
        from_grapheme_idx: GraphemeIdx,
    ) -> Option<GraphemeIdx> {
        debug_assert!(from_grapheme_idx <= self.grapheme_count());
        if from_grapheme_idx == self.grapheme_count() {
            return None;
        }
        let start_byte_idx = self.grapheme_idx_to_byte_idx(from_grapheme_idx);
        self.find_all(query, start_byte_idx..self.string.len())
            .first()
            .map(|(_, grapheme_idx)| *grapheme_idx)
    }
    pub fn search_backward(
        &self,
        query: &str,
        from_grapheme_idx: GraphemeIdx,
    ) -> Option<GraphemeIdx> {
        debug_assert!(from_grapheme_idx <= self.grapheme_count());

        if from_grapheme_idx == 0 {
            return None;
        }
        let end_byte_index = if from_grapheme_idx == self.grapheme_count() {
            self.string.len()
        } else {
            self.grapheme_idx_to_byte_idx(from_grapheme_idx)
        };
        self.find_all(query, 0..end_byte_index)
            .last()
            .map(|(_, grapheme_idx)| *grapheme_idx)
    }
    pub fn find_all(&self, query: &str, range: Range<ByteIdx>) -> Vec<(ByteIdx, GraphemeIdx)> {
        let end_byte_idx = min(range.end, self.string.len());
        let start_byte_idx = range.start;
        debug_assert!(start_byte_idx <= end_byte_idx);
        debug_assert!(start_byte_idx <= self.string.len());
        self.string
            .get(start_byte_idx..end_byte_idx)
            .map_or_else(Vec::new, |substr| {
                let potential_matches: Vec<ByteIdx> = substr
                    .match_indices(query)
                    .map(|(relative_start_idx, _)| {
                        relative_start_idx.saturating_add(start_byte_idx)
                    })
                    .collect();
                self.match_grapheme_clusters(&potential_matches, query)
            })
    }
    fn match_grapheme_clusters(
        &self,
        matches: &[ByteIdx],
        query: &str,
    ) -> Vec<(ByteIdx, GraphemeIdx)> {
        let grapheme_count = query.graphemes(true).count();
        matches
            .iter()
            .filter_map(|&start| {
                self.byte_idx_to_grapheme_idx(start)
                    .and_then(|grapheme_idx| {
                        self.fragments
                            .get(grapheme_idx..grapheme_idx.saturating_add(grapheme_count))
                            .and_then(|fragments| {
                                let substring = fragments
                                    .iter()
                                    .map(|fragment| fragment.grapheme.as_str())
                                    .collect::<String>(); // combining the fragments in a single string.
                                (substring == query).then_some((start, grapheme_idx))
                            })
                    })
            })
            .collect()
    }
}

impl Display for Line {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "{}", self.string)
    }
}

impl Deref for Line {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.string
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Basic Construction tests
    #[test]
    fn line_from_basic_ascii() {
        let line = Line::from("hello");

        assert_eq!(&line.to_string(), "hello");
        assert_eq!(line.grapheme_count(), 5);
    }

    #[test]
    fn line_from_empty() {
        let line = Line::from("");

        assert_eq!(&line.to_string(), "");
        assert_eq!(line.grapheme_count(), 0);
    }

    // Grapheme Tests
    #[test]
    fn line_unicode_graphemes() {
        let line = Line::from("नमस्ते");

        assert!(line.grapheme_count() > 0);
    }

    #[test]
    fn line_emoji_graphemes() {
        let line = Line::from("👨‍💻");

        assert_eq!(line.grapheme_count(), 1);
    }

    #[test]
    fn line_combining_characters() {
        let line = Line::from("é"); // e + accent

        assert_eq!(line.grapheme_count(), 1);
    }

    //Grapheme width calculation
    #[test]
    fn width_ascii() {
        let line = Line::from("abc");

        assert_eq!(line.width(), 3);
    }

    #[test]
    fn width_fullwidth_characters() {
        let line = Line::from("你好");

        assert_eq!(line.width(), 4);
    }

    #[test]
    fn width_mixed_characters() {
        let line = Line::from("a你");

        assert_eq!(line.width(), 3);
    }

    //Insert Tests
    #[test]
    fn insert_char_middle() {
        let mut line = Line::from("helo");

        line.insert_char('l', 2);

        assert_eq!(&line.to_string(), "hello");
    }

    #[test]
    fn insert_char_end() {
        let mut line = Line::from("hell");

        line.insert_char('o', 4);

        assert_eq!(&line.to_string(), "hello");
    }

    #[test]
    fn append_char() {
        let mut line = Line::from("hell");

        line.append_char('o');

        assert_eq!(&line.to_string(), "hello");
    }

    //Delete Tests
    #[test]
    fn delete_middle() {
        let mut line = Line::from("hello");

        line.delete(2);

        assert_eq!(&line.to_string(), "helo");
    }

    #[test]
    fn delete_last() {
        let mut line = Line::from("hello");

        line.delete_last();

        assert_eq!(&line.to_string(), "hell");
    }

    #[test]
    fn delete_unicode() {
        let mut line = Line::from("a👨‍💻b");

        line.delete(1);

        assert_eq!(&line.to_string(), "ab");
    }
    // Split Tests
    #[test]
    fn split_middle() {
        let mut line = Line::from("hello");

        let second = line.split(2);

        assert_eq!(&line.to_string(), "he");
        assert_eq!(&second.to_string(), "llo");
    }

    #[test]
    fn split_end() {
        let mut line = Line::from("hello");

        let second = line.split(5);

        assert_eq!(&line.to_string(), "hello");
        assert_eq!(&second.to_string(), "");
    }

    // Append tests
    #[test]
    fn append_lines() {
        let mut line1 = Line::from("hello");
        let line2 = Line::from(" world");

        line1.append(&line2);

        assert_eq!(&line1.to_string(), "hello world");
    }

    // Search tests
    #[test]
    fn search_forward_basic() {
        let line = Line::from("hello world");

        let result = line.search_forward("world", 0);

        assert_eq!(result, Some(6));
    }

    #[test]
    fn search_backward_basic() {
        let line = Line::from("hello hello");

        let result = line.search_backward("hello", line.grapheme_count());

        assert_eq!(result, Some(6));
    }
    //Find all
    #[test]
    fn find_all_matches() {
        let line = Line::from("hello hello hello");

        let matches = line.find_all("hello", 0..line.len());

        assert_eq!(matches.len(), 3);
    }

    // visible grapheme test
    #[test]
    fn visible_range_basic() {
        let line = Line::from("hello");

        let visible = line.get_visible_graphemes(0..3);

        assert_eq!(visible, "hel");
    }

    #[test]
    fn grapheme_idx_to_byte_idx_at_eol_returns_len() {
        // PLAN.md Phase 1.4: `grapheme_idx == count` (cursor at EOL) must
        // return one-past-end instead of panicking (debug) / returning 0 (release).
        let line = Line::from("hello");
        assert_eq!(line.grapheme_idx_to_byte_idx(5), line.len());
        assert_eq!(line.grapheme_idx_to_byte_idx(99), line.len());

        let empty = Line::from("");
        assert_eq!(empty.grapheme_idx_to_byte_idx(0), 0);

        // Multibyte: "aé" where é = e + combining acute (2 chars, 1 grapheme).
        let uni = Line::from("aé");
        assert_eq!(
            uni.grapheme_idx_to_byte_idx(uni.grapheme_count()),
            uni.len()
        );
    }

    // Soft-wrap tests (PLAN.md Phase 4.1)
    #[test]
    fn wrap_short_line_single_segment() {
        let line = Line::from("hello");
        assert_eq!(line.wrap_segments(80), vec![0..5]);
        assert_eq!(line.wrap_segments(5), vec![0..5]);
    }

    #[test]
    fn wrap_empty_line_single_empty_segment() {
        let line = Line::from("");
        assert_eq!(line.wrap_segments(80), vec![0..0]);
    }

    #[test]
    fn wrap_breaks_after_spaces() {
        // "hello world foo" @ width 8 → "hello " | "world " | "foo"
        let line = Line::from("hello world foo");
        assert_eq!(line.wrap_segments(8), vec![0..6, 6..12, 12..15]);
    }

    #[test]
    fn wrap_long_word_hard_breaks() {
        // 10-char word @ width 4 → 4/4/2.
        let line = Line::from("abcdefghij");
        assert_eq!(line.wrap_segments(4), vec![0..4, 4..8, 8..10]);
    }

    #[test]
    fn wrap_never_splits_grapheme_cluster() {
        // Family emoji = 1 grapheme of width 2; width 1 still keeps it whole.
        let line = Line::from("👨‍👩‍👧‍👦");
        assert_eq!(line.grapheme_count(), 1);
        assert_eq!(line.wrap_segments(1), vec![0..1]);
    }

    #[test]
    fn wrap_zero_width_returns_whole_line() {
        let line = Line::from("hello world");
        assert_eq!(line.wrap_segments(0), vec![0..11]);
    }
}
