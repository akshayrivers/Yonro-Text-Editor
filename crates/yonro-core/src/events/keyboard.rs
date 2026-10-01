use crate::command::{Command, Edit, Move, System};

/// Mirrors crossterm's KeyCode but owned by us.
/// Only the variants we currently use are listed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    Char(char),
    Backspace,
    Delete,
    Enter,
    Tab,
    Esc,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    // Catch-all for anything we don't handle yet
    Other,
}

/// Mirrors crossterm's KeyModifiers as a simple bitflag-style struct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeyModifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl KeyModifiers {
    pub const NONE: Self = Self {
        ctrl: false,
        shift: false,
        alt: false,
    };
    pub const CTRL: Self = Self {
        ctrl: true,
        shift: false,
        alt: false,
    };
    pub const SHIFT: Self = Self {
        ctrl: false,
        shift: true,
        alt: false,
    };
}

#[derive(Debug, Clone, Copy)]
pub struct KeyInput {
    pub key_code: KeyCode,
    pub modifiers: KeyModifiers,
}

// Command conversion
// Mapped to our existing Command enum

pub fn key_to_command(key: KeyInput) -> Result<Command, String> {
    let KeyInput {
        key_code,
        modifiers,
    } = key;

    //Ctrl combos
    if modifiers.ctrl && !modifiers.shift && !modifiers.alt {
        if let KeyCode::Char(c) = key_code {
            let system = match c {
                'q' => System::Quit,
                's' => System::Save,
                'f' => System::Search,
                'z' => System::Undo,
                'r' => System::Redo,
                'h' => System::SplitHorizontal,
                'v' => System::SplitVertical,
                ' ' => System::OpenCommandBar,
                _ => return Err(format!("Unbound Ctrl+{c}")),
            };
            return Ok(Command::System(system));
        }
        return Err(format!("Unbound Ctrl+{key_code:?}"));
    }

    // ── Esc ───────────────────────────────────────────────────────────────
    if key_code == KeyCode::Esc && modifiers == KeyModifiers::NONE {
        return Ok(Command::System(System::Dismiss));
    }

    // ── Movement ─────────────────────────────────────────────────────────
    if modifiers == KeyModifiers::NONE {
        let move_cmd = match key_code {
            KeyCode::Up => Some(Move::Up),
            KeyCode::Down => Some(Move::Down),
            KeyCode::Left => Some(Move::Left),
            KeyCode::Right => Some(Move::Right),
            KeyCode::PageUp => Some(Move::PageUp),
            KeyCode::PageDown => Some(Move::PageDown),
            KeyCode::Home => Some(Move::StartOfLine),
            KeyCode::End => Some(Move::EndOfLine),
            _ => None,
        };
        if let Some(mv) = move_cmd {
            return Ok(Command::Move(mv));
        }
    }

    // ── Editing ───────────────────────────────────────────────────────────
    let no_mod = modifiers == KeyModifiers::NONE;
    let shift_only = modifiers == KeyModifiers::SHIFT;

    let edit_cmd = match key_code {
        KeyCode::Char(c) if no_mod || shift_only => Edit::Insert(c),
        KeyCode::Tab if no_mod => Edit::Insert('\t'),
        KeyCode::Enter if no_mod => Edit::InsertNewLine,
        KeyCode::Backspace if no_mod => Edit::DeleteBackward,
        KeyCode::Delete if no_mod => Edit::Delete,
        _ => return Err(format!("Unbound key {key_code:?} with {modifiers:?}")),
    };

    Ok(Command::Edit(edit_cmd))
}
