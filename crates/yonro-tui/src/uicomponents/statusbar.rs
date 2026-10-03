use super::UIComponent;
use crate::prelude::*;
use crate::terminal::Terminal;
use std::io::Error;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;
use yonro_core::DocumentStatus;
#[derive(Default)]
pub struct StatusBar {
    current_status: DocumentStatus,
    needs_redraw: bool,
    rect: Rect,
}

impl StatusBar {
    pub fn update_status(&mut self, new_status: DocumentStatus) {
        if new_status != self.current_status {
            self.current_status = new_status;
            self.mark_redraw(true);
        }
    }
}

impl UIComponent for StatusBar {
    fn mark_redraw(&mut self, value: bool) {
        self.needs_redraw = value;
    }
    fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }
    fn rect(&self) -> Rect {
        self.rect
    }
    fn set_size(&mut self, rect: Rect) {
        self.rect = rect;
    }

    fn draw(&mut self) -> Result<(), Error> {
        let width = self.rect.size.width;

        let line_count = self.current_status.line_count_to_string();

        let modified_indicator = self.current_status.modified_indicator_to_string();

        let beginning = format!(
            "{} - {line_count} {} words {modified_indicator}",
            self.current_status.file_name, self.current_status.word_count
        );
        // Assemble the back part
        let position_indicator = self.current_status.position_indicator_to_string();

        let file_type = self.current_status.file_type_to_string();

        let back_part = format!("{file_type} | {position_indicator}");

        // assemble the whole status bar (grapheme-width aware per AGENTS.md §2.1)
        let beginning_width = UnicodeWidthStr::width(beginning.as_str());
        let back_width = UnicodeWidthStr::width(back_part.as_str());
        let remainder_len = width
            .saturating_sub(beginning_width)
            .saturating_sub(back_width);

        let status = format!("{beginning}{back_part:>remainder_len$}");

        // Truncate (not blank) when the terminal is narrow.
        let to_print = if UnicodeWidthStr::width(status.as_str()) <= width {
            status
        } else {
            // Keep a readable prefix; `width` is a display-column budget.
            let mut kept = String::new();
            let mut used: usize = 0;
            for g in status.as_str().graphemes(true) {
                let w = UnicodeWidthStr::width(g);
                if used.saturating_add(w) > width {
                    break;
                }
                kept.push_str(g);
                used = used.saturating_add(w);
            }
            kept
        };

        Terminal::clear_rect_line(self.rect, self.rect.position.row)?;

        Terminal::print_at(
            self.rect.position,
            &format!(
                "{}{:width$}{}",
                crossterm::style::Attribute::Reverse,
                to_print,
                crossterm::style::Attribute::Reset,
                width = width,
            ),
        )?;

        Ok(())
    }
}
