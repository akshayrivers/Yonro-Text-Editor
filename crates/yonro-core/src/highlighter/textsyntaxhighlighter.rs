use super::{Annotation, AnnotationType, Line, SyntaxHighlighter};
use crate::prelude::*;

#[derive(Default)]
pub struct TextSyntaxHighlighter {
    highlights: Vec<Vec<Annotation>>,
}

impl TextSyntaxHighlighter {
    fn annotate_url(string: &str) -> Option<Annotation> {
        if let Some(start) = string.find("http://").or_else(|| string.find("https://")) {
            let end = string[start..]
                .find(char::is_whitespace)
                .map(|i| start + i)
                .unwrap_or(string.len());

            return Some(Annotation {
                annotation_type: AnnotationType::Link,
                start_byte_idx: start,
                end_byte_idx: end,
            });
        }
        None
    }

    fn annotate_number(string: &str) -> Option<Annotation> {
        let mut start = None;

        for (i, c) in string.char_indices() {
            if c.is_ascii_digit() {
                start.get_or_insert(i);
            } else if let Some(s) = start {
                return Some(Annotation {
                    annotation_type: AnnotationType::Number,
                    start_byte_idx: s,
                    end_byte_idx: i,
                });
            }
        }

        start.map(|s| Annotation {
            annotation_type: AnnotationType::Number,
            start_byte_idx: s,
            end_byte_idx: string.len(),
        })
    }

    fn annotate_email(string: &str) -> Option<Annotation> {
        if let Some(at) = string.find('@') {
            let start = string[..at]
                .rfind(char::is_whitespace)
                .map(|i| i + 1)
                .unwrap_or(0);

            let end = string[at..]
                .find(char::is_whitespace)
                .map(|i| at + i)
                .unwrap_or(string.len());

            if string[start..end].contains('.') {
                return Some(Annotation {
                    annotation_type: AnnotationType::Link,
                    start_byte_idx: start,
                    end_byte_idx: end,
                });
            }
        }
        None
    }
}

impl SyntaxHighlighter for TextSyntaxHighlighter {
    fn highlight(&mut self, idx: LineIdx, line: &Line) {
        debug_assert_eq!(idx, self.highlights.len());

        let mut result = Vec::new();
        let line_str: &str = line;

        if let Some(a) = Self::annotate_url(line_str) {
            result.push(a);
        }

        if let Some(a) = Self::annotate_email(line_str) {
            result.push(a);
        }

        if let Some(a) = Self::annotate_number(line_str) {
            result.push(a);
        }

        self.highlights.push(result);
    }

    fn get_annotations(&self, idx: LineIdx) -> Option<&Vec<Annotation>> {
        self.highlights.get(idx)
    }
}
