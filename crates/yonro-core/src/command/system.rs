use crate::prelude::*;
#[derive(Clone, Copy, Debug)]
pub enum System {
    Save,
    Resize(Size),
    Quit,
    Dismiss,
    Search,
    Undo,
    Redo,
    SplitHorizontal,
    SplitVertical,
    OpenCommandBar,
    /// Distraction-free Zen mode: document only, centered column,
    /// typewriter scrolling (`PLAN.md Phase 4.2`).
    ZenToggle,
}
