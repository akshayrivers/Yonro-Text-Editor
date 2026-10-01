use super::{AnnotatedString, AnnotatedStringPart};
use crate::prelude::*;
use std::cmp::min;

pub struct AnnotatedStringIterator<'a> {
    pub annotated_string: &'a AnnotatedString,
    pub current_idx: ByteIdx,
}

impl<'a> Iterator for AnnotatedStringIterator<'a> {
    type Item = AnnotatedStringPart<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let s = &self.annotated_string.string;

        if self.current_idx >= s.len() {
            return None;
        }

        // Ensure current index is always on a valid UTF-8 boundary
        while self.current_idx > 0 && !s.is_char_boundary(self.current_idx) {
            self.current_idx -= 1;
        }

        // Find the current active annotation
        if let Some(annotation) = self
            .annotated_string
            .annotations
            .iter()
            .filter(|annotation| {
                annotation.start_byte_idx <= self.current_idx
                    && annotation.end_byte_idx > self.current_idx
            })
            .last()
        {
            let mut start_idx = self.current_idx;
            let mut end_idx = min(annotation.end_byte_idx, s.len());

            // Normalize start boundary
            while start_idx > 0 && !s.is_char_boundary(start_idx) {
                start_idx -= 1;
            }

            // Normalize end boundary
            while end_idx < s.len() && !s.is_char_boundary(end_idx) {
                end_idx += 1;
            }

            self.current_idx = end_idx;

            return Some(AnnotatedStringPart {
                string: &s[start_idx..end_idx],
                annotation_type: Some(annotation.annotation_type),
            });
        }

        // Find the boundary of the nearest annotation
        let mut end_idx = s.len();
        for annotation in &self.annotated_string.annotations {
            if annotation.start_byte_idx > self.current_idx && annotation.start_byte_idx < end_idx {
                end_idx = annotation.start_byte_idx;
            }
        }

        let mut start_idx = self.current_idx;

        // Normalize start boundary
        while start_idx > 0 && !s.is_char_boundary(start_idx) {
            start_idx -= 1;
        }

        // Normalize end boundary
        while end_idx < s.len() && !s.is_char_boundary(end_idx) {
            end_idx += 1;
        }

        self.current_idx = end_idx;

        Some(AnnotatedStringPart {
            string: &s[start_idx..end_idx],
            annotation_type: None,
        })
    }
}
