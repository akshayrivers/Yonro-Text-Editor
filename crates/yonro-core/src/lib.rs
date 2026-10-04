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
pub mod api;
pub mod buffers;
pub mod command;
pub mod documentstatus;
pub mod events;
pub mod fileinfo;
pub mod filetype;
pub mod graph;
pub mod highlighter;
pub mod history;
pub mod line;
pub mod lore;
pub mod manuscript;
pub mod prelude;
pub mod project;
pub mod recents;
pub mod search;
pub mod timeline;

pub use annotatedstring::AnnotatedString;
pub use annotation::Annotation;
pub use annotationtype::AnnotationType;
pub use buffers::{Buffer, BufferManager};
pub use command::{Command, Edit, MouseCommand, Move, System};
pub use documentstatus::DocumentStatus;
pub use events::EditorEvent;
pub use fileinfo::FileInfo;
pub use filetype::FileType;
pub use graph::{Edge, EdgeKind, Graph, GraphError};
pub use highlighter::{
    Highlighter, MarkDownSyntaxHighlighter, RustSyntaxHighlighter, SearchResultHighlighter,
    SyntaxHighlighter, TextSyntaxHighlighter,
};
pub use history::UndoStack;
pub use line::Line;
pub use lore::{is_mention_char, Entity, EntityId, EntityKind, LoreBook, LoreError, Mention};
pub use manuscript::{Manuscript, ManuscriptError, Node, NodeId, NodeKind, SceneMeta};
pub use project::{Project, ProjectError, SceneMetaFields};
pub use recents::{RecentEntry, Recents, RecentsError, MAX_RECENTS};
pub use search::{ProjectHit, Utf16Span, EXCERPT_CHARS, MAX_PROJECT_HITS};
pub use timeline::{ContinuityNote, Timeline, TimelineEntry};
