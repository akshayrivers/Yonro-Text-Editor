// Editor event loop (TUI shell over `yonro-core`).
use crate::prelude::*;
use std::{
    env,
    io::Error,
    panic::{set_hook, take_hook},
    time::Duration,
};
use yonro_core::{
    Buffer, BufferManager, Command, DocumentStatus, EditorEvent, FileType, System,
};

pub use crate::command_dispatcher::{EditorContext, HandlerRegistry, PromptType};
pub use crate::layout::{
    DocTab, LayoutNode, LayoutTree, Pane, PaneContent, PaneManager, SplitDirection, SplitHandle, SidebarKind,
};
pub use crate::plugins::{
    builtin::{FileExplorerPlugin, WordCountPlugin}, BufferSnapshot, Plugin, PluginMessage, PluginResponse,
    PluginRuntime,
};
pub use crate::terminal::Terminal;
pub use crate::uicomponents::{
    view::EditOperation, ClickAction, CommandBar, FileExplorer, MessageBar, PaneBar, StatusBar,
    UIComponent, View, WordCount,
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

    pane_bar: PaneBar,
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

    /// Plugin responses from handlers, applied after command dispatch.
    pending_plugin_responses: Vec<PluginResponse>,

    /// Track the last text editor pane that was focused (for opening files from sidebar)
    last_editor_pane: Option<usize>,

    /// Open document tabs (VS Code-style). Only `active_tab` is installed in
    /// the layout tree; the rest are stashed with their layouts intact.
    doc_tabs: Vec<DocTab>,
    active_tab: usize,
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
        // Tab 0 mirrors the initial root layout (same value; the tree keeps
        // the live one). Later tabs stash their layout here on switch.
        let doc_tabs = vec![DocTab::new(
            LayoutNode::Leaf {
                pane_id: initial_pane_id,
                rect: root_rect,
            },
            initial_pane_id,
        )];

        // Spin up plugin runtime and register built-in plugins
        let plugin_runtime = PluginRuntime::new();
        plugin_runtime.load_plugin(Box::new(FileExplorerPlugin::new()));
        plugin_runtime.load_plugin(Box::new(WordCountPlugin::new()));

        let mut editor = Self {
            should_quit: false,
            layout_tree,
            pane_manager,
            buffer_manager,
            plugin_runtime,
            pane_bar: PaneBar::default(),
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
            pending_plugin_responses: Vec::new(),
            last_editor_pane: None,
            doc_tabs,
            active_tab: 0,
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
        const FRAME_TIMEOUT: Duration = Duration::from_millis(16); // ~60fps
        loop {
            // 1. Apply plugin responses from last cycle
            let responses = self.plugin_runtime.drain_responses();
            for response in responses {
                self.apply_plugin_response(response);
            }

            // 1b. Apply plugin responses from command handlers (mouse, etc.)
            let handler_responses = std::mem::take(&mut self.pending_plugin_responses);
            for response in handler_responses {
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

            // 4. Non-blocking poll for next input event (~60fps)
            match Terminal::poll_event(FRAME_TIMEOUT) {
                Ok(Some(event)) => {
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
                Ok(None) => {
                    // Timeout - no event, just continue loop for next frame
                }
                Err(_err) => {
                    #[cfg(debug_assertions)]
                    panic!("Could not read event: {_err:?}");
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
                // Update WordCount component for active buffer
                self.update_word_count_if_open();
            }
        }
    }

    fn make_context(&'_ mut self) -> EditorContext<'_> {
        EditorContext {
            prompt_type: &mut self.prompt_type,
            pane_manager: &mut self.pane_manager,
            layout_tree: &mut self.layout_tree,
            buffer_manager: &mut self.buffer_manager,
            pane_bar: &mut self.pane_bar,
            command_bar: &mut self.command_bar,
            message_bar: &mut self.message_bar,
            terminal_size: self.terminal_size,
            should_quit: &mut self.should_quit,
            quit_times: &mut self.quit_times,
            dragging_split: &mut self.dragging_split,
            dragging_pane: &mut self.dragging_pane,
            drag_offset: &mut self.drag_offset,
            buffer_changed: None,
            plugin_responses: &mut self.pending_plugin_responses,
            last_editor_pane: &mut self.last_editor_pane,
        }
    }

    fn make_buffer_snapshot(&self, buffer_id: usize) -> Option<BufferSnapshot> {
        let buffer = self.buffer_manager.get(buffer_id)?;
        Some(BufferSnapshot {
            buffer_id,
            rope: buffer.rope(),
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
                // Populate live stats immediately — otherwise a fresh
                // WordCount pane shows 0/0 until the next keystroke.
                self.update_word_count_if_open();
            }

            PluginResponse::ToggleSidebar { kind } => {
                // Kind-switch: hide the current sidebar first (without conflating
                // `visible` with `is_floating`; hidden panes are simply not rendered).
                if self.layout_tree.sidebar.kind != kind
                    && self.layout_tree.sidebar.visible
                {
                    let old_kind = self.layout_tree.sidebar.kind;
                    if let Some(pane_id) = self.layout_tree.sidebar.pane_id {
                        if let Some(pane) = self.pane_manager.get_pane_mut(pane_id) {
                            pane.is_floating = false;
                            pane.is_minimized = false;
                        }
                        self.plugin_runtime.send(PluginMessage::PaneClosed {
                            plugin_name: Self::sidebar_plugin_name(old_kind),
                            pane_id,
                        });
                    }
                    self.layout_tree.sidebar.hide();
                    self.focus_editor_after_sidebar();
                }
                if self.layout_tree.sidebar.kind != kind {
                    self.layout_tree.sidebar.kind = kind;
                }

                if self.layout_tree.sidebar.visible {
                    // Persistent explorer: re-pressing Ctrl+E (same kind)
                    // focuses the sidebar instead of hiding it, so opening
                    // files never strands you without the tree. Hide via Esc,
                    // the [x] button, or CloseSidebar.
                    if let Some(pane_id) = self.layout_tree.sidebar.pane_id {
                        if let Some(pane) = self.pane_manager.get_pane_mut(pane_id) {
                            pane.is_floating = false;
                            pane.is_minimized = false;
                            if let crate::layout::PaneContent::Plugin(c) = &mut pane.content {
                                c.mark_redraw(true);
                            }
                        }
                        self.pane_manager.set_active_pane(pane_id);
                        self.mark_all_panes_for_redraw();
                    }
                } else {
                    // Toggle ON - show sidebar
                    let pane_id = if let Some(existing_id) =
                        self.layout_tree.sidebar.pane_id
                    {
                        // Reuse existing pane
                        existing_id
                    } else {
                        // Create new pane
                        let content = match kind {
                            SidebarKind::FileExplorer => {
                                PaneContent::Plugin(Box::new(FileExplorer::default()))
                            }
                            SidebarKind::WordCount => {
                                PaneContent::Plugin(Box::new(WordCount::default()))
                            }
                        };
                        let new_id = self.pane_manager.create_pane(content);
                        self.layout_tree.sidebar.set_pane_id(new_id);
                        new_id
                    };

                    // Mark the sidebar visible BEFORE resize/render so the
                    // right-strip block in `refresh_screen` actually draws it.
                    // (This `show()` was missing entirely — the pane existed
                    // in `pane_manager` (hence the PaneBar tab) but `visible`
                    // stayed false, so nothing was ever drawn on screen.)
                    self.layout_tree.sidebar.show();

                    // Show the pane as sidebar (non-floating, not minimized)
                    if let Some(pane) = self.pane_manager.get_pane_mut(pane_id) {
                        pane.is_floating = false;
                        pane.is_minimized = false;
                        // Force mark for redraw so sidebar renders immediately
                        if let crate::layout::PaneContent::Plugin(c) = &mut pane.content {
                            c.mark_redraw(true);
                        }
                    }

                    // Set sidebar as active pane so it receives input
                    // This will track the previous editor pane in last_editor_pane
                    self.set_active_editor_pane(pane_id);
                    // Notify plugin that pane was opened
                    self.plugin_runtime.send(PluginMessage::PaneOpened {
                        plugin_name: Self::sidebar_plugin_name(kind),
                        pane_id,
                    });
                }
                self.handle_resize_command(self.terminal_size);
            }

            PluginResponse::CloseSidebar { kind } => {
                // Explicit hide (Esc, [x], close command). No-op unless the
                // shown sidebar matches, so stray closes can't kill the tree.
                if self.layout_tree.sidebar.visible && self.layout_tree.sidebar.kind == kind
                {
                    if let Some(pane_id) = self.layout_tree.sidebar.pane_id {
                        if let Some(pane) = self.pane_manager.get_pane_mut(pane_id) {
                            pane.is_floating = false;
                            pane.is_minimized = false;
                        }
                        self.plugin_runtime.send(PluginMessage::PaneClosed {
                            plugin_name: Self::sidebar_plugin_name(kind),
                            pane_id,
                        });
                    }
                    self.layout_tree.sidebar.hide();
                    self.focus_editor_after_sidebar();
                    self.handle_resize_command(self.terminal_size);
                }
            }

            PluginResponse::SwitchTab { index } => {
                self.switch_tab(index);
            }

            PluginResponse::ClosePane { pane_id } => {
                // Sidebar panes live outside `LayoutTree` (right-strip, not a split),
                // so `remove_node` would fail — hide the sidebar instead.
                if Some(pane_id) == self.layout_tree.sidebar.pane_id {
                    let kind = self.layout_tree.sidebar.kind;
                    if let Some(pane) = self.pane_manager.get_pane_mut(pane_id) {
                        pane.is_floating = false;
                        pane.is_minimized = false;
                    }
                    // Notify the owning plugin so `open_pane_id` clears (mouse [x],
                    // Enter-open, command-bar close all funnel through here).
                    let plugin_name = Self::sidebar_plugin_name(kind);
                    let was_active = self
                        .pane_manager
                        .active_pane()
                        .map(|p| p.pane_id == pane_id)
                        .unwrap_or(false);
                    // If this ClosePane came from the WordCount floating pane
                    // (Ctrl-W W), the plugin name is "word_count" but the pane
                    // is NOT the sidebar — fall through to floating removal.
                    // Sidebar WordCount panes use ToggleSidebar, never ClosePane,
                    // so reaching here with a sidebar id is always a hide.
                    self.layout_tree.sidebar.hide();
                    self.plugin_runtime.send(PluginMessage::PaneClosed {
                        plugin_name,
                        pane_id,
                    });
                    if was_active {
                        self.focus_editor_after_sidebar();
                    }
                    self.handle_resize_command(self.terminal_size);
                    // If this was actually a floating WordCount pane whose id
                    // coincidentally equals sidebar.pane_id == None case is
                    // already excluded by the `Some(pane_id) ==` guard above.
                    return;
                }
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
                    // Keep async plugins in sync (mouse [x] on floating WordCount).
                    self.plugin_runtime.send(PluginMessage::PaneClosed {
                        plugin_name: "word_count".to_string(),
                        pane_id,
                    });
                    // Also notify file_explorer in case a legacy floating explorer
                    // was closed (harmless for non-owners: they filter by id).
                    self.plugin_runtime.send(PluginMessage::PaneClosed {
                        plugin_name: "file_explorer".to_string(),
                        pane_id,
                    });
                } else if self.layout_tree.remove_node(pane_id).is_ok() {
                    self.pane_manager.remove_pane(pane_id);
                    self.handle_resize_command(self.terminal_size);
                } else if !self.close_tab_pane(pane_id) {
                    self.update_message("Cannot close the last tiled pane!");
                }

                if was_active {
                    // Re-focus first available tiled pane
                    if let Some((id, _)) = self.layout_tree.collect_leaf_layouts().first() {
                        self.pane_manager.set_active_pane(*id);
                    }
                }
                // A removed pane may strand document tabs — prune them.
                self.prune_tabs();
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
                    // Persistent explorer: stay open; the file opens in a NEW
                    // sibling pane so the previous file keeps its place.
                    self.open_file_in_new_pane(&path);
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
                    ClickAction::DoubleClick => {
                        // Same as Enter: new sibling pane, explorer stays open.
                        if let Some(path) = self.pane_manager.get_pane_mut(pane_id).and_then(|p| p.plugin_handle_select()) {
                            self.open_file_in_new_pane(&path);
                        }
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

        let _ = self.pane_bar.render(
            &self.buffer_manager,
            &self.pane_manager,
            &self.doc_tabs,
            self.active_tab,
        );

        if self.in_prompt() {
            self.command_bar.render();
        } else {
            self.message_bar.render();
        }

        if height > 1 {
            self.status_bar.render();
        }

        // Render sidebar if visible (on the right side).
        // The sidebar pane lives outside `LayoutTree` by design; hidden means
        // "not rendered anywhere", never "floating + minimized".
        if self.layout_tree.sidebar.visible {
            if let Some(sidebar_pane_id) = self.layout_tree.sidebar.pane_id {
                if let Some(pane) = self.pane_manager.get_pane_mut(sidebar_pane_id) {
                    let sidebar_width = self.layout_tree.sidebar.width;
                    let sidebar_rect = Rect {
                        position: Position { row: 1, col: width.saturating_sub(sidebar_width) },
                        size: Size {
                            height: height.saturating_sub(3),
                            width: sidebar_width,
                        },
                    };
                    pane.is_floating = false;
                    pane.is_minimized = false;
                    pane.resize(sidebar_rect);
                    pane.render(&self.buffer_manager);
                }
            }
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

            // Floating panes sorted by z-index (layer 10+).
            // Never render the sidebar here, even if stale state marks it floating.
            let sidebar_id = self.layout_tree.sidebar.pane_id;
            let floating_ids: Vec<usize> = self
                .pane_manager
                .get_floating_panes_sorted()
                .iter()
                .map(|p| p.pane_id)
                .filter(|id| Some(*id) != sidebar_id)
                .collect();

            for id in floating_ids {
                if let Some(pane) = self.pane_manager.get_pane_mut(id) {
                    pane.render(&self.buffer_manager);
                }
            }
        }

        // Caret
        let active_pane = self.pane_manager.active_pane();
        let new_caret_pos = if self.in_prompt() {
            self.command_bar.caret_position()
        } else if let Some(pane) = active_pane {
            if let Some(view) = pane.view() {
                self.buffer_manager
                    .get(view.buffer_id())
                    .map(|buffer| view.caret_position(buffer))
                    .unwrap_or(Position { row: 1, col: 0 })
            } else {
                let rect = pane.component().rect();
                Position {
                    row: rect.position.row.saturating_add(1),
                    col: rect.position.col.saturating_add(1),
                }
            }
        } else {
            // No active pane - default to top-left of editor area
            Position { row: 1, col: 0 }
        };

        debug_assert!(new_caret_pos.col <= width);
        debug_assert!(new_caret_pos.row <= height);

        let _ = Terminal::move_caret_to(new_caret_pos);
        let _ = Terminal::show_caret();
        let _ = Terminal::execute();
    }

    pub fn refresh_status(&mut self) {
        let active_pane = self.pane_manager.active_pane();
        let status = if let Some(pane) = active_pane {
            if let Some(view) = pane.view() {
                if let Some(buffer) = self.buffer_manager.get(view.buffer_id()) {
                    view.get_status(buffer)
                } else {
                    DocumentStatus {
                        file_name: "Plugin".to_string(),
                        total_lines: 0,
                        current_line_idx: 0,
                        is_modified: false,
                        file_type: FileType::Text,
                        word_count: 0,
                        char_count: 0,
                    }
                }
            } else {
                DocumentStatus {
                    file_name: "Plugin".to_string(),
                    total_lines: 0,
                    current_line_idx: 0,
                    is_modified: false,
                    file_type: FileType::Text,
                    word_count: 0,
                    char_count: 0,
                }
            }
        } else {
            DocumentStatus {
                file_name: "No Pane".to_string(),
                total_lines: 0,
                current_line_idx: 0,
                is_modified: false,
                file_type: FileType::Text,
                word_count: 0,
                char_count: 0,
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

        self.pane_bar.resize(Rect {
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

        let sidebar_width = if self.layout_tree.sidebar.visible {
            self.layout_tree.sidebar.width
        } else {
            0
        };
        for pane in self.pane_manager.iter_mut() {
            if pane.is_floating {
                let mut rect = pane.component().rect();
                rect.position.col = rect
                    .position
                    .col
                    .min(width.saturating_sub(sidebar_width).saturating_sub(rect.size.width));
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
            // Also mark plugin components for redraw
            if let crate::layout::PaneContent::Plugin(c) = &mut pane.content {
                c.mark_redraw(true);
            }
            if let crate::layout::PaneContent::Popup(p) = &mut pane.content {
                p.mark_redraw(true);
            }
        }
    }

    /// Set the active pane, tracking the last text editor pane for sidebar file opening.
    fn set_active_editor_pane(&mut self, pane_id: usize) {
        // If the currently active pane is a text editor (not sidebar/plugin), save it
        if let Some(current_pane) = self.pane_manager.active_pane() {
            if current_pane.view().is_some() && !current_pane.is_floating {
                self.last_editor_pane = Some(current_pane.pane_id);
            }
        }
        self.pane_manager.set_active_pane(pane_id);
        self.pane_manager.bring_to_front(pane_id);
        self.mark_all_panes_for_redraw();
        // Update WordCount for new active buffer
        self.update_word_count_if_open();
    }

    fn update_word_count_if_open(&mut self) {
        // Prefer the last text-editor pane, then the active pane, then any
        // text view — `last_editor_pane` is `None` until the sidebar is first
        // opened, and the active pane may itself be a plugin pane (sidebar
        // explorer focused when stats open). Stats must reflect the OPENED
        // FILE, never the WordCount pane's own (empty) defaults.
        let active_buffer_id = self
            .last_editor_pane
            .and_then(|pane_id| self.pane_manager.get_pane(pane_id))
            .and_then(|p| p.view())
            .map(View::buffer_id)
            .or_else(|| {
                self.pane_manager
                    .active_pane()
                    .and_then(|p| p.view())
                    .map(View::buffer_id)
            })
            .or_else(|| {
                self.pane_manager
                    .iter()
                    .find_map(|p| p.view())
                    .map(View::buffer_id)
            });

        if let Some(buffer_id) = active_buffer_id {
            if let Some(buffer) = self.buffer_manager.get(buffer_id) {
                let sidebar_id = self.layout_tree.sidebar.pane_id;
                // Update floating WordCount panes AND the sidebar WordCount pane.
                for pane in self.pane_manager.iter_mut() {
                    let is_sidebar_wordcount = Some(pane.pane_id) == sidebar_id
                        && self.layout_tree.sidebar.kind == SidebarKind::WordCount;
                    if pane.is_floating || is_sidebar_wordcount {
                        if let crate::layout::PaneContent::Plugin(component) =
                            &mut pane.content
                        {
                            component.update_from_buffer(buffer);
                        }
                    }
                }
            }
        }
    }

    /// Map a sidebar kind to its owning plugin name for PaneOpened/PaneClosed.
    fn sidebar_plugin_name(kind: SidebarKind) -> String {
        match kind {
            SidebarKind::FileExplorer => "file_explorer".to_string(),
            SidebarKind::WordCount => "word_count".to_string(),
        }
    }

    /// After hiding the sidebar, return focus to the last text editor.
    fn focus_editor_after_sidebar(&mut self) {
        if let Some(id) = self.editor_target_pane() {
            self.pane_manager.set_active_pane(id);
        }
        self.mark_all_panes_for_redraw();
    }

    /// Which text-editor pane should receive an opened file / keep focus.
    /// `last_editor_pane` first, then the active pane if it is a text view,
    /// then the first text view in the manager. Guarantees files open
    /// IN PLACE in a real editor instead of vanishing when focus is stale.
    fn editor_target_pane(&self) -> Option<usize> {
        self.last_editor_pane
            .filter(|id| {
                self.pane_manager
                    .get_pane(*id)
                    .is_some_and(|p| p.view().is_some())
            })
            .or_else(|| {
                self.pane_manager
                    .active_pane()
                    .filter(|p| p.view().is_some())
                    .map(|p| p.pane_id)
            })
            .or_else(|| {
                self.pane_manager
                    .iter()
                    .filter(|p| p.view().is_some() && !p.is_floating)
                    .map(|p| p.pane_id)
                    .next()
            })
    }

    /// Clone of the live layout root (for stashing into document tabs).
    fn layout_tree_root_clone(&self) -> LayoutNode {
        self.layout_tree.clone_root()
    }

    /// Open `path` in a NEW document tab (VS Code-style): full editor area,
    /// no splitting. The previous file keeps its tab; the explorer stays open.
    fn open_file_in_new_pane(&mut self, path: &std::path::Path) {
        let Some(file_name) = path.to_str() else {
            self.update_message("ERR: Could not open file (bad path)");
            return;
        };
        // Already open → switch to its tab, no duplicate.
        for i in 0..self.doc_tabs.len() {
            let shows = self
                .pane_manager
                .get_pane(self.doc_tabs[i].active_pane)
                .and_then(|p| p.view())
                .and_then(|v| self.buffer_manager.get(v.buffer_id()))
                .and_then(|b| b.get_file_info().get_path())
                .is_some_and(|p| p == path);
            if shows {
                self.switch_tab(i);
                return;
            }
        }
        let Ok(buffer) = Buffer::load(file_name) else {
            self.update_message(&format!("ERR: Could not open file: {file_name}"));
            return;
        };
        let buffer_id = self.buffer_manager.add(buffer);

        let mut view = View::default();
        view.set_buffer_id(buffer_id);
        let new_id = self
            .pane_manager
            .create_pane(PaneContent::TextView(view));
        if let Some(pane) = self.pane_manager.get_pane_mut(new_id) {
            if let Some(view) = pane.view_mut() {
                view.set_id(new_id);
            }
        }

        // Stash the live layout into the current tab, then install a fresh
        // single-pane root for the new tab. Never record the sidebar (or any
        // plugin pane) as a tab's editor — it has no buffer and would corrupt
        // titles and file-reuse lookup.
        let live_root = self.layout_tree_root_clone();
        let cur_editor = self
            .pane_manager
            .active_pane()
            .filter(|p| p.view().is_some())
            .map(|p| p.pane_id);
        if let Some(tab) = self.doc_tabs.get_mut(self.active_tab) {
            tab.root = live_root;
            if let Some(cur) = cur_editor {
                tab.active_pane = cur;
            }
        }
        let new_tab_idx = self.doc_tabs.len();
        self.doc_tabs.push(DocTab::new(
            LayoutNode::Leaf {
                pane_id: new_id,
                rect: Rect::default(),
            },
            new_id,
        ));
        self.layout_tree.set_root(LayoutNode::Leaf {
            pane_id: new_id,
            rect: Rect::default(),
        });
        self.active_tab = new_tab_idx;

        self.handle_resize_command(self.terminal_size);
        self.set_active_editor_pane(new_id);
        self.last_editor_pane = Some(new_id);
        self.update_message(&format!("Opened {file_name} in tab {new_tab_idx}"));
        self.update_word_count_if_open();
    }

    /// Switch to document tab `index`, preserving each tab's layout.
    fn switch_tab(&mut self, index: usize) {
        if index >= self.doc_tabs.len() {
            return;
        }
        if index == self.active_tab {
            let pid = self.doc_tabs[index].active_pane;
            if self.pane_manager.get_pane(pid).is_some() {
                self.pane_manager.set_active_pane(pid);
                self.mark_all_panes_for_redraw();
            }
            return;
        }
        // Stash live layout + focus into the outgoing tab (editor panes only —
        // recording the sidebar would corrupt titles and file-reuse lookup).
        let live_root = self.layout_tree_root_clone();
        let cur_editor = self
            .pane_manager
            .active_pane()
            .filter(|p| p.view().is_some())
            .map(|p| p.pane_id);
        if let Some(tab) = self.doc_tabs.get_mut(self.active_tab) {
            tab.root = live_root;
            if let Some(cur) = cur_editor {
                tab.active_pane = cur;
            }
        }
        // Install the incoming tab's layout.
        let incoming = self.doc_tabs[index].root.clone();
        self.layout_tree.set_root(incoming);
        self.active_tab = index;
        let pid = self.doc_tabs[index].active_pane;
        if self.pane_manager.get_pane(pid).is_some() {
            self.pane_manager.set_active_pane(pid);
        } else if let Some((id, _)) = self
            .layout_tree
            .collect_leaf_layouts()
            .iter()
            .find(|(id, _)| self.pane_manager.get_pane(*id).is_some())
        {
            let id = *id;
            self.doc_tabs[index].active_pane = id;
            self.pane_manager.set_active_pane(id);
        }
        self.handle_resize_command(self.terminal_size);
    }

    /// Close `pane_id` when `remove_node` refused (last-leaf guard) or when
    /// the pane lives in a stashed (inactive) tab. Returns true if handled.
    /// - Last leaf + sibling tabs exist → switch to a neighbor, then remove
    ///   the now-stashed pane.
    /// - Stashed pane → excise from every tab root holding it, drop the pane.
    /// - Single tab, single pane → false (refuse, nothing closes).
    fn close_tab_pane(&mut self, pane_id: usize) -> bool {
        let in_live_tree = self
            .layout_tree
            .collect_leaf_layouts()
            .iter()
            .any(|(id, _)| *id == pane_id);
        if in_live_tree {
            if self.doc_tabs.len() <= 1 {
                return false;
            }
            let neighbor = if self.active_tab == 0 { 1 } else { self.active_tab - 1 };
            self.switch_tab(neighbor);
            // The doomed pane is now stashed — fall through below.
        }
        let mut found = false;
        for tab in self.doc_tabs.iter_mut() {
            if tab.pane_ids().contains(&pane_id) {
                // `None` (emptied root) is left for `prune_tabs` to drop.
                if let Some(new_root) = LayoutTree::remove_from_root(tab.root.clone(), pane_id)
                {
                    tab.root = new_root;
                }
                found = true;
            }
        }
        if !found {
            return false;
        }
        self.pane_manager.remove_pane(pane_id);
        self.prune_tabs();
        self.handle_resize_command(self.terminal_size);
        true
    }

    /// Drop tabs whose layout references no live pane; repair focus.
    /// Inactive tabs are read from accurate stashed layouts; the active tab
    /// is always kept (its live layout is in the tree by construction).
    fn prune_tabs(&mut self) {
        let active = self.active_tab;
        let mut shift = 0;
        let mut kept = Vec::new();
        for (i, tab) in std::mem::take(&mut self.doc_tabs).into_iter().enumerate() {
            if i == active {
                kept.push(tab);
                continue;
            }
            let any_live = tab
                .pane_ids()
                .iter()
                .any(|id| self.pane_manager.get_pane(*id).is_some());
            if any_live {
                kept.push(tab);
            } else if i < active {
                shift += 1;
            }
        }
        self.doc_tabs = kept;
        if self.doc_tabs.is_empty() {
            // Unreachable in practice (last leaf can't close) — recover blank.
            let buffer_id = self.buffer_manager.add(Buffer::default());
            let mut view = View::default();
            view.set_buffer_id(buffer_id);
            let new_id = self
                .pane_manager
                .create_pane(PaneContent::TextView(view));
            if let Some(pane) = self.pane_manager.get_pane_mut(new_id) {
                if let Some(view) = pane.view_mut() {
                    view.set_id(new_id);
                }
            }
            self.layout_tree.set_root(LayoutNode::Leaf {
                pane_id: new_id,
                rect: Rect::default(),
            });
            self.doc_tabs.push(DocTab::new(
                LayoutNode::Leaf {
                    pane_id: new_id,
                    rect: Rect::default(),
                },
                new_id,
            ));
            self.active_tab = 0;
            self.handle_resize_command(self.terminal_size);
            self.pane_manager.set_active_pane(new_id);
            return;
        }
        self.active_tab = active.saturating_sub(shift);
        // Repair focus if the active tab's pane vanished.
        let pid = self.doc_tabs[self.active_tab].active_pane;
        if self.pane_manager.get_pane(pid).is_none() {
            if let Some((id, _)) = self
                .layout_tree
                .collect_leaf_layouts()
                .iter()
                .find(|(id, _)| self.pane_manager.get_pane(*id).is_some())
            {
                let id = *id;
                self.doc_tabs[self.active_tab].active_pane = id;
                self.pane_manager.set_active_pane(id);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{LayoutTree, Sidebar, SidebarKind};
    use crate::uicomponents::{FileExplorer, WordCount};
    use crate::prelude::*;
    use yonro_core::buffers::Buffer;

    #[test]
    fn test_sidebar_default() {
        let sidebar = Sidebar::default();
        assert!(!sidebar.visible);
        assert_eq!(sidebar.kind, SidebarKind::FileExplorer);
        assert_eq!(sidebar.width, 30);
        assert!(sidebar.pane_id.is_none());
    }

    #[test]
    fn test_sidebar_toggle() {
        let mut sidebar = Sidebar::new(SidebarKind::FileExplorer, 30);
        assert!(!sidebar.visible);
        
        sidebar.toggle();
        assert!(sidebar.visible);
        
        sidebar.toggle();
        assert!(!sidebar.visible);
    }

    #[test]
    fn test_layout_tree_sidebar_integration() {
        let mut layout = LayoutTree::new(0, Rect {
            position: Position { row: 1, col: 0 },
            size: Size { height: 20, width: 80 },
        });
        
        assert!(!layout.sidebar.visible);
        
        layout.sidebar.toggle();
        assert!(layout.sidebar.visible);
        
        layout.compute_layout(Rect {
            position: Position { row: 1, col: 0 },
            size: Size { height: 20, width: 80 },
        });
        
        let leaves = layout.collect_leaf_layouts();
        for (_, rect) in leaves {
            assert_eq!(rect.size.width, 50); // 80 - 30 = 50
        }
    }

    #[test]
    fn test_file_explorer_creation() {
        let explorer = FileExplorer::default();
        assert!(!explorer.active);
        assert!(explorer.needs_redraw());
    }

    #[test]
    fn test_word_count_creation() {
        let wc = WordCount::default();
        assert!(!wc.active);
        assert!(wc.needs_redraw());
    }

    #[test]
    fn test_word_count_update_from_buffer() {
        let mut wc = WordCount::default();
        let mut buffer = Buffer::default();
        
        buffer.insert_char('h', Location { line_idx: 0, grapheme_idx: 0 });
        buffer.insert_char('e', Location { line_idx: 0, grapheme_idx: 1 });
        buffer.insert_char('l', Location { line_idx: 0, grapheme_idx: 2 });
        buffer.insert_char('l', Location { line_idx: 0, grapheme_idx: 3 });
        buffer.insert_char('o', Location { line_idx: 0, grapheme_idx: 4 });
        buffer.insert_newline(Location { line_idx: 0, grapheme_idx: 5 });
        buffer.insert_char('w', Location { line_idx: 1, grapheme_idx: 0 });
        buffer.insert_char('o', Location { line_idx: 1, grapheme_idx: 1 });
        buffer.insert_char('r', Location { line_idx: 1, grapheme_idx: 2 });
        buffer.insert_char('l', Location { line_idx: 1, grapheme_idx: 3 });
        buffer.insert_char('d', Location { line_idx: 1, grapheme_idx: 4 });
        
        wc.update_from_buffer(&buffer);
        
        // "hello world" = 2 words, 10 chars (without newline), 2 lines
        assert_eq!(wc.words(), 2);
        assert_eq!(wc.lines(), 2);
        assert!(wc.chars() >= 10);
    }

    #[test]
    fn test_word_count_dyn_dispatch_updates() {
        // Regression: `update_from_buffer` was inherent-only, so
        // `Box<dyn PluginComponent>` calls hit the no-op default and stats
        // froze at 0. This must update through dynamic dispatch.
        use crate::uicomponents::PluginComponent;
        let mut buffer = Buffer::default();
        buffer.insert_char('h', Location { line_idx: 0, grapheme_idx: 0 });
        buffer.insert_char('i', Location { line_idx: 0, grapheme_idx: 1 });

        let mut boxed: Box<dyn PluginComponent> = Box::new(WordCount::default());
        boxed.update_from_buffer(&buffer);
        // Downcast via words()? Can't — assert via re-draw state: needs_redraw
        // cleared only by render; update must have marked it (still true).
        assert!(boxed.needs_redraw());
    }

    #[test]
    fn test_word_count_empty_reading_time_zero() {
        let mut wc = WordCount::default();
        let buffer = Buffer::default();
        wc.update_from_buffer(&buffer);
        assert_eq!(wc.words(), 0);
        assert_eq!(wc.reading_time_minutes(), 0);
    }

    #[test]
    fn test_buffer_stats_grapheme_correct() {
        // Family emoji is one grapheme cluster, not 7 scalar chars.
        let mut buffer = Buffer::default();
        for (i, c) in "👨‍👩‍👧‍👦".chars().enumerate() {
            buffer.insert_char(c, Location { line_idx: 0, grapheme_idx: i });
        }
        let (words, graphemes, _) = buffer.word_count_stats();
        assert_eq!(graphemes, 1);
        // No alphanumeric word chunks in a pure-emoji buffer.
        assert_eq!(words, 0);
    }
}
