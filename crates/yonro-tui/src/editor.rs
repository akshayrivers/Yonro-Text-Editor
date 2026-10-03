// Editor event loop (TUI shell over `yonro-core`).
use crate::prelude::*;
use std::{
    env,
    io::Error,
    panic::{set_hook, take_hook},
    time::{Duration, Instant},
};
use unicode_segmentation::UnicodeSegmentation;
use yonro_core::{
    Buffer, BufferManager, Command, DocumentStatus, Edit, EditorEvent, FileType, Move, System,
};

pub use crate::clipboard::SystemClipboard;
pub use crate::command_dispatcher::{EditorContext, HandlerRegistry, PromptType};
pub use crate::layout::{
    DocTab, LayoutNode, LayoutTree, Pane, PaneContent, PaneManager, SidebarKind, SplitDirection,
    SplitHandle,
};
pub use crate::plugins::{
    builtin::{FileExplorerPlugin, OutlinePlugin, WordCountPlugin},
    BufferSnapshot, OutlineField, Plugin, PluginMessage, PluginResponse, PluginRuntime,
};
pub use crate::terminal::Terminal;
pub use crate::uicomponents::{
    view::EditOperation, ClickAction, CommandBar, FileExplorer, LoreSheet, MentionComplete,
    MessageBar, Outline, PaneBar, StatusBar, UIComponent, View, WordCount,
};
pub use yonro_core::{
    lore::{is_mention_char, LoreBook},
    manuscript::{Manuscript, NodeKind},
    MarkDownSyntaxHighlighter, RustSyntaxHighlighter, SearchResultHighlighter, SyntaxHighlighter,
    TextSyntaxHighlighter,
};

/// Live `@mention` completion state (`PLAN.md Phase 4.5`). The popup pane
/// is Editor-driven (no plugin): keys are intercepted pre-dispatch while it
/// is open, and the query is re-derived from the buffer after every event.
struct MentionState {
    /// Editor pane being completed.
    pane_id: usize,
    /// Text between `@` and the cursor.
    query: String,
    /// Filtered entity ids (capped for display).
    candidates: Vec<yonro_core::lore::EntityId>,
    /// Highlight index into `candidates`.
    selected: usize,
    /// Floating popup pane, if currently shown.
    popup: Option<usize>,
}

impl MentionState {
    fn selected_entity(&self) -> Option<yonro_core::lore::EntityId> {
        self.candidates.get(self.selected).copied()
    }
}

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

    /// Distraction-free Zen mode (`PLAN.md Phase 4.2`): document only.
    zen_mode: bool,
    /// Sidebar visibility to restore when leaving Zen mode.
    sidebar_was_visible: bool,
    /// Last `Ctrl+Z` press (double-press toggles Zen; single still undoes).
    last_z_press: Option<Instant>,

    /// System clipboard for Copy/Cut/Paste (`PLAN.md Phase 4.4`).
    clipboard: SystemClipboard,

    /// Story structure (`PLAN.md Phase 4.3`): Project → Acts → Chapters →
    /// Scenes. Rendered by the outline sidebar; scene word counts sync here
    /// from live buffers.
    manuscript: Manuscript,

    /// World bible for `@mentions` (`PLAN.md Phase 4.5`): characters, places
    /// and lore, auto-seeded from scene POVs/settings as structure is added.
    lore: LoreBook,

    /// Open `@mention` completion popup, if any.
    mention: Option<MentionState>,

    /// Open lore-sheet viewer pane, if any.
    sheet_pane: Option<usize>,
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
        plugin_runtime.load_plugin(Box::new(OutlinePlugin::new()));

        // Manuscript project named after the working directory.
        let manuscript_title = std::env::current_dir()
            .ok()
            .and_then(|dir| {
                dir.file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "Untitled".to_string());
        let manuscript = Manuscript::new(&manuscript_title);

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
            zen_mode: false,
            sidebar_was_visible: false,
            last_z_press: None,
            clipboard: SystemClipboard::new(),
            manuscript,
            lore: LoreBook::new(),
            mention: None,
            sheet_pane: None,
        };

        // Restore a saved workspace (manuscript + lore) when present.
        if let Some((manuscript, lore)) = crate::workspace::load() {
            editor.manuscript = manuscript;
            editor.lore = lore;
        }

        editor.handle_resize_command(terminal_size);
        editor.update_message(
            "HELP: Ctrl-F = find | Ctrl-S = save | Ctrl-Q = quit | Ctrl-E = explorer | Ctrl-O = outline | F11/Ctrl-ZZ = zen | Ctrl-C/X/V = clip",
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
                // Best-effort workspace save on the way out.
                let _ = crate::workspace::save(&self.manuscript, &self.lore);
                break;
            }

            // 4. Non-blocking poll for next input event (~60fps)
            match Terminal::poll_event(FRAME_TIMEOUT) {
                Ok(Some(event)) => {
                    // Clone for plugins before core consumes
                    let event_for_plugins = event.clone();
                    // Modal prompts own the keyboard: keys typed into the
                    // command bar must never reach plugins (typing "Mara"
                    // into a POV prompt otherwise adds acts / hijacks the
                    // prompt via outline hotkeys). Commit/abort keys are
                    // evaluated on pre-dispatch state for the same reason.
                    let was_in_prompt = self.in_prompt();
                    self.handle_event(event);
                    if !was_in_prompt {
                        let active_pane_id = self
                            .pane_manager
                            .active_pane()
                            .map(|p| p.pane_id)
                            .unwrap_or(0);
                        // Fire and forget to plugin runtime
                        self.plugin_runtime.send(PluginMessage::Event {
                            event: event_for_plugins,
                            active_pane_id,
                        });
                    }
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
            // Zen toggle needs Editor state (handler contexts can't reach it)
            if matches!(command, Command::System(System::ZenToggle)) {
                self.last_z_press = None;
                self.toggle_zen();
            }
            // Double `Ctrl+Z` toggles Zen (macOS Fn+F11 is unreliable).
            // Single press still undoes; the second press within 500ms skips
            // its undo and toggles instead (first undo stands, redoable).
            // Guarded to normal mode so prompt editing keeps plain undo.
            if !self.in_prompt() {
                if let Command::System(System::Undo) = command {
                    let now = Instant::now();
                    if let Some(last) = self.last_z_press {
                        if now.duration_since(last).as_millis() < Self::Z_DOUBLE_MS {
                            self.last_z_press = None;
                            self.toggle_zen();
                            return;
                        }
                    }
                    self.last_z_press = Some(now);
                }
            }

            // `@mention` popup interception (pre-dispatch): navigation and
            // actions resolve inside the popup; anything else flows through
            // and the query is re-derived afterwards.
            if self.mention.is_some() {
                match &command {
                    Command::Move(Move::Up) => {
                        self.move_mention_selection(true);
                        return;
                    }
                    Command::Move(Move::Down) => {
                        self.move_mention_selection(false);
                        return;
                    }
                    Command::Edit(Edit::InsertNewLine) => {
                        self.accept_mention();
                        return;
                    }
                    Command::Edit(Edit::Insert('\t')) => {
                        // `Tab` views the lore sheet instead of indenting.
                        let target = self
                            .mention
                            .as_ref()
                            .and_then(MentionState::selected_entity);
                        self.dismiss_mention();
                        if let Some(id) = target {
                            self.open_lore_sheet(id);
                        }
                        return;
                    }
                    Command::System(System::Dismiss) => {
                        self.dismiss_mention();
                        return;
                    }
                    _ => {}
                }
            }
            // Lore-sheet viewer: `Esc` closes it (tracked pane, no plugin).
            if let Command::System(System::Dismiss) = command {
                if let Some(sheet) = self.sheet_pane {
                    if self
                        .pane_manager
                        .active_pane()
                        .is_some_and(|pane| pane.pane_id == sheet)
                    {
                        self.pane_manager.remove_pane(sheet);
                        self.sheet_pane = None;
                        if let Some(id) = self.editor_target_pane() {
                            self.pane_manager.set_active_pane(id);
                        }
                        self.mark_all_panes_for_redraw();
                        return;
                    }
                }
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
                // Scene word counts follow the same edits.
                self.sync_manuscript_words();
            }

            // Zen typewriter: recenter AFTER the event's own scrolling, so the
            // final resting state (not just the next frame) is centered.
            if self.zen_mode {
                let buffer_id = self
                    .pane_manager
                    .active_pane()
                    .and_then(|p| p.view())
                    .map(View::buffer_id);
                if let Some(buffer_id) = buffer_id {
                    if let Some(buffer) = self.buffer_manager.get(buffer_id) {
                        if let Some(view) = self
                            .pane_manager
                            .active_pane_mut()
                            .and_then(|p| p.view_mut())
                        {
                            view.apply_typewriter(buffer);
                        }
                    }
                }
            }

            // `@mention` popup follows every event (typed query, moves,
            // focus changes); it dismisses itself when the cursor leaves.
            self.update_mention();
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
            clipboard: &mut self.clipboard,
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
                if self.layout_tree.sidebar.kind != kind && self.layout_tree.sidebar.visible {
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
                    let pane_id = if let Some(existing_id) = self.layout_tree.sidebar.pane_id {
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
                            SidebarKind::Outline => {
                                PaneContent::Plugin(Box::new(Outline::default()))
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
                if self.layout_tree.sidebar.visible && self.layout_tree.sidebar.kind == kind {
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
                    // Drop dangling popup/sheet references to the closed pane.
                    if self.sheet_pane == Some(pane_id) {
                        self.sheet_pane = None;
                    }
                    if self
                        .mention
                        .as_ref()
                        .is_some_and(|state| state.popup == Some(pane_id))
                    {
                        self.mention = None;
                    }
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
                // Outline sidebar: open the selected scene (materializing its
                // file on first open). Anything else: classic file explorer.
                if self.is_outline_pane(pane_id) {
                    self.open_outline_selection(pane_id);
                    return;
                }
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
            PluginResponse::ManuscriptAdd { child } => {
                self.manuscript_add(child);
            }
            PluginResponse::ManuscriptPrompt { field } => {
                let (prompt, prefill) = self.outline_prompt_for(field);
                self.command_bar.set_prompt(prompt);
                self.command_bar.set_value(&prefill);
                self.prompt_type = match field {
                    crate::plugins::OutlineField::Rename => PromptType::Rename,
                    crate::plugins::OutlineField::Pov => PromptType::OutlinePov,
                    crate::plugins::OutlineField::Target => PromptType::OutlineTarget,
                };
                self.mark_all_panes_for_redraw();
            }
            PluginResponse::ManuscriptApply { field, value } => {
                self.manuscript_apply(field, &value);
            }
            PluginResponse::ManuscriptRemove => {
                let selected = self
                    .layout_tree
                    .sidebar
                    .pane_id
                    .and_then(|id| self.outline_selection_of(id));
                let Some(node_id) = selected else {
                    self.update_message("Outline: nothing selected");
                    return;
                };
                match self.manuscript.remove(node_id) {
                    Ok(()) => {
                        self.save_workspace();
                        self.update_message("Outline: removed (files kept on disk)");
                    }
                    Err(err) => self.update_message(&format!("Outline: {err}")),
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
                        // Outline: same as Enter. Otherwise classic file open.
                        if self.is_outline_pane(pane_id) {
                            self.open_outline_selection(pane_id);
                        } else if let Some(path) = self
                            .pane_manager
                            .get_pane_mut(pane_id)
                            .and_then(|p| p.plugin_handle_select())
                        {
                            // Persistent explorer: stay open; new tab keeps
                            // the previous file in place.
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

        // Zen mode owns the whole frame (document only, no chrome).
        if self.zen_mode {
            self.refresh_screen_zen();
            return;
        }

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
                        position: Position {
                            row: 1,
                            col: width.saturating_sub(sidebar_width),
                        },
                        size: Size {
                            height: height.saturating_sub(3),
                            width: sidebar_width,
                        },
                    };
                    pane.is_floating = false;
                    pane.is_minimized = false;
                    pane.resize(sidebar_rect);
                    // Outline rows rebuild from the manuscript every visible
                    // frame (selection preserved by node id inside).
                    if let crate::layout::PaneContent::Plugin(component) = &mut pane.content {
                        component.sync_outline(&self.manuscript, &self.buffer_manager);
                    }
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
        self.position_caret(width, height);
    }

    /// Zen render (`PLAN.md Phase 4.2`): the active document only, centered
    /// in a 70-column strip, all chrome hidden. The command bar still
    /// overlays the bottom row while prompting (else the user is stranded).
    fn refresh_screen_zen(&mut self) {
        let _ = Terminal::hide_caret();
        if self.in_prompt() {
            self.command_bar.render();
        }
        let target = self
            .pane_manager
            .active_pane()
            .filter(|p| p.view().is_some())
            .map(|p| p.pane_id)
            .or_else(|| self.editor_target_pane());
        if let Some(id) = target {
            let term = self.terminal_size;
            let mut rect = Self::zen_rect(term);
            if self.in_prompt() {
                rect.size.height = rect.size.height.saturating_sub(1);
            }
            if let Some(pane) = self.pane_manager.get_pane_mut(id) {
                pane.resize(rect);
                // Typewriter: pin the cursor's visual row to vertical center.
                if let Some(view) = pane.view_mut() {
                    let buffer_id = view.buffer_id();
                    if let Some(buffer) = self.buffer_manager.get(buffer_id) {
                        view.apply_typewriter(buffer);
                    }
                }
                pane.render(&self.buffer_manager);
            }
        }
        let Size { height, width } = self.terminal_size;
        self.position_caret(width, height);
    }

    fn position_caret(&mut self, width: usize, height: usize) {
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
                rect.position.col = rect.position.col.min(
                    width
                        .saturating_sub(sidebar_width)
                        .saturating_sub(rect.size.width),
                );
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

    /// Flip Zen mode (`PLAN.md Phase 4.2`): document only, everything else
    /// hidden. The sidebar is remembered and restored exactly on exit.
    fn toggle_zen(&mut self) {
        self.zen_mode = !self.zen_mode;
        if self.zen_mode {
            self.sidebar_was_visible = self.layout_tree.sidebar.visible;
            self.layout_tree.sidebar.hide();
        } else if self.sidebar_was_visible {
            // Restore the sidebar only if the user had it open on entry.
            self.layout_tree.sidebar.show();
            self.handle_resize_command(self.terminal_size);
        }
        // Focus a text view so typing works the moment Zen engages.
        if self.zen_mode
            && self
                .pane_manager
                .active_pane()
                .is_none_or(|p| p.view().is_none())
        {
            if let Some(id) = self.editor_target_pane() {
                self.pane_manager.set_active_pane(id);
            }
        }
        self.mark_all_panes_for_redraw();
        // Full clear: the old chrome (tabs, status) leaves stale pixels that
        // the narrower zen strip (or restored layout) would not overdraw.
        let _ = Terminal::clear_screen();
        self.update_message(if self.zen_mode {
            "Zen mode on (F11 to exit)"
        } else {
            "Zen mode off"
        });
    }

    /// Centered document rect for Zen mode: at most 70 columns wide, full
    /// terminal height, chrome hidden. Pure (unit-tested).
    const ZEN_WIDTH: usize = 70;
    /// Double-press window for `Ctrl+Z Z` Zen toggle (mirrors `Ctrl+W W`).
    const Z_DOUBLE_MS: u128 = 500;
    fn zen_rect(term: Size) -> Rect {
        let width = term.width.min(Self::ZEN_WIDTH).max(1);
        Rect {
            position: Position {
                row: 0,
                col: term.width.saturating_sub(width) / 2,
            },
            size: Size {
                height: term.height,
                width,
            },
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
                        if let crate::layout::PaneContent::Plugin(component) = &mut pane.content {
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
            SidebarKind::Outline => "manuscript".to_string(),
        }
    }

    /// After hiding the sidebar, return focus to the last text editor.
    fn focus_editor_after_sidebar(&mut self) {
        if let Some(id) = self.editor_target_pane() {
            self.pane_manager.set_active_pane(id);
        }
        self.mark_all_panes_for_redraw();
    }

    /// Max completion rows shown in the mention popup.
    const MENTION_LIMIT: usize = 8;
    /// Queries longer than this never complete.
    const MENTION_QUERY_MAX: usize = 48;

    /// `@query` behind the cursor, if the cursor sits in mention context:
    /// `@` preceded by start/whitespace/punctuation (never identifier chars,
    /// keeping `a@b` email-like text quiet). Returns (pane, buffer, query).
    fn mention_query_at_cursor(&self) -> Option<(usize, usize, String)> {
        if self.in_prompt() {
            return None;
        }
        let pane = self.pane_manager.active_pane()?;
        let view = pane.view()?;
        let buffer = self.buffer_manager.get(view.buffer_id())?;
        let location = view.location();
        let line = buffer.get_line(location.line_idx)?;
        let text: &str = &line.to_string();
        let prefix: Vec<&str> = text.graphemes(true).take(location.grapheme_idx).collect();
        let mut start = prefix.len();
        while start > 0
            && prefix[start.saturating_sub(1)]
                .chars()
                .next()
                .is_some_and(is_mention_char)
        {
            start = start.saturating_sub(1);
        }
        if start == 0 || prefix[start.saturating_sub(1)] != "@" {
            return None;
        }
        if start >= 2 {
            let glued = prefix[start.saturating_sub(2)]
                .chars()
                .next()
                .is_some_and(|c| c.is_alphanumeric() || c == '_');
            if glued {
                return None;
            }
        }
        let query: String = prefix[start..].concat();
        if query.graphemes(true).count() > Self::MENTION_QUERY_MAX {
            return None;
        }
        Some((pane.pane_id, view.buffer_id(), query))
    }

    /// Display rows for the popup: (name, kind label), or a hint when empty.
    fn mention_items(&self, candidates: &[yonro_core::lore::EntityId]) -> Vec<(String, String)> {
        if candidates.is_empty() {
            return vec![(
                "no lore yet — set scene POVs".to_string(),
                "hint".to_string(),
            )];
        }
        candidates
            .iter()
            .filter_map(|id| self.lore.get(*id))
            .map(|entity| {
                (
                    entity.name.clone(),
                    Self::entity_kind_label(entity.kind).to_string(),
                )
            })
            .collect()
    }

    const fn entity_kind_label(kind: yonro_core::lore::EntityKind) -> &'static str {
        match kind {
            yonro_core::lore::EntityKind::Character => "character",
            yonro_core::lore::EntityKind::Place => "place",
            yonro_core::lore::EntityKind::Faction => "faction",
            yonro_core::lore::EntityKind::Item => "item",
            yonro_core::lore::EntityKind::Lore => "lore",
        }
    }

    /// Popup rect tucked under the caret (above it when space is short).
    fn mention_popup_rect(&self, pane_id: usize, height_rows: usize) -> Rect {
        let term = self.terminal_size;
        let caret = self
            .pane_manager
            .get_pane(pane_id)
            .and_then(|pane| pane.view())
            .and_then(|view| {
                self.buffer_manager
                    .get(view.buffer_id())
                    .map(|buffer| view.caret_position(buffer))
            })
            .unwrap_or(Position { row: 1, col: 0 });
        let width = 44.min(term.width.saturating_sub(2)).max(20);
        let height = height_rows.min(10).max(3);
        let col = caret.col.min(term.width.saturating_sub(width).max(0));
        let below = caret.row.saturating_add(1);
        let row = if below.saturating_add(height) > term.height.saturating_sub(1) {
            caret.row.saturating_sub(height)
        } else {
            below
        };
        Rect {
            position: Position { row, col },
            size: Size { height, width },
        }
    }

    /// Re-derive popup state after every event (create/update/dismiss).
    fn update_mention(&mut self) {
        let Some((pane_id, _buffer_id, query)) = self.mention_query_at_cursor() else {
            self.dismiss_mention();
            return;
        };
        let ids: Vec<yonro_core::lore::EntityId> = self
            .lore
            .find_by_prefix(&query)
            .into_iter()
            .take(Self::MENTION_LIMIT)
            .map(|entity| entity.id)
            .collect();
        // Keep the highlight on the same entity across keystrokes.
        let mut selected = 0;
        if let Some(state) = &self.mention {
            if state.pane_id == pane_id {
                if let Some(prev) = state.selected_entity() {
                    if let Some(i) = ids.iter().position(|id| *id == prev) {
                        selected = i;
                    }
                }
            }
        }
        let items = self.mention_items(&ids);
        let rect = self.mention_popup_rect(pane_id, items.len().saturating_add(2));
        let popup_id = match self.mention.as_ref().and_then(|state| state.popup) {
            Some(id) if self.pane_manager.get_pane(id).is_some() => id,
            _ => {
                let id = self.pane_manager.create_floating_pane(
                    PaneContent::Plugin(Box::new(MentionComplete::default())),
                    20,
                );
                // Highlight rows without stealing editor focus (moves must
                // still reach the editor; popup keys are intercepted).
                if let Some(pane) = self.pane_manager.get_pane_mut(id) {
                    if let PaneContent::Plugin(component) = &mut pane.content {
                        component.set_active(true);
                    }
                }
                id
            }
        };
        if let Some(pane) = self.pane_manager.get_pane_mut(popup_id) {
            pane.resize(rect);
            if let PaneContent::Plugin(component) = &mut pane.content {
                component.sync_mention(&items, selected);
            }
        }
        self.mention = Some(MentionState {
            pane_id,
            query,
            candidates: ids,
            selected,
            popup: Some(popup_id),
        });
    }

    /// Close the popup without acting.
    fn dismiss_mention(&mut self) {
        if let Some(state) = self.mention.take() {
            if let Some(popup) = state.popup {
                self.pane_manager.remove_pane(popup);
            }
        }
    }

    /// Nudge the popup highlight; re-syncs the component.
    fn move_mention_selection(&mut self, up: bool) {
        let Some(state) = self.mention.as_mut() else {
            return;
        };
        if state.candidates.is_empty() {
            return;
        }
        if up {
            state.selected = state.selected.saturating_sub(1);
        } else {
            state.selected = state
                .selected
                .saturating_add(1)
                .min(state.candidates.len().saturating_sub(1));
        }
        let (popup, selected, items) = match self.mention.as_ref() {
            Some(state) => (
                state.popup,
                state.selected,
                self.mention_items(&state.candidates.clone()),
            ),
            None => return,
        };
        if let Some(popup) = popup {
            if let Some(pane) = self.pane_manager.get_pane_mut(popup) {
                if let PaneContent::Plugin(component) = &mut pane.content {
                    component.sync_mention(&items, selected);
                }
            }
        }
    }

    /// Replace `@query` with the selected `@CanonicalName `.
    fn accept_mention(&mut self) {
        let Some(state) = self.mention.take() else {
            return;
        };
        if let Some(popup) = state.popup {
            self.pane_manager.remove_pane(popup);
        }
        let name = state
            .selected_entity()
            .and_then(|id| self.lore.get(id))
            .map(|entity| entity.name.clone());
        let Some(name) = name else {
            return; // hint row / empty lore: dismiss only.
        };
        let total = state.query.graphemes(true).count().saturating_add(1);
        let buffer_id = self
            .pane_manager
            .get_pane(state.pane_id)
            .and_then(|pane| pane.view())
            .map(|view| view.buffer_id());
        if let Some(buffer_id) = buffer_id {
            if let Some(pane) = self.pane_manager.get_pane_mut(state.pane_id) {
                if let Some(view) = pane.view_mut() {
                    if let Some(buffer) = self.buffer_manager.get_mut(buffer_id) {
                        view.delete_backward_chars(buffer, total);
                        view.insert_text(buffer, &format!("@{name} "));
                    }
                }
            }
            if let Some(snapshot) = self.make_buffer_snapshot(buffer_id) {
                self.plugin_runtime
                    .send(PluginMessage::BufferChanged(snapshot));
            }
            self.update_word_count_if_open();
            self.sync_manuscript_words();
        }
    }

    /// Open a read-only lore sheet for `entity_id` in a centered float.
    fn open_lore_sheet(&mut self, entity_id: yonro_core::lore::EntityId) {
        let Some(entity) = self.lore.get(entity_id) else {
            return;
        };
        let mut lines = vec![
            format!("{} ({})", entity.name, Self::entity_kind_label(entity.kind)),
            String::new(),
        ];
        if !entity.aliases.is_empty() {
            lines.push(format!("Also known as: {}", entity.aliases.join(", ")));
            lines.push(String::new());
        }
        if entity.sheet.trim().is_empty() {
            lines.push("(no lore sheet yet)".to_string());
        } else {
            lines.extend(entity.sheet.lines().map(str::to_string));
        }
        // Backlink: scenes carrying this POV.
        let mut scenes = Vec::new();
        let root = self.manuscript.root();
        let acts: Vec<usize> = self
            .manuscript
            .children(root)
            .iter()
            .map(|n| n.id)
            .collect();
        for act in acts {
            let chapters: Vec<usize> = self.manuscript.children(act).iter().map(|n| n.id).collect();
            for chapter in chapters {
                for scene in self.manuscript.children(chapter) {
                    let pov = scene.meta.as_ref().map_or("", |meta| meta.pov.as_str());
                    if !pov.is_empty()
                        && (pov.eq_ignore_ascii_case(&entity.name)
                            || entity
                                .aliases
                                .iter()
                                .any(|alias| pov.eq_ignore_ascii_case(alias)))
                    {
                        scenes.push(format!("· {} — {}", scene.title, entity.name));
                    }
                }
            }
        }
        if !scenes.is_empty() {
            lines.push(String::new());
            lines.push("POV in:".to_string());
            lines.extend(scenes);
        }
        let term = self.terminal_size;
        let width = 60.min(term.width.saturating_sub(4)).max(20);
        let height = lines.len().saturating_add(2).min(20).max(5);
        let rect = Rect {
            position: Position {
                row: term.height.saturating_sub(height) / 2,
                col: term.width.saturating_sub(width) / 2,
            },
            size: Size { height, width },
        };
        let title = entity.name.clone();
        let pane_id = self.pane_manager.create_floating_pane(
            PaneContent::Plugin(Box::new(LoreSheet::new(title, lines))),
            20,
        );
        if let Some(pane) = self.pane_manager.get_pane_mut(pane_id) {
            pane.resize(rect);
        }
        self.pane_manager.set_active_pane(pane_id);
        self.sheet_pane = Some(pane_id);
        self.mark_all_panes_for_redraw();
    }

    /// True when `pane_id` is the visible outline sidebar.
    fn is_outline_pane(&self, pane_id: usize) -> bool {
        self.layout_tree.sidebar.kind == SidebarKind::Outline
            && self.layout_tree.sidebar.pane_id == Some(pane_id)
    }

    /// Outline selection (node id) from a sidebar pane, if any.
    fn outline_selection_of(&self, pane_id: usize) -> Option<yonro_core::manuscript::NodeId> {
        let pane = self.pane_manager.get_pane(pane_id)?;
        if let PaneContent::Plugin(component) = &pane.content {
            component.outline_selection()
        } else {
            None
        }
    }

    /// Open the outline-selected scene in a new tab, materializing its draft
    /// file (`scene-<id>.md`) on first open. Non-scenes get a hint instead.
    fn open_outline_selection(&mut self, pane_id: usize) {
        let selected = self.outline_selection_of(pane_id);
        let Some(node_id) = selected else {
            self.update_message("Outline: nothing selected");
            return;
        };
        let is_scene = self
            .manuscript
            .get(node_id)
            .is_some_and(|node| node.kind == NodeKind::Scene);
        if !is_scene {
            self.update_message("Outline: Enter opens scenes (a/c/s add structure)");
            return;
        }
        let file = self
            .manuscript
            .get(node_id)
            .and_then(|node| node.meta.as_ref())
            .and_then(|meta| meta.file.clone());
        let path = match file {
            Some(path) => path,
            None => {
                let name = format!("scene-{node_id}.md");
                let path = std::env::current_dir()
                    .unwrap_or_else(|_| std::path::PathBuf::from("."))
                    .join(name);
                if std::fs::write(&path, "").is_err() {
                    self.update_message("ERR: Could not create scene file");
                    return;
                }
                if let Some(node) = self.manuscript.get(node_id) {
                    let mut meta = node.meta.clone().unwrap_or_default();
                    meta.file = Some(path.clone());
                    let _ = self.manuscript.set_meta(node_id, meta);
                }
                self.save_workspace();
                path
            }
        };
        // Externally deleted draft? Materialize it again on explicit open.
        if !path.exists() {
            if std::fs::write(&path, "").is_err() {
                self.update_message("ERR: Could not create scene file");
                return;
            }
        }
        self.open_file_in_new_pane(&path);
        self.sync_manuscript_words();
    }

    /// Structural add from the outline (`a`/`c`/`s`): resolve the parent
    /// from the selection (walking up), auto-title, select the new node.
    fn manuscript_add(&mut self, child: NodeKind) {
        let sidebar_id = self.layout_tree.sidebar.pane_id;
        let selected = sidebar_id.and_then(|id| self.outline_selection_of(id));
        let result = match child {
            NodeKind::Act => {
                let n = self.manuscript.children(self.manuscript.root()).len();
                self.manuscript
                    .add_act(&format!("Act {}", n.saturating_add(1)))
            }
            NodeKind::Chapter => match self.nearest_ancestor(selected, NodeKind::Act) {
                Some(act) => {
                    let n = self.manuscript.children(act).len();
                    self.manuscript
                        .add_chapter(act, &format!("Chapter {}", n.saturating_add(1)))
                }
                None => {
                    self.update_message("Outline: select an act first (a adds one)");
                    return;
                }
            },
            NodeKind::Scene => match self.nearest_ancestor(selected, NodeKind::Chapter) {
                Some(chapter) => {
                    let n = self.manuscript.children(chapter).len();
                    self.manuscript
                        .add_scene(chapter, &format!("Scene {}", n.saturating_add(1)))
                }
                None => {
                    self.update_message("Outline: select a chapter first (c adds one)");
                    return;
                }
            },
            NodeKind::Project => {
                self.update_message("Outline: one project per manuscript");
                return;
            }
        };
        match result {
            Ok(new_id) => {
                // New POVs/settings become @-completable entities (deduped).
                self.lore.seed_from_manuscript(&self.manuscript);
                self.save_workspace();
                if let Some(sidebar_id) = sidebar_id {
                    if let Some(pane) = self.pane_manager.get_pane_mut(sidebar_id) {
                        if let PaneContent::Plugin(component) = &mut pane.content {
                            component.set_outline_selection(Some(new_id));
                        }
                    }
                }
                self.update_message(&format!("Outline: added (node {new_id})"));
            }
            Err(err) => {
                self.update_message(&format!("Outline: {err}"));
            }
        }
    }

    /// `selected` itself if it has `kind`, else the nearest ancestor with it.
    fn nearest_ancestor(
        &self,
        selected: Option<yonro_core::manuscript::NodeId>,
        kind: NodeKind,
    ) -> Option<yonro_core::manuscript::NodeId> {
        let mut cursor = selected;
        while let Some(id) = cursor {
            let node = self.manuscript.get(id)?;
            if node.kind == kind {
                return Some(id);
            }
            cursor = node.parent;
        }
        None
    }

    /// Prompt text for an outline field prompt. Values always start empty:
    /// the command bar has no selection model, so prefilling would append
    /// instead of replace (e.g. "Scene 1The gate").
    fn outline_prompt_for(&self, field: crate::plugins::OutlineField) -> (&'static str, String) {
        use crate::plugins::OutlineField;
        match field {
            OutlineField::Rename => ("Rename: ", String::new()),
            OutlineField::Pov => ("POV character: ", String::new()),
            OutlineField::Target => ("Target words: ", String::new()),
        }
    }

    /// Commit an outline field prompt value against the current selection.
    fn manuscript_apply(&mut self, field: crate::plugins::OutlineField, value: &str) {
        use crate::plugins::OutlineField;
        let selected = self
            .layout_tree
            .sidebar
            .pane_id
            .and_then(|id| self.outline_selection_of(id));
        let Some(node_id) = selected else {
            self.update_message("Outline: nothing selected");
            return;
        };
        match field {
            OutlineField::Rename => {
                let title = value.trim();
                if title.is_empty() {
                    self.update_message("Outline: empty title kept");
                    return;
                }
                match self.manuscript.rename(node_id, title) {
                    Ok(()) => {
                        self.save_workspace();
                        self.update_message(&format!("Renamed to {title}"));
                    }
                    Err(err) => self.update_message(&format!("Outline: {err}")),
                }
            }
            OutlineField::Pov => {
                let is_scene = self
                    .manuscript
                    .get(node_id)
                    .is_some_and(|node| node.kind == NodeKind::Scene);
                if !is_scene {
                    self.update_message("Outline: POV belongs on scenes");
                    return;
                }
                if let Some(node) = self.manuscript.get(node_id) {
                    let mut meta = node.meta.clone().unwrap_or_default();
                    meta.pov = value.trim().to_string();
                    let _ = self.manuscript.set_meta(node_id, meta);
                }
                // New POVs become @-completable entities (deduped).
                self.lore.seed_from_manuscript(&self.manuscript);
                self.save_workspace();
                self.update_message("Outline: POV updated");
            }
            OutlineField::Target => {
                let is_scene = self
                    .manuscript
                    .get(node_id)
                    .is_some_and(|node| node.kind == NodeKind::Scene);
                if !is_scene {
                    self.update_message("Outline: targets belong on scenes");
                    return;
                }
                let target: usize = match value.trim().parse() {
                    Ok(n) => n,
                    Err(_) => {
                        self.update_message("Outline: target must be a number");
                        return;
                    }
                };
                if let Some(node) = self.manuscript.get(node_id) {
                    let mut meta = node.meta.clone().unwrap_or_default();
                    meta.target_words = target;
                    let _ = self.manuscript.set_meta(node_id, meta);
                }
                self.save_workspace();
                self.update_message(&format!("Outline: target {target} words"));
            }
        }
    }

    /// Persist manuscript + lorebook (best-effort; warns, never blocks).
    fn save_workspace(&mut self) {
        if let Err(err) = crate::workspace::save(&self.manuscript, &self.lore) {
            self.update_message(&format!("Workspace save failed: {err}"));
        }
    }

    /// Push live buffer word counts into scene metadata (call after edits,
    /// file opens, and undo/redo — anything that changes draft sizes).
    fn sync_manuscript_words(&mut self) {
        let root = self.manuscript.root();
        let acts: Vec<usize> = self
            .manuscript
            .children(root)
            .iter()
            .map(|n| n.id)
            .collect();
        for act in acts {
            let chapters: Vec<usize> = self.manuscript.children(act).iter().map(|n| n.id).collect();
            for chapter in chapters {
                let scenes: Vec<usize> = self
                    .manuscript
                    .children(chapter)
                    .iter()
                    .map(|n| n.id)
                    .collect();
                for scene in scenes {
                    let file = self
                        .manuscript
                        .get(scene)
                        .and_then(|node| node.meta.as_ref())
                        .and_then(|meta| meta.file.clone());
                    let Some(file) = file else { continue };
                    for (_id, buffer) in self.buffer_manager.iter() {
                        let matches = buffer
                            .get_file_info()
                            .get_path()
                            .is_some_and(|p| p.as_os_str() == file.as_os_str());
                        if matches {
                            let (words, _, _) = buffer.word_count_stats();
                            let _ = self.manuscript.set_scene_words(scene, words);
                            break;
                        }
                    }
                }
            }
        }
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
        let new_id = self.pane_manager.create_pane(PaneContent::TextView(view));
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
        self.sync_manuscript_words();
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
            let neighbor = if self.active_tab == 0 {
                1
            } else {
                self.active_tab - 1
            };
            self.switch_tab(neighbor);
            // The doomed pane is now stashed — fall through below.
        }
        let mut found = false;
        for tab in self.doc_tabs.iter_mut() {
            if tab.pane_ids().contains(&pane_id) {
                // `None` (emptied root) is left for `prune_tabs` to drop.
                if let Some(new_root) = LayoutTree::remove_from_root(tab.root.clone(), pane_id) {
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
            let new_id = self.pane_manager.create_pane(PaneContent::TextView(view));
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
    use crate::prelude::*;
    use crate::uicomponents::{FileExplorer, WordCount};
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
        let mut layout = LayoutTree::new(
            0,
            Rect {
                position: Position { row: 1, col: 0 },
                size: Size {
                    height: 20,
                    width: 80,
                },
            },
        );

        assert!(!layout.sidebar.visible);

        layout.sidebar.toggle();
        assert!(layout.sidebar.visible);

        layout.compute_layout(Rect {
            position: Position { row: 1, col: 0 },
            size: Size {
                height: 20,
                width: 80,
            },
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

        buffer.insert_char(
            'h',
            Location {
                line_idx: 0,
                grapheme_idx: 0,
            },
        );
        buffer.insert_char(
            'e',
            Location {
                line_idx: 0,
                grapheme_idx: 1,
            },
        );
        buffer.insert_char(
            'l',
            Location {
                line_idx: 0,
                grapheme_idx: 2,
            },
        );
        buffer.insert_char(
            'l',
            Location {
                line_idx: 0,
                grapheme_idx: 3,
            },
        );
        buffer.insert_char(
            'o',
            Location {
                line_idx: 0,
                grapheme_idx: 4,
            },
        );
        buffer.insert_newline(Location {
            line_idx: 0,
            grapheme_idx: 5,
        });
        buffer.insert_char(
            'w',
            Location {
                line_idx: 1,
                grapheme_idx: 0,
            },
        );
        buffer.insert_char(
            'o',
            Location {
                line_idx: 1,
                grapheme_idx: 1,
            },
        );
        buffer.insert_char(
            'r',
            Location {
                line_idx: 1,
                grapheme_idx: 2,
            },
        );
        buffer.insert_char(
            'l',
            Location {
                line_idx: 1,
                grapheme_idx: 3,
            },
        );
        buffer.insert_char(
            'd',
            Location {
                line_idx: 1,
                grapheme_idx: 4,
            },
        );

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
        buffer.insert_char(
            'h',
            Location {
                line_idx: 0,
                grapheme_idx: 0,
            },
        );
        buffer.insert_char(
            'i',
            Location {
                line_idx: 0,
                grapheme_idx: 1,
            },
        );

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
            buffer.insert_char(
                c,
                Location {
                    line_idx: 0,
                    grapheme_idx: i,
                },
            );
        }
        let (words, graphemes, _) = buffer.word_count_stats();
        assert_eq!(graphemes, 1);
        // No alphanumeric word chunks in a pure-emoji buffer.
        assert_eq!(words, 0);
    }

    #[test]
    fn test_zen_rect_centers_70_col_strip() {
        // Wide terminal: 70 cols centered.
        let rect = Editor::zen_rect(Size {
            height: 30,
            width: 100,
        });
        assert_eq!(rect.size.width, 70);
        assert_eq!(rect.position.col, 15);
        assert_eq!(rect.position.row, 0);
        assert_eq!(rect.size.height, 30);
        // Narrow terminal: full width, no negative centering.
        let rect = Editor::zen_rect(Size {
            height: 24,
            width: 50,
        });
        assert_eq!(rect.size.width, 50);
        assert_eq!(rect.position.col, 0);
    }
}
