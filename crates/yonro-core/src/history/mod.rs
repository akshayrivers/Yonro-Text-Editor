//! Coalescing undo (`P0.5`): one undo step per typing burst, not per keystroke.
//!
//! `UndoStack` stores whole-text snapshots (like the GUI's v1 undo) but
//! coalesces bursts: `record` pushes the `before` text only when the previous
//! edit is older than 500ms. History is capped by total bytes (32 MiB),
//! evicting the oldest entries — never by count, so a novel's long session
//! stays usable.

use std::time::Instant;

/// Total snapshot budget (32 MiB).
const MAX_BYTES: usize = 33_554_432;

/// Coalescing whole-text undo history.
#[derive(Debug, Clone, Default)]
pub struct UndoStack {
    /// Older `before` snapshots, oldest first.
    pub undo: Vec<String>,
    /// Redo snapshots, newest last.
    pub redo: Vec<String>,
    /// Time of the last recorded edit (`None` after undo/redo/fresh).
    pub last_edit: Option<Instant>,
    /// Total bytes held in `undo` + `redo`.
    pub bytes: usize,
}

impl UndoStack {
    /// Empty history.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an edit that replaced `before` with the current text.
    ///
    /// Pushes `before` only when no edit happened within the last 500ms
    /// (burst coalescing); always stamps `last_edit` with `now` and clears
    /// the redo stack. Evicts the oldest entries while over the byte cap.
    pub fn record(&mut self, before: &str, now: Instant) {
        let is_new_step = self
            .last_edit
            .is_none_or(|last| now.saturating_duration_since(last).as_millis() > 500);
        // A new edit invalidates redo even when coalesced.
        for entry in self.redo.drain(..) {
            self.bytes = self.bytes.saturating_sub(entry.len());
        }
        if is_new_step {
            self.bytes = self.bytes.saturating_add(before.len());
            self.undo.push(before.to_string());
            self.evict_oldest();
        }
        self.last_edit = Some(now);
    }

    /// Undo one step, returning the text to restore.
    ///
    /// Pushes `current` onto redo so `redo` can reverse it. Resets the
    /// burst timer so the next edit starts a fresh step.
    #[must_use]
    pub fn undo(&mut self, current: &str) -> Option<String> {
        let prev = self.undo.pop()?;
        self.bytes = self.bytes.saturating_sub(prev.len());
        self.bytes = self.bytes.saturating_add(current.len());
        self.redo.push(current.to_string());
        self.last_edit = None;
        Some(prev)
    }

    /// Redo one step, returning the text to restore.
    #[must_use]
    pub fn redo(&mut self, current: &str) -> Option<String> {
        let next = self.redo.pop()?;
        self.bytes = self.bytes.saturating_sub(next.len());
        self.bytes = self.bytes.saturating_add(current.len());
        self.undo.push(current.to_string());
        self.evict_oldest();
        self.last_edit = None;
        Some(next)
    }

    fn evict_oldest(&mut self) {
        while self.bytes > MAX_BYTES && (!self.undo.is_empty() || !self.redo.is_empty()) {
            if self.undo.is_empty() {
                let oldest = self.redo.remove(0);
                self.bytes = self.bytes.saturating_sub(oldest.len());
            } else {
                let oldest = self.undo.remove(0);
                self.bytes = self.bytes.saturating_sub(oldest.len());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn instant_at(millis: u64) -> Instant {
        Instant::now()
            .checked_add(Duration::from_millis(millis))
            .unwrap_or_else(Instant::now)
    }

    #[test]
    fn burst_coalesces_into_one_undo_step() {
        let mut history = UndoStack::new();
        let base = instant_at(0);
        history.record("a", base);
        for i in 1..100 {
            let now = base.checked_add(Duration::from_millis(i)).unwrap_or(base);
            history.record(&format!("a{i}"), now);
        }
        assert_eq!(history.undo.len(), 1);
        assert_eq!(history.undo[0], "a");
    }

    #[test]
    fn pause_over_500ms_starts_new_step() {
        let mut history = UndoStack::new();
        let base = instant_at(0);
        history.record("a", base);
        let later = base.checked_add(Duration::from_millis(600)).unwrap_or(base);
        history.record("b", later);
        assert_eq!(history.undo.len(), 2);
    }

    #[test]
    fn redo_cleared_on_new_edit() {
        let mut history = UndoStack::new();
        let base = instant_at(0);
        history.record("a", base);
        assert_eq!(history.undo("b"), Some("a".to_string()));
        assert_eq!(history.redo.len(), 1);
        let later = base.checked_add(Duration::from_secs(1)).unwrap_or(base);
        history.record("b", later);
        assert!(history.redo.is_empty());
    }

    #[test]
    fn undo_redo_round_trip() {
        let mut history = UndoStack::new();
        let base = instant_at(0);
        history.record("a", base);
        assert_eq!(history.undo("b"), Some("a".to_string()));
        assert_eq!(history.redo("a"), Some("b".to_string()));
        assert_eq!(history.undo("b"), Some("a".to_string()));
        assert!(history.undo("a").is_none());
    }

    #[test]
    fn byte_cap_evicts_oldest() {
        let mut history = UndoStack::new();
        let base = instant_at(0);
        // Each step is 1 MiB; space edits >500ms apart so every record pushes.
        let chunk = "x".repeat(1024 * 1024);
        for i in 0_u64..40_u64 {
            let now = base
                .checked_add(Duration::from_millis(i.saturating_mul(600)))
                .unwrap_or(base);
            history.record(&format!("{chunk}{i}"), now);
        }
        assert!(history.bytes <= MAX_BYTES);
        // Oldest evicted: first entry is not the original chunk-0.
        assert!(!history.undo.iter().any(|entry| entry.ends_with('0')
            && entry.len() == chunk.len().saturating_add(1)
            && entry.starts_with('x')));
        assert!(history.undo.len() < 40);
    }
}
