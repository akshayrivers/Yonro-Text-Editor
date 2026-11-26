//! The format strip. Marks live in core. This only toggles the span under the caret.

use yonro_core::{flat_index, prelude::Location, MarkKind};

use crate::document::Page;

impl Page {
    pub(crate) fn toggle_kind(&mut self, kind: MarkKind) {
        let (start, end) = self.selected_flats();
        if start == end {
            if let Some(index) = self
                .sticky
                .iter()
                .position(|have| std::mem::discriminant(have) == std::mem::discriminant(&kind))
            {
                self.sticky.remove(index);
            } else {
                self.sticky.push(kind);
            }
            return;
        }
        self.log.format_mut().toggle(start, end, kind);
    }

    pub(crate) fn clear_format(&mut self) {
        let (start, end) = self.selected_flats();
        if start == end {
            self.sticky.clear();
            return;
        }
        self.log.format_mut().clear_range(start, end);
    }

    pub(crate) fn apply_sticky(&mut self, before: Location) {
        if self.sticky.is_empty() {
            return;
        }
        let start = flat_index(&self.buffer, before);
        let end = flat_index(&self.buffer, self.caret);
        for kind in self.sticky.clone() {
            self.log.format_mut().add_mark(start, end, kind);
        }
    }

    pub(crate) fn strip_on(&self, kind: &MarkKind) -> bool {
        if self
            .sticky
            .iter()
            .any(|have| std::mem::discriminant(have) == std::mem::discriminant(kind))
        {
            return true;
        }
        let at = flat_index(&self.buffer, self.caret);
        if at == 0 {
            return false;
        }
        let at = at.saturating_sub(1);
        self.log.format().marks().iter().any(|mark| {
            std::mem::discriminant(&mark.kind) == std::mem::discriminant(kind)
                && at >= mark.start
                && at < mark.end
        })
    }

    fn selected_flats(&self) -> (usize, usize) {
        let head = flat_index(&self.buffer, self.caret);
        let tail = self
            .anchor
            .map(|mark| flat_index(&self.buffer, mark))
            .unwrap_or(head);
        (head.min(tail), head.max(tail))
    }
}
