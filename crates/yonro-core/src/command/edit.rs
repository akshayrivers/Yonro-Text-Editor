#[derive(Clone, Copy, Debug)]
pub enum Edit {
    Insert(char),
    InsertNewLine,
    Delete,
    DeleteBackward,
}
