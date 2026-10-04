# Yonro Text Editor: Architecture & Plugin Workflow

This document provides a detailed overview of the Yonro text editor's architecture, including clear flow diagrams for input handling, the command dispatcher, the layout tree split system, and the background plugin architecture. It also lists the bugs identified and fixed.



## 1. General Architecture Overview
Yonro uses a **Layered Architecture** designed to keep the core terminal rendering and input loop highly responsive and synchronous, while extending capabilities (like the File Explorer) through an asynchronous **Plugin Runtime**.

```mermaid
graph TD
    A[Terminal Input] -->|Raw crossterm Event| B[Terminal::wait_for_event]
    B -->|Convert to EditorEvent| C[Editor::run - Event Loop]
    C -->|Dispatch Command Sync| D[HandlerRegistry]
    C -->|Forward Event Async| E[PluginRuntime]
    D -->|Modify State| F[EditorContext / PaneManager / BufferManager]
    E -->|Process & Return PluginResponse| C
    C -->|Refresh Screen| G[Terminal Screen Output]
```



## 2. The Main Event Loop Flow

The `Editor::run` event loop runs continuously on the main thread, completing four steps in each cycle:

```mermaid
sequenceDiagram
    autonumber
    participant Core as Core Event Loop
    participant Plugins as Plugin Runtime Thread
    participant Term as Terminal / OS
    
    rect rgb(240, 240, 240)
        Note over Core, Plugins: Step 1: Process Plugin Responses
        Core->>Core: Drain responses from PluginRuntime rx channel
        loop for each response
            Core->>Core: apply_plugin_response(response)
        end
    end
    
    rect rgb(230, 240, 230)
        Note over Core: Step 2: Inject Custom Pending Events
        Core->>Core: Dispatch events in pending_events queue
    end
    
    rect rgb(230, 230, 245)
        Note over Core, Term: Step 3: Render Screen
        Core->>Term: Render BufferBar, CommandBar, Tiled & Floating Panes
        Core->>Term: Move Caret to focus location & flush screen
    end
    
    rect rgb(245, 230, 230)
        Note over Core, Term: Step 4: Await & Dispatch Next Input Event
        Term->>Core: KeyPress / MouseClick / Resize
        Core->>Core: handle_event(event) (Sync Dispatcher)
        Core->>Plugins: send(PluginMessage::Event + active_pane_id) (Async Channel)
    end
```



## 3. Command Dispatcher Flow

Keystrokes and mouse events are converted from Crossterm-specific events into `EditorEvent`s and then into `Command`s. The `HandlerRegistry` dispatches these command objects to registered handler traits.

```mermaid
graph TD
    E[EditorEvent] -->|TryFrom| C{Command}
    C -->|MoveCommand| MH[MoveHandler]
    C -->|EditCommand| EH[EditHandler]
    C -->|SystemCommand| SH[SystemHandler]
    C -->|MouseCommand| MoH[MouseHandler]
    
    subgraph Registry Dispatch Sequence
        PA[PromptAwareHandler] -->|Intercepts if prompt active| SD[Search / Save / Pane prompts]
        PA -->|pass-through if no prompt| SH
        SH -->|pass-through| EH
        EH -->|pass-through| MH
        MH -->|pass-through| MoH
    end
```



## 4. Plugin Architecture and Messaging

The plugin runtime runs on its own background thread powered by a single-threaded Tokio runtime. Communication is done entirely through asynchronous `std::sync::mpsc` channels.

### Message Flow Diagram

```mermaid
graph LR
    subgraph Core Thread
        E[Editor Loop] -->|tx.send| MSG[PluginMessage]
        RESP[PluginResponse] -->|rx.try_recv| E
    end
    
    subgraph Tokio Background Thread
        MSG -->|mpsc rx| WORKER[plugin_worker]
        WORKER -->|on_event / on_buffer_change| PLUGINS[Plugins]
        PLUGINS -->|tx.send| RESP
    end
```

### Messaging Protocol

*   **`PluginMessage` (Core $\rightarrow$ Plugins)**:
    *   `LoadPlugin(Box<dyn Plugin>)`: Dynamically loads a plugin into the worker.
    *   `Event { event, active_pane_id }`: Relays key, mouse, and resize events along with the current focused pane ID.
    *   `BufferChanged(BufferSnapshot)`: Sent whenever a text buffer is modified.
    *   `PaneOpened { plugin_name, pane_id }`: Notifies a plugin that a pane it requested was successfully opened by the core.
    *   `Shutdown`: Triggers dynamic unloading of all plugins.
*   **`PluginResponse` (Plugins $\rightarrow$ Core)**:
    *   `OpenFloatingPane`: Requests a new window for custom UI.
    *   `ClosePane` / `ToggleMinimize`: Resizes or dismisses panes.
    *   `MoveInPane` / `SelectInPane` / `MouseClickInPane`: Intercepts and delegates actions to custom components.
    *   `UpdateMessage`: Writes text to the editor message bar.



## 5. GUI adapter seam (`yonro-gui`)

All narrative facts are computed in `yonro-core`; the Tauri frontend only renders. The seam is the `#[tauri::command]` set in `crates/yonro-gui/src/main.rs` — the single list a future pure-web build reimplements over WASM:

`get_outline` / `get_stats` / `get_lore` / `get_workspace` / `open_workspace` / `create_workspace` / `open_file` / `set_text` / `save_file` / `close_buffer` / `undo_buffer` / `redo_buffer` / `get_graph` / `get_timeline` / `add_node` / `rename_node` / `move_node` / `remove_node` / `set_scene_meta` / `get_scene` / `open_scene` / `list_files` / `add_entity` / `update_entity` / `remove_entity` / `lore_search` / `get_entity` / `get_mentions` / `search_buffer` / `search_project` / `get_session` / `set_goal` / `export_manuscript` / `get_history` / `restore_snapshot` / `sweep_recovery` / `check_recovery` / `discard_recovery` (plus `get_workspace_dir`).

Only `crates/yonro-gui/ui/core.js` touches `window.__TAURI__`. Every other file calls `core.<fn>()`.

`ui/` file layout (classic `<script>` files, loaded in order, no modules/bundler):

- `index.html` — shell: topnav tablist, binder/inspector drawers, five `tabpanel` views, palette/find/prose overlays, statusline with `aria-live` slot.
- `theme.js` / `themes.css` — theme choice + all color tokens (only file with hex colors).
- `styles.css` — layout only, `var(--token)` colors.
- `core.js` — adapter seam (above).
- `util.js` — `esc`, `isMod`, `el`, `debounce`, `setMessage`, `errText`, `openModal`.
- `app.js` — shell boot, `COMMANDS` registry (palette `>` mode + shortcut-sheet source), view switching (lazy per-view loads), global keymap, drawers, zen.
- `editor.js` — write view: local dirty flag + debounced `set_text` sync, tabs, find/highlight backdrop, `@` autocomplete, save flows, recovery.
- `outline.js` — outline table + binder tree (roving tabindex, inline add/rename/remove, context menu, files section).
- `graph.js` — SVG relationship graph (layout in JS, facts from core), spotlight + keyboard walk.
- `timeline.js` — chapter-grouped entries + continuity cards, POV filter (UI-only).
- `lore.js` — entity list + detail (aliases, sheet, scene links), add/delete dialogs.
- `inspector.js` — selection inspector (scene meta form, entity sheet, history + restore).
- `workspace.js` — start screen, workspace switch/create, warnings banner.
- `palette.js` — `<dialog>` command palette (scenes/files, `>` commands, `@` entities, `/` project search).
- `find.js` — find bar over `search_buffer` spans.
- `session.js` — session bar/streaks + goal dialog.
- `export.js` — export flows with receipt message.

## 6. Layout Tree and Pane Management

Yonro uses a **Binary Layout Tree** representing tiled windows. Splits are vertical or horizontal.

```mermaid
graph TD
    Root[SplitNode Vertical, ratio: 0.3]
    Root -->|First 30%| Pane0[Leaf: Pane 0 - File Explorer]
    Root -->|Second 70%| Split2[SplitNode Horizontal, ratio: 0.5]
    Split2 -->|Top 50%| Pane1[Leaf: Pane 1 - TextView]
    Split2 -->|Bottom 50%| Pane2[Leaf: Pane 2 - TextView]
```

### Window Management Flow (Floating, Minimizing, Resizing)

*   **Splitting Tiled Panes**: When a split command is executed, the leaf node is replaced by a split node containing two leaves (original pane and a new pane).
*   **Floating Panes**: Float panes are managed by `PaneManager` directly rather than being inside the `LayoutTree`. They are drawn on layer 10+ sorted by `z-index`.
*   **Split Dragging**: A click inside the tolerance zone of a divider registers a `dragging_split` handle. Dragging adjusts the split ratio, triggering `handle_resize` to recalculate all layouts.