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
    /// Function key by number (F1 == `F(1)`). Only mapped variants are used.
    F(u8),
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
            // Clipboard (`PLAN.md Phase 4.4`): plain `Edit` commands.
            // Raw mode already disables SIGINT, so `Ctrl+C` is safe to take.
            match c {
                'c' => return Ok(Command::Edit(Edit::Copy)),
                'x' => return Ok(Command::Edit(Edit::Cut)),
                'v' => return Ok(Command::Edit(Edit::Paste)),
                _ => {}
            }
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

    // ── Function keys ───────────────────────────────────────────────────
    // F11 toggles Zen mode (no modifiers). Other function keys are unbound.
    if let KeyCode::F(n) = key_code {
        if modifiers == KeyModifiers::NONE && n == 11 {
            return Ok(Command::System(System::ZenToggle));
        }
        return Err(format!("Unbound key F{n} with {modifiers:?}"));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f11_maps_to_zen_toggle() {
        let key = KeyInput {
            key_code: KeyCode::F(11),
            modifiers: KeyModifiers::NONE,
        };
        assert!(matches!(
            key_to_command(key),
            Ok(Command::System(System::ZenToggle))
        ));
    }

    #[test]
    fn other_function_keys_are_unbound() {
        let key = KeyInput {
            key_code: KeyCode::F(5),
            modifiers: KeyModifiers::NONE,
        };
        assert!(key_to_command(key).is_err());
    }

    #[test]
    fn ctrl_e_remains_unbound_for_plugins() {
        let key = KeyInput {
            key_code: KeyCode::Char('e'),
            modifiers: KeyModifiers::CTRL,
        };
        assert!(key_to_command(key).is_err());
    }
}
