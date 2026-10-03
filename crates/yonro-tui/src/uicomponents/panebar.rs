use super::UIComponent;
use crate::layout::{DocTab, PaneManager};
use crate::prelude::*;
use crate::terminal::Terminal;
use std::io::Error;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;
use yonro_core::buffers::BufferManager;

#[derive(Default)]
pub struct PaneBar {
    rect: Rect,
    needs_redraw: bool,
    /// (tab index, tab's active pane, start_col, end_col)
    pub tab_hitboxes: Vec<(usize, usize, usize, usize)>,
    /// (tab index, tab's active pane, start_col, end_col)
    pub close_hitboxes: Vec<(usize, usize, usize, usize)>,
    pub minimized_hitboxes: Vec<(usize, usize, usize)>, // (pane_id, start_col, end_col)
}

impl PaneBar {
    /// Render document tabs (VS Code-style). The sidebar and floating panes
    /// are never tabs; minimized panes keep their own section below.
    pub fn render(
        &mut self,
        buffer_manager: &BufferManager,
        pane_manager: &PaneManager,
        tabs: &[DocTab],
        active_tab: usize,
    ) -> Result<(), Error> {
        let width = self.rect.size.width;
        let mut current_col = 0;

        self.tab_hitboxes.clear();
        self.close_hitboxes.clear();
        self.minimized_hitboxes.clear();

        Terminal::clear_rect_line(self.rect, self.rect.position.row)?;

        // 1. Render document tabs (one per open file, no splits).
        for (tab_idx, tab) in tabs.iter().enumerate() {
            let buffer_name = pane_manager
                .get_pane(tab.active_pane)
                .and_then(|p| p.view())
                .and_then(|v| buffer_manager.get(v.buffer_id()))
                .and_then(|b| {
                    b.get_file_info()
                        .get_path()
                        .and_then(|p| p.file_name())
                        .and_then(|n| n.to_str())
                })
                .unwrap_or("untitled");

            let tab_text = format!(" [{tab_idx}: {buffer_name}] ");

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

            let is_active = tab_idx == active_tab;
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
            self.tab_hitboxes
                .push((tab_idx, tab.active_pane, start_col, end_col));

            // Close button inline: "✕"
            let close_text = "✕";
            let close_start = end_col;
            let close_end = close_start + close_text.width();
            if close_end <= self.rect.position.col + width {
                self.close_hitboxes
                    .push((tab_idx, tab.active_pane, close_start, close_end));
            }

            let full_text = format!(
                "{}{}",
                formatted,
                if close_end <= self.rect.position.col + width {
                    close_text
                } else {
                    ""
                }
            );

            Terminal::print_at(
                Position {
                    row: self.rect.position.row,
                    col: start_col,
                },
                &full_text,
            )?;

            current_col += full_text.width();
        }

        // 2. Render Minimized Panes (sidebar is never minimized, no filter needed).
        let minimized_panes: Vec<_> = pane_manager
            .iter()
            .filter(|p| p.is_minimized && !p.is_floating)
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
                    self.minimized_hitboxes
                        .push((pane.pane_id, start_col, end_col));

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
