// src/editor/uicomponents/plugin_component.rs
//
// PluginComponent — extends UIComponent for pane contents that
// handle input. Only plugin pane contents implement this.
// View, CommandBar, StatusBar etc. are untouched.

use super::UIComponent;
use yonro_core::command::Move;
use yonro_core::buffers::Buffer;
use crate::prelude::*;
use std::io::Error;

/// What a mouse click on a plugin component resolved to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickAction {
    /// User clicked the close [x] button.
    Close,
    /// User clicked the minimize [-] button.
    Minimize,
    /// Normal click inside content (focus / select row etc.)
    None,
    /// Double-click on a file entry - open the file
    DoubleClick,
}

/// A UIComponent that also handles keyboard navigation and mouse clicks.
/// Implemented by FileExplorer and future plugin pane types.
/// Never implemented by View, CommandBar, StatusBar, etc.
pub trait PluginComponent: UIComponent + Send {
    /// Arrow-key navigation inside the component.
    fn handle_move(&mut self, direction: Move);

    /// Enter / selection action.
    fn handle_select(&mut self) -> Option<std::path::PathBuf>;

    /// Mouse click at `position` — returns what the click resolved to.
    fn handle_click(&mut self, position: Position) -> ClickAction;

    /// Set whether this component is active/focused.
    fn set_active(&mut self, active: bool);

    /// Render only the content area (no border/title bar).
    /// Called by Pane for tiled panes; floating panes use full render().
    /// Default implementation does nothing (for backward compatibility).
    fn render_content(&mut self, _rect: Rect) -> Result<(), Error> {
        Ok(())
    }

    /// Update component state from a buffer (e.g., word count, syntax info).
    /// Called when the associated buffer changes.
    /// Default implementation does nothing.
    fn update_from_buffer(&mut self, _buffer: &Buffer) {
    }
}
