use super::AnnotationType;
use crate::prelude::*;

#[derive(Copy, Clone, Debug)]
//clippy::struct_field_names: naming the field `type` is disallowed due to type being a keyword
#[allow(clippy::struct_field_names)]
pub struct Annotation {
    pub annotation_type: AnnotationType,
    pub start_byte_idx: ByteIdx,
    pub end_byte_idx: ByteIdx,
}

impl Annotation {
    pub fn shift(&mut self, offset: ByteIdx) {
        self.start_byte_idx = self.start_byte_idx.saturating_add(offset);
        self.end_byte_idx = self.end_byte_idx.saturating_add(offset);
    }
}
