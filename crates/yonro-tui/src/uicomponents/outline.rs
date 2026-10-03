// Manuscript outline sidebar (`PLAN.md Phase 4.3`): renders the project's
// Act → Chapter → Scene tree with live word-count progress. The component
// owns only *view* state (rows, selection); the `Manuscript` itself lives in
// `Editor` and is pushed in via `sync_outline` every visible frame.

use crate::prelude::*;
use crate::terminal::Terminal;
use crate::uicomponents::{ClickAction, PluginComponent, UIComponent};
use std::io::Error;
use std::path::PathBuf;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;
use yonro_core::buffers::BufferManager;
use yonro_core::command::Move;
use yonro_core::manuscript::{Manuscript, NodeId, NodeKind};
use yonro_core::Buffer;

struct OutlineRow {
    id: NodeId,
    text: String,
}

pub struct Outline {
    rows: Vec<OutlineRow>,
    selected_idx: usize,
    /// Selection requested before rows rebuild (structural adds): applied by
    /// the next `rebuild`, which is the first place the new id exists.
    pending_selection: Option<NodeId>,
    scroll_top: usize,
    rect: Rect,
    needs_redraw: bool,
    pub active: bool,
    last_click_time: Option<std::time::Instant>,
    last_click_idx: Option<usize>,
}

impl Default for Outline {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            selected_idx: 0,
            pending_selection: None,
            scroll_top: 0,
            rect: Rect::default(),
            needs_redraw: true,
            active: false,
            last_click_time: None,
            last_click_idx: None,
        }
    }
}

impl Outline {
    fn push_children(
        rows: &mut Vec<OutlineRow>,
        manuscript: &Manuscript,
        parent: NodeId,
        depth: usize,
    ) {
        for child in manuscript.children(parent) {
            let label = match child.kind {
                NodeKind::Act => format!("§ {}", child.title),
                NodeKind::Chapter => format!("{} {}", "·".repeat(depth), child.title),
                NodeKind::Scene => {
                    let words = child.meta.as_ref().map_or(0, |m| m.current_words);
                    let target = child.meta.as_ref().map_or(0, |m| m.target_words);
                    if target > 0 {
                        let pct = words.saturating_mul(100).checked_div(target).unwrap_or(0);
                        format!(
                            "{}◦ {} [{words}/{target} {pct}%]",
                            "·".repeat(depth),
                            child.title
                        )
                    } else if words > 0 {
                        format!("{}◦ {} [{words}w]", "·".repeat(depth), child.title)
                    } else {
                        format!("{}◦ {}", "·".repeat(depth), child.title)
                    }
                }
                NodeKind::Project => child.title.clone(),
            };
            rows.push(OutlineRow {
                id: child.id,
                text: label,
            });
            Self::push_children(rows, manuscript, child.id, depth.saturating_add(1));
        }
    }

    fn rebuild(&mut self, manuscript: &Manuscript) {
        let current_id = self.rows.get(self.selected_idx).map(|row| row.id);
        let mut rows = vec![OutlineRow {
            id: manuscript.root(),
            text: format!("✎ {}", manuscript.title()),
        }];
        Self::push_children(&mut rows, manuscript, manuscript.root(), 1);
        self.rows = rows;
        // Pending (just-added node) wins; else preserve by id; else top.
        let wanted = self.pending_selection.take().or(current_id);
        self.selected_idx = wanted
            .and_then(|id| self.rows.iter().position(|row| row.id == id))
            .unwrap_or(0);
        self.adjust_scroll();
        self.needs_redraw = true;
    }

    fn adjust_scroll(&mut self) {
        let content_height = self.rect.size.height.saturating_sub(3);
        if content_height == 0 || self.rows.is_empty() {
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
        let max_scroll = self.rows.len().saturating_sub(1);
        self.scroll_top = self.scroll_top.min(max_scroll);
    }

    fn move_selection(&mut self, direction: Move) {
        if self.rows.is_empty() {
            return;
        }
        let prev = self.selected_idx;
        match direction {
            Move::Up => {
                self.selected_idx = self.selected_idx.saturating_sub(1);
            }
            Move::Down => {
                self.selected_idx = self
                    .selected_idx
                    .saturating_add(1)
                    .min(self.rows.len().saturating_sub(1));
            }
            _ => {}
        }
        self.adjust_scroll();
        if prev != self.selected_idx {
            self.needs_redraw = true;
        }
    }

    fn draw_rows(&self) -> Result<(), Error> {
        let rect = self.rect;
        let content_row_start = rect.position.row.saturating_add(1);
        let content_col = rect.position.col.saturating_add(1);
        let content_width = rect.size.width.saturating_sub(2);
        let content_height = rect.size.height.saturating_sub(2);
        for screen_row in 0..content_height {
            let abs_row = content_row_start.saturating_add(screen_row);
            let line = match self.rows.get(self.scroll_top.saturating_add(screen_row)) {
                Some(row) => {
                    let prefix = if Some(row.id)
                        == self.rows.get(self.selected_idx).map(|r| r.id)
                        && self.active
                    {
                        "▶ "
                    } else {
                        "  "
                    };
                    let full = format!("{prefix}{}", row.text);
                    truncate_to_width(&full, content_width)
                }
                None => " ".repeat(content_width),
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

    fn draw_hint(&self) -> Result<(), Error> {
        // Footer hint inside the border when space allows.
        if self.rect.size.height < 5 || self.rect.size.width < 12 {
            return Ok(());
        }
        let hint = "a/c/s add · Enter open";
        let row = self
            .rect
            .position
            .row
            .saturating_add(self.rect.size.height)
            .saturating_sub(1);
        let line = truncate_to_width(hint, self.rect.size.width.saturating_sub(2));
        Terminal::print_at(
            Position {
                row,
                col: self.rect.position.col.saturating_add(1),
            },
            &line,
        )?;
        Ok(())
    }

    fn click_row(&mut self, position: Position) -> bool {
        let content_row_start = self.rect.position.row.saturating_add(1);
        let content_height = self.rect.size.height.saturating_sub(2);
        if position.row >= content_row_start
            && position.row < content_row_start.saturating_add(content_height)
        {
            let click_idx = self
                .scroll_top
                .saturating_add(position.row.saturating_sub(content_row_start));
            if click_idx < self.rows.len() {
                let prev = self.selected_idx;
                self.selected_idx = click_idx;
                if prev != self.selected_idx {
                    self.needs_redraw = true;
                }
                return true;
            }
        }
        false
    }
}

/// Truncate grapheme-safe to `width` display columns (ellipsis on cut),
/// padding short lines to clear stale characters.
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

impl UIComponent for Outline {
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
        self.draw_rows()?;
        self.needs_redraw = false;
        Ok(())
    }
}

impl PluginComponent for Outline {
    fn handle_move(&mut self, direction: Move) {
        // Sync-dispatched by `MoveHandler` (same single-path rule as the
        // explorer: no `MoveInPane` emission, or every press moves twice).
        self.move_selection(direction);
    }

    fn handle_select(&mut self) -> Option<PathBuf> {
        // The editor resolves the selected node (scene file materialization
        // needs filesystem + manuscript access the component doesn't have).
        // Returning `None` here would swallow Enter, so surface intent via a
        // sentinel: the editor checks `outline_selection` instead.
        None
    }

    fn handle_click(&mut self, position: Position) -> ClickAction {
        if self.click_row(position) {
            let now = std::time::Instant::now();
            let is_double = self.last_click_idx == Some(self.selected_idx)
                && self
                    .last_click_time
                    .is_some_and(|t| now.duration_since(t).as_millis() < 300);
            self.last_click_time = Some(now);
            self.last_click_idx = Some(self.selected_idx);
            if is_double {
                return ClickAction::DoubleClick;
            }
        }
        ClickAction::None
    }

    fn set_active(&mut self, active: bool) {
        if self.active != active {
            self.active = active;
            self.needs_redraw = true;
        }
    }

    fn render_content(&mut self, rect: Rect) -> Result<(), Error> {
        self.rect = rect;
        self.adjust_scroll();
        self.draw_rows()?;
        self.draw_hint()?;
        self.needs_redraw = false;
        Ok(())
    }

    fn sync_outline(&mut self, manuscript: &Manuscript, _buffers: &BufferManager) {
        self.rebuild(manuscript);
    }

    fn outline_selection(&self) -> Option<NodeId> {
        self.rows.get(self.selected_idx).map(|row| row.id)
    }

    fn set_outline_selection(&mut self, id: Option<NodeId>) {
        // Rows rebuild on the next sync (the id may not exist yet when a
        // structural add lands), so stage it for `rebuild` either way.
        self.pending_selection = id;
        if let Some(id) = id {
            if let Some(idx) = self.rows.iter().position(|row| row.id == id) {
                self.pending_selection = None;
                self.selected_idx = idx;
                self.adjust_scroll();
                self.needs_redraw = true;
            }
        }
    }

    fn update_from_buffer(&mut self, _buffer: &Buffer) {
        // Word counts arrive via `sync_outline` (manuscript-synced by core).
    }
}
