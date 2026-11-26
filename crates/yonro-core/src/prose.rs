//! Visual rows for the page. Core owns grapheme breaks and display columns.
//! Pixels stay in the view.

use unicode_segmentation::UnicodeSegmentation;

use super::buffers::Buffer;
use super::prelude::{ColIdx, Location};

/// One soft-wrapped row of a source line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisualRow {
    /// Source line.
    pub line_idx: usize,
    /// First grapheme, inclusive.
    pub start: usize,
    /// One past the last grapheme.
    pub end: usize,
}

/// Soft-wrapped rows for every line. `columns == 0` does not wrap.
#[must_use]
pub fn visual_rows(buffer: &Buffer, columns: ColIdx) -> Vec<VisualRow> {
    let mut rows = Vec::new();
    let height = buffer.height().max(1);
    for line_idx in 0..height {
        let Some(line) = buffer.get_line(line_idx) else {
            rows.push(VisualRow {
                line_idx,
                start: 0,
                end: 0,
            });
            continue;
        };
        for range in line.wrap_segments(columns) {
            rows.push(VisualRow {
                line_idx,
                start: range.start,
                end: range.end,
            });
        }
    }
    if rows.is_empty() {
        rows.push(VisualRow {
            line_idx: 0,
            start: 0,
            end: 0,
        });
    }
    rows
}

/// The words of one visual row, graphemes `[start, end)`.
#[must_use]
pub fn row_text(buffer: &Buffer, row: &VisualRow) -> String {
    buffer
        .get_line(row.line_idx)
        .map_or_else(String::new, |line| {
            line.graphemes(true)
                .skip(row.start)
                .take(row.end.saturating_sub(row.start))
                .collect()
        })
}

/// `(row index, display column within that row)` for a caret.
#[must_use]
pub fn location_to_point(buffer: &Buffer, columns: ColIdx, at: Location) -> (usize, ColIdx) {
    let rows = visual_rows(buffer, columns);
    let row_index = rows
        .iter()
        .position(|row| {
            row.line_idx == at.line_idx && at.grapheme_idx >= row.start && at.grapheme_idx < row.end
        })
        .or_else(|| rows.iter().rposition(|row| row.line_idx == at.line_idx))
        .unwrap_or(0);
    let row = rows.get(row_index);
    let column = row.map_or(0, |row| {
        buffer
            .width_until(row.line_idx, at.grapheme_idx)
            .saturating_sub(buffer.width_until(row.line_idx, row.start))
    });
    (row_index, column)
}

/// Caret for a click on `row_index` at a display `column`.
#[must_use]
pub fn point_to_location(
    buffer: &Buffer,
    columns: ColIdx,
    row_index: usize,
    column: ColIdx,
) -> Location {
    let rows = visual_rows(buffer, columns);
    let Some(row) = rows.get(row_index).or_else(|| rows.last()) else {
        return Location::default();
    };
    let grapheme_idx = grapheme_at_column(buffer, row, column);
    Location {
        grapheme_idx,
        line_idx: row.line_idx,
    }
}

/// Half-open grapheme span of the word that contains `at`.
///
/// A click on whitespace selects that whitespace run. An empty line selects
/// an empty span at the caret.
#[must_use]
pub fn word_bounds_at(buffer: &Buffer, at: Location) -> (Location, Location) {
    let Some(line) = buffer.get_line(at.line_idx) else {
        return (at, at);
    };
    let text: &str = &line;
    let mut grapheme_at = 0usize;
    for chunk in text.split_word_bounds() {
        let len = chunk.graphemes(true).count();
        let next = grapheme_at.saturating_add(len);
        if at.grapheme_idx < next {
            return (
                Location {
                    grapheme_idx: grapheme_at,
                    line_idx: at.line_idx,
                },
                Location {
                    grapheme_idx: next,
                    line_idx: at.line_idx,
                },
            );
        }
        grapheme_at = next;
    }
    (at, at)
}

/// Byte offset of grapheme `index` in `text`, or `text.len()` past the end.
#[must_use]
pub fn grapheme_byte(text: &str, index: usize) -> usize {
    text.grapheme_indices(true)
        .nth(index)
        .map_or(text.len(), |(byte, _)| byte)
}

fn grapheme_at_column(buffer: &Buffer, row: &VisualRow, column: ColIdx) -> usize {
    let origin = buffer.width_until(row.line_idx, row.start);
    let mut grapheme_idx = row.start;
    while grapheme_idx < row.end {
        let next = grapheme_idx.saturating_add(1);
        let width = buffer
            .width_until(row.line_idx, next)
            .saturating_sub(origin);
        if width > column {
            break;
        }
        grapheme_idx = next;
    }
    grapheme_idx
}

/// UTF-16 code-unit offset of a caret. The platform speaks UTF-16; the rope does not.
#[must_use]
pub fn location_to_utf16(buffer: &Buffer, at: Location) -> usize {
    let mut units = 0usize;
    let height = buffer.height();
    let line_idx = at.line_idx.min(height.saturating_sub(1));
    for index in 0..line_idx {
        if let Some(line) = buffer.get_line(index) {
            units = units.saturating_add(line.encode_utf16().count());
        }
        units = units.saturating_add(1);
    }
    if let Some(line) = buffer.get_line(line_idx) {
        let prefix: String = line.graphemes(true).take(at.grapheme_idx).collect();
        units = units.saturating_add(prefix.encode_utf16().count());
    }
    units
}

/// Caret for a UTF-16 offset. A multi-unit grapheme is not split.
#[must_use]
pub fn utf16_to_location(buffer: &Buffer, mut index: usize) -> Location {
    let height = buffer.height().max(1);
    for line_idx in 0..height {
        let Some(line) = buffer.get_line(line_idx) else {
            return Location::default();
        };
        let mut grapheme_idx = 0usize;
        for grapheme in line.graphemes(true) {
            let units = grapheme.encode_utf16().count();
            if index < units {
                return Location {
                    grapheme_idx,
                    line_idx,
                };
            }
            index = index.saturating_sub(units);
            grapheme_idx = grapheme_idx.saturating_add(1);
        }
        if line_idx.saturating_add(1) < height {
            if index == 0 {
                return Location {
                    grapheme_idx,
                    line_idx,
                };
            }
            index = index.saturating_sub(1);
        } else {
            return Location {
                grapheme_idx,
                line_idx,
            };
        }
    }
    Location::default()
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
    fn wide_grapheme_advances_two_columns() {
        let mut buffer = Buffer::default();
        buffer.insert_str(loc(0, 0), "a你b");
        let (row, column) = location_to_point(&buffer, 0, loc(0, 1));
        assert_eq!(row, 0);
        assert_eq!(column, 1);
        let after = location_to_point(&buffer, 0, loc(0, 2));
        assert_eq!(after.1, 3);
        let hit = point_to_location(&buffer, 0, 0, 1);
        assert_eq!(hit.grapheme_idx, 1);
        let mid = point_to_location(&buffer, 0, 0, 2);
        assert_eq!(mid.grapheme_idx, 1);
    }

    #[test]
    fn wrap_maps_the_second_visual_row() {
        let mut buffer = Buffer::default();
        buffer.insert_str(loc(0, 0), "one two three");
        let rows = visual_rows(&buffer, 8);
        assert!(rows.len() >= 2);
        let start = rows[1].start;
        let back = point_to_location(&buffer, 8, 1, 0);
        assert_eq!(back.grapheme_idx, start);
        let (row, _) = location_to_point(&buffer, 8, loc(0, start));
        assert_eq!(row, 1);
    }

    #[test]
    fn word_bounds_cover_the_word() {
        let mut buffer = Buffer::default();
        buffer.insert_str(loc(0, 0), "hello world");
        let (start, end) = word_bounds_at(&buffer, loc(0, 1));
        assert_eq!(start.grapheme_idx, 0);
        assert_eq!(end.grapheme_idx, 5);
    }

    #[test]
    fn emoji_is_one_grapheme_and_two_utf16_units() {
        let mut buffer = Buffer::default();
        buffer.insert_str(loc(0, 0), "a😀b");
        assert_eq!(location_to_utf16(&buffer, loc(0, 1)), 1);
        assert_eq!(location_to_utf16(&buffer, loc(0, 2)), 3);
        assert_eq!(utf16_to_location(&buffer, 2).grapheme_idx, 1);
    }

    #[test]
    fn dead_key_emoji_and_devanagari_stay_whole_graphemes() {
        let mut buffer = Buffer::default();
        buffer.insert_str(loc(0, 0), "é");
        assert_eq!(buffer.grapheme_count(0), 1);
        assert_eq!(location_to_utf16(&buffer, loc(0, 1)), 1);

        buffer.insert_str(loc(0, 1), "😀");
        assert_eq!(buffer.grapheme_count(0), 2);
        assert_eq!(location_to_utf16(&buffer, loc(0, 2)), 3);
        assert_eq!(utf16_to_location(&buffer, 2).grapheme_idx, 1);

        buffer.insert_str(loc(0, 2), "क्ष");
        assert_eq!(buffer.grapheme_count(0), 3);
        let after = location_to_utf16(&buffer, loc(0, 3));
        assert!(after > 3);
        assert_eq!(
            utf16_to_location(&buffer, after.saturating_sub(1)).grapheme_idx,
            2
        );
    }
}
