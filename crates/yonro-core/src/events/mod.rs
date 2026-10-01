// EditorEvent: input-agnostic event abstraction.
// The TUI translates raw terminal events into this enum
// (`yonro-tui::input::from_crossterm`); core never sees terminal types.
use crate::command::{Command, System};
use crate::prelude::*;
pub mod customevent;
pub mod keyboard;
pub mod mouse;

pub use customevent::CustomEvent;
pub use keyboard::{key_to_command, KeyCode, KeyInput, KeyModifiers};
pub use mouse::{mouse_to_command, MouseAction, MouseButton, MouseInput};
// Top-level event for editor. Just in case in future if we had workspace feature

#[derive(Debug, Clone)]
pub enum EditorEvent {
    Key(KeyInput),
    Mouse(MouseInput),
    Resize(Size),
    Custom(CustomEvent),
    /// The frontend emitted something we don't model yet
    Unhandled,
}

// Command conversion
// Mapped to our existing Command enum

impl TryFrom<EditorEvent> for Command {
    type Error = String;

    fn try_from(event: EditorEvent) -> Result<Self, Self::Error> {
        match event {
            EditorEvent::Key(key) => key_to_command(key),
            EditorEvent::Mouse(mouse) => mouse_to_command(mouse),
            EditorEvent::Resize(size) => Ok(Command::System(System::Resize(size))),
            EditorEvent::Unhandled | EditorEvent::Custom(_) => {
                Err("Event not mapped to a command".to_string())
            }
        }
    }
}
