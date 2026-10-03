// Manuscript outline plugin (`PLAN.md Phase 4.3`): routes keys for the
// outline sidebar. Arrow keys are sync-dispatched by `MoveHandler` (same
// single-path rule as the explorer: never emit `MoveInPane`, or every press
// moves twice). Structural adds (`a`/`c`/`s`) go through `ManuscriptAdd`.
use crate::layout::SidebarKind;
use crate::plugins::{BufferSnapshot, Plugin, PluginResponse};
use async_trait::async_trait;
use yonro_core::events::keyboard::{KeyCode, KeyModifiers};
use yonro_core::events::mouse::{MouseAction, MouseButton};
use yonro_core::events::EditorEvent;
use yonro_core::manuscript::NodeKind;

pub struct OutlinePlugin {
    open_pane_id: Option<usize>,
}

impl OutlinePlugin {
    #[must_use]
    pub fn new() -> Self {
        Self { open_pane_id: None }
    }
}

impl Default for OutlinePlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for OutlinePlugin {
    fn name(&self) -> &str {
        "manuscript"
    }

    async fn on_load(&mut self) {}

    async fn on_pane_opened(&mut self, pane_id: usize) {
        self.open_pane_id = Some(pane_id);
    }

    async fn on_pane_closed(&mut self, pane_id: usize) {
        if self.open_pane_id == Some(pane_id) {
            self.open_pane_id = None;
        }
    }

    async fn on_event(
        &mut self,
        event: &EditorEvent,
        active_pane_id: usize,
    ) -> Option<PluginResponse> {
        match event {
            // ── Ctrl+O — toggle outline sidebar ─────────────────────────────
            EditorEvent::Key(key)
                if key.modifiers == KeyModifiers::CTRL && key.key_code == KeyCode::Char('o') =>
            {
                return Some(PluginResponse::ToggleSidebar {
                    kind: SidebarKind::Outline,
                });
            }

            // Other events only when the outline is the active pane.
            _ if self.open_pane_id == Some(active_pane_id) => {
                match event {
                    EditorEvent::Key(key) => {
                        let pane_id = self.open_pane_id.unwrap();
                        if key.modifiers == KeyModifiers::NONE {
                            match key.key_code {
                                KeyCode::Enter => {
                                    return Some(PluginResponse::SelectInPane { pane_id });
                                }
                                KeyCode::Esc => {
                                    self.open_pane_id = None;
                                    return Some(PluginResponse::CloseSidebar {
                                        kind: SidebarKind::Outline,
                                    });
                                }
                                // Structural adds (core applies + selects).
                                KeyCode::Char('a') => {
                                    return Some(PluginResponse::ManuscriptAdd {
                                        child: NodeKind::Act,
                                    });
                                }
                                KeyCode::Char('c') => {
                                    return Some(PluginResponse::ManuscriptAdd {
                                        child: NodeKind::Chapter,
                                    });
                                }
                                KeyCode::Char('s') => {
                                    return Some(PluginResponse::ManuscriptAdd {
                                        child: NodeKind::Scene,
                                    });
                                }
                                // Field prompts: rename, POV, word target.
                                KeyCode::Char('r') => {
                                    return Some(PluginResponse::ManuscriptPrompt {
                                        field: crate::plugins::OutlineField::Rename,
                                    });
                                }
                                KeyCode::Char('p') => {
                                    return Some(PluginResponse::ManuscriptPrompt {
                                        field: crate::plugins::OutlineField::Pov,
                                    });
                                }
                                KeyCode::Char('t') => {
                                    return Some(PluginResponse::ManuscriptPrompt {
                                        field: crate::plugins::OutlineField::Target,
                                    });
                                }
                                // Delete removes the selected node (files kept).
                                KeyCode::Delete => {
                                    return Some(PluginResponse::ManuscriptRemove);
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
