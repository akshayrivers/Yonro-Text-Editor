// Lore sheet viewer (`PLAN.md Phase 4.5`): read-only floating pane showing
// an entity's sheet. Content is built once at open (static); `Up`/`Down`
// scroll via the synchronous `MoveHandler` path, `Esc` is closed by `Editor`
// (tracked `sheet_pane`), `[x]` closes through the normal click path.

use crate::prelude::*;
use crate::terminal::Terminal;
use crate::uicomponents::{PluginComponent, UIComponent};
use std::io::Error;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;
use yonro_core::buffers::Buffer;
use yonro_core::command::Move;

pub struct LoreSheet {
    rect: Rect,
    needs_redraw: bool,
    active: bool,
    title: String,
    lines: Vec<String>,
    scroll_top: usize,
}

impl LoreSheet {
    #[must_use]
    pub fn new(title: String, lines: Vec<String>) -> Self {
        Self {
            rect: Rect::default(),
            needs_redraw: true,
            active: false,
            title,
            lines,
            scroll_top: 0,
        }
    }

    fn draw_sheet(&mut self) {
        let Rect {
            position: Position { row, col },
            size: Size { height, width },
        } = self.rect;
        if width < 4 || height < 3 {
            return;
        }
        let _ = Terminal::draw_border(self.rect);
        let title = truncate_to_width(&format!("─ 📖 {} ", self.title), width.saturating_sub(8));
        let _ = Terminal::print_at(
            Position {
                row,
                col: col.saturating_add(1),
            },
            &title,
        );
        if width >= 10 {
            let _ = Terminal::print_at(
                Position {
                    row,
                    col: col.saturating_add(width).saturating_sub(4),
                },
                "[x]",
            );
        }
        let content_col = col.saturating_add(1);
        let content_width = width.saturating_sub(2);
        let capacity = height.saturating_sub(2);
        for screen_row in 0..capacity {
            let abs_row = row.saturating_add(1).saturating_add(screen_row);
            let line = match self.lines.get(self.scroll_top.saturating_add(screen_row)) {
                Some(text) => truncate_to_width(text, content_width),
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

impl UIComponent for LoreSheet {
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
        self.draw_sheet();
        Ok(())
    }
}

impl PluginComponent for LoreSheet {
    fn handle_move(&mut self, direction: Move) {
        let prev = self.scroll_top;
        match direction {
            Move::Up => {
                self.scroll_top = self.scroll_top.saturating_sub(1);
            }
            Move::Down => {
                let max = self.lines.len().saturating_sub(1);
                if self.scroll_top < max {
                    self.scroll_top = self.scroll_top.saturating_add(1);
                }
            }
            _ => {}
        }
        if prev != self.scroll_top {
            self.needs_redraw = true;
        }
    }

    fn handle_select(&mut self) -> Option<std::path::PathBuf> {
        None
    }

    fn handle_click(&mut self, position: Position) -> crate::uicomponents::ClickAction {
        // Reuse pane-level hit-testing via our own rect (close button only;
        // body clicks just focus, handled by the mouse layer).
        let close_col = self
            .rect
            .position
            .col
            .saturating_add(self.rect.size.width)
            .saturating_sub(4);
        if position.row == self.rect.position.row
            && position.col >= close_col
            && position.col < close_col.saturating_add(3)
        {
            crate::uicomponents::ClickAction::Close
        } else {
            crate::uicomponents::ClickAction::None
        }
    }

    fn set_active(&mut self, active: bool) {
        if self.active != active {
            self.active = active;
            self.needs_redraw = true;
        }
    }

    fn render_content(&mut self, _rect: Rect) -> Result<(), Error> {
        self.draw_sheet();
        Ok(())
    }

    fn update_from_buffer(&mut self, _buffer: &Buffer) {}
}
