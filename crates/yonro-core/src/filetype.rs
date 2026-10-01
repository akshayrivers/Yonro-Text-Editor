use std::fmt::{Display, Formatter, Result};

#[derive(Default, Eq, PartialEq, Debug, Copy, Clone)]
pub enum FileType {
    Rust,
    #[default]
    Text,
    MarkDown,
}

impl Display for FileType {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> Result {
        match self {
            Self::Rust => write!(formatter, "Rust"),
            Self::Text => write!(formatter, "Text"),
            Self::MarkDown => write!(formatter, "Markdown"),
        }
    }
}
