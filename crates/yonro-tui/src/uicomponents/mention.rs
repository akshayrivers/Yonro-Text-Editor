// `@mention` autocomplete popup (`PLAN.md Phase 4.5`): a small floating
// pane under the caret listing lore candidates for the `@query` being typed.
// Owned by `Editor` (`MentionState`); this component is a thin renderer fed
// via `sync_mention` every frame the popup is open.

use crate::prelude::*;
use crate::terminal::Terminal;
use crate::uicomponents::{PluginComponent, UIComponent};
use std::io::Error;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;
use yonro_core::buffers::Buffer;
use yonro_core::command::Move;

pub struct MentionComplete {
    rect: Rect,
    needs_redraw: bool,
    active: bool,
    items: Vec<(String, String)>,
    selected: usize,
}

impl Default for MentionComplete {
    fn default() -> Self {
        Self {
            rect: Rect::default(),
            needs_redraw: true,
            active: false,
            items: Vec::new(),
            selected: 0,
        }
    }
}

impl MentionComplete {
    fn draw_list(&mut self) {
        let Rect {
            position: Position { row, col },
            size: Size { height, width },
        } = self.rect;
        if width < 4 || height < 3 {
            return;
        }
        let _ = Terminal::draw_border(self.rect);
        let _ = Terminal::print_at(
            Position {
                row,
                col: col.saturating_add(1),
            },
            "─ @mention ",
        );
        let content_col = col.saturating_add(1);
        let content_width = width.saturating_sub(2);
        let capacity = height.saturating_sub(2);
        for screen_row in 0..capacity {
            let abs_row = row.saturating_add(1).saturating_add(screen_row);
            let line = match self.items.get(screen_row) {
                Some((name, kind)) => {
                    let prefix = if screen_row == self.selected && self.active {
                        "▶ "
                    } else {
                        "  "
                    };
                    truncate_to_width(&format!("{prefix}{name} <{kind}>"), content_width)
                }
                None => " ".repeat(content_width),
            };
            let _ = Terminal::print_at(
                Position {
                    row: abs_row,
                    col: content_col,
                },
                &line,
            );
        }
        self.needs_redraw = false;
    }
}

fn truncate_to_width(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(text) <= width {
        let mut out = text.to_string();
        while UnicodeWidthStr::width(out.as_str()) < width {
            out.push(' ');
        }
        return out;
    }
    let mut out = String::new();
    let mut used: usize = 0;
    for grapheme in text.graphemes(true) {
        let w = UnicodeWidthStr::width(grapheme);
        if used.saturating_add(w).saturating_add(1) > width {
            break;
        }
        out.push_str(grapheme);
        used = used.saturating_add(w);
    }
    out.push('…');
    out
}

impl UIComponent for MentionComplete {
    fn mark_redraw(&mut self, value: bool) {
        self.needs_redraw = value;
    }

    fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }

    fn set_size(&mut self, rect: Rect) {
        if self.rect != rect {
            self.rect = rect;
            self.needs_redraw = true;
        }
    }

    fn rect(&self) -> Rect {
        self.rect
    }

    fn draw(&mut self) -> Result<(), Error> {
        self.draw_list();
        Ok(())
    }
}

impl PluginComponent for MentionComplete {
    fn handle_move(&mut self, _direction: Move) {}

    fn handle_select(&mut self) -> Option<std::path::PathBuf> {
        None
    }

    fn handle_click(&mut self, _position: Position) -> crate::uicomponents::ClickAction {
        crate::uicomponents::ClickAction::None
    }

    fn set_active(&mut self, active: bool) {
        if self.active != active {
            self.active = active;
            self.needs_redraw = true;
        }
    }

    fn render_content(&mut self, _rect: Rect) -> Result<(), Error> {
        // Floating-only component: full `draw` owns border + list.
        self.draw_list();
        Ok(())
    }

    fn sync_mention(&mut self, items: &[(String, String)], selected: usize) {
        let changed = self.items != items || self.selected != selected;
        self.items = items.to_vec();
        self.selected = selected.min(self.items.len().saturating_sub(1));
        if changed {
            self.needs_redraw = true;
        }
    }

    fn update_from_buffer(&mut self, _buffer: &Buffer) {}
}
