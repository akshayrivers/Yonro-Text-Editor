//! Terminal input adapter: the ONLY place that knows about `crossterm` events.
//!
//! Converts raw `crossterm::event::Event` into headless
//! `yonro_core::events::EditorEvent`. Moved verbatim from
//! `yonro-core::events` during the Phase 2 workspace split so core keeps
//! zero UI dependencies (`AGENTS.md §2.2`).

use yonro_core::events::{EditorEvent, KeyCode, KeyInput, KeyModifiers, MouseAction, MouseButton, MouseInput};
use yonro_core::prelude::{Position, Size};

/// Convert a raw crossterm event into an `EditorEvent`.
/// Called only from `Terminal::wait_for_event()`.
#[allow(clippy::as_conversions)]
pub fn from_crossterm(event: crossterm::event::Event) -> EditorEvent {
    use crossterm::event::{Event, KeyCode as CtKeyCode, KeyEventKind, MouseEventKind};

    match event {
        //Keyboard
        Event::Key(key_event) => {
            if key_event.kind != KeyEventKind::Press {
                return EditorEvent::Unhandled;
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

            EditorEvent::Key(KeyInput {
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
                _ => return EditorEvent::Unhandled,
            };

            EditorEvent::Mouse(MouseInput {
                position,
                button,
                action,
            })
        }

        //Resize
        Event::Resize(width, height) => EditorEvent::Resize(Size {
            width: width as usize,
            height: height as usize,
        }),

        // everything else
        _ => EditorEvent::Unhandled,
    }
}

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
