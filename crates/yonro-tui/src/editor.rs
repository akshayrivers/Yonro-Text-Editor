// Editor event loop (TUI shell over `yonro-core`).
use crate::prelude::*;
use std::{
    env,
    io::Error,
    panic::{set_hook, take_hook},
};
use yonro_core::{
    Buffer, BufferManager, Command, DocumentStatus, EditorEvent, FileType, System,
};

pub use crate::command_dispatcher::{EditorContext, HandlerRegistry, PromptType};
pub use crate::layout::{
    LayoutNode, LayoutTree, Pane, PaneContent, PaneManager, SplitDirection, SplitHandle,
};
pub use crate::plugins::{
    builtin::FileExplorerPlugin, BufferSnapshot, Plugin, PluginMessage, PluginResponse,
    PluginRuntime,
};
pub use crate::terminal::Terminal;
pub use crate::uicomponents::{
    view::EditOperation, BufferBar, ClickAction, CommandBar, FileExplorer, MessageBar, StatusBar,
    UIComponent, View,
};
pub use yonro_core::{
    MarkDownSyntaxHighlighter, RustSyntaxHighlighter, SearchResultHighlighter, SyntaxHighlighter,
    TextSyntaxHighlighter,
};

pub struct Editor {
    should_quit: bool,
    layout_tree: LayoutTree,
    pane_manager: PaneManager,
    buffer_manager: BufferManager,

    /// The async plugin runtime — runs on its own thread.
    plugin_runtime: PluginRuntime,

    buffer_bar: BufferBar,
    status_bar: StatusBar,
    message_bar: MessageBar,
    command_bar: CommandBar,
    prompt_type: PromptType,
    terminal_size: Size,
    title: String,
    quit_times: u8,
    dragging_split: Option<usize>,
    dragging_pane: Option<usize>,
    drag_offset: Position,
    command_handler: HandlerRegistry,

    /// Custom events emitted by plugins, injected into the next cycle.
    pending_events: Vec<EditorEvent>,
}

impl Editor {
    pub fn new() -> Result<Self, Error> {
        let current_hook = take_hook();
        set_hook(Box::new(move |panic_info| {
            let _ = Terminal::terminate();
            current_hook(panic_info);
        }));

        Terminal::initialize()?;

        let terminal_size = Terminal::size().unwrap_or_default();

        let root_rect = Rect {
            position: Position { row: 1, col: 0 },
            size: Size {
                height: terminal_size.height.saturating_sub(3),
                width: terminal_size.width,
            },
        };

        let mut buffer_manager = BufferManager::new();
        let initial_buffer_id = buffer_manager.add(Buffer::default());

        let initial_pane_id = 0;
        let mut initial_view = View::default();
        initial_view.set_id(initial_pane_id);
        initial_view.set_buffer_id(initial_buffer_id);

        let initial_pane = Pane {
            pane_id: initial_pane_id,
            content: PaneContent::TextView(initial_view),
            active: true,
            is_floating: false,
            z_index: 0,
            is_minimized: false,
            rect: root_rect,
        };

        let pane_manager = PaneManager::new(initial_pane);
        let layout_tree = LayoutTree::new(0, root_rect);

        // Spin up plugin runtime and register built-in plugins
        let plugin_runtime = PluginRuntime::new();
        plugin_runtime.load_plugin(Box::new(FileExplorerPlugin::new()));

        let mut editor = Self {
            should_quit: false,
            layout_tree,
            pane_manager,
            buffer_manager,
            plugin_runtime,
            buffer_bar: BufferBar::default(),
            status_bar: StatusBar::default(),
            message_bar: MessageBar::default(),
            command_bar: CommandBar::default(),
            prompt_type: PromptType::None,
            terminal_size,
            title: String::new(),
            quit_times: 0,
            dragging_split: None,
            dragging_pane: None,
            drag_offset: Position::default(),
            command_handler: HandlerRegistry::default(),
            pending_events: Vec::new(),
        };

        editor.handle_resize_command(terminal_size);
        editor.update_message(
            "HELP: Ctrl-F = find | Ctrl-S = save | Ctrl-Q = quit | Ctrl-E = explorer",
        );

        let args: Vec<String> = env::args().collect();
        if let Some(file_name) = args.get(1) {
            debug_assert!(!file_name.is_empty());
            match Buffer::load(file_name) {
                Ok(buffer) => {
                    let buffer_id = editor.buffer_manager.add(buffer);
                    if let Some(view) = editor
                        .pane_manager
                        .active_pane_mut()
                        .and_then(|p| p.view_mut())
                    {
                        view.set_buffer_id(buffer_id);
                    }
                }
                Err(_) => {
                    editor.update_message(&format!("ERR: Could not open file: {file_name}"));
                }
            }
        }

        editor.refresh_status();
        Ok(editor)
    }

    // Event loop

    pub fn run(&mut self) {
        loop {
            // 1. Apply plugin responses from last cycle
            let responses = self.plugin_runtime.drain_responses();
            for response in responses {
                self.apply_plugin_response(response);
            }

            // 2. Inject pending custom events from plugins
            let pending = std::mem::take(&mut self.pending_events);
            for event in pending {
                self.handle_event(event);
            }

            // 3. Render
            self.refresh_screen();
            if self.should_quit {
                break;
            }

            // 4. Wait for next input event
            match Terminal::wait_for_event() {
                Ok(event) => {
                    // Clone for plugins before core consumes
                    let event_for_plugins = event.clone();
                    self.handle_event(event);
                    let active_pane_id = self.pane_manager.active_pane().map(|p| p.pane_id).unwrap_or(0);
                    // Fire and forget to plugin runtime
                    self.plugin_runtime
                        .send(PluginMessage::Event {
                            event: event_for_plugins,
                            active_pane_id,
                        });
                }
                Err(err) => {
                    #[cfg(debug_assertions)]
                    panic!("Could not read event: {err:?}");
                }
            }

            self.refresh_status();
        }
    }

    /// Handle one EditorEvent through the core dispatcher.
    fn handle_event(&mut self, event: EditorEvent) {
        if let Ok(command) = Command::try_from(event) {
            // Resize needs to update UI bars too, not just layout
            if let Command::System(System::Resize(size)) = command {
                self.handle_resize_command(size);
            }

            let mut handler = std::mem::take(&mut self.command_handler);
            let mut ctx = self.make_context();
            let _ = handler.dispatch(&command, &mut ctx);

            // If a buffer changed, notify plugins
            let buffer_changed = ctx.buffer_changed.take();
            self.command_handler = handler;

            if let Some(buffer_id) = buffer_changed {
                if let Some(snapshot) = self.make_buffer_snapshot(buffer_id) {
                    self.plugin_runtime
                        .send(PluginMessage::BufferChanged(snapshot));
                }
            }
        }
    }

    fn make_context(&'_ mut self) -> EditorContext<'_> {
        EditorContext {
            prompt_type: &mut self.prompt_type,
            pane_manager: &mut self.pane_manager,
            layout_tree: &mut self.layout_tree,
            buffer_manager: &mut self.buffer_manager,
            buffer_bar: &mut self.buffer_bar,
            command_bar: &mut self.command_bar,
            message_bar: &mut self.message_bar,
            terminal_size: self.terminal_size,
            should_quit: &mut self.should_quit,
            quit_times: &mut self.quit_times,
            dragging_split: &mut self.dragging_split,
            dragging_pane: &mut self.dragging_pane,
            drag_offset: &mut self.drag_offset,
            buffer_changed: None,
        }
    }

    fn make_buffer_snapshot(&self, buffer_id: usize) -> Option<BufferSnapshot> {
        let buffer = self.buffer_manager.get(buffer_id)?;
        Some(BufferSnapshot {
            buffer_id,
            lines: buffer.lines_as_strings(),
            file_name: buffer
                .get_file_info()
                .get_path()
                .and_then(|p| p.to_str())
                .map(|s| s.to_string()),
            is_dirty: buffer.is_dirty(),
        })
    }

    // Apply plugin responses

    fn apply_plugin_response(&mut self, response: PluginResponse) {
        match response {
            PluginResponse::OpenFloatingPane {
                plugin_name,
                content_factory,
                rect,
            } => {
                let content = content_factory();
                let pane_id = self.pane_manager.create_floating_pane(content, 10);
                if let Some(pane) = self.pane_manager.get_pane_mut(pane_id) {
                    pane.resize(rect);
                }
                self.plugin_runtime.send(PluginMessage::PaneOpened {
                    plugin_name,
                    pane_id,
                });
            }

            PluginResponse::ClosePane { pane_id } => {
                let is_floating = self
                    .pane_manager
                    .get_pane(pane_id)
                    .map_or(false, |p| p.is_floating);

                let was_active = self
                    .pane_manager
                    .active_pane()
                    .map(|p| p.pane_id == pane_id)
                    .unwrap_or(false);

                if is_floating {
                    self.pane_manager.remove_pane(pane_id);
                } else if self.layout_tree.remove_node(pane_id).is_ok() {
                    self.pane_manager.remove_pane(pane_id);
                    self.handle_resize_command(self.terminal_size);
                }

                if was_active {
                    // Re-focus first available tiled pane
                    if let Some((id, _)) = self.layout_tree.collect_leaf_layouts().first() {
                        self.pane_manager.set_active_pane(*id);
                    }
                }
            }

            PluginResponse::UpdateMessage(msg) => {
                self.message_bar.update_message(&msg);
            }

            PluginResponse::EmitCustomEvent(custom) => {
                self.pending_events.push(EditorEvent::Custom(custom));
            }

            PluginResponse::RequestSnapshot { buffer_id } => {
                if let Some(snapshot) = self.make_buffer_snapshot(buffer_id) {
                    self.plugin_runtime
                        .send(PluginMessage::BufferChanged(snapshot));
                }
            }
            PluginResponse::ToggleMinimize { pane_id } => {
                if let Some(pane) = self.pane_manager.get_pane_mut(pane_id) {
                    pane.is_minimized = !pane.is_minimized;
                }
            }
            PluginResponse::MoveInPane { pane_id, direction } => {
                if let Some(pane) = self.pane_manager.get_pane_mut(pane_id) {
                    pane.plugin_handle_move(direction);
                }
            }
            PluginResponse::SelectInPane { pane_id } => {
                let file_to_open = if let Some(pane) = self.pane_manager.get_pane_mut(pane_id) {
                    pane.plugin_handle_select()
                } else {
                    None
                };

                if let Some(path) = file_to_open {
                    self.apply_plugin_response(PluginResponse::ClosePane { pane_id });
                    if let Some(file_name) = path.to_str() {
                        match Buffer::load(file_name) {
                            Ok(buffer) => {
                                let buffer_id = self.buffer_manager.add(buffer);
                                if let Some(view) = self
                                    .pane_manager
                                    .active_pane_mut()
                                    .and_then(|p| p.view_mut())
                                {
                                    view.set_buffer_id(buffer_id);
                                }
                            }
                            Err(_) => {
                                self.update_message(&format!("ERR: Could not open file: {}", file_name));
                            }
                        }
                    }
                }
            }
            PluginResponse::MouseClickInPane { pane_id, position } => {
                let action = if let Some(pane) = self.pane_manager.get_pane_mut(pane_id) {
                    pane.plugin_handle_click(position)
                } else {
                    ClickAction::None
                };

                match action {
                    ClickAction::Close => {
                        self.apply_plugin_response(PluginResponse::ClosePane { pane_id });
                    }
                    ClickAction::Minimize => {
                        self.apply_plugin_response(PluginResponse::ToggleMinimize { pane_id });
                    }
                    ClickAction::None => {}
                }
            }
        }
    }

    // Rendering

    fn refresh_screen(&mut self) {
        if self.terminal_size.height == 0 || self.terminal_size.width == 0 {
            return;
        }

        let Size { height, width } = self.terminal_size;

        let _ = Terminal::hide_caret();

        let _ = self
            .buffer_bar
            .render(&self.buffer_manager, &self.pane_manager);

        if self.in_prompt() {
            self.command_bar.render();
        } else {
            self.message_bar.render();
        }

        if height > 1 {
            self.status_bar.render();
        }

        if height > 2 {
            // Tiled panes (layer 0)
            for (pane_id, _) in self.layout_tree.collect_leaf_layouts() {
                if let Some(pane) = self.pane_manager.get_pane_mut(pane_id) {
                    if !pane.is_floating {
                        pane.render(&self.buffer_manager);
                    }
                }
            }

            // Floating panes sorted by z-index (layer 10+)
            let floating_ids: Vec<usize> = self
                .pane_manager
                .get_floating_panes_sorted()
                .iter()
                .map(|p| p.pane_id)
                .collect();

            for id in floating_ids {
                if let Some(pane) = self.pane_manager.get_pane_mut(id) {
                    pane.render(&self.buffer_manager);
                }
            }
        }

        // Caret
        let active_pane = self.pane_manager.active_pane().unwrap();
        let new_caret_pos = if self.in_prompt() {
            self.command_bar.caret_position()
        } else if let Some(view) = active_pane.view() {
            let buffer = self.buffer_manager.get(view.buffer_id()).unwrap();
            view.caret_position(buffer)
        } else {
            let rect = active_pane.component().rect();
            Position {
                row: rect.position.row.saturating_add(1),
                col: rect.position.col.saturating_add(1),
            }
        };

        debug_assert!(new_caret_pos.col <= width);
        debug_assert!(new_caret_pos.row <= height);

        let _ = Terminal::move_caret_to(new_caret_pos);
        let _ = Terminal::show_caret();
        let _ = Terminal::execute();
    }

    pub fn refresh_status(&mut self) {
        let active_pane = self.pane_manager.active_pane().unwrap();
        let status = if let Some(view) = active_pane.view() {
            let buffer = self.buffer_manager.get(view.buffer_id()).unwrap();
            view.get_status(buffer)
        } else {
            DocumentStatus {
                file_name: "Plugin".to_string(),
                total_lines: 0,
                current_line_idx: 0,
                is_modified: false,
                file_type: FileType::Text,
            }
        };

        let title = format!("{} - {NAME}", status.file_name);
        self.status_bar.update_status(status);
        if title != self.title && matches!(Terminal::set_title(&title), Ok(())) {
            self.title = title;
        }
    }

    // Resize

    pub fn handle_resize_command(&mut self, size: Size) {
        self.terminal_size = size;
        let Size { height, width } = size;

        self.buffer_bar.resize(Rect {
            position: Position { row: 0, col: 0 },
            size: Size { height: 1, width },
        });

        let editor_rect = Rect {
            position: Position { row: 1, col: 0 },
            size: Size {
                height: height.saturating_sub(3),
                width,
            },
        };
        self.layout_tree.compute_layout(editor_rect);
        self.sync_pane_rects();

        for pane in self.pane_manager.iter_mut() {
            if pane.is_floating {
                let mut rect = pane.component().rect();
                rect.position.col = rect
                    .position
                    .col
                    .min(width.saturating_sub(rect.size.width));
                let max_row = height
                    .saturating_sub(rect.size.height.saturating_add(2))
                    .max(1);
                rect.position.row = rect.position.row.clamp(1, max_row);
                pane.resize(rect);
            }
        }

        self.status_bar.resize(Rect {
            position: Position {
                row: height.saturating_sub(2),
                col: 0,
            },
            size: Size { height: 1, width },
        });

        let bottom_rect = Rect {
            position: Position {
                row: height.saturating_sub(1),
                col: 0,
            },
            size: Size { height: 1, width },
        };
        self.message_bar.resize(bottom_rect);
        self.command_bar.resize(bottom_rect);
        self.mark_all_panes_for_redraw();
    }

    //Helpers

    fn in_prompt(&self) -> bool {
        !self.prompt_type.is_none()
    }

    fn update_message(&mut self, new_message: &str) {
        self.message_bar.update_message(new_message);
    }

    fn sync_pane_rects(&mut self) {
        for (pane_id, rect) in self.layout_tree.collect_leaf_layouts() {
            if let Some(pane) = self.pane_manager.get_pane_mut(pane_id) {
                pane.resize(rect);
            }
        }
    }

    fn mark_all_panes_for_redraw(&mut self) {
        for pane in self.pane_manager.iter_mut() {
            if let Some(view) = pane.view_mut() {
                view.mark_redraw(true);
            }
        }
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        self.plugin_runtime.shutdown();
        let _ = Terminal::terminate();
        if self.should_quit {
            let _ = Terminal::print("Goodbye.\r\n");
        }
    }
}
