#![warn(
    clippy::all,
    clippy::pedantic,
    clippy::print_stdout,
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::integer_division
)]

//! `yonro-core`: headless narrative text engine.
//!
//! Zero UI dependencies: no `crossterm`, no terminal, no rendering.
//! Frontends (`yonro-tui`, future `yonro-gui`) are thin clients over this API.

pub mod annotatedstring;
pub mod annotation;
pub mod annotationtype;
pub mod buffers;
pub mod command;
pub mod documentstatus;
pub mod events;
pub mod fileinfo;
pub mod filetype;
pub mod highlighter;
pub mod line;
pub mod prelude;

pub use annotatedstring::AnnotatedString;
pub use annotation::Annotation;
pub use annotationtype::AnnotationType;
pub use buffers::{Buffer, BufferManager};
pub use command::{Command, Edit, MouseCommand, Move, System};
pub use documentstatus::DocumentStatus;
pub use events::EditorEvent;
pub use fileinfo::FileInfo;
pub use filetype::FileType;
pub use highlighter::{
    Highlighter, MarkDownSyntaxHighlighter, RustSyntaxHighlighter, SearchResultHighlighter,
    SyntaxHighlighter, TextSyntaxHighlighter,
};
pub use line::Line;
