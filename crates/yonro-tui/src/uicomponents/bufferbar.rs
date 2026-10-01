use super::UIComponent;
use yonro_core::buffers::BufferManager;
use crate::layout::PaneManager;
use crate::terminal::Terminal;
use crate::prelude::*;
use std::io::Error;

#[derive(Default)]
pub struct BufferBar {
    rect: Rect,
    needs_redraw: bool,
    pub tab_hitboxes: Vec<(usize, usize, usize)>, // (buffer_id, start_col, end_col)
    pub minimized_hitboxes: Vec<(usize, usize, usize)>, // (pane_id, start_col, end_col)
}

impl BufferBar {
    pub fn render(
        &mut self,
        buffer_manager: &BufferManager,
        pane_manager: &PaneManager,
    ) -> Result<(), Error> {
        let width = self.rect.size.width;
        let mut current_col = 0;

        self.tab_hitboxes.clear();
        self.minimized_hitboxes.clear();

        Terminal::clear_rect_line(self.rect, self.rect.position.row)?;

        // 1. Render Open Buffers
        let active_buffer_id = pane_manager
            .active_pane()
            .and_then(|p| p.view())
            .map(|v| v.buffer_id());

        for (id, buffer) in buffer_manager.iter() {
            let name = buffer
                .get_file_info()
                .get_path()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("untitled");
            let tab_text = format!(" [{}: {}] ", id, name);

            if current_col >= width as usize {
                break;
            }
            let remaining = (width as usize).saturating_sub(current_col);
            let display_text = if tab_text.len() > remaining {
                &tab_text[..remaining]
            } else {
                &tab_text
            };

            let is_active = Some(*id) == active_buffer_id;
            let formatted = if is_active {
                format!(
                    "{}{}{}",
                    crossterm::style::Attribute::Reverse,
                    display_text,
                    crossterm::style::Attribute::Reset,
                )
            } else {
                display_text.to_string()
            };

            let start_col = self.rect.position.col + current_col;
            let end_col = start_col + display_text.len();
            self.tab_hitboxes.push((*id, start_col, end_col));

            Terminal::print_at(
                Position {
                    row: self.rect.position.row,
                    col: start_col,
                },
                &formatted,
            )?;

            current_col += display_text.len();
        }

        // 2. Render Minimized Panes
        let minimized_panes: Vec<_> = pane_manager.iter().filter(|p| p.is_minimized).collect();
        if !minimized_panes.is_empty() {
            let min_label = String::from(" | MIN: ");
            if current_col + min_label.len() < width as usize {
                Terminal::print_at(
                    Position {
                        row: self.rect.position.row,
                        col: self.rect.position.col + current_col as usize,
                    },
                    &min_label,
                )?;
                current_col += min_label.len();

                for pane in minimized_panes {
                    let pane_str = format!("<P{}> ", pane.pane_id);
                    if current_col >= width as usize {
                        break;
                    }
                    let remaining = (width as usize).saturating_sub(current_col);
                    let display_text = if pane_str.len() > remaining {
                        &pane_str[..remaining]
                    } else {
                        &pane_str
                    };

                    let start_col = self.rect.position.col + current_col;
                    let end_col = start_col + display_text.len();
                    self.minimized_hitboxes.push((pane.pane_id, start_col, end_col));

                    Terminal::print_at(
                        Position {
                            row: self.rect.position.row,
                            col: start_col,
                        },
                        display_text,
                    )?;
                    current_col += display_text.len();
                }
            }
        }

        self.needs_redraw = false;
        Ok(())
    }
}

impl UIComponent for BufferBar {
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
        // This requires BufferManager and PaneManager, so we use the custom render method instead
        Ok(())
    }
}
