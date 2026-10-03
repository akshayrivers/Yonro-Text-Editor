use crate::clipboard::Clipboard;
use crate::terminal::Terminal;
use yonro_core::{command::Edit, Line};
use super::UIComponent;

use crate::prelude::*;

use std::{cmp::min, io::Error};

#[derive(Default)]
pub struct CommandBar {
    prompt: String,
    value: Line,
    needs_redraw: bool,
    rect: Rect,
}

impl CommandBar {
    pub fn handle_edit_command(&mut self, command: Edit, clipboard: &mut dyn Clipboard) {
        match command {
            Edit::Insert(character) => self.value.append_char(character),

            Edit::Delete | Edit::InsertNewLine => {}

            Edit::DeleteBackward => self.value.delete_last(),

            // Prompts hold no buffer: copy/cut are no-ops, paste inserts the
            // first clipboard line.
            Edit::Copy | Edit::Cut => {}
            Edit::Paste => {
                if let Some(text) = clipboard.get() {
                    for ch in text.chars() {
                        if ch == '\n' || ch == '\r' {
                            break;
                        }
                        self.value.append_char(ch);
                    }
                }
            }
        }

        self.mark_redraw(true);
    }

    pub fn caret_position_col(&self) -> ColIdx {
        let max_width = self
            .prompt
            .len()
            .saturating_add(self.value.grapheme_count());

        min(max_width, self.rect.size.width)
    }

    pub fn caret_position(&self) -> Position {
        Position {
            row: self.rect.position.row,
            col: self
                .rect
                .position
                .col
                .saturating_add(self.caret_position_col()),
        }
    }

    pub fn value(&self) -> String {
        self.value.to_string()
    }

    pub fn set_prompt(&mut self, prompt: &str) {
        self.prompt = prompt.to_string();
        self.mark_redraw(true);
    }

    pub fn clear_value(&mut self) {
        self.value = Line::default();
        self.mark_redraw(true);
    }

    /// Prefill the prompt value (rename/metadata flows).
    pub fn set_value(&mut self, text: &str) {
        // Single-line prompts: stop at the first line break.
        let first = text.lines().next().unwrap_or("");
        self.value = Line::from(first);
        self.mark_redraw(true);
    }
}

impl UIComponent for CommandBar {
    fn mark_redraw(&mut self, value: bool) {
        self.needs_redraw = value;
    }

    fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }

    fn set_size(&mut self, rect: Rect) {
        self.rect = rect;
    }

    fn rect(&self) -> Rect {
        self.rect
    }

    fn draw(&mut self) -> Result<(), Error> {
        let width = self.rect.size.width;

        // this is how much space there is between
        // the right side of the prompt and the edge of the bar
        let area_for_value = width.saturating_sub(self.prompt.len());

        // we always want to show the left part of the value,
        // therefore the end of the visible range we try to access
        // will be equal to the full width
        let value_end = self.value.width();

        // This should give us the start for the grapheme
        // subrange we want to print out.
        let value_start = value_end.saturating_sub(area_for_value);

        let message = format!(
            "{}{}",
            self.prompt,
            self.value.get_visible_graphemes(value_start..value_end)
        );

        let to_print = if message.len() <= width {
            message
        } else {
            String::new()
        };

        Terminal::print_rect(self.rect, 0, &to_print)
    }
}
