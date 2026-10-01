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
}
