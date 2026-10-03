use super::context::EditorContext;
use yonro_core::command::{Command, Edit, Move, System};
use crate::command_dispatcher::PromptType;
use crate::plugins::{OutlineField, PluginResponse};

pub mod edit;
pub mod mouse;
pub mod move_cmd;
pub mod system;

pub use edit::EditHandler;
pub use mouse::MouseHandler;
pub use move_cmd::MoveHandler;
pub use system::SystemHandler;

// Re-exporting pane ops so that the prompt handler can call them
pub use mouse::{close_pane, open_file_explorer, toggle_floating, unfloat_pane};
pub use system::save;

// All handlers must implement this trait
pub trait CommandHandler {
    fn can_handle(&self, command: &Command) -> bool;
    fn handle(&mut self, command: &Command, ctx: &mut EditorContext) -> Result<(), String>;
}

// Registry of all handlers.
// Dispatch order: PromptAware runs first (handles search/save/pane prompts),
// then the regular type-based handlers for the no-prompt case.
pub struct HandlerRegistry {
    handlers: Vec<Box<dyn CommandHandler>>,
}

impl HandlerRegistry {
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    pub fn register(&mut self, handler: Box<dyn CommandHandler>) {
        self.handlers.push(handler);
    }

    pub fn dispatch(&mut self, command: &Command, ctx: &mut EditorContext) -> Result<(), String> {
        for handler in &mut self.handlers {
            if handler.can_handle(command) {
                match handler.handle(command, ctx) {
                    Ok(()) => return Ok(()),                   // consumed — stop
                    Err(e) if e == "pass-through" => continue, // not consumed — try next
                    Err(e) => return Err(e),                   // real error — propagate
                }
            }
        }
        Ok(()) // no handler claimed it — that's fine
    }
}

impl Default for HandlerRegistry {
    fn default() -> Self {
        let mut registry = Self::new();
        // PromptAwareHandler must be first — it intercepts commands during prompts
        registry.register(Box::new(PromptAwareHandler));
        registry.register(Box::new(SystemHandler));
        registry.register(Box::new(EditHandler));
        registry.register(Box::new(MoveHandler));
        registry.register(Box::new(MouseHandler));
        registry
    }
}

// PromptAwareHandler
// Intercepts ALL commands when a prompt is active.
// Returns early so the type-based handlers below never see them.

pub struct PromptAwareHandler;

impl CommandHandler for PromptAwareHandler {
    fn can_handle(&self, command: &Command) -> bool {
        // Only intercept when a prompt is actually active.
        // We don't have ctx here, so we match in handle() and use a sentinel.
        // The trick: always return true, but check prompt_type in handle().
        // Returning false here would skip us and fall through to type handlers,
        // which is wrong during a prompt. So we always claim we can handle it
        // and decide in handle() whether to consume or pass through.
        let _ = command;
        true // always intercept — handle() decides what to do
    }

    fn handle(&mut self, command: &Command, ctx: &mut EditorContext) -> Result<(), String> {
        match *ctx.prompt_type {
            PromptType::None => {
                // Not in a prompt — let type-based handlers below deal with it.
                // Signal "not consumed" by returning an Err that the registry ignores.
                // The registry returns Ok regardless (see dispatch above), so we need
                // a different mechanism: we return Ok but mark the command as unhandled
                // by returning early without consuming.
                // Actually the cleanest way: return Err here and let registry continue.
                return Err("pass-through".to_string());
            }
            PromptType::Search => handle_search_prompt(command, ctx),
            PromptType::Save => handle_save_prompt(command, ctx),
            PromptType::FocusPane | PromptType::ClosePane => handle_pane_prompt(command, ctx),
            PromptType::Rename | PromptType::OutlinePov | PromptType::OutlineTarget => {
                handle_outline_prompt(command, ctx);
            }
        }
        Ok(())
    }
}

// Fix: HandlerRegistry needs to not stop on Err("pass-through")
// See the fixed dispatch() above — it already continues on Err.
// But wait, our current dispatch() returns Err on first Err.
// We need to special-case pass-through. Let's use a cleaner approach:
// PromptAwareHandler returns Ok(()) for "consumed" and we add a
// separate "consumed" signal. Simplest: use a bool wrapper.

// Actually the simplest fix is to change dispatch() to continue
// on Err rather than return. Let's update HandlerRegistry::dispatch:
// (Already done above — dispatch now calls handle() and continues even on Err,
//  only stopping when handler returns Ok)

// Search prompt

fn handle_search_prompt(command: &Command, ctx: &mut EditorContext) {
    match command {
        Command::System(System::Dismiss) => {
            ctx.set_prompt(PromptType::None);
            // Get buffer_id immutably, then dismiss search
            let buffer_id = ctx
                .pane_manager
                .active_pane()
                .and_then(|p| p.view())
                .map(|v| v.buffer_id());

            if let Some(id) = buffer_id {
                if let Some(buffer) = ctx.buffer_manager.get(id) {
                    if let Some(view) = ctx
                        .pane_manager
                        .active_pane_mut()
                        .and_then(|p| p.view_mut())
                    {
                        view.dismiss_search(buffer);
                    }
                }
            }
        }

        Command::Edit(Edit::InsertNewLine) => {
            ctx.set_prompt(PromptType::None);
            if let Some(view) = ctx
                .pane_manager
                .active_pane_mut()
                .and_then(|p| p.view_mut())
            {
                view.exit_search();
            }
        }

        Command::Edit(edit_cmd) => {
            ctx.command_bar.handle_edit_command(*edit_cmd, ctx.clipboard);
            let query = ctx.command_bar.value();

            let buffer_id = ctx
                .pane_manager
                .active_pane()
                .and_then(|p| p.view())
                .map(|v| v.buffer_id());

            if let Some(id) = buffer_id {
                if let Some(buffer) = ctx.buffer_manager.get(id) {
                    if let Some(view) = ctx
                        .pane_manager
                        .active_pane_mut()
                        .and_then(|p| p.view_mut())
                    {
                        view.search(&query, buffer);
                    }
                }
            }
        }

        Command::Move(Move::Right | Move::Down) => {
            let buffer_id = ctx
                .pane_manager
                .active_pane()
                .and_then(|p| p.view())
                .map(|v| v.buffer_id());

            if let Some(id) = buffer_id {
                if let Some(buffer) = ctx.buffer_manager.get(id) {
                    if let Some(view) = ctx
                        .pane_manager
                        .active_pane_mut()
                        .and_then(|p| p.view_mut())
                    {
                        view.search_next(buffer);
                    }
                }
            }
        }

        Command::Move(Move::Up | Move::Left) => {
            let buffer_id = ctx
                .pane_manager
                .active_pane()
                .and_then(|p| p.view())
                .map(|v| v.buffer_id());

            if let Some(id) = buffer_id {
                if let Some(buffer) = ctx.buffer_manager.get(id) {
                    if let Some(view) = ctx
                        .pane_manager
                        .active_pane_mut()
                        .and_then(|p| p.view_mut())
                    {
                        view.search_prev(buffer);
                    }
                }
            }
        }

        _ => {} // everything else ignored during search
    }
}

// Save prompt

fn handle_save_prompt(command: &Command, ctx: &mut EditorContext) {
    match command {
        Command::System(System::Dismiss) => {
            ctx.set_prompt(PromptType::None);
            ctx.update_message("Save aborted");
        }

        Command::Edit(Edit::InsertNewLine) => {
            let file_name = ctx.command_bar.value();
            save(ctx, Some(&file_name));
            ctx.set_prompt(PromptType::None);
        }

        Command::Edit(edit_cmd) => {
            ctx.command_bar.handle_edit_command(*edit_cmd, ctx.clipboard);
        }

        _ => {}
    }
}

// Pane prompt

fn handle_pane_prompt(command: &Command, ctx: &mut EditorContext) {
    match command {
        Command::System(System::Dismiss) => {
            ctx.set_prompt(PromptType::None);
        }

        Command::Edit(Edit::InsertNewLine) => {
            let input = ctx.command_bar.value();
            execute_pane_command(&input, ctx);
            ctx.set_prompt(PromptType::None);
        }

        Command::Edit(edit_cmd) => {
            ctx.command_bar.handle_edit_command(*edit_cmd, ctx.clipboard);
        }

        _ => {}
    }
}

// Outline field prompt (rename / POV / word target): commit pushes a typed
// response; the editor resolves the outline selection at commit time.
fn handle_outline_prompt(command: &Command, ctx: &mut EditorContext) {
    let field = match *ctx.prompt_type {
        PromptType::Rename => OutlineField::Rename,
        PromptType::OutlinePov => OutlineField::Pov,
        PromptType::OutlineTarget => OutlineField::Target,
        _ => return,
    };
    match command {
        Command::System(System::Dismiss) => {
            ctx.set_prompt(PromptType::None);
            ctx.update_message("Edit aborted");
        }
        Command::Edit(Edit::InsertNewLine) => {
            let value = ctx.command_bar.value();
            ctx.plugin_responses
                .push(PluginResponse::ManuscriptApply { field, value });
            ctx.set_prompt(PromptType::None);
        }
        Command::Edit(edit_cmd) => {
            ctx.command_bar.handle_edit_command(*edit_cmd, ctx.clipboard);
        }
        _ => {}
    }
}

fn execute_pane_command(input: &str, ctx: &mut EditorContext) {
    let parts: Vec<&str> = input.split_whitespace().collect();
    match parts.as_slice() {
        ["focus", id_str] => {
            if let Ok(id) = id_str.parse::<usize>() {
                if ctx.pane_manager.get_pane(id).is_some() {
                    ctx.pane_manager.set_active_pane(id);
                } else {
                    ctx.update_message(&format!("Pane {} not found", id));
                }
            }
        }
        [id_str] if id_str.parse::<usize>().is_ok() => {
            let id = id_str.parse::<usize>().unwrap();
            if ctx.pane_manager.get_pane(id).is_some() {
                ctx.pane_manager.set_active_pane(id);
            } else {
                ctx.update_message(&format!("Pane {} not found", id));
            }
        }
        ["close", id_str] => {
            if let Ok(id) = id_str.parse::<usize>() {
                close_pane(id, ctx);
            }
        }
        ["close"] => {
            if let Some(id) = ctx.pane_manager.active_pane().map(|p| p.pane_id) {
                close_pane(id, ctx);
            }
        }
        ["float"] => {
            if let Some(id) = ctx.pane_manager.active_pane().map(|p| p.pane_id) {
                toggle_floating(id, ctx);
            }
        }
        ["unfloat"] => {
            if let Some(id) = ctx.pane_manager.active_pane().map(|p| p.pane_id) {
                unfloat_pane(id, ctx);
            }
        }
        ["explore"] => open_file_explorer(ctx),
        _ => ctx.update_message("Commands: focus <id> (or just <id>), close [<id>], float, unfloat, explore"),
    }
}
