use crate::prelude::*;
use crate::terminal::Terminal;
use crate::uicomponents::{PluginComponent, UIComponent};
use std::io::Error;
use yonro_core::buffers::Buffer;

pub struct WordCount {
    rect: Rect,
    needs_redraw: bool,
    pub active: bool,
    words: usize,
    chars: usize,
    lines: usize,
    reading_time_minutes: usize,
}

impl Default for WordCount {
    fn default() -> Self {
        Self {
            rect: Rect::default(),
            needs_redraw: true,
            active: false,
            words: 0,
            chars: 0,
            lines: 0,
            reading_time_minutes: 0,
        }
    }
}

impl WordCount {
    /// Direct (inherent) updater — kept so concrete callers and tests
    /// (`wc.update_from_buffer(&buffer)`) keep working without importing
    /// the trait. The `PluginComponent` override below mirrors this logic
    /// so `Box<dyn PluginComponent>` dynamic dispatch also updates.
    pub fn update_from_buffer(&mut self, buffer: &Buffer) {
        let (words, graphemes, lines) = buffer.word_count_stats();
        self.words = words;
        self.chars = graphemes;
        self.lines = lines;
        self.reading_time_minutes = if words == 0 {
            0
        } else {
            (words as f64 / 200.0).ceil() as usize
        };
        self.needs_redraw = true;
    }

    pub fn words(&self) -> usize {
        self.words
    }

    pub fn chars(&self) -> usize {
        self.chars
    }

    pub fn lines(&self) -> usize {
        self.lines
    }

    pub fn reading_time_minutes(&self) -> usize {
        self.reading_time_minutes
    }
}

impl UIComponent for WordCount {
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
        if self.rect.size.height < 3 || self.rect.size.width < 20 {
            return Ok(());
        }

        let Rect {
            position: Position { row, col },
            size: Size { height, width },
        } = self.rect;

        // Draw border
        let _ = Terminal::draw_border(self.rect);

        // Title bar
        let title = if self.active {
            "─ [Word Count]* "
        } else {
            "─ [Word Count]  "
        };
        let _ = Terminal::print_at(
            Position {
                row,
                col: col.saturating_add(1),
            },
            title,
        );

        // Close button
        if width >= 10 {
            let _ = Terminal::print_at(
                Position {
                    row,
                    col: col.saturating_add(width).saturating_sub(4),
                },
                "[x]",
            );
        }

        // Content area
        let content_row = row.saturating_add(1);
        let content_col = col.saturating_add(1);
        let content_width = width.saturating_sub(2);

        let lines = [
            format!("Words:     {}", self.words),
            format!("Characters: {}", self.chars),
            format!("Lines:     {}", self.lines),
            format!("Reading:   ~{} min", self.reading_time_minutes),
        ];

        for (i, line) in lines.iter().enumerate() {
            let line_row = content_row.saturating_add(i);
            if line_row < row.saturating_add(height).saturating_sub(1) {
                let _ = Terminal::print_at(
                    Position {
                        row: line_row,
                        col: content_col,
                    },
                    &format!("{:<width$}", line, width = content_width),
                );
            }
        }

        self.needs_redraw = false;
        Ok(())
    }
}

impl PluginComponent for WordCount {
    fn update_from_buffer(&mut self, buffer: &Buffer) {
        // Grapheme-correct stats shared with the status bar (`Buffer::word_count_stats`).
        let (words, graphemes, lines) = buffer.word_count_stats();
        self.words = words;
        self.chars = graphemes;
        self.lines = lines;
        // Average reading speed: 200 words per minute. Empty docs read as 0 min.
        self.reading_time_minutes = if words == 0 {
            0
        } else {
            (words as f64 / 200.0).ceil() as usize
        };
        self.needs_redraw = true;
    }

    fn handle_move(&mut self, _direction: yonro_core::command::Move) {
        // WordCount doesn't handle keyboard navigation
    }

    fn handle_select(&mut self) -> Option<std::path::PathBuf> {
        None
    }

    fn handle_click(&mut self, position: Position) -> crate::uicomponents::ClickAction {
        // Check if close button clicked
        let close_col = self
            .rect
            .position
            .col
            .saturating_add(self.rect.size.width)
            .saturating_sub(4);
        if position.row == self.rect.position.row
            && position.col >= close_col
            && position.col < close_col + 3
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

    fn render_content(&mut self, rect: Rect) -> Result<(), Error> {
        self.rect = rect;
        self.draw()?;
        Ok(())
    }
}
