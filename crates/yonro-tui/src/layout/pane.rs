/*
So what should a pane contain?
If I think from the perspective of PHASE I the view for now is just one bigass Pane and it supports and need
    1. Buffer
    2. viewport and scrolling
    3. cursor tracking

Ultimately in this pane we will also track the 4. paneID 5. Active status and buffer will be just a reference or will have only buffer ID
hmmmm so a pane should just not support the buffer view and its functionality that is already being handled by the View, so pane should only
be concerned with what is inside and what size and geometry.
Pane should know:
    - what it displays
    - whether it is focused
*/
use crate::{
    prelude::*,
    terminal::Terminal,
    uicomponents::{ClickAction, PluginComponent, UIComponent, View},
};
use yonro_core::{buffers::BufferManager, command::Move};

pub enum PaneContent {
    /// A text editing view backed by a Buffer.
    TextView(View),
    /// Any plugin-provided UI component (FileExplorer, CharacterMap, etc.)
    Plugin(Box<dyn PluginComponent + Send>),
    /// A temporary popup overlay.
    Popup(Box<dyn PluginComponent + Send>),
}

pub struct Pane {
    pub pane_id: usize,
    pub content: PaneContent,
    pub active: bool,
    pub is_floating: bool,
    pub z_index: usize,
    pub is_minimized: bool,
    pub rect: Rect,
}

impl Pane {
    pub fn view(&self) -> Option<&View> {
        if let PaneContent::TextView(v) = &self.content {
            Some(v)
        } else {
            None
        }
    }

    pub fn view_mut(&mut self) -> Option<&mut View> {
        if let PaneContent::TextView(v) = &mut self.content {
            Some(v)
        } else {
            None
        }
    }

    pub fn component(&self) -> &dyn UIComponent {
        match &self.content {
            PaneContent::TextView(v) => v,
            PaneContent::Plugin(c) => c.as_ref(),
            PaneContent::Popup(p) => p.as_ref(),
        }
    }

    pub fn component_mut(&mut self) -> &mut dyn UIComponent {
        match &mut self.content {
            PaneContent::TextView(v) => v,
            PaneContent::Plugin(c) => c.as_mut(),
            PaneContent::Popup(p) => p.as_mut(),
        }
    }

    /// Propagate active state to the underlying view or plugin component.
    pub fn set_content_active(&mut self, active: bool) {
        match &mut self.content {
            PaneContent::TextView(view) => {
                view.set_active(active);
            }
            PaneContent::Plugin(component) => {
                component.set_active(active);
            }
            PaneContent::Popup(popup) => {
                popup.set_active(active);
            }
        }
    }
    // Plugin-specific input forwarding

    /// Forward an arrow key to a Plugin/Popup pane. No-op for TextView.
    pub fn plugin_handle_move(&mut self, direction: Move) {
        match &mut self.content {
            PaneContent::Plugin(c) | PaneContent::Popup(c) => c.handle_move(direction),
            PaneContent::TextView(_) => {}
        }
    }

    /// Forward Enter/select to a Plugin/Popup pane. No-op for TextView.
    pub fn plugin_handle_select(&mut self) -> Option<std::path::PathBuf> {
        match &mut self.content {
            PaneContent::Plugin(c) | PaneContent::Popup(c) => c.handle_select(),
            PaneContent::TextView(_) => None,
        }
    }

    /// Forward a mouse click to a Plugin/Popup pane.
    /// Returns ClickAction so the caller knows what to do next.
    pub fn plugin_handle_click(&mut self, position: Position) -> ClickAction {
        match &mut self.content {
            PaneContent::Plugin(c) | PaneContent::Popup(c) => c.handle_click(position),
            PaneContent::TextView(_) => ClickAction::None,
        }
    }

    pub fn close_button_col(&self) -> usize {
        self.rect.position.col + self.rect.size.width.saturating_sub(4)
    }

    pub fn min_button_col(&self) -> usize {
        self.rect.position.col + self.rect.size.width.saturating_sub(7)
    }

    pub fn is_on_close_button(&self, pos: Position) -> bool {
        pos.row == self.rect.position.row
            && pos.col >= self.close_button_col()
            && pos.col < self.close_button_col() + 3
    }

    pub fn is_on_min_button(&self, pos: Position) -> bool {
        pos.row == self.rect.position.row
            && pos.col >= self.min_button_col()
            && pos.col < self.min_button_col() + 3
    }

    pub fn is_on_title_bar(&self, pos: Position) -> bool {
        pos.row == self.rect.position.row
            && pos.col >= self.rect.position.col
            && pos.col < self.rect.position.col + self.rect.size.width
    }

    pub fn resize(&mut self, rect: Rect) {
        self.rect = rect;
        self.component_mut().set_size(rect);
    }

    pub fn render(&mut self, buffer_manager: &BufferManager) {
        let rect = self.rect;
        let min_button_col = self.min_button_col();

        if self.is_minimized {
            let title = match &self.content {
                PaneContent::TextView(_) => {
                    if self.active {
                        format!("─ [{}]* ", self.pane_id)
                    } else {
                        format!("─ [{}]  ", self.pane_id)
                    }
                }
                PaneContent::Plugin(_) | PaneContent::Popup(_) => {
                    if self.active {
                        format!("─ [Plugin {}]* ", self.pane_id)
                    } else {
                        format!("─ [Plugin {}]  ", self.pane_id)
                    }
                }
            };

            let width = rect.size.width;
            if width >= 2 {
                let button_str = "[-][x]";
                let button_len = button_str.len();
                let title_part = format!("┌{}", title);
                let fill_len = width
                    .saturating_sub(title_part.len())
                    .saturating_sub(button_len)
                    .saturating_sub(1); // corner ┐
                let fill = "─".repeat(fill_len);
                let top_line = format!("{}{}{}┐", title_part, fill, button_str);
                let _ = Terminal::print_at(rect.position, &top_line);
            }

            // Clear rows below the title bar
            for r in rect.position.row.saturating_add(1)
                ..rect.position.row.saturating_add(rect.size.height)
            {
                let _ = Terminal::print_at(
                    Position {
                        row: r,
                        col: rect.position.col,
                    },
                    &" ".repeat(width),
                );
            }
            return;
        }

        match &mut self.content {
            PaneContent::TextView(view) => {
                if !view.needs_redraw() {
                    return;
                }

                let buffer_id = view.buffer_id();
                let buffer = match buffer_manager.get(buffer_id) {
                    Some(b) => b,
                    None => return,
                };

                // Draw the border around the full pane rect
                let _ = Terminal::draw_border(rect);

                // Draw title bar (pane id + active indicator)
                let title = if self.active {
                    format!("─ [{}]* ", self.pane_id)
                } else {
                    format!("─ [{}]  ", self.pane_id)
                };
                let _ = Terminal::print_at(
                    Position {
                        row: rect.position.row,
                        col: rect.position.col.saturating_add(1),
                    },
                    &title,
                );

                // Render minimize and close buttons on the top border
                if rect.size.width >= 10 {
                    let _ = Terminal::print_at(
                        Position {
                            row: rect.position.row,
                            col: min_button_col,
                        },
                        "[-][x]",
                    );
                }

                // Content rect is inset by 1 on all sides (inside the border)
                let content_rect = Rect {
                    position: Position {
                        row: rect.position.row.saturating_add(1),
                        col: rect.position.col.saturating_add(1),
                    },
                    size: Size {
                        height: rect.size.height.saturating_sub(2),
                        width: rect.size.width.saturating_sub(2),
                    },
                };

                // Render text content into the inset rect
                if let Err(_e) = view.draw_content_with_buffer(content_rect, buffer) {
                    #[cfg(debug_assertions)]
                    eprintln!("View render error: {_e:?}");
                } else {
                    view.mark_redraw(false);
                }
            }

            PaneContent::Plugin(component) => {
                if !component.needs_redraw() {
                    return;
                }
                // Calculate button positions for title bar
                let min_button_col = self
                    .rect
                    .position
                    .col
                    .saturating_add(self.rect.size.width)
                    .saturating_sub(7);
                if self.is_floating {
                    // Floating panes: plugin draws its own border/title
                    component.render();
                } else {
                    // Tiled panes: Pane draws border, plugin draws content only
                    let _ = Terminal::draw_border(rect);

                    // Draw title bar (pane id + active indicator)
                    let title = if self.active {
                        format!("─ [{}]* ", self.pane_id)
                    } else {
                        format!("─ [{}]  ", self.pane_id)
                    };
                    let _ = Terminal::print_at(
                        Position {
                            row: rect.position.row,
                            col: rect.position.col.saturating_add(1),
                        },
                        &title,
                    );

                    // Render minimize and close buttons on the top border
                    if rect.size.width >= 10 {
                        let _ = Terminal::print_at(
                            Position {
                                row: rect.position.row,
                                col: min_button_col,
                            },
                            "[-][x]",
                        );
                    }

                    // Content rect is inset by 1 on all sides (inside the border)
                    let content_rect = Rect {
                        position: Position {
                            row: rect.position.row.saturating_add(1),
                            col: rect.position.col.saturating_add(1),
                        },
                        size: Size {
                            height: rect.size.height.saturating_sub(2),
                            width: rect.size.width.saturating_sub(2),
                        },
                    };

                    if let Err(_e) = component.render_content(content_rect) {
                        #[cfg(debug_assertions)]
                        eprintln!("Plugin content render error: {_e:?}");
                    } else {
                        component.mark_redraw(false);
                    }
                }
            }

            PaneContent::Popup(popup) => {
                if !popup.needs_redraw() {
                    return;
                }
                popup.render();
            }
        }
    }
}
