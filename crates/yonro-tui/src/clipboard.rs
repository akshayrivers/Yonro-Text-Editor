//! System clipboard (`PLAN.md Phase 4.4`): cross-platform Copy/Cut/Paste.
//!
//! `Clipboard` is a tiny trait so views stay testable without a display
//! server: production uses [`SystemClipboard`] (arboard with an in-memory
//! fallback when no clipboard is available, e.g. headless SSH), while unit
//! tests and benchmarks use [`MemClipboard`].

/// Minimal clipboard backend: whole-text get/set.
pub trait Clipboard {
    /// Current contents, if any.
    fn get(&mut self) -> Option<String>;
    /// Replace contents.
    fn set(&mut self, text: String);
}

/// Production clipboard: OS clipboard first, memory fallback always.
///
/// `arboard::Clipboard::new()` fails without a display server/clipboard
/// daemon — instead of erroring (which would break editing over SSH), we
/// silently degrade to an in-memory clipboard for the session.
pub struct SystemClipboard {
    system: Option<arboard::Clipboard>,
    fallback: Option<String>,
}

impl SystemClipboard {
    #[must_use]
    pub fn new() -> Self {
        Self {
            system: arboard::Clipboard::new().ok(),
            fallback: None,
        }
    }
}

impl Default for SystemClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl Clipboard for SystemClipboard {
    fn get(&mut self) -> Option<String> {
        if let Some(system) = self.system.as_mut() {
            if let Ok(text) = system.get_text() {
                return Some(text);
            }
        }
        self.fallback.clone()
    }

    fn set(&mut self, text: String) {
        if let Some(system) = self.system.as_mut() {
            if system.set_text(text.clone()).is_ok() {
                return;
            }
        }
        self.fallback = Some(text);
    }
}

/// In-memory clipboard for tests and benchmarks.
#[derive(Debug, Default, Clone)]
pub struct MemClipboard {
    text: Option<String>,
}

impl MemClipboard {
    #[must_use]
    pub fn with_text(text: &str) -> Self {
        Self {
            text: Some(text.to_string()),
        }
    }

    #[must_use]
    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }
}

impl Clipboard for MemClipboard {
    fn get(&mut self) -> Option<String> {
        self.text.clone()
    }

    fn set(&mut self, text: String) {
        self.text = Some(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mem_clipboard_roundtrip() {
        let mut clip = MemClipboard::default();
        assert!(clip.get().is_none());
        clip.set("hello".to_string());
        assert_eq!(clip.get().as_deref(), Some("hello"));
    }

    #[test]
    fn system_clipboard_never_panics_headless() {
        // Must not panic even with no display server; worst case it
        // degrades to the in-memory fallback.
        let mut clip = SystemClipboard::new();
        clip.set("probe".to_string());
        assert_eq!(clip.get().as_deref(), Some("probe"));
    }
}
