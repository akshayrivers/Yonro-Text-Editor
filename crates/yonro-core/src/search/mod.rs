//! Text search (`P5.3`): literal substring search over buffers and scenes.
//!
//! Offsets follow the `P4.5` rule: UTF-16 code units, so JS `string`
//! indices slice exactly. Matching is literal (no regex); an empty query
//! matches nothing. Project hits anchor to 1-based lines with 1-based
//! UTF-16 columns, mirroring the statusline `Ln/Col` convention.

use std::collections::BTreeMap;

use super::manuscript::{Manuscript, NodeId};

/// One match as UTF-16 code-unit offsets (`start` inclusive, `end` exclusive).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Utf16Span {
    pub start: usize,
    pub end: usize,
}

/// One project-wide hit, anchored to a line.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProjectHit {
    pub scene_id: NodeId,
    pub title: String,
    pub file: Option<String>,
    /// 1-based line number.
    pub line: usize,
    /// 1-based UTF-16 column where the match starts.
    pub col_start: usize,
    /// 1-based UTF-16 column where the match ends (exclusive).
    pub col_end: usize,
    /// Source line trimmed to about [`EXCERPT_CHARS`] characters.
    pub excerpt: String,
}

/// Cap on project-wide hits (outline order wins).
pub const MAX_PROJECT_HITS: usize = 500;

/// Width of the server-side excerpt window (characters, approximate).
pub const EXCERPT_CHARS: usize = 80;

/// Single-character equality with optional ASCII-friendly case folding.
///
/// Multi-character lowercases (e.g. `İ`) fall back to exact equality so
/// offsets into the original text always stay exact.
fn chars_equal(left: char, right: char, case_sensitive: bool) -> bool {
    if left == right {
        return true;
    }
    if case_sensitive {
        return false;
    }
    let mut lower_left = left.to_lowercase();
    let mut lower_right = right.to_lowercase();
    match (lower_left.next(), lower_right.next()) {
        (Some(first_left), Some(first_right)) => {
            first_left == first_right && lower_left.next().is_none() && lower_right.next().is_none()
        }
        (None, None) => true,
        (None, Some(_)) | (Some(_), None) => false,
    }
}

/// All non-overlapping literal occurrences of `query` in `text`.
///
/// Empty queries match nothing. Offsets are UTF-16 code units.
#[must_use]
pub fn find_in_text(text: &str, query: &str, case_sensitive: bool) -> Vec<Utf16Span> {
    let query_chars: Vec<char> = query.chars().collect();
    if query_chars.is_empty() {
        return Vec::new();
    }
    let text_chars: Vec<char> = text.chars().collect();
    let mut utf16: Vec<usize> = Vec::with_capacity(text_chars.len().saturating_add(1));
    let mut acc = 0usize;
    for ch in &text_chars {
        utf16.push(acc);
        acc = acc.saturating_add(ch.len_utf16());
    }
    utf16.push(acc);
    let mut out = Vec::new();
    let mut index = 0usize;
    while index.saturating_add(query_chars.len()) <= text_chars.len() {
        let mut matched = true;
        for (offset, wanted) in query_chars.iter().enumerate() {
            let got = text_chars.get(index.saturating_add(offset));
            if got.is_none_or(|got| !chars_equal(*got, *wanted, case_sensitive)) {
                matched = false;
                break;
            }
        }
        if matched {
            let start = utf16.get(index).copied().unwrap_or(0);
            let end = utf16
                .get(index.saturating_add(query_chars.len()))
                .copied()
                .unwrap_or(start);
            out.push(Utf16Span { start, end });
            index = index.saturating_add(query_chars.len().max(1));
        } else {
            index = index.saturating_add(1);
        }
    }
    out
}

/// Source line trimmed to about [`EXCERPT_CHARS`] characters, centered on
/// the match (ellipsis marks either trimmed side).
fn excerpt_for(line: &str, match_start: usize) -> String {
    let chars: Vec<char> = line.chars().collect();
    if chars.len() <= EXCERPT_CHARS {
        return line.trim().to_string();
    }
    let mut seen = 0usize;
    let mut char_idx = 0usize;
    for (position, ch) in chars.iter().enumerate() {
        if seen >= match_start {
            char_idx = position;
            break;
        }
        seen = seen.saturating_add(ch.len_utf16());
        char_idx = position.saturating_add(1);
    }
    let start = char_idx.saturating_sub(20);
    let window: String = chars.iter().skip(start).take(EXCERPT_CHARS).collect();
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.push_str(window.trim());
    if start.saturating_add(EXCERPT_CHARS) < chars.len() {
        out.push('…');
    }
    out
}

/// Search every scene in outline order, line by line.
///
/// `scene_texts` maps scene ids to text (open buffers win over disk — see
/// `api::gather_scene_texts`). Blank queries match nothing; output caps at
/// [`MAX_PROJECT_HITS`] with earlier outline order winning.
#[must_use]
pub fn search_project(
    manuscript: &Manuscript,
    scene_texts: &BTreeMap<NodeId, String>,
    query: &str,
    case_sensitive: bool,
) -> Vec<ProjectHit> {
    if query.trim().is_empty() {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for scene_id in super::graph::Graph::scene_order(manuscript) {
        let Some(text) = scene_texts.get(&scene_id) else {
            continue;
        };
        let node = manuscript.get(scene_id);
        let title = node.map_or_else(|| format!("scene-{scene_id}"), |node| node.title.clone());
        let file = node
            .and_then(|node| node.meta.as_ref())
            .and_then(|meta| meta.file.clone())
            .map(|path| path.to_string_lossy().to_string());
        for (index, line) in text.lines().enumerate() {
            let line_no = index.saturating_add(1);
            for span in find_in_text(line, query, case_sensitive) {
                hits.push(ProjectHit {
                    scene_id,
                    title: title.clone(),
                    file: file.clone(),
                    line: line_no,
                    col_start: span.start.saturating_add(1),
                    col_end: span.end.saturating_add(1),
                    excerpt: excerpt_for(line, span.start),
                });
                if hits.len() >= MAX_PROJECT_HITS {
                    return hits;
                }
            }
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed_manuscript() -> (Manuscript, BTreeMap<NodeId, String>) {
        let mut manuscript = Manuscript::new("Probe");
        let act = manuscript.add_act("Act I").unwrap();
        let ch = manuscript.add_chapter(act, "Chapter 1").unwrap();
        let first = manuscript.add_scene(ch, "Gate").unwrap();
        let second = manuscript.add_scene(ch, "River").unwrap();
        let mut texts = BTreeMap::new();
        texts.insert(first, "Mara walked home.\nMara slept well.".to_string());
        texts.insert(second, "Joren waved at Mara.".to_string());
        (manuscript, texts)
    }

    #[test]
    fn finds_ascii_spans_with_utf16_offsets() {
        let spans = find_in_text("Hello @Mara world", "Mara", true);
        assert_eq!(spans, vec![Utf16Span { start: 7, end: 11 }]);
    }

    #[test]
    fn emoji_before_match_shifts_utf16_offsets() {
        // "👋 " is 3 UTF-16 units (surrogate pair + space).
        let spans = find_in_text("👋 Mara", "Mara", true);
        assert_eq!(spans, vec![Utf16Span { start: 3, end: 7 }]);
        let text = "👋 Mara";
        let units: Vec<u16> = text.encode_utf16().collect();
        let slice = String::from_utf16(&units[spans[0].start..spans[0].end]).unwrap();
        assert_eq!(slice, "Mara");
    }

    #[test]
    fn case_folding_matches_umlauts_but_case_sensitive_does_not() {
        assert_eq!(find_in_text("Äpfel", "äpfel", false).len(), 1);
        assert!(find_in_text("Äpfel", "äpfel", true).is_empty());
        assert_eq!(find_in_text("MARA mara", "mara", false).len(), 2);
    }

    #[test]
    fn empty_query_matches_nothing_and_hits_do_not_overlap() {
        assert!(find_in_text("anything", "", true).is_empty());
        assert!(find_in_text("anything", "   ".trim(), true).is_empty());
        assert_eq!(
            find_in_text("aaa", "aa", true),
            vec![Utf16Span { start: 0, end: 2 }]
        );
    }

    #[test]
    fn project_search_uses_outline_order_line_cols_and_excerpts() {
        let (manuscript, texts) = seed_manuscript();
        let hits = search_project(&manuscript, &texts, "Mara", true);
        assert_eq!(hits.len(), 3);
        // Outline order: both "Gate" hits before "River".
        assert_eq!(hits[0].title, "Gate");
        assert_eq!(hits[0].line, 1);
        assert_eq!((hits[0].col_start, hits[0].col_end), (1, 5));
        assert_eq!(hits[1].line, 2);
        assert_eq!(hits[2].title, "River");
        assert_eq!(hits[2].excerpt, "Joren waved at Mara.");
        assert!(search_project(&manuscript, &texts, "   ", true).is_empty());
    }

    #[test]
    fn project_search_trims_long_lines_and_caps_results() {
        let mut manuscript = Manuscript::new("Probe");
        let act = manuscript.add_act("A").unwrap();
        let ch = manuscript.add_chapter(act, "C").unwrap();
        let scene = manuscript.add_scene(ch, "S").unwrap();
        let long = format!("x{}needle{}y", "o".repeat(100), "o".repeat(100));
        let mut texts = BTreeMap::new();
        texts.insert(scene, long);
        let hits = search_project(&manuscript, &texts, "needle", true);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].excerpt.contains("needle"));
        assert!(hits[0].excerpt.chars().count() <= EXCERPT_CHARS + 2);
    }
}
