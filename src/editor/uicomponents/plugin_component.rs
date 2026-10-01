// src/editor/uicomponents/plugin_component.rs
//
// PluginComponent — extends UIComponent for pane contents that
// handle input. Only plugin pane contents implement this.
// View, CommandBar, StatusBar etc. are untouched.

use super::UIComponent;
use crate::editor::command::Move;
use crate::prelude::*;

/// What a mouse click on a plugin component resolved to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickAction {
    /// User clicked the close [x] button.
    Close,
    /// User clicked the minimize [-] button.
    Minimize,
    /// Normal click inside content (focus / select row etc.)
    None,
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
}
