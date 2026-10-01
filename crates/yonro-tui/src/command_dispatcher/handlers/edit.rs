use super::{CommandHandler, EditorContext};
use yonro_core::command::Command;
use crate::layout::PaneContent;

pub struct EditHandler;

impl CommandHandler for EditHandler {
    fn can_handle(&self, command: &Command) -> bool {
        matches!(command, Command::Edit(_))
    }

    fn handle(&mut self, command: &Command, ctx: &mut EditorContext) -> Result<(), String> {
        if let Command::Edit(edit_cmd) = command {
            if let Some(pane) = ctx.pane_manager.active_pane_mut() {
                if let PaneContent::TextView(view) = &mut pane.content {
                    let buffer_id = view.buffer_id();
                    if let Some(buffer) = ctx.buffer_manager.get_mut(buffer_id) {
                        view.handle_edit_command(*edit_cmd, buffer);
                        ctx.notify_buffer_changed(buffer_id);
                    }
                }
            }
            Ok(())
        } else {
            Err("Not an Edit command".to_string())
        }
    }
}
