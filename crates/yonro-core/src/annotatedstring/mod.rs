use crate::prelude::ByteIdx;
use crate::{Annotation, AnnotationType};
use std::{
    cmp::{max, min},
    fmt::{self, Display},
};
mod annotatedstringpart;
use annotatedstringpart::AnnotatedStringPart;
mod annotatedstringiterator;
use annotatedstringiterator::AnnotatedStringIterator;

#[derive(Default, Debug, Clone)]
pub struct AnnotatedString {
    string: String,
    annotations: Vec<Annotation>,
}

impl AnnotatedString {
    pub fn from(string: &str) -> Self {
        Self {
            string: String::from(string),
            annotations: Vec::new(),
        }
    }
    pub fn add_annotation(
        &mut self,
        annotation_type: AnnotationType,
        start_byte_idx: ByteIdx,
        end_byte_idx: ByteIdx,
    ) {
        debug_assert!(start_byte_idx <= end_byte_idx);
        self.annotations.push(Annotation {
            annotation_type,
            start_byte_idx,
            end_byte_idx,
        });
    }
    pub fn truncate_left_until(&mut self, until: ByteIdx) {
        self.replace(0, until, "");
    }
    pub fn truncate_right_from(&mut self, from: ByteIdx) {
        self.replace(from, self.string.len(), "");
    }

    pub fn replace(&mut self, start_byte_idx: ByteIdx, end_byte_idx: ByteIdx, new_string: &str) {
        let end = min(end_byte_idx, self.string.len());
        debug_assert!(start_byte_idx <= end);
        debug_assert!(start_byte_idx <= self.string.len());
        if start_byte_idx > end {
            return;
        }
        self.string
            .replace_range(start_byte_idx..end_byte_idx, new_string);

        let replaced_range_len = end_byte_idx.saturating_sub(start_byte_idx); // This is the range we want to replace.
        let shortened = new_string.len() < replaced_range_len;
        let len_difference = new_string.len().abs_diff(replaced_range_len); // This is how much longer or shorter the new range is.

        if len_difference == 0 {
            //No adjustment of annotations needed in case the replacement did not result in a change in length.
            return;
        }

        self.annotations.iter_mut().for_each(|annotation| {
            annotation.start_byte_idx = if annotation.start_byte_idx >= end_byte_idx {
                // For annotations starting after the replaced range, we move the start index by the difference in length.
                if shortened {
                    annotation.start_byte_idx.saturating_sub(len_difference)
                } else {
                    annotation.start_byte_idx.saturating_add(len_difference)
                }
            } else if annotation.start_byte_idx >= start_byte_idx {
                // For annotations starting within the replaced range, we move the start index by the difference in length, constrained to the beginning or end of the replaced range.
                if shortened {
                    max(
                        start_byte_idx,
                        annotation.start_byte_idx.saturating_sub(len_difference),
                    )
                } else {
                    min(
                        end,
                        annotation.start_byte_idx.saturating_add(len_difference),
                    )
                }
            } else {
                annotation.start_byte_idx
            };

            annotation.end_byte_idx = if annotation.end_byte_idx >= end {
                // For annotations ending after the replaced range, we move the end index by the difference in length.
                if shortened {
                    annotation.end_byte_idx.saturating_sub(len_difference)
                } else {
                    annotation.end_byte_idx.saturating_add(len_difference)
                }
            } else if annotation.end_byte_idx >= start_byte_idx {
                // For annotations ending within the replaced range, we move the end index by the difference in length, constrained to the beginning or end of the replaced range.
                if shortened {
                    max(
                        start_byte_idx,
                        annotation.end_byte_idx.saturating_sub(len_difference),
                    )
                } else {
                    min(
                        end_byte_idx,
                        annotation.end_byte_idx.saturating_add(len_difference),
                    )
                }
            } else {
                annotation.end_byte_idx
            }
        });

        //Filter out empty annotations, in case the previous step resulted in any.
        self.annotations.retain(|annotation| {
            annotation.start_byte_idx < annotation.end_byte_idx
                && annotation.start_byte_idx < self.string.len()
        });
    }
}
impl Display for AnnotatedString {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "{}", self.string)
    }
}

impl<'a> IntoIterator for &'a AnnotatedString {
    type Item = AnnotatedStringPart<'a>;
    type IntoIter = AnnotatedStringIterator<'a>;

    fn into_iter(self) -> Self::IntoIter {
        AnnotatedStringIterator {
            annotated_string: self,
            current_idx: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn iterator_with_annotation() {
        let mut s = AnnotatedString::from("hello world");

        s.add_annotation(AnnotationType::Keyword, 0, 5);

        let parts: Vec<_> = (&s).into_iter().collect();

        assert_eq!(parts.len(), 2);

        assert_eq!(parts[0].string, "hello");
        assert!(parts[0].annotation_type.is_some());

        assert_eq!(parts[1].string, " world");
        assert!(parts[1].annotation_type.is_none());
    }

    #[test]
    fn unicode_handling() {
        let mut s = AnnotatedString::from("héllo");

        s.add_annotation(AnnotationType::Keyword, 0, 3);

        let parts: Vec<_> = (&s).into_iter().collect();

        assert!(!parts.is_empty());
    }

    #[test]
    fn overlapping_annotations() {
        let mut s = AnnotatedString::from("hello world");

        s.add_annotation(AnnotationType::Keyword, 0, 5);

        s.add_annotation(AnnotationType::String, 3, 8);

        let parts: Vec<_> = (&s).into_iter().collect();

        assert!(!parts.is_empty());
    }
    #[test]
    fn empty_annotation_removed() {
        let mut s = AnnotatedString::from("hello");

        s.add_annotation(AnnotationType::Keyword, 2, 2);

        s.replace(0, 5, "hi");

        assert!(s.annotations.is_empty());
    }
    #[test]
    fn replace_entire_string() {
        let mut s = AnnotatedString::from("hello world");

        s.add_annotation(AnnotationType::Keyword, 0, 5);

        s.replace(0, 11, "hi");

        assert_eq!(s.string, "hi");
        assert!(s.annotations.is_empty());
    }
    #[test]
    fn replace_with_empty() {
        let mut s = AnnotatedString::from("hello world");

        s.replace(0, 5, "");

        assert_eq!(s.string, " world");
    }
    #[test]
    fn annotation_at_end() {
        let mut s = AnnotatedString::from("hello world");

        s.add_annotation(AnnotationType::Keyword, 6, 11);

        let parts: Vec<_> = (&s).into_iter().collect();

        assert_eq!(parts.len(), 2);
    }
    #[test]
    fn adjacent_annotations() {
        let mut s = AnnotatedString::from("abcdef");

        s.add_annotation(AnnotationType::Keyword, 0, 3);

        s.add_annotation(AnnotationType::String, 3, 6);

        let parts: Vec<_> = (&s).into_iter().collect();

        assert_eq!(parts.len(), 2);
    }
    #[test]
    fn complex_unicode() {
        let mut s = AnnotatedString::from("a🙂bé漢字");

        s.add_annotation(AnnotationType::Keyword, 0, 4);

        let parts: Vec<_> = (&s).into_iter().collect();

        assert!(!parts.is_empty());
    }
    #[test]
    fn editing_flow() {
        let mut s = AnnotatedString::from("hello");

        s.add_annotation(AnnotationType::Keyword, 0, 5);

        s.replace(5, 5, " world");

        assert_eq!(s.string, "hello world");
    }
}
