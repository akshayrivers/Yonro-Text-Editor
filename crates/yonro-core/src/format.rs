//! Inline marks and paragraph props. The prose file stays plain.
//!
//! Ranges are flat grapheme offsets. A newline counts as one. Missing or
//! corrupt `.yonro/format.json` is an empty book, never a crash.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

use super::buffers::Buffer;
use super::prelude::Location;

/// One scene's marks. Empty means the prose has no formatting.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Format {
    #[serde(default)]
    marks: Vec<InlineMark>,
    #[serde(default)]
    paragraphs: Vec<Paragraph>,
    /// Empty means the page's default serif.
    #[serde(default)]
    face: String,
    #[serde(default)]
    spacing: Spacing,
}

/// A half-open grapheme span and what it looks like.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InlineMark {
    /// First grapheme, flat across the scene.
    pub start: usize,
    /// One past the last grapheme.
    pub end: usize,
    /// What the span carries.
    pub kind: MarkKind,
}

/// Inline formatting. Palette indexes are fixed swatches, not raw colors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarkKind {
    /// Bold.
    Bold,
    /// Italic.
    Italic,
    /// Underline.
    Underline,
    /// Strike.
    Strike,
    /// Font size in points.
    Size {
        /// Point size.
        points: u16,
    },
    /// A link.
    Link {
        /// Destination.
        href: String,
    },
    /// Highlight swatch.
    Highlight {
        /// Palette index.
        swatch: u8,
    },
    /// Text color swatch.
    Color {
        /// Palette index.
        swatch: u8,
    },
}

/// How one source line is set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Paragraph {
    /// Source line.
    pub line: usize,
    /// Heading or body.
    pub style: ParaStyle,
    /// Horizontal alignment.
    pub align: Align,
    /// List marker.
    pub list: ListKind,
    /// Indent steps.
    pub indent: u8,
}

/// Body or one of three headings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParaStyle {
    /// Body text.
    #[default]
    Normal,
    /// Heading 1.
    H1,
    /// Heading 2.
    H2,
    /// Heading 3.
    H3,
}

/// Paragraph alignment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Align {
    /// Left.
    #[default]
    Left,
    /// Center.
    Center,
    /// Right.
    Right,
    /// Justified.
    Justify,
}

/// List marker on a paragraph.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ListKind {
    /// Not a list.
    #[default]
    None,
    /// Bullet.
    Bullet,
    /// Numbered.
    Number,
}

/// Document line spacing. Not a per-paragraph fact.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Spacing {
    /// Tight.
    Compact,
    /// The usual page.
    #[default]
    Roomy,
    /// Open.
    Airy,
}

/// Every scene's format, keyed by the scene file name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormatFile {
    #[serde(default)]
    scenes: BTreeMap<String, Format>,
}

impl Format {
    /// Marks currently stored.
    #[must_use]
    pub fn marks(&self) -> &[InlineMark] {
        &self.marks
    }

    /// Paragraph props.
    #[must_use]
    pub fn paragraphs(&self) -> &[Paragraph] {
        &self.paragraphs
    }

    /// Document face. Empty means the default serif.
    #[must_use]
    pub fn face(&self) -> &str {
        &self.face
    }

    /// Document line spacing.
    #[must_use]
    pub const fn spacing(&self) -> Spacing {
        self.spacing
    }

    /// Record a mark. An empty span is ignored.
    pub fn add_mark(&mut self, start: usize, end: usize, kind: MarkKind) {
        if end <= start {
            return;
        }
        self.marks.push(InlineMark { start, end, kind });
    }

    /// Add `kind` on the span, or remove it when the span is already covered.
    pub fn toggle(&mut self, start: usize, end: usize, kind: MarkKind) {
        if end <= start {
            return;
        }
        let covered = self
            .marks
            .iter()
            .any(|mark| same_kind(&mark.kind, &kind) && mark.start <= start && mark.end >= end);
        if covered {
            self.trim_kind(start, end, &kind);
        } else {
            self.add_mark(start, end, kind);
        }
    }

    /// Drop every mark on the span. Marks outside it stay put.
    pub fn clear_range(&mut self, start: usize, end: usize) {
        if end <= start {
            return;
        }
        let mut next = Vec::new();
        for mark in self.marks.drain(..) {
            for (part_start, part_end) in trim_span(mark.start, mark.end, start, end) {
                next.push(InlineMark {
                    start: part_start,
                    end: part_end,
                    kind: mark.kind.clone(),
                });
            }
        }
        self.marks = next;
    }

    fn trim_kind(&mut self, start: usize, end: usize, kind: &MarkKind) {
        let mut next = Vec::new();
        for mark in self.marks.drain(..) {
            if !same_kind(&mark.kind, kind) {
                next.push(mark);
                continue;
            }
            for (part_start, part_end) in trim_span(mark.start, mark.end, start, end) {
                next.push(InlineMark {
                    start: part_start,
                    end: part_end,
                    kind: mark.kind.clone(),
                });
            }
        }
        self.marks = next;
    }

    /// Replace paragraph props for `line`, or append them.
    pub fn set_paragraph(&mut self, paragraph: Paragraph) {
        if let Some(found) = self
            .paragraphs
            .iter_mut()
            .find(|item| item.line == paragraph.line)
        {
            *found = paragraph;
        } else {
            self.paragraphs.push(paragraph);
        }
    }

    /// Grow or slide marks after an insert of `len` graphemes at `at`.
    ///
    /// A mark grows when the insert is strictly inside it. Typing at its
    /// start edge does not pull the new graphemes in.
    pub fn shift_insert(&mut self, at: usize, len: usize) {
        if len == 0 {
            return;
        }
        for mark in &mut self.marks {
            if mark.start > at {
                mark.start = mark.start.saturating_add(len);
            }
            if mark.end > at {
                mark.end = mark.end.saturating_add(len);
            }
        }
    }

    /// Shrink marks that overlap `[start, end)`, and slide the ones after it.
    pub fn shift_delete(&mut self, start: usize, end: usize) {
        if end <= start {
            return;
        }
        let mut next = Vec::new();
        for mark in self.marks.drain(..) {
            if let Some((mark_start, mark_end)) = subtract_span(mark.start, mark.end, start, end) {
                next.push(InlineMark {
                    start: mark_start,
                    end: mark_end,
                    kind: mark.kind,
                });
            }
        }
        self.marks = next;
    }

    /// A newline at `line` copies that line's props onto the new line.
    pub fn split_line(&mut self, line: usize) {
        for paragraph in &mut self.paragraphs {
            if paragraph.line > line {
                paragraph.line = paragraph.line.saturating_add(1);
            }
        }
        if let Some(source) = self
            .paragraphs
            .iter()
            .find(|paragraph| paragraph.line == line)
            .cloned()
        {
            let mut copy = source;
            copy.line = line.saturating_add(1);
            self.paragraphs.push(copy);
        }
    }

    /// Joining `line` with the next drops the next line's props.
    pub fn join_line(&mut self, line: usize) {
        let dropped = line.saturating_add(1);
        self.paragraphs
            .retain(|paragraph| paragraph.line != dropped);
        for paragraph in &mut self.paragraphs {
            if paragraph.line > line {
                paragraph.line = paragraph.line.saturating_sub(1);
            }
        }
    }
}

impl FormatFile {
    /// Load `.yonro/format.json`. Missing or corrupt data is an empty book.
    #[must_use]
    pub fn load(root: &Path) -> Self {
        let path = root.join(".yonro/format.json");
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }

    /// Write the sidecar. Does not touch scene prose.
    ///
    /// # Errors
    /// Returns an IO error when the directory or the temp file cannot be written.
    pub fn save(&self, root: &Path) -> Result<(), std::io::Error> {
        let dir = root.join(".yonro");
        std::fs::create_dir_all(&dir)?;
        let text = serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string());
        let temp = dir.join("format.json.tmp");
        let dest = dir.join("format.json");
        std::fs::write(&temp, text)?;
        std::fs::rename(temp, dest)?;
        Ok(())
    }

    /// Format for one scene file name, or empty.
    #[must_use]
    pub fn scene(&self, name: &str) -> Format {
        self.scenes.get(name).cloned().unwrap_or_default()
    }

    /// Replace one scene's format.
    pub fn set_scene(&mut self, name: &str, format: Format) {
        self.scenes.insert(name.to_string(), format);
    }
}

/// Flat grapheme index of a caret. A newline counts as one.
#[must_use]
pub fn flat_index(buffer: &Buffer, at: Location) -> usize {
    let mut flat = 0usize;
    let height = buffer.height().max(1);
    let line_idx = at.line_idx.min(height.saturating_sub(1));
    for index in 0..line_idx {
        flat = flat
            .saturating_add(buffer.grapheme_count(index))
            .saturating_add(1);
    }
    flat.saturating_add(at.grapheme_idx)
}

/// Graphemes in `text`, counting each `\n` as one.
#[must_use]
pub fn flat_len(text: &str) -> usize {
    let mut count = 0usize;
    for (index, part) in text.split('\n').enumerate() {
        if index > 0 {
            count = count.saturating_add(1);
        }
        count = count.saturating_add(part.graphemes(true).count());
    }
    count
}

fn same_kind(left: &MarkKind, right: &MarkKind) -> bool {
    std::mem::discriminant(left) == std::mem::discriminant(right)
}

fn trim_span(
    mark_start: usize,
    mark_end: usize,
    cut_start: usize,
    cut_end: usize,
) -> Vec<(usize, usize)> {
    if mark_end <= cut_start || mark_start >= cut_end || mark_end <= mark_start {
        return vec![(mark_start, mark_end)];
    }
    let mut parts = Vec::new();
    if mark_start < cut_start {
        parts.push((mark_start, cut_start.min(mark_end)));
    }
    if mark_end > cut_end {
        parts.push((cut_end.max(mark_start), mark_end));
    }
    parts
}

fn subtract_span(
    mark_start: usize,
    mark_end: usize,
    delete_start: usize,
    delete_end: usize,
) -> Option<(usize, usize)> {
    if mark_end <= delete_start {
        return Some((mark_start, mark_end));
    }
    let deleted = delete_end.saturating_sub(delete_start);
    if mark_start >= delete_end {
        return Some((
            mark_start.saturating_sub(deleted),
            mark_end.saturating_sub(deleted),
        ));
    }
    let overlap = mark_end
        .min(delete_end)
        .saturating_sub(mark_start.max(delete_start));
    let span = mark_end.saturating_sub(mark_start).saturating_sub(overlap);
    if span == 0 {
        return None;
    }
    let start = if mark_start < delete_start {
        mark_start
    } else {
        delete_start
    };
    Some((start, start.saturating_add(span)))
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::editlog::EditLog;

    fn loc(grapheme_idx: usize) -> Location {
        Location {
            grapheme_idx,
            line_idx: 0,
        }
    }

    #[test]
    fn typing_inside_a_bold_word_keeps_the_mark_after_undo() {
        let mut buffer = Buffer::default();
        let mut log = EditLog::new();
        log.insert_text(&mut buffer, loc(0), "word", Instant::now());
        log.format_mut().add_mark(0, 4, MarkKind::Bold);
        log.insert_char(
            &mut buffer,
            loc(2),
            'x',
            Instant::now() + Duration::from_secs(2),
        );
        assert_eq!(buffer.text().trim_end_matches(['\n', '\r']), "woxrd");
        assert_eq!(log.format().marks()[0].end, 5);
        log.undo(&mut buffer);
        assert_eq!(buffer.text().trim_end_matches(['\n', '\r']), "word");
        let mark = &log.format().marks()[0];
        assert_eq!(mark.start, 0);
        assert_eq!(mark.end, 4);
        assert!(matches!(mark.kind, MarkKind::Bold));
    }

    #[test]
    fn delete_across_two_marks_trims_both() {
        let mut format = Format::default();
        format.add_mark(0, 3, MarkKind::Italic);
        format.add_mark(3, 6, MarkKind::Underline);
        format.shift_delete(2, 5);
        let marks = format.marks();
        assert_eq!(marks.len(), 2);
        assert_eq!((marks[0].start, marks[0].end), (0, 2));
        assert_eq!((marks[1].start, marks[1].end), (2, 3));
    }

    #[test]
    fn corrupt_sidecar_is_empty_and_prose_is_untouched() {
        let root = std::env::temp_dir().join(format!("yonro-format-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".yonro")).unwrap();
        let scene = root.join("scene-1.md");
        std::fs::write(&scene, "plain words\n").unwrap();
        std::fs::write(root.join(".yonro/format.json"), "{not json").unwrap();
        let loaded = FormatFile::load(&root);
        assert!(loaded.scene("scene-1.md").marks().is_empty());
        let mut file = loaded;
        let mut format = Format::default();
        format.add_mark(0, 5, MarkKind::Bold);
        file.set_scene("scene-1.md", format);
        file.save(&root).unwrap();
        assert_eq!(std::fs::read_to_string(&scene).unwrap(), "plain words\n");
        assert!(!FormatFile::load(&root)
            .scene("scene-1.md")
            .marks()
            .is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
