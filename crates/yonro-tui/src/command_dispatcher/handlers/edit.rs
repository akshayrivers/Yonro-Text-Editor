use super::{CommandHandler, EditorContext};
use crate::layout::PaneContent;
use yonro_core::command::{Command, Edit};

pub struct EditHandler;

impl CommandHandler for EditHandler {
    fn can_handle(&self, command: &Command) -> bool {
        matches!(command, Command::Edit(_))
    }

    fn handle(&mut self, command: &Command, ctx: &mut EditorContext) -> Result<(), String> {
        if let Command::Edit(edit_cmd) = command {
            // Empty paste short-circuits with a hint instead of touching state.
            if matches!(edit_cmd, Edit::Paste) && ctx.clipboard.get().is_none_or(|t| t.is_empty()) {
                ctx.update_message("Clipboard empty");
                return Ok(());
            }
            if let Some(pane) = ctx.pane_manager.active_pane_mut() {
                if let PaneContent::TextView(view) = &mut pane.content {
                    let buffer_id = view.buffer_id();
                    if let Some(buffer) = ctx.buffer_manager.get_mut(buffer_id) {
                        view.handle_edit_command(*edit_cmd, buffer, ctx.clipboard);
                        ctx.notify_buffer_changed(buffer_id);
                    }
                }
            }
            match edit_cmd {
                Edit::Copy => ctx.update_message("Copied line"),
                Edit::Cut => ctx.update_message("Cut line"),
                Edit::Paste => ctx.update_message("Pasted"),
                _ => {}
            }
            Ok(())
        } else {
            Err("Not an Edit command".to_string())
        }
    }
}
