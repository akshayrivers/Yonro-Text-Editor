use crate::command::{Command, MouseCommand};
use crate::prelude::*;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseAction {
    Down,
    Up,
    Drag,
    ScrollUp,
    ScrollDown,
}

#[derive(Debug, Clone, Copy)]
pub struct MouseInput {
    pub position: Position,
    pub button: Option<MouseButton>, // None for scroll events
    pub action: MouseAction,
}

pub fn mouse_to_command(mouse: MouseInput) -> Result<Command, String> {
    let MouseInput {
        position,
        button,
        action,
    } = mouse;

    let cmd = match (button, action) {
        (Some(MouseButton::Left), MouseAction::Down) => MouseCommand::LeftClick(position),
        (Some(MouseButton::Left), MouseAction::Drag) => MouseCommand::LeftDrag(position),
        (Some(MouseButton::Left), MouseAction::Up) => MouseCommand::LeftRelease(position),
        (None, MouseAction::ScrollUp) => MouseCommand::ScrollUp(position),
        (None, MouseAction::ScrollDown) => MouseCommand::ScrollDown(position),
        _ => return Err(format!("Unhandled mouse event {mouse:?}")),
    };

    Ok(Command::Mouse(cmd))
}
