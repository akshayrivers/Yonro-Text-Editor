// src/editor/command_dispatcher/handlers/move_cmd.rs
use super::{CommandHandler, EditorContext};
use yonro_core::command::Command;
use crate::layout::PaneContent;

pub struct MoveHandler;

impl CommandHandler for MoveHandler {
    fn can_handle(&self, command: &Command) -> bool {
        matches!(command, Command::Move(_))
    }

    fn handle(&mut self, command: &Command, ctx: &mut EditorContext) -> Result<(), String> {
        if let Command::Move(move_cmd) = command {
            if let Some(pane) = ctx.pane_manager.active_pane_mut() {
                match &mut pane.content {
                    PaneContent::TextView(view) => {
                        // Get buffer_id immutably, drop borrow, then get buffer
                        let buffer_id = view.buffer_id();
                        if let Some(buffer) = ctx.buffer_manager.get(buffer_id) {
                            view.handle_move_command(*move_cmd, buffer);
                        }
                    }
                    PaneContent::Plugin(_component) => {
                        // need to figure this out soon
                    }
                    PaneContent::Popup(_) => {}
                }
            }
            Ok(())
        } else {
            Err("Not a Move command".to_string())
        }
    }
}
