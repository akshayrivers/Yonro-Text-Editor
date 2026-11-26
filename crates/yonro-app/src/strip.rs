//! The format strip. Marks and paragraph props live in core.

use yonro_core::{
    flat_index, prelude::Location, Align, ListKind, MarkKind, ParaStyle, Paragraph, Spacing,
};

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
        self.log.note_format(self.caret);
        self.log.format_mut().toggle(start, end, kind);
    }

    pub(crate) fn clear_format(&mut self) {
        let (start, end) = self.selected_flats();
        self.sticky.clear();
        self.log.note_format(self.caret);
        if start < end {
            self.log.format_mut().clear_range(start, end);
        }
        for line in self.touched_lines() {
            self.log.format_mut().set_paragraph(plain_paragraph(line));
        }
    }

    pub(crate) fn set_style(&mut self, style: ParaStyle) {
        self.edit_lines(|para| para.style = style);
    }

    pub(crate) fn cycle_align(&mut self) {
        let next = match self.caret_para().align {
            Align::Left => Align::Center,
            Align::Center => Align::Right,
            Align::Right => Align::Justify,
            Align::Justify => Align::Left,
        };
        self.edit_lines(|para| para.align = next);
    }

    pub(crate) fn toggle_list(&mut self, kind: ListKind) {
        let next = if self.caret_para().list == kind {
            ListKind::None
        } else {
            kind
        };
        self.edit_lines(|para| para.list = next);
    }

    pub(crate) fn indent_lines(&mut self, out: bool) {
        self.edit_lines(|para| {
            para.indent = if out {
                para.indent.saturating_sub(1)
            } else {
                para.indent.saturating_add(1).min(8)
            };
        });
    }

    pub(crate) fn cycle_spacing(&mut self) {
        let next = match self.log.format().spacing() {
            Spacing::Compact => Spacing::Roomy,
            Spacing::Roomy => Spacing::Airy,
            Spacing::Airy => Spacing::Compact,
        };
        self.log.note_format(self.caret);
        self.log.format_mut().set_spacing(next);
    }

    pub(crate) fn cycle_face(&mut self) {
        let next = match self.log.format().face() {
            "Times New Roman" => "Palatino",
            "Palatino" => "Georgia",
            _ => "Times New Roman",
        };
        self.log.note_format(self.caret);
        self.log.format_mut().set_face(next);
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

    pub(crate) fn style_label(&self) -> &'static str {
        match self.caret_para().style {
            ParaStyle::Normal => "body",
            ParaStyle::H1 => "h1",
            ParaStyle::H2 => "h2",
            ParaStyle::H3 => "h3",
        }
    }

    pub(crate) fn align_label(&self) -> &'static str {
        match self.caret_para().align {
            Align::Left => "left",
            Align::Center => "center",
            Align::Right => "right",
            Align::Justify => "justify",
        }
    }

    pub(crate) fn list_label(&self) -> &'static str {
        match self.caret_para().list {
            ListKind::None => "list",
            ListKind::Bullet => "bullet",
            ListKind::Number => "number",
        }
    }

    pub(crate) fn spacing_label(&self) -> &'static str {
        match self.log.format().spacing() {
            Spacing::Compact => "compact",
            Spacing::Roomy => "roomy",
            Spacing::Airy => "airy",
        }
    }

    pub(crate) fn face_label(&self) -> &'static str {
        match self.log.format().face() {
            "Times New Roman" => "times",
            "Palatino" => "palatino",
            _ => "georgia",
        }
    }

    pub(crate) fn use_painter(&mut self) {
        let (start, end) = self.selected_flats();
        if self.painter.is_empty() || start == end {
            self.painter = self.marks_covering(start, end);
            return;
        }
        self.log.note_format(self.caret);
        for kind in self.painter.clone() {
            self.log.format_mut().add_mark(start, end, kind);
        }
        self.painter.clear();
    }

    pub(crate) fn open_note(&mut self, window: &mut gpui::Window) {
        let (start, end) = self.selected_flats();
        if start == end {
            return;
        }
        self.note_open = true;
        self.note_draft.clear();
        self.note_focus.focus(window);
    }

    pub(crate) fn on_note_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let mods = &event.keystroke.modifiers;
        if mods.platform || mods.control {
            return;
        }
        match event.keystroke.key.as_str() {
            "escape" => {
                self.note_open = false;
                self.focus.focus(window);
            }
            "enter" => self.commit_note(window),
            "backspace" => {
                let mut chars: Vec<char> = self.note_draft.chars().collect();
                chars.pop();
                self.note_draft = chars.into_iter().collect();
            }
            _ => {
                if let Some(text) = event.keystroke.key_char.as_deref() {
                    if !text.is_empty() && text.chars().all(|ch| !ch.is_control()) {
                        self.note_draft.push_str(text);
                    }
                }
            }
        }
        cx.notify();
    }

    fn commit_note(&mut self, window: &mut gpui::Window) {
        let (start, end) = self.selected_flats();
        if start < end && !self.note_draft.is_empty() {
            self.log.note_format(self.caret);
            self.log
                .format_mut()
                .set_note(start, end, self.note_draft.clone());
        }
        self.note_open = false;
        self.focus.focus(window);
    }

    fn marks_covering(&self, start: usize, end: usize) -> Vec<yonro_core::MarkKind> {
        let end = if start == end {
            start.saturating_add(1)
        } else {
            end
        };
        let mut kinds = Vec::new();
        for mark in self.log.format().marks() {
            if mark.end <= start || mark.start >= end {
                continue;
            }
            if kinds
                .iter()
                .any(|kind| std::mem::discriminant(kind) == std::mem::discriminant(&mark.kind))
            {
                continue;
            }
            kinds.push(mark.kind.clone());
        }
        kinds
    }

    fn caret_para(&self) -> Paragraph {
        paragraph_on(self.log.format(), self.caret.line_idx)
    }

    fn edit_lines(&mut self, change: impl Fn(&mut Paragraph)) {
        let caret = self.caret;
        let lines: Vec<usize> = self.touched_lines().collect();
        let mut next = Vec::new();
        for line in lines {
            let mut para = paragraph_on(self.log.format(), line);
            change(&mut para);
            next.push(para);
        }
        self.log.note_format(caret);
        for para in next {
            self.log.format_mut().set_paragraph(para);
        }
    }

    fn touched_lines(&self) -> std::ops::RangeInclusive<usize> {
        let head = self.caret.line_idx;
        let tail = self.anchor.map(|mark| mark.line_idx).unwrap_or(head);
        head.min(tail)..=head.max(tail)
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

pub(crate) fn paragraph_on(format: &yonro_core::Format, line: usize) -> Paragraph {
    format
        .paragraphs()
        .iter()
        .find(|item| item.line == line)
        .cloned()
        .unwrap_or_else(|| plain_paragraph(line))
}

fn plain_paragraph(line: usize) -> Paragraph {
    Paragraph {
        line,
        style: ParaStyle::Normal,
        align: Align::Left,
        list: ListKind::None,
        indent: 0,
    }
}
