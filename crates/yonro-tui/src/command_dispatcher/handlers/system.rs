use super::{CommandHandler, EditorContext};
use crate::command_dispatcher::PromptType;
use crate::layout::{PaneContent, SplitDirection};
use crate::uicomponents::View;
use yonro_core::buffers::Buffer;
use yonro_core::command::{Command, System};

pub struct SystemHandler;

impl CommandHandler for SystemHandler {
    fn can_handle(&self, command: &Command) -> bool {
        matches!(command, Command::System(_))
    }

    fn handle(&mut self, command: &Command, ctx: &mut EditorContext) -> Result<(), String> {
        if let Command::System(sys) = command {
            match sys {
                System::Resize(size) => ctx.handle_resize(*size),
                System::Quit => handle_quit(ctx),
                System::Search => ctx.set_prompt(PromptType::Search),
                System::Save => handle_save_command(ctx),
                System::Undo => handle_undo(ctx),
                System::Redo => handle_redo(ctx),
                System::SplitHorizontal => split_active_pane(ctx, SplitDirection::Horizontal),
                System::SplitVertical => split_active_pane(ctx, SplitDirection::Vertical),
                System::OpenCommandBar => ctx.set_prompt(PromptType::FocusPane),
                // Handled by `Editor::handle_event` pre-dispatch (needs Editor
                // state, not just the handler context) — unreachable here.
                System::ZenToggle => {}
                System::Dismiss => {} // handled by prompt handlers
            }
            Ok(())
        } else {
            Err("Not a System command".to_string())
        }
    }
}

const QUIT_TIMES: u8 = 3;

fn handle_quit(ctx: &mut EditorContext) {
    let is_modified = ctx
        .pane_manager
        .active_pane()
        .and_then(|p| p.view())
        .and_then(|v| ctx.buffer_manager.get(v.buffer_id()))
        .map(|b| b.is_dirty())
        .unwrap_or(false);

    if !is_modified || *ctx.quit_times + 1 == QUIT_TIMES {
        *ctx.should_quit = true;
    } else {
        ctx.update_message(&format!(
            "Warning! File has unsaved changes. Press Ctrl-Q {} more times to quit.",
            QUIT_TIMES - *ctx.quit_times - 1
        ));
        *ctx.quit_times += 1;
    }
}

fn handle_save_command(ctx: &mut EditorContext) {
    let result = {
        match ctx.pane_manager.active_pane_mut() {
            None => return,
            Some(pane) => match &mut pane.content {
                PaneContent::TextView(view) => {
                    let id = view.buffer_id();
                    let loaded = ctx
                        .buffer_manager
                        .get(id)
                        .map(|b| b.is_file_loaded())
                        .unwrap_or(false);
                    Some((id, loaded))
                }
                PaneContent::Plugin(_) => {
                    // Can't call ctx.update_message here — pane is still borrowed.
                    None
                }
                _ => None,
            },
        }
    };

    match result {
        None => {
            ctx.update_message("Cannot save from this pane type");
        }
        Some((_buffer_id, true)) => {
            save(ctx, None);
        }
        Some((_buffer_id, false)) => {
            ctx.set_prompt(PromptType::Save);
        }
    }
}

pub fn save(ctx: &mut EditorContext, file_name: Option<&str>) {
    let buffer_id = match ctx
        .pane_manager
        .active_pane()
        .and_then(|p| p.view())
        .map(|v| v.buffer_id())
    {
        Some(id) => id,
        None => return,
    };

    // Now get mutable buffer — no pane borrow in scope
    let buffer = match ctx.buffer_manager.get_mut(buffer_id) {
        Some(b) => b,
        None => return,
    };

    let result = match file_name {
        Some(name) => buffer.save_as(name),
        None => buffer.save(),
    };

    if result.is_ok() {
        ctx.update_message("File saved successfully.");
    } else {
        ctx.update_message("Error writing file!");
    }
}

fn handle_undo(ctx: &mut EditorContext) {
    // Get buffer_id first with immutable borrow
    let buffer_id = match ctx
        .pane_manager
        .active_pane()
        .and_then(|p| p.view())
        .map(|v| v.buffer_id())
    {
        Some(id) => id,
        None => return,
    };

    // Now get mutable pane — previous borrow dropped
    let pane = match ctx.pane_manager.active_pane_mut() {
        Some(p) => p,
        None => return,
    };

    if let PaneContent::TextView(view) = &mut pane.content {
        if let Some(buffer) = ctx.buffer_manager.get_mut(buffer_id) {
            view.undo(buffer);
            // Undo changes draft sizes: stats + manuscript follow the edit.
            ctx.notify_buffer_changed(buffer_id);
        }
    }
}

fn handle_redo(ctx: &mut EditorContext) {
    let buffer_id = match ctx
        .pane_manager
        .active_pane()
        .and_then(|p| p.view())
        .map(|v| v.buffer_id())
    {
        Some(id) => id,
        None => return,
    };

    let pane = match ctx.pane_manager.active_pane_mut() {
        Some(p) => p,
        None => return,
    };

    if let PaneContent::TextView(view) = &mut pane.content {
        if let Some(buffer) = ctx.buffer_manager.get_mut(buffer_id) {
            view.redo(buffer);
            // Redo changes draft sizes: stats + manuscript follow the edit.
            ctx.notify_buffer_changed(buffer_id);
        }
    }
}

fn split_active_pane(ctx: &mut EditorContext, direction: SplitDirection) {
    let active_id = match ctx.pane_manager.active_pane().map(|p| p.pane_id) {
        Some(id) => id,
        None => return,
    };
    // creating a new id for  every new buffer
    let new_buffer_id = {
        let buffer = Buffer::default();
        ctx.buffer_manager.add(buffer)
    };

    let new_pane_id = {
        let mut view = View::default();
        view.set_buffer_id(new_buffer_id);
        ctx.pane_manager.create_pane(PaneContent::TextView(view))
    };

    if let Some(pane) = ctx.pane_manager.get_pane_mut(new_pane_id) {
        if let Some(view) = pane.view_mut() {
            view.set_id(new_pane_id);
        }
    }

    if ctx
        .layout_tree
        .split_pane(active_id, new_pane_id, direction, 0.5)
        .is_err()
    {
        ctx.update_message("Pane too small to split");
        ctx.pane_manager.remove_pane(new_pane_id);
        return;
    }

    let size = ctx.terminal_size;
    ctx.handle_resize(size);
    ctx.pane_manager.set_active_pane(new_pane_id);
}
