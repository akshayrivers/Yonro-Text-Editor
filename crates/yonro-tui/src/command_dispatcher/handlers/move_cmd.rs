// src/editor/command_dispatcher/handlers/move_cmd.rs
use super::{CommandHandler, EditorContext};
use crate::layout::PaneContent;
use yonro_core::command::Command;

pub struct MoveHandler;

impl CommandHandler for MoveHandler {
    fn can_handle(&self, command: &Command) -> bool {
        matches!(command, Command::Move(_))
    }

    fn handle(&mut self, command: &Command, ctx: &mut EditorContext) -> Result<(), String> {
        if let Command::Move(move_cmd) = command {
            if let Some(pane) = ctx.pane_manager.active_pane_mut() {
                // Direct dispatch to plugin panes (FileExplorer, etc.) - synchronous, no channel round-trip
                pane.plugin_handle_move(*move_cmd);

                // For TextView, handle cursor movement
                if let PaneContent::TextView(view) = &mut pane.content {
                    let buffer_id = view.buffer_id();
                    if let Some(buffer) = ctx.buffer_manager.get(buffer_id) {
                        view.handle_move_command(*move_cmd, buffer);
                    }
                }
            }
            Ok(())
        } else {
            Err("Not a Move command".to_string())
        }
    }
}
