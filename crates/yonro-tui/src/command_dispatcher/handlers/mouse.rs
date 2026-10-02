use super::{CommandHandler, EditorContext};
use crate::layout::SidebarKind;
use crate::plugins::PluginResponse;
use crate::prelude::*;
use yonro_core::command::{Command, MouseCommand};

pub struct MouseHandler;

impl CommandHandler for MouseHandler {
    fn can_handle(&self, command: &Command) -> bool {
        matches!(command, Command::Mouse(_))
    }

    fn handle(&mut self, command: &Command, ctx: &mut EditorContext) -> Result<(), String> {
        if let Command::Mouse(mouse_cmd) = command {
            match mouse_cmd {
                MouseCommand::LeftClick(pos) => handle_left_click(*pos, ctx),
                MouseCommand::LeftDrag(pos) => handle_left_drag(*pos, ctx),
                MouseCommand::LeftRelease(_) => handle_left_release(ctx),
                MouseCommand::ScrollUp(_) => pane_scroll_up(ctx),
                MouseCommand::ScrollDown(_) => pane_scroll_down(ctx),
            }
            Ok(())
        } else {
            Err("Not a Mouse command".to_string())
        }
    }
}

fn handle_left_click(position: Position, ctx: &mut EditorContext) {
    // 0. Check if user clicked on the top bar (PaneBar)
    if position.row == 0 {
        // Document tabs → switch tab (VS Code-style).
        let clicked_tab = ctx
            .pane_bar
            .tab_hitboxes
            .iter()
            .find(|&&(_, _, start, end)| position.col >= start && position.col < end)
            .map(|&(idx, _, _, _)| idx);

        if let Some(tab_idx) = clicked_tab {
            ctx.plugin_responses.push(PluginResponse::SwitchTab { index: tab_idx });
            return;
        }

        // Close buttons on tabs → close that tab's pane (pruned by core).
        let clicked_close = ctx
            .pane_bar
            .close_hitboxes
            .iter()
            .find(|&&(_, _, start, end)| position.col >= start && position.col < end)
            .map(|&(_, pane_id, _, _)| pane_id);

        if let Some(pane_id) = clicked_close {
            // Route through the core so document tabs are pruned as well.
            ctx.plugin_responses
                .push(PluginResponse::ClosePane { pane_id });
            ctx.update_message("Pane closed");
            return;
        }

        // Check minimized panes
        let clicked_min = ctx
            .pane_bar
            .minimized_hitboxes
            .iter()
            .find(|&&(_, start, end)| position.col >= start && position.col < end)
            .map(|&(id, _, _)| id);

        if let Some(pane_id) = clicked_min {
            if let Some(pane) = ctx.pane_manager.get_pane_mut(pane_id) {
                pane.is_minimized = false;
                ctx.mark_all_panes_for_redraw();
            }
            return;
        }
        return;
    }

    // Sidebar (right strip when visible): title [x] closes, body focuses so
    // the plugin emits MouseClickInPane next. Returns true if consumed.
    if handle_sidebar_click(position, ctx) {
        return;
    }

    // 1. Check floating panes top-down (highest z first)
    // Collecting IDs first to avoid holding a borrow into pane_manager
    let floating_hit = {
        let mut floating = ctx.pane_manager.get_floating_panes_sorted_mut();
        floating.reverse();

        let mut hit: Option<(usize, bool, bool, bool, Position)> = None; // (id, is_close, is_min, is_title_drag, drag_offset)
        for pane in &floating {
            let rect = pane.component().rect();
            // If minimized, only the title bar (first row) is clickable (minimization is yet to be implemented)
            let height = if pane.is_minimized {
                1
            } else {
                rect.size.height
            };

            let inside = position.row >= rect.position.row
                && position.row < rect.position.row + height
                && position.col >= rect.position.col
                && position.col < rect.position.col + rect.size.width;

            if inside {
                let is_close = pane.is_on_close_button(position);
                let is_min = pane.is_on_min_button(position);
                let is_title = pane.is_on_title_bar(position) && !is_close && !is_min;
                let offset = Position {
                    col: position.col.saturating_sub(rect.position.col),
                    row: position.row.saturating_sub(rect.position.row),
                };
                hit = Some((pane.pane_id, is_close, is_min, is_title, offset));
                break;
            }
        }
        hit
    };

    if let Some((id, is_close, is_min, is_title_drag, drag_offset)) = floating_hit {
        if is_close {
            close_pane(id, ctx);
            return;
        }
        if is_min {
            if let Some(p) = ctx.pane_manager.get_pane_mut(id) {
                p.is_minimized = !p.is_minimized;
                ctx.mark_all_panes_for_redraw();
            }
            return;
        }
        ctx.set_active_pane(id);
        if is_title_drag {
            *ctx.dragging_pane = Some(id);
            *ctx.drag_offset = drag_offset;
        }
        return;
    }

    // 2. Check if user clicked on a split divider
    if let Some(split) = ctx.layout_tree.find_split(position) {
        *ctx.dragging_split = Some(split.id);
        return;
    }

    // 3. Focus tiled pane under the cursor
    let tiled_hit = ctx
        .layout_tree
        .collect_leaf_layouts()
        .into_iter()
        .find(|(_, rect)| {
            position.row >= rect.position.row
                && position.row < rect.position.row + rect.size.height
                && position.col >= rect.position.col
                && position.col < rect.position.col + rect.size.width
        })
        .map(|(id, _)| id);

    if let Some(pane_id) = tiled_hit {
        // Read button state before any mutation
        let (is_close, is_min) = ctx
            .pane_manager
            .get_pane(pane_id)
            .map(|p| (p.is_on_close_button(position), p.is_on_min_button(position)))
            .unwrap_or((false, false));

        if is_close {
            close_pane(pane_id, ctx);
            return;
        }
        if is_min {
            if let Some(p) = ctx.pane_manager.get_pane_mut(pane_id) {
                p.is_minimized = !p.is_minimized;
                ctx.mark_all_panes_for_redraw();
            }
            return;
        }
        ctx.set_active_pane(pane_id);
    }
}

/// Sidebar hit-test for left clicks. Returns true when consumed.
fn handle_sidebar_click(position: Position, ctx: &mut EditorContext) -> bool {
    let sidebar = &ctx.layout_tree.sidebar;
    if !sidebar.visible {
        return false;
    }
    let Some(pane_id) = sidebar.pane_id else {
        return false;
    };
    let term_width = ctx.terminal_size.width;
    let term_height = ctx.terminal_size.height;
    let sidebar_width = sidebar.width;
    let sidebar_col = term_width.saturating_sub(sidebar_width);
    let in_sidebar_col = position.col >= sidebar_col
        && position.col < sidebar_col.saturating_add(sidebar_width);
    let in_sidebar_row =
        position.row >= 1 && position.row < 1_usize.saturating_add(term_height.saturating_sub(3));
    if !(in_sidebar_col && in_sidebar_row) {
        return false;
    }
    // Absolute close-button column (was `sidebar_width - 4`, which never hit).
    let close_col = term_width.saturating_sub(4);
    if position.row == 1
        && position.col >= close_col
        && position.col < close_col.saturating_add(3)
    {
        ctx.plugin_responses
            .push(PluginResponse::CloseSidebar { kind: sidebar.kind });
        return true;
    }
    ctx.set_active_pane(pane_id);
    true
}

fn handle_left_drag(position: Position, ctx: &mut EditorContext) {    if let Some(split_id) = *ctx.dragging_split {
        ctx.layout_tree.resize_split(split_id, position);
        let size = ctx.terminal_size;
        ctx.handle_resize(size);
        return;
    }

    if let Some(pane_id) = *ctx.dragging_pane {
        if let Some(pane) = ctx.pane_manager.get_pane_mut(pane_id) {
            let mut rect = pane.component().rect();
            rect.position.col = position.col.saturating_sub(ctx.drag_offset.col);
            rect.position.row = position.row.saturating_sub(ctx.drag_offset.row);
            let sidebar_width = if ctx.layout_tree.sidebar.visible {
                ctx.layout_tree.sidebar.width
            } else {
                0
            };
            clamp_floating_rect(&mut rect, ctx.terminal_size, sidebar_width);
            pane.resize(rect);
        }
        ctx.mark_all_panes_for_redraw();
    }
}

/// Keep floating panes inside the editor area: below `BufferBar` (row 0),
/// above `StatusBar` (`height - 2`) and `CommandBar` (`height - 1`),
/// and to the left of the sidebar if visible.
/// `saturating_*` is mandatory: terminal sizes are `usize`, so plain `-`
/// panics in debug / wraps in release on tiny terminals or oversized panes.
fn clamp_floating_rect(rect: &mut Rect, term: Size, sidebar_width: usize) {
    let max_col = term.width.saturating_sub(sidebar_width).saturating_sub(rect.size.width);
    rect.position.col = rect.position.col.min(max_col);
    let max_row = term
        .height
        .saturating_sub(rect.size.height.saturating_add(2))
        .max(1);
    rect.position.row = rect.position.row.clamp(1, max_row);
}

fn handle_left_release(ctx: &mut EditorContext) {
    *ctx.dragging_split = None;
    *ctx.dragging_pane = None;
}

fn pane_scroll_up(ctx: &mut EditorContext) {
    // Getting buffer_id immutably first
    let buffer_id = match ctx
        .pane_manager
        .active_pane()
        .and_then(|p| p.view())
        .map(|v| v.buffer_id())
    {
        Some(id) => id,
        None => return,
    };

    let buffer = match ctx.buffer_manager.get(buffer_id) {
        Some(b) => b,
        None => return,
    };

    if let Some(view) = ctx
        .pane_manager
        .active_pane_mut()
        .and_then(|p| p.view_mut())
    {
        view.handle_move_command(yonro_core::command::Move::PageUp, buffer);
    }
}

fn pane_scroll_down(ctx: &mut EditorContext) {
    let buffer_id = match ctx
        .pane_manager
        .active_pane()
        .and_then(|p| p.view())
        .map(|v| v.buffer_id())
    {
        Some(id) => id,
        None => return,
    };

    let buffer = match ctx.buffer_manager.get(buffer_id) {
        Some(b) => b,
        None => return,
    };

    if let Some(view) = ctx
        .pane_manager
        .active_pane_mut()
        .and_then(|p| p.view_mut())
    {
        view.handle_move_command(yonro_core::command::Move::PageDown, buffer);
    }
}

// Pane lifecycle

pub fn close_pane(id: usize, ctx: &mut EditorContext) {
    // Sidebar panes live outside `LayoutTree` — route through CloseSidebar so
    // `Editor::apply_plugin_response` hides, refocuses, and sends PaneClosed.
    if Some(id) == ctx.layout_tree.sidebar.pane_id {
        let kind = ctx.layout_tree.sidebar.kind;
        ctx.plugin_responses
            .push(PluginResponse::CloseSidebar { kind });
        ctx.update_message("Sidebar closed");
        return;
    }
    // Route through the core so document tabs are pruned as well
    // (direct removal here would strand tabs pointing at dead panes).
    ctx.plugin_responses
        .push(PluginResponse::ClosePane { pane_id: id });
    ctx.update_message("Pane closed");
}

pub fn toggle_floating(id: usize, ctx: &mut EditorContext) {
    let is_floating = ctx
        .pane_manager
        .get_pane(id)
        .map_or(false, |p| p.is_floating);

    if is_floating {
        ctx.update_message("Pane is already floating.");
        return;
    }

    let was_active = ctx
        .pane_manager
        .active_pane()
        .map(|p| p.pane_id == id)
        .unwrap_or(false);

    if ctx.layout_tree.remove_node(id).is_err() {
        ctx.update_message("Cannot float the last tiled pane!");
        return;
    }

    if let Some(pane) = ctx.pane_manager.get_pane_mut(id) {
        pane.is_floating = true;
        let mut rect = pane.component().rect();
        rect.size.height = rect.size.height.min(15);
        rect.size.width = rect.size.width.min(40);
        rect.position.col = rect
            .position
            .col
            .min(ctx.terminal_size.width.saturating_sub(rect.size.width));
        let max_row = ctx
            .terminal_size
            .height
            .saturating_sub(rect.size.height.saturating_add(2))
            .max(1);
        rect.position.row = rect.position.row.clamp(1, max_row);
        pane.resize(rect);
    }

    ctx.pane_manager.bring_to_front(id);

    let size = ctx.terminal_size;
    ctx.handle_resize(size);

    if was_active {
        ctx.pane_manager.set_active_pane(id);
    }

    ctx.update_message(&format!("Pane {} is now floating", id));
}

pub fn unfloat_pane(id: usize, ctx: &mut EditorContext) {
    let is_floating = ctx
        .pane_manager
        .get_pane(id)
        .map_or(false, |p| p.is_floating);

    if !is_floating {
        ctx.update_message("Pane is already tiled.");
        return;
    }

    let target_id = ctx
        .layout_tree
        .collect_leaf_layouts()
        .first()
        .map(|(id, _)| *id);

    match target_id {
        None => ctx.update_message("No tiled panes found."),
        Some(tid) => {
            if ctx
                .layout_tree
                .split_pane(
                    tid,
                    id,
                    crate::layout::SplitDirection::Vertical,
                    0.5,
                )
                .is_ok()
            {
                if let Some(pane) = ctx.pane_manager.get_pane_mut(id) {
                    pane.is_floating = false;
                    pane.is_minimized = false;
                }
                let size = ctx.terminal_size;
                ctx.handle_resize(size);
                ctx.update_message(&format!("Pane {} is now tiled", id));
            } else {
                ctx.update_message("Failed to tile pane (target too small?)");
            }
        }
    }
}

pub fn open_file_explorer(ctx: &mut EditorContext) {
    // Unified path: the sidebar is the only explorer (Ctrl+E / ToggleSidebar).
    // Legacy split-pane explorer created a second, untracked pane whose
    // `open_pane_id` never matched, breaking Enter/Esc/click routing.
    ctx.plugin_responses.push(PluginResponse::ToggleSidebar {
        kind: SidebarKind::FileExplorer,
    });
}