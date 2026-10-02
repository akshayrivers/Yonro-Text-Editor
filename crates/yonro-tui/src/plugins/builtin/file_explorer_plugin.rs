// src/editor/plugins/builtin/file_explorer_plugin.rs
use yonro_core::events::keyboard::{KeyCode, KeyModifiers};
use yonro_core::events::mouse::{MouseAction, MouseButton};
use yonro_core::events::EditorEvent;
use crate::layout::SidebarKind;
use crate::plugins::{BufferSnapshot, Plugin, PluginResponse};
use async_trait::async_trait;

pub struct FileExplorerPlugin {
    open_pane_id: Option<usize>,
}

impl FileExplorerPlugin {
    pub fn new() -> Self {
        Self { open_pane_id: None }
    }
}

impl Default for FileExplorerPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for FileExplorerPlugin {
    fn name(&self) -> &str {
        "file_explorer"
    }

    async fn on_load(&mut self) {}

    /// Core calls this after our sidebar pane was created.
    async fn on_pane_opened(&mut self, pane_id: usize) {
        self.open_pane_id = Some(pane_id);
    }

    /// Core calls this after our sidebar was hidden/closed.
    /// Clears stale state so the next `Ctrl+E` reopens instead of no-op.
    async fn on_pane_closed(&mut self, pane_id: usize) {
        if self.open_pane_id == Some(pane_id) {
            self.open_pane_id = None;
        }
    }

    async fn on_event(&mut self, event: &EditorEvent, active_pane_id: usize) -> Option<PluginResponse> {
        match event {
            // ── Ctrl+E — toggle sidebar ───────────────────────────────────────
            EditorEvent::Key(key)
                if key.modifiers == KeyModifiers::CTRL && key.key_code == KeyCode::Char('e') =>
            {
                return Some(PluginResponse::ToggleSidebar { kind: SidebarKind::FileExplorer });
            }

            // Other events are only processed if the explorer is currently the active pane
            _ if self.open_pane_id == Some(active_pane_id) => {
                match event {
                    EditorEvent::Key(key) => {
                        let pane_id = self.open_pane_id.unwrap();
                        if key.modifiers == KeyModifiers::NONE {
                            match key.key_code {
                                // NOTE: Up/Down are deliberately NOT answered here.
                                // `MoveHandler` already forwards arrows synchronously
                                // to the active plugin pane (`PLAN.md Phase 3.3`
                                // direct dispatch); emitting `MoveInPane` as well
                                // would move the selection TWICE per keypress.
                                KeyCode::Enter => {
                                    return Some(PluginResponse::SelectInPane { pane_id });
                                }
                                KeyCode::Esc => {
                                    self.open_pane_id = None;
                                    return Some(PluginResponse::CloseSidebar { kind: SidebarKind::FileExplorer });
                                }
                                _ => {}
                            }
                        }
                    }

                    EditorEvent::Mouse(mouse)
                        if mouse.action == MouseAction::Down
                            && mouse.button == Some(MouseButton::Left) =>
                    {
                        return Some(PluginResponse::MouseClickInPane {
                            pane_id: self.open_pane_id.unwrap(),
                            position: mouse.position,
                        });
                    }

                    _ => {}
                }
            }

            _ => {}
        }

        None
    }

    async fn on_buffer_change(&mut self, _snapshot: BufferSnapshot) -> Option<PluginResponse> {
        None
    }
}
