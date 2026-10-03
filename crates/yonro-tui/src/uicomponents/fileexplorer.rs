// src/editor/uicomponents/fileexplorer.rs
use crate::prelude::*;
use crate::terminal::Terminal;
use crate::uicomponents::{ClickAction, PluginComponent, UIComponent};
use std::fs;
use std::io::Error;
use std::path::PathBuf;
use yonro_core::buffers::Buffer;
use yonro_core::command::Move;

// Action
// Returned by perform_selection() so the plugin can act on it.

pub enum FileExplorerAction {
    DirectoryChanged,
    OpenFile(PathBuf),
    None,
}

// FileExplorer

pub struct FileExplorer {
    current_dir: PathBuf,
    entries: Vec<FileEntry>,
    selected_idx: usize,
    scroll_top: usize,
    rect: Rect,
    needs_redraw: bool,
    pub active: bool,
    // For double-click detection
    last_click_time: Option<std::time::Instant>,
    last_click_idx: Option<usize>,
}

struct FileEntry {
    name: String,
    is_dir: bool,
}

impl Default for FileExplorer {
    fn default() -> Self {
        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut explorer = Self {
            current_dir,
            entries: Vec::new(),
            selected_idx: 0,
            scroll_top: 0,
            rect: Rect::default(),
            needs_redraw: true,
            active: false,
            last_click_time: None,
            last_click_idx: None,
        };
        explorer.refresh_entries();
        explorer
    }
}

impl FileExplorer {
    // Public API

    pub fn current_dir(&self) -> &PathBuf {
        &self.current_dir
    }

    pub fn refresh_entries(&mut self) {
        self.entries.clear();
        // Parent dir entry
        self.entries.push(FileEntry {
            name: "..".to_string(),
            is_dir: true,
        });

        if let Ok(read_dir) = fs::read_dir(&self.current_dir) {
            let mut dirs: Vec<FileEntry> = Vec::new();
            let mut files: Vec<FileEntry> = Vec::new();

            for entry in read_dir.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    let is_dir = entry.path().is_dir();
                    let fe = FileEntry {
                        name: name.to_string(),
                        is_dir,
                    };
                    if is_dir {
                        dirs.push(fe);
                    } else {
                        files.push(fe);
                    }
                }
            }

            // Dirs first, then files — both alphabetical
            dirs.sort_by(|a, b| a.name.cmp(&b.name));
            files.sort_by(|a, b| a.name.cmp(&b.name));
            self.entries.extend(dirs);
            self.entries.extend(files);
        }

        self.selected_idx = self.selected_idx.min(self.entries.len().saturating_sub(1));
        self.adjust_scroll();
        self.needs_redraw = true;
    }

    pub fn perform_selection(&mut self) -> FileExplorerAction {
        if self.entries.is_empty() {
            return FileExplorerAction::None;
        }
        if let Some(entry) = self.entries.get(self.selected_idx) {
            let target_path = if entry.name == ".." {
                self.current_dir.parent().map(|p| p.to_path_buf())
            } else {
                Some(self.current_dir.join(&entry.name))
            };

            if let Some(path) = target_path {
                if path.is_dir() {
                    self.current_dir = path;
                    self.selected_idx = 0;
                    self.refresh_entries();
                    return FileExplorerAction::DirectoryChanged;
                } else if path.is_file() {
                    return FileExplorerAction::OpenFile(path);
                }
            }
        }
        FileExplorerAction::None
    }

    pub fn handle_move_command(&mut self, command: Move) {
        let prev = self.selected_idx;
        match command {
            Move::Up => {
                self.selected_idx = self.selected_idx.saturating_sub(1);
            }
            Move::Down => {
                if self.selected_idx.saturating_add(1) < self.entries.len() {
                    self.selected_idx = self.selected_idx.saturating_add(1);
                }
            }
            _ => {}
        }
        self.adjust_scroll();
        if prev != self.selected_idx {
            self.needs_redraw = true;
        }
    }

    fn adjust_scroll(&mut self) {
        // Content area height = rect height - 3 (border top + title + border bottom)
        let content_height = self.rect.size.height.saturating_sub(3);
        if content_height == 0 || self.entries.is_empty() {
            return;
        }
        if self.selected_idx < self.scroll_top {
            self.scroll_top = self.selected_idx;
        } else if self.selected_idx >= self.scroll_top.saturating_add(content_height) {
            self.scroll_top = self
                .selected_idx
                .saturating_sub(content_height)
                .saturating_add(1);
        }
        let max_scroll = self.entries.len().saturating_sub(1);
        self.scroll_top = self.scroll_top.min(max_scroll);
    }

    fn draw_frame(&self) -> Result<(), Error> {
        let rect = self.rect;
        let Rect {
            position: Position { row, col },
            size: Size { height, width },
        } = rect;

        if width < 4 || height < 3 {
            return Ok(());
        }

        // Top border with title
        // Format: ┌─ 📁 /path/to/dir ─── [-][x]┐
        let dir_str = self.current_dir.to_str().unwrap_or("?").to_string();

        // buttons occupy 6 chars: " [-][x]"
        let button_str = " [-][x]";
        let button_len = button_str.len();

        // title: " dir_str "
        let max_title_len = width
            .saturating_sub(4) // ┌─ ... ─┐  (2 corners + 2 dashes)
            .saturating_sub(button_len);
        let truncated_dir = if dir_str.len() > max_title_len {
            format!(
                "…{}",
                &dir_str[dir_str.len().saturating_sub(max_title_len)..]
            )
        } else {
            dir_str.clone()
        };

        let title_part = format!(" {} ", truncated_dir);
        let fill_len = width
            .saturating_sub(2) // corners
            .saturating_sub(title_part.len())
            .saturating_sub(button_len);
        let fill = "─".repeat(fill_len);
        let top_line = format!("┌{}{}{}┐", title_part, fill, button_str);

        Terminal::print_at(Position { row, col }, &top_line)?;

        // Side borders
        for r in row.saturating_add(1)..row.saturating_add(height).saturating_sub(1) {
            Terminal::print_at(Position { row: r, col }, "│")?;
            Terminal::print_at(
                Position {
                    row: r,
                    col: col.saturating_add(width).saturating_sub(1),
                },
                "│",
            )?;
        }

        // Bottom border
        let bottom_line = format!("└{}┘", "─".repeat(width.saturating_sub(2)));
        Terminal::print_at(
            Position {
                row: row.saturating_add(height).saturating_sub(1),
                col,
            },
            &bottom_line,
        )?;

        Ok(())
    }

    fn draw_entries(&self) -> Result<(), Error> {
        let rect = self.rect;
        let Rect {
            position: Position { row, col },
            size: Size { height, width },
        } = rect;

        // Content area: inset by 1 on all sides
        let content_row_start = row.saturating_add(1);
        let content_col = col.saturating_add(1);
        let content_width = width.saturating_sub(2);
        let content_height = height.saturating_sub(2);

        for screen_row in 0..content_height {
            let entry_idx = self.scroll_top.saturating_add(screen_row);
            let abs_row = content_row_start.saturating_add(screen_row);

            let line = if let Some(entry) = self.entries.get(entry_idx) {
                let prefix = if entry_idx == self.selected_idx && self.active {
                    "▶ "
                } else {
                    "  "
                };
                let icon = if entry.is_dir { "📁 " } else { "   " };
                let full = format!("{}{}{}", prefix, icon, entry.name);

                // Truncate to content width
                if full.len() > content_width {
                    format!("{}…", &full[..content_width.saturating_sub(1)])
                } else {
                    // Pad to content width to clear stale chars
                    format!("{:<width$}", full, width = content_width)
                }
            } else {
                // Clear empty rows
                " ".repeat(content_width)
            };

            Terminal::print_at(
                Position {
                    row: abs_row,
                    col: content_col,
                },
                &line,
            )?;
        }

        Ok(())
    }

    /// Returns the column of the close [x] button on the title bar row.
    pub fn close_button_col(&self) -> usize {
        // " [-][x]" — [x] starts at width-4, the 'x' is at width-3
        // ┌─ title ─── [-][x]┐
        //                    ^ col + width - 1  (the ┐)
        //                   ^ col + width - 2  (the ])  ← close button end
        //                  ^ col + width - 3  (the x)  ← close button char
        //                 ^ col + width - 4  (the [)   ← close button start
        self.rect
            .position
            .col
            .saturating_add(self.rect.size.width)
            .saturating_sub(4)
    }

    /// Returns the column of the minimize [-] button on the title bar row.
    pub fn min_button_col(&self) -> usize {
        // "[-]" sits just before "[x]": col + width - 7 to col + width - 5
        self.rect
            .position
            .col
            .saturating_add(self.rect.size.width)
            .saturating_sub(7)
    }

    pub fn title_bar_row(&self) -> usize {
        self.rect.position.row
    }

    pub fn is_on_close_button(&self, pos: Position) -> bool {
        pos.row == self.title_bar_row()
            && pos.col >= self.close_button_col()
            && pos.col < self.close_button_col().saturating_add(3)
    }

    pub fn is_on_min_button(&self, pos: Position) -> bool {
        pos.row == self.title_bar_row()
            && pos.col >= self.min_button_col()
            && pos.col < self.min_button_col().saturating_add(3)
    }

    pub fn is_on_title_bar(&self, pos: Position) -> bool {
        pos.row == self.title_bar_row()
            && pos.col >= self.rect.position.col
            && pos.col < self.rect.position.col.saturating_add(self.rect.size.width)
    }
}

// UIComponent

impl UIComponent for FileExplorer {
    fn mark_redraw(&mut self, value: bool) {
        self.needs_redraw = value;
    }

    fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }

    fn set_size(&mut self, rect: Rect) {
        if self.rect != rect {
            self.rect = rect;
            self.adjust_scroll();
            self.needs_redraw = true;
        }
    }

    fn rect(&self) -> Rect {
        self.rect
    }

    fn draw(&mut self) -> Result<(), Error> {
        if self.rect.size.height < 3 || self.rect.size.width < 4 {
            return Ok(());
        }
        self.draw_frame()?;
        self.draw_entries()?;
        self.needs_redraw = false;
        Ok(())
    }
}

impl PluginComponent for FileExplorer {
    fn handle_move(&mut self, direction: Move) {
        self.handle_move_command(direction);
    }

    fn handle_select(&mut self) -> Option<PathBuf> {
        match self.perform_selection() {
            FileExplorerAction::OpenFile(path) => Some(path),
            _ => None,
        }
    }

    fn handle_click(&mut self, position: Position) -> ClickAction {
        if self.is_on_close_button(position) {
            ClickAction::Close
        } else if self.is_on_min_button(position) {
            ClickAction::Minimize
        } else {
            // Check if inside content area
            let content_row_start = self.rect.position.row.saturating_add(1);
            let content_height = self.rect.size.height.saturating_sub(2);
            if position.row >= content_row_start
                && position.row < content_row_start.saturating_add(content_height)
            {
                let click_idx = self
                    .scroll_top
                    .saturating_add(position.row.saturating_sub(content_row_start));
                if click_idx < self.entries.len() {
                    let prev = self.selected_idx;
                    self.selected_idx = click_idx;
                    if prev != self.selected_idx {
                        self.needs_redraw = true;
                    }

                    // Check for double-click on the same item
                    let now = std::time::Instant::now();
                    let is_double_click = self.last_click_idx == Some(click_idx)
                        && self
                            .last_click_time
                            .map_or(false, |t| t.elapsed().as_millis() < 300);

                    self.last_click_time = Some(now);
                    self.last_click_idx = Some(click_idx);

                    if is_double_click && !self.entries[click_idx].is_dir {
                        return ClickAction::DoubleClick;
                    }
                }
            }
            ClickAction::None
        }
    }

    fn set_active(&mut self, active: bool) {
        if self.active != active {
            self.active = active;
            self.needs_redraw = true;
        }
    }

    fn update_from_buffer(&mut self, _buffer: &Buffer) {
        // FileExplorer doesn't need buffer updates
    }

    fn render_content(&mut self, rect: Rect) -> Result<(), Error> {
        // Update our internal rect to match the content rect for hit-testing
        self.rect = rect;
        self.adjust_scroll();
        self.draw_entries()?;
        self.needs_redraw = false;
        Ok(())
    }
}
