use super::{Annotation, AnnotationType, Line, SyntaxHighlighter};
use crate::prelude::*;

#[derive(Default)]
pub struct MarkDownSyntaxHighlighter {
    highlights: Vec<Vec<Annotation>>,
    in_code_block: bool,
}

impl MarkDownSyntaxHighlighter {
    fn annotate_code_block(&mut self, line: &str) -> Option<Annotation> {
        if line.trim_start().starts_with("```") {
            self.in_code_block = !self.in_code_block;
            return Some(Annotation {
                annotation_type: AnnotationType::CodeBlock,
                start_byte_idx: 0,
                end_byte_idx: line.len(),
            });
        }

        if self.in_code_block {
            return Some(Annotation {
                annotation_type: AnnotationType::CodeBlock,
                start_byte_idx: 0,
                end_byte_idx: line.len(),
            });
        }

        None
    }

    fn annotate_heading(line: &str) -> Option<Annotation> {
        let trimmed = line.trim_start();
        let level = trimmed.chars().take_while(|&c| c == '#').count();

        if level > 0 && trimmed.chars().nth(level) == Some(' ') {
            return Some(Annotation {
                annotation_type: AnnotationType::Heading,
                start_byte_idx: 0,
                end_byte_idx: line.len(),
            });
        }

        None
    }

    fn annotate_inline_code(string: &str) -> Option<Annotation> {
        if let Some(start) = string.find('`') {
            if let Some(end) = string[start + 1..].find('`') {
                return Some(Annotation {
                    annotation_type: AnnotationType::InlineCode,
                    start_byte_idx: start,
                    end_byte_idx: start + end + 2,
                });
            }
        }
        None
    }

    fn annotate_bold_italic(string: &str) -> Option<Annotation> {
        let bytes = string.as_bytes();

        for i in 0..bytes.len() {
            if bytes[i] == b'*' || bytes[i] == b'_' {
                let marker = bytes[i];
                let mut j = i + 1;

                while j < bytes.len() {
                    if bytes[j] == marker {
                        return Some(Annotation {
                            annotation_type: AnnotationType::Emphasis,
                            start_byte_idx: i,
                            end_byte_idx: j + 1,
                        });
                    }
                    j += 1;
                }
            }
        }
        None
    }

    fn annotate_link(string: &str) -> Option<Annotation> {
        if let Some(start) = string.find('[') {
            if let Some(mid) = string[start..].find("](") {
                if let Some(end) = string[start + mid + 2..].find(')') {
                    return Some(Annotation {
                        annotation_type: AnnotationType::Link,
                        start_byte_idx: start,
                        end_byte_idx: start + mid + 2 + end + 1,
                    });
                }
            }
        }
        None
    }

    fn annotate_list(line: &str) -> Option<Annotation> {
        let trimmed = line.trim_start();

        if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
            return Some(Annotation {
                annotation_type: AnnotationType::ListItem,
                start_byte_idx: 0,
                end_byte_idx: line.len(),
            });
        }

        None
    }
}

impl SyntaxHighlighter for MarkDownSyntaxHighlighter {
    fn highlight(&mut self, idx: LineIdx, line: &Line) {
        debug_assert_eq!(idx, self.highlights.len());

        let mut result = Vec::new();
        let line_str: &str = line;

        // Priority order matters
        if let Some(a) = self.annotate_code_block(line_str) {
            result.push(a);
        } else if let Some(a) = Self::annotate_heading(line_str) {
            result.push(a);
        } else if let Some(a) = Self::annotate_list(line_str) {
            result.push(a);
        } else {
            if let Some(a) = Self::annotate_inline_code(line_str) {
                result.push(a);
            }
            if let Some(a) = Self::annotate_bold_italic(line_str) {
                result.push(a);
            }
            if let Some(a) = Self::annotate_link(line_str) {
                result.push(a);
            }
        }

        self.highlights.push(result);
    }

    fn get_annotations(&self, idx: LineIdx) -> Option<&Vec<Annotation>> {
        self.highlights.get(idx)
    }
}
