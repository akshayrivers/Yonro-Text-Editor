#![warn(
    clippy::all,
    clippy::pedantic,
    clippy::print_stdout,
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::integer_division
)]

//! `yonro-tui`: terminal frontend over `yonro-core`.
//!
//! Owns everything `crossterm`-adjacent: raw terminal, input adapter,
//! layout/panes, rendering components, and the editor event loop.

pub mod command_dispatcher;
pub mod editor;
pub mod input;
pub mod layout;
pub mod plugins;
pub mod terminal;
pub mod uicomponents;

// Re-export the core prelude under the old path so moved files keep working.
pub use yonro_core::prelude;

pub use editor::Editor;
pub use uicomponents::view::EditOperation;
pub use yonro_core::{
    AnnotatedString, Annotation, AnnotationType, Buffer, BufferManager, Command, DocumentStatus,
    Edit, EditorEvent, FileInfo, FileType, Highlighter, Line, MarkDownSyntaxHighlighter,
    MouseCommand, Move, RustSyntaxHighlighter, SearchResultHighlighter, SyntaxHighlighter, System,
    TextSyntaxHighlighter,
};
