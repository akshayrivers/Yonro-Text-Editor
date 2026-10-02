use super::UIComponent;
use yonro_core::buffers::BufferManager;
use crate::layout::PaneManager;
use crate::terminal::Terminal;
use crate::prelude::*;
use std::io::Error;
use unicode_width::UnicodeWidthStr;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Default)]
pub struct PaneBar {
    rect: Rect,
    needs_redraw: bool,
    pub tab_hitboxes: Vec<(usize, usize, usize)>, // (pane_id, start_col, end_col)
    pub close_hitboxes: Vec<(usize, usize, usize)>, // (pane_id, start_col, end_col)
    pub minimized_hitboxes: Vec<(usize, usize, usize)>, // (pane_id, start_col, end_col)
}

impl PaneBar {
    pub fn render(
        &mut self,
        buffer_manager: &BufferManager,
        pane_manager: &PaneManager,
        sidebar_pane_id: Option<usize>,
    ) -> Result<(), Error> {
        let width = self.rect.size.width;
        let mut current_col = 0;

        self.tab_hitboxes.clear();
        self.close_hitboxes.clear();
        self.minimized_hitboxes.clear();

        Terminal::clear_rect_line(self.rect, self.rect.position.row)?;

        // 1. Render Pane Tabs.
        // The sidebar pane lives outside `LayoutTree` (right strip) — it is
        // not a document tab, so never list it here (open or hidden).
        // Otherwise opening the explorer looks like "a new pane" appeared.
        let active_pane_id = pane_manager.active_pane().map(|p| p.pane_id);

        for pane in pane_manager
            .iter()
            .filter(|p| !p.is_floating && !p.is_minimized)
            .filter(|p| Some(p.pane_id) != sidebar_pane_id)
        {
            let buffer_name = pane
                .view()
                .and_then(|v| buffer_manager.get(v.buffer_id()))
                .and_then(|b| {
                    b.get_file_info()
                        .get_path()
                        .and_then(|p| p.file_name())
                        .and_then(|n| n.to_str())
                })
                .unwrap_or("untitled");

            let tab_text = format!(" [{}: {}] ", pane.pane_id, buffer_name);

            if current_col >= width as usize {
                break;
            }
            let remaining = (width as usize).saturating_sub(current_col);
            // Use grapheme width for display truncation
            let display_text = if tab_text.width() > remaining {
                // Truncate by grapheme width
                let mut w = 0;
                let mut end = 0;
                for (i, g) in tab_text.graphemes(true).enumerate() {
                    let gw = unicode_width::UnicodeWidthStr::width(g);
                    if w + gw > remaining {
                        break;
                    }
                    w += gw;
                    end = i + g.len();
                }
                &tab_text[..end]
            } else {
                &tab_text
            };

            let is_active = Some(pane.pane_id) == active_pane_id;
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
            let end_col = start_col + display_text.width();
            self.tab_hitboxes.push((pane.pane_id, start_col, end_col));

            // Close button inline: "✕" 
            let close_text = "✕";
            let close_start = end_col;
            let close_end = close_start + close_text.width();
            if close_end <= self.rect.position.col + width {
                self.close_hitboxes.push((pane.pane_id, close_start, close_end));
            }

            let full_text = format!("{}{}", formatted, if close_end <= self.rect.position.col + width { close_text } else { "" });

            Terminal::print_at(
                Position {
                    row: self.rect.position.row,
                    col: start_col,
                },
                &full_text,
            )?;

            current_col += full_text.width();
        }

        // 2. Render Minimized Panes (sidebar excluded — same reason as tabs).
        let minimized_panes: Vec<_> = pane_manager
            .iter()
            .filter(|p| p.is_minimized && !p.is_floating)
            .filter(|p| Some(p.pane_id) != sidebar_pane_id)
            .collect();
        if !minimized_panes.is_empty() {
            let min_label = String::from(" | MIN: ");
            if current_col + min_label.width() < width as usize {
                Terminal::print_at(
                    Position {
                        row: self.rect.position.row,
                        col: self.rect.position.col + current_col,
                    },
                    &min_label,
                )?;
                current_col += min_label.width();

                for pane in minimized_panes {
                    let pane_str = format!("<P{}> ", pane.pane_id);
                    if current_col >= width as usize {
                        break;
                    }
                    let remaining = (width as usize).saturating_sub(current_col);
                    let display_text = if pane_str.width() > remaining {
                        let mut w = 0;
                        let mut end = 0;
                        for (i, g) in pane_str.graphemes(true).enumerate() {
                            let gw = unicode_width::UnicodeWidthStr::width(g);
                            if w + gw > remaining {
                                break;
                            }
                            w += gw;
                            end = i + g.len();
                        }
                        &pane_str[..end]
                    } else {
                        &pane_str
                    };

                    let start_col = self.rect.position.col + current_col;
                    let end_col = start_col + display_text.width();
                    self.minimized_hitboxes.push((pane.pane_id, start_col, end_col));

                    Terminal::print_at(
                        Position {
                            row: self.rect.position.row,
                            col: start_col,
                        },
                        display_text,
                    )?;
                    current_col += display_text.width();
                }
            }
        }

        self.needs_redraw = false;
        Ok(())
    }
}

impl UIComponent for PaneBar {
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
        Ok(())
    }
}