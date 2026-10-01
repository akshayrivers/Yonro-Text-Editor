use crate::editor::events::customevent::*;
use crate::editor::events::keyboard::*;
use crate::editor::events::mouse::*;
// EditorEvent: crossterm-agnostic event abstraction.
use crate::prelude::*;
pub mod customevent;
pub mod keyboard;
pub mod mouse;
// Top-level event for editor. Just in case in future if we had workspace feature

#[derive(Debug, Clone)]
pub enum EditorEvent {
    Key(KeyInput),
    Mouse(MouseInput),
    Resize(Size),
    Custom(CustomEvent),
    /// Crossterm emitted something we don't model yet
    Unhandled,
}

// EditorEvent::from_crossterm(event) and returns the result.

impl EditorEvent {
    // Convert a raw crossterm event into an EditorEvent.
    // we Call this only from Terminal::wait_for_event().
    #[allow(clippy::as_conversions)]
    pub fn from_crossterm(event: crossterm::event::Event) -> Self {
        use crossterm::event::{
            Event, KeyCode as CtKeyCode, KeyEventKind, MouseEventKind,
        };

        match event {
            //Keyboard
            Event::Key(key_event) => {
                if key_event.kind != KeyEventKind::Press {
                    return Self::Unhandled;
                }

                let modifiers = convert_modifiers(key_event.modifiers);

                let key_code = match key_event.code {
                    CtKeyCode::Char(c) => KeyCode::Char(c),
                    CtKeyCode::Backspace => KeyCode::Backspace,
                    CtKeyCode::Delete => KeyCode::Delete,
                    CtKeyCode::Enter => KeyCode::Enter,
                    CtKeyCode::Tab => KeyCode::Tab,
                    CtKeyCode::Esc => KeyCode::Esc,
                    CtKeyCode::Up => KeyCode::Up,
                    CtKeyCode::Down => KeyCode::Down,
                    CtKeyCode::Left => KeyCode::Left,
                    CtKeyCode::Right => KeyCode::Right,
                    CtKeyCode::Home => KeyCode::Home,
                    CtKeyCode::End => KeyCode::End,
                    CtKeyCode::PageUp => KeyCode::PageUp,
                    CtKeyCode::PageDown => KeyCode::PageDown,
                    _ => KeyCode::Other,
                };

                Self::Key(KeyInput {
                    key_code,
                    modifiers,
                })
            }

            //Mouse
            Event::Mouse(mouse_event) => {
                let position = Position {
                    row: mouse_event.row as usize,
                    col: mouse_event.column as usize,
                };

                let (button, action) = match mouse_event.kind {
                    MouseEventKind::Down(btn) => (Some(convert_button(btn)), MouseAction::Down),
                    MouseEventKind::Up(btn) => (Some(convert_button(btn)), MouseAction::Up),
                    MouseEventKind::Drag(btn) => (Some(convert_button(btn)), MouseAction::Drag),
                    MouseEventKind::ScrollUp => (None, MouseAction::ScrollUp),
                    MouseEventKind::ScrollDown => (None, MouseAction::ScrollDown),
                    _ => return Self::Unhandled,
                };

                Self::Mouse(MouseInput {
                    position,
                    button,
                    action,
                })
            }

            //Resize
            Event::Resize(width, height) => Self::Resize(Size {
                width: width as usize,
                height: height as usize,
            }),

            // everything else
            _ => Self::Unhandled,
        }
    }
}

//Helpers

fn convert_modifiers(mods: crossterm::event::KeyModifiers) -> KeyModifiers {
    KeyModifiers {
        ctrl: mods.contains(crossterm::event::KeyModifiers::CONTROL),
        shift: mods.contains(crossterm::event::KeyModifiers::SHIFT),
        alt: mods.contains(crossterm::event::KeyModifiers::ALT),
    }
}

fn convert_button(btn: crossterm::event::MouseButton) -> MouseButton {
    use crossterm::event::MouseButton as CtBtn;
    match btn {
        CtBtn::Left => MouseButton::Left,
        CtBtn::Right => MouseButton::Right,
        CtBtn::Middle => MouseButton::Middle,
    }
}

// Command conversion
// Mapped to our existing Command enum

use crate::editor::command::{Command, System};

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
