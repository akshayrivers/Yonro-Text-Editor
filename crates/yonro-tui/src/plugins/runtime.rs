use super::{Plugin, PluginMessage, PluginResponse};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};

pub struct PluginRuntime {
    pub tx: Sender<PluginMessage>,
    pub rx: Receiver<PluginResponse>,
}

impl Default for PluginRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginRuntime {
    pub fn new() -> Self {
        let (core_tx, worker_rx) = mpsc::channel::<PluginMessage>();
        let (worker_tx, core_rx) = mpsc::channel::<PluginResponse>();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to build plugin tokio runtime");
            rt.block_on(plugin_worker(worker_rx, worker_tx));
        });

        Self {
            tx: core_tx,
            rx: core_rx,
        }
    }

    pub fn send(&self, msg: PluginMessage) {
        let _ = self.tx.send(msg);
    }

    pub fn drain_responses(&self) -> Vec<PluginResponse> {
        let mut responses = Vec::new();
        loop {
            match self.rx.try_recv() {
                Ok(r) => responses.push(r),
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        responses
    }

    pub fn load_plugin(&self, plugin: Box<dyn Plugin>) {
        let _ = self.tx.send(PluginMessage::LoadPlugin(plugin));
    }

    pub fn shutdown(&self) {
        let _ = self.tx.send(PluginMessage::Shutdown);
    }
}

async fn plugin_worker(rx: std::sync::mpsc::Receiver<PluginMessage>, tx: Sender<PluginResponse>) {
    let mut plugins: Vec<Box<dyn Plugin>> = Vec::new();

    loop {
        let msg = match rx.recv() {
            Ok(m) => m,
            Err(_) => break,
        };

        match msg {
            PluginMessage::LoadPlugin(mut plugin) => {
                plugin.on_load().await;
                plugins.push(plugin);
            }

            PluginMessage::Event {
                event,
                active_pane_id,
            } => {
                for plugin in &mut plugins {
                    if let Some(response) = plugin.on_event(&event, active_pane_id).await {
                        let _ = tx.send(response);
                    }
                }
            }

            PluginMessage::BufferChanged(snapshot) => {
                for plugin in &mut plugins {
                    if let Some(response) = plugin.on_buffer_change(snapshot.clone()).await {
                        let _ = tx.send(response);
                    }
                }
            }

            // Core telling a plugin that its requested pane was opened
            PluginMessage::PaneOpened {
                plugin_name,
                pane_id,
            } => {
                for plugin in &mut plugins {
                    if plugin.name() == plugin_name {
                        plugin.on_pane_opened(pane_id).await;
                    }
                }
            }

            PluginMessage::Shutdown => {
                for plugin in &mut plugins {
                    plugin.on_unload().await;
                }
                break;
            }
        }
    }
}
