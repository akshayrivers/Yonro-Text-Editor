#[derive(Clone, Copy, Debug)]
pub enum Edit {
    Insert(char),
    InsertNewLine,
    Delete,
    DeleteBackward,
    /// Copy current line to the clipboard (`PLAN.md Phase 4.4`).
    Copy,
    /// Cut current line to the clipboard.
    Cut,
    /// Paste clipboard contents at the cursor.
    Paste,
}
