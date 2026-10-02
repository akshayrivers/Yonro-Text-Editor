// src/editor/plugins/builtin/word_count_plugin.rs
use yonro_core::events::keyboard::{KeyCode, KeyModifiers};
use yonro_core::events::EditorEvent;
use crate::layout::PaneContent;
use crate::plugins::{BufferSnapshot, Plugin, PluginResponse};
use crate::uicomponents::WordCount;
use crate::prelude::*;
use async_trait::async_trait;

pub struct WordCountPlugin {
    open_pane_id: Option<usize>,
    last_w_press: Option<std::time::Instant>,
}

impl WordCountPlugin {
    pub fn new() -> Self {
        Self { 
            open_pane_id: None,
            last_w_press: None,
        }
    }
}

impl Default for WordCountPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for WordCountPlugin {
    fn name(&self) -> &str {
        "word_count"
    }

    async fn on_load(&mut self) {}

    /// Core calls this after our OpenFloatingPane was applied.
    async fn on_pane_opened(&mut self, pane_id: usize) {
        self.open_pane_id = Some(pane_id);
    }

    /// Core calls this after our pane was closed (mouse [x], Esc, toggle).
    async fn on_pane_closed(&mut self, pane_id: usize) {
        if self.open_pane_id == Some(pane_id) {
            self.open_pane_id = None;
        }
    }

    async fn on_event(&mut self, event: &EditorEvent, active_pane_id: usize) -> Option<PluginResponse> {
        match event {
            // ── Ctrl+W W — toggle word count floating pane ─────────────────────
            EditorEvent::Key(key)
                if key.modifiers == KeyModifiers::CTRL && key.key_code == KeyCode::Char('w') =>
            {
                let now = std::time::Instant::now();
                if let Some(last) = self.last_w_press {
                    if now.duration_since(last).as_millis() < 500 {
                        // Double W within 500ms - toggle word count
                        self.last_w_press = None;
                        if let Some(pane_id) = self.open_pane_id.take() {
                            return Some(PluginResponse::ClosePane { pane_id });
                        }
                        return Some(PluginResponse::OpenFloatingPane {
                            plugin_name: self.name().to_string(),
                            content_factory: Box::new(|| {
                                PaneContent::Plugin(Box::new(WordCount::default()))
                            }),
                            rect: Rect {
                                position: Position { row: 3, col: 10 },
                                size: Size {
                                    height: 12,
                                    width: 35,
                                },
                            },
                        });
                    }
                }
                self.last_w_press = Some(now);
            }

            // Other events are only processed if the word count is currently the active pane
            _ if self.open_pane_id == Some(active_pane_id) => {
                match event {
                    EditorEvent::Key(key) => {
                        if key.modifiers == KeyModifiers::NONE {
                            match key.key_code {
                                KeyCode::Esc => {
                                    self.open_pane_id = None;
                                    return Some(PluginResponse::ClosePane { pane_id: active_pane_id });
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }

            _ => {}
        }

        None
    }

    async fn on_buffer_change(&mut self, _snapshot: BufferSnapshot) -> Option<PluginResponse> {
        // Update word count display if floating pane is open
        if let Some(_pane_id) = self.open_pane_id {
            // The editor will handle updating the WordCount component
            // We can emit a custom event or just rely on the next render
        }
        None
    }
}