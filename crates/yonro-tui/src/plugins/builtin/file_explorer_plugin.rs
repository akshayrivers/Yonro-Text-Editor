// src/editor/plugins/builtin/file_explorer_plugin.rs
use yonro_core::command::Move;
use yonro_core::events::keyboard::{KeyCode, KeyModifiers};
use yonro_core::events::mouse::{MouseAction, MouseButton};
use yonro_core::events::EditorEvent;
use crate::layout::PaneContent;
use crate::plugins::{BufferSnapshot, Plugin, PluginResponse};
use crate::uicomponents::FileExplorer;
use crate::prelude::*;
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

    /// Core calls this after our OpenFloatingPane was applied.
    async fn on_pane_opened(&mut self, pane_id: usize) {
        self.open_pane_id = Some(pane_id);
    }

    async fn on_event(&mut self, event: &EditorEvent, active_pane_id: usize) -> Option<PluginResponse> {
        match event {
            // ── Ctrl+E — toggle ───────────────────────────────────────────
            EditorEvent::Key(key)
                if key.modifiers == KeyModifiers::CTRL && key.key_code == KeyCode::Char('e') =>
            {
                if let Some(pane_id) = self.open_pane_id.take() {
                    return Some(PluginResponse::ClosePane { pane_id });
                }
                // Open — core will call on_pane_opened with the new pane_id
                return Some(PluginResponse::OpenFloatingPane {
                    plugin_name: self.name().to_string(),
                    content_factory: Box::new(|| {
                        PaneContent::Plugin(Box::new(FileExplorer::default()))
                    }),
                    rect: Rect {
                        position: Position { row: 2, col: 4 },
                        size: Size {
                            height: 24,
                            width: 42,
                        },
                    },
                });
            }

            // Other events are only processed if the explorer is currently the active pane
            _ if self.open_pane_id == Some(active_pane_id) => {
                match event {
                    EditorEvent::Key(key) => {
                        let pane_id = self.open_pane_id.unwrap();
                        if key.modifiers == KeyModifiers::NONE {
                            match key.key_code {
                                KeyCode::Up => {
                                    return Some(PluginResponse::MoveInPane {
                                        pane_id,
                                        direction: Move::Up,
                                    });
                                }
                                KeyCode::Down => {
                                    return Some(PluginResponse::MoveInPane {
                                        pane_id,
                                        direction: Move::Down,
                                    });
                                }
                                KeyCode::Enter => {
                                    return Some(PluginResponse::SelectInPane { pane_id });
                                }
                                KeyCode::Esc => {
                                    self.open_pane_id = None;
                                    return Some(PluginResponse::ClosePane { pane_id });
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
