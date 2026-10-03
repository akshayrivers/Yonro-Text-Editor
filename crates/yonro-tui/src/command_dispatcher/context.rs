use crate::{
    layout::{LayoutTree, PaneManager},
    plugins::PluginResponse,
    uicomponents::{CommandBar, MessageBar, PaneBar, UIComponent},
};
use yonro_core::buffers::BufferManager;
use crate::clipboard::Clipboard;
use crate::prelude::*;

/// Thin view into Editor's state for handlers
/// This is the ONLY thing handlers receive
pub struct EditorContext<'a> {
    pub prompt_type: &'a mut PromptType,
    pub pane_manager: &'a mut PaneManager,
    pub layout_tree: &'a mut LayoutTree,
    pub buffer_manager: &'a mut BufferManager,
    pub pane_bar: &'a mut PaneBar,
    pub command_bar: &'a mut CommandBar,
    pub message_bar: &'a mut MessageBar,
    pub terminal_size: Size,
    pub should_quit: &'a mut bool,
    pub quit_times: &'a mut u8,
    pub dragging_split: &'a mut Option<usize>,
    pub dragging_pane: &'a mut Option<usize>,
    pub drag_offset: &'a mut Position,

    pub buffer_changed: Option<usize>,
    pub plugin_responses: &'a mut Vec<PluginResponse>,
    /// System clipboard (Copy/Cut/Paste target).
    pub clipboard: &'a mut dyn Clipboard,
    /// Tracks the last focused text editor (for explorer file opens).
    /// Mirrors `Editor::last_editor_pane`; handlers must keep it fresh.
    pub last_editor_pane: &'a mut Option<usize>,
}

#[derive(Eq, PartialEq, Default, Clone, Copy)]
pub enum PromptType {
    Search,
    Save,
    #[default]
    None,
    FocusPane,
    ClosePane,
    /// Rename outline node (`r` in outline).
    Rename,
    /// Set scene POV (`p` in outline).
    OutlinePov,
    /// Set scene word target (`t` in outline).
    OutlineTarget,
}

impl PromptType {
    pub fn is_none(&self) -> bool {
        *self == Self::None
    }
}

// Convenience methods
impl<'a> EditorContext<'a> {
    pub fn update_message(&mut self, msg: &str) {
        self.message_bar.update_message(msg);
    }

    pub fn active_buffer_id(&self) -> Option<usize> {
        self.pane_manager
            .active_pane()
            .and_then(|p| p.view())
            .map(|v| v.buffer_id())
    }

    pub fn set_prompt(&mut self, prompt_type: PromptType) {
        match prompt_type {
            PromptType::None => {}
            PromptType::Save => self.command_bar.set_prompt("Save as: "),
            PromptType::Search => {
                if let Some(view) = self
                    .pane_manager
                    .active_pane_mut()
                    .and_then(|p| p.view_mut())
                {
                    view.enter_search();
                }
                self.command_bar
                    .set_prompt("Search (Esc to cancel, Arrows to navigate): ");
            }
            PromptType::FocusPane => self
                .command_bar
                .set_prompt("Pane command (focus <id>, close <id>, float, unfloat, explore): "),
            PromptType::ClosePane => self
                .command_bar
                .set_prompt("close [Pane ID] to close that pane"),
            PromptType::Rename => self.command_bar.set_prompt("Rename: "),
            PromptType::OutlinePov => self.command_bar.set_prompt("POV character: "),
            PromptType::OutlineTarget => self.command_bar.set_prompt("Target words: "),
        }
        self.command_bar.clear_value();
        *self.prompt_type = prompt_type;
    }

    pub fn mark_all_panes_for_redraw(&mut self) {
        for pane in self.pane_manager.iter_mut() {
            if let Some(view) = pane.view_mut() {
                view.mark_redraw(true);
            }
            // Sidebar + floating plugin panes must also redraw on focus/resize,
            // otherwise the explorer stays blank after `set_active_pane`.
            if let crate::layout::PaneContent::Plugin(c) = &mut pane.content {
                c.mark_redraw(true);
            }
            if let crate::layout::PaneContent::Popup(p) = &mut pane.content {
                p.mark_redraw(true);
            }
        }
    }

    pub fn sync_pane_rects(&mut self) {
        for (pane_id, rect) in self.layout_tree.collect_leaf_layouts() {
            if let Some(pane) = self.pane_manager.get_pane_mut(pane_id) {
                pane.resize(rect);
            }
        }
    }

    pub fn handle_resize(&mut self, size: Size) {
        self.terminal_size = size;
        let editor_rect = Rect {
            position: Position { row: 1, col: 0 },
            size: Size {
                height: size.height.saturating_sub(3),
                width: size.width,
            },
        };
        self.layout_tree.compute_layout(editor_rect);
        self.sync_pane_rects();

        // Bounds check for floating panes: keep them off the chrome
        // (BufferBar row 0, StatusBar height-2, CommandBar height-1).
        for pane in self.pane_manager.iter_mut() {
            if pane.is_floating {
                let mut rect = pane.component().rect();
                rect.position.col = rect
                    .position
                    .col
                    .min(size.width.saturating_sub(rect.size.width));
                let max_row = size
                    .height
                    .saturating_sub(rect.size.height.saturating_add(2))
                    .max(1);
                rect.position.row = rect.position.row.clamp(1, max_row);
                pane.resize(rect);
            }
        }

        self.mark_all_panes_for_redraw();
    }

    pub fn assign_active_pane(&mut self) {
        if let Some((pane_id, _)) = self.layout_tree.collect_leaf_layouts().first() {
            self.pane_manager.set_active_pane(*pane_id);
            return;
        }
        if let Some(floating) = self.pane_manager.get_floating_panes_sorted().first() {
            let id = floating.pane_id;
            self.pane_manager.set_active_pane(id);
        }
    }

    pub fn set_active_pane(&mut self, pane_id: usize) {
        if self
            .pane_manager
            .active_pane()
            .map(|p| p.pane_id == pane_id)
            .unwrap_or(false)
        {
            return;
        }
        // Remember text editors when focus leaves them, so explorer file
        // opens split the pane the user was just in (mouse clicks included).
        if let Some(current) = self.pane_manager.active_pane() {
            if current.view().is_some() && !current.is_floating {
                *self.last_editor_pane = Some(current.pane_id);
            }
        }
        self.pane_manager.set_active_pane(pane_id);
        self.pane_manager.bring_to_front(pane_id);
        self.mark_all_panes_for_redraw();
    }
    pub fn notify_buffer_changed(&mut self, buffer_id: usize) {
        self.buffer_changed = Some(buffer_id);
    }
}
