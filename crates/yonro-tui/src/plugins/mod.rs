// Additions needed to support FileExplorerPlugin properly:
//   1. PluginResponse gains MoveInPane, SelectInPane, MouseClickInPane
//   2. PluginMessage gains PaneOpened
//   3. Plugin trait gains on_pane_opened

pub mod builtin;
pub mod runtime;
pub use runtime::PluginRuntime;

use crate::prelude::*;
use ropey::Rope;
use std::sync::Arc;
use yonro_core::events::EditorEvent;
use async_trait::async_trait;

#[derive(Clone, Debug)]
pub struct BufferSnapshot {
    pub buffer_id: usize,
    pub rope: Arc<Rope>,
    pub file_name: Option<String>,
    pub is_dirty: bool,
}

impl BufferSnapshot {
    /// Get all lines as owned Strings (materializes from rope).
    /// Use sparingly; prefer working with `rope` directly for performance.
    pub fn lines(&self) -> Vec<String> {
        (0..self.rope.len_lines())
            .filter_map(|idx| self.rope.get_line(idx).map(|slice| slice.to_string()))
            .collect()
    }

    /// Get the full text as a single String.
    pub fn text(&self) -> String {
        self.rope.to_string()
    }
}

// PluginMessage

pub enum PluginMessage {
    LoadPlugin(Box<dyn Plugin>),
    Event {
        event: EditorEvent,
        active_pane_id: usize,
    },
    BufferChanged(BufferSnapshot),
    /// Core tells a plugin that a pane it requested was opened.
    PaneOpened {
        plugin_name: String,
        pane_id: usize,
    },
    /// Core tells a plugin that its pane was closed/hidden (sidebar toggle,
    /// mouse [x], command-bar close). Plugins must clear `open_pane_id`.
    PaneClosed {
        plugin_name: String,
        pane_id: usize,
    },
    Shutdown,
}

// PluginResponse

pub enum PluginResponse {
    /// Open a floating pane — core assigns pane_id and sends PaneOpened back.
    OpenFloatingPane {
        plugin_name: String,
        content_factory: Box<dyn FnOnce() -> crate::layout::PaneContent + Send>,
        rect: Rect,
    },
    /// Close a pane by id.
    ClosePane { pane_id: usize },
    /// Minimize/restore a floating pane.
    ToggleMinimize { pane_id: usize },
    /// Move selection in a Plugin pane (e.g. FileExplorer arrow keys).
    MoveInPane {
        pane_id: usize,
        direction: yonro_core::command::Move,
    },
    /// Trigger selection/enter in a Plugin pane.
    SelectInPane { pane_id: usize },
    /// Forward a mouse click position to a Plugin pane.
    MouseClickInPane { pane_id: usize, position: Position },
    /// Show a message in the message bar.
    UpdateMessage(String),
    /// Inject a custom event back into the next event cycle.
    EmitCustomEvent(yonro_core::events::customevent::CustomEvent),
    /// Ask the core for a fresh buffer snapshot.
    RequestSnapshot { buffer_id: usize },
    /// Toggle a sidebar (FileExplorer, WordCount, etc.)
    ToggleSidebar { kind: crate::layout::SidebarKind },
    /// Hide a sidebar without toggling (Esc, [x] button, close command).
    /// No-op when the sidebar is already hidden.
    CloseSidebar { kind: crate::layout::SidebarKind },
    /// Switch to document tab `index` (PaneBar click).
    SwitchTab { index: usize },
}

impl std::fmt::Debug for PluginResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OpenFloatingPane {
                plugin_name, rect, ..
            } => write!(f, "OpenFloatingPane({plugin_name}, {rect:?})"),
            Self::ClosePane { pane_id } => write!(f, "ClosePane({pane_id})"),
            Self::ToggleMinimize { pane_id } => write!(f, "ToggleMinimize({pane_id})"),
            Self::MoveInPane { pane_id, .. } => write!(f, "MoveInPane({pane_id})"),
            Self::SelectInPane { pane_id } => write!(f, "SelectInPane({pane_id})"),
            Self::MouseClickInPane { pane_id, position } => {
                write!(f, "MouseClickInPane({pane_id}, {position:?})")
            }
            Self::UpdateMessage(m) => write!(f, "UpdateMessage({m})"),
            Self::EmitCustomEvent(_) => write!(f, "EmitCustomEvent"),
            Self::RequestSnapshot { buffer_id } => write!(f, "RequestSnapshot({buffer_id})"),
            Self::ToggleSidebar { kind } => write!(f, "ToggleSidebar({kind:?})"),
            Self::CloseSidebar { kind } => write!(f, "CloseSidebar({kind:?})"),
            Self::SwitchTab { index } => write!(f, "SwitchTab({index})"),
        }
    }
}

// Plugin trait

#[async_trait]
pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;

    async fn on_load(&mut self) {}
    async fn on_unload(&mut self) {}

    /// Called for every EditorEvent after the core has handled it.
    async fn on_event(
        &mut self,
        _event: &EditorEvent,
        _active_pane_id: usize,
    ) -> Option<PluginResponse> {
        None
    }

    /// Called whenever a buffer changes.
    async fn on_buffer_change(&mut self, _snapshot: BufferSnapshot) -> Option<PluginResponse> {
        None
    }

    /// Called when a pane this plugin requested has been opened.
    /// The plugin should store pane_id for future close/move calls.
    async fn on_pane_opened(&mut self, _pane_id: usize) {}

    /// Called when this plugin's pane was closed or hidden.
    /// Default clears stale state; override when tracking `open_pane_id`.
    async fn on_pane_closed(&mut self, _pane_id: usize) {}
}
