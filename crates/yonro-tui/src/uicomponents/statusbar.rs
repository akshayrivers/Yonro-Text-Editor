use crate::terminal::Terminal;
use yonro_core::DocumentStatus;
use super::UIComponent;
use crate::prelude::*;
use std::io::Error;
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
            "{} - {line_count} {modified_indicator}",
            self.current_status.file_name
        );
        // Assemble the back part
        let position_indicator = self.current_status.position_indicator_to_string();

        let file_type = self.current_status.file_type_to_string();

        let back_part = format!("{file_type} | {position_indicator}");

        // assemble the whole status bar
        let remainder_len = width.saturating_sub(beginning.len());

        let status = format!("{beginning}{back_part:>remainder_len$}");

        // Only print out the status if it fits.
        // Otherwise write out an empty string
        // to ensure the row is cleared.
        let to_print = if status.len() <= width {
            status
        } else {
            String::new()
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
