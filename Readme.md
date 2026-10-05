# 🌲 Yonro Terminal-Based Text Editor

[![Build Status](https://github.com/akshayrivers/Text_Editorr/actions/workflows/rust.yml/badge.svg)](https://github.com/akshayrivers/Text_Editorr/actions/workflows/rust.yml)

Yonro is a highly enhanced, layered, and extensible terminal-based text editor written in Rust. Originally inspired by the [Hecto tutorial](https://www.flenker.blog/hecto/), Yonro has diverged significantly to incorporate a robust **split-pane layout tree**, multi-pane floating windows, an asynchronous **plugin architecture**, an active **File Explorer plugin**, and a robust command dispatcher.


## 📸 Demos

### Multi-Pane Splitting & Floating Windows
![pane](./pane.gif)

### File Editing and Highlighting
![demo](./demo.gif)



## 🚀 Quick Start

### Requirements
*   **Rust** (stable toolchain)
*   A terminal with **UTF-8** and **grapheme support**

### Build and Run
```bash
# Clone the repository
git clone https://github.com/akshayrivers/Text_Editor.git
cd Text_Editor

# Terminal editor
cargo run -p yonro-tui

# Desktop narrative studio (Tauri)
cargo run -p yonro-gui -- <workspace_dir>
```



## ⌨️ Keybindings & Controls

### Core Commands
*   `Ctrl-S`: Save current file
*   `Ctrl-Q`: Quit editor (warns if file has unsaved changes, press 3 times to force quit)
*   `Ctrl-F`: Interactive search within the active buffer (Esc to cancel, Arrows to navigate matches)
*   `Ctrl-Z`: Undo last editing step (supports word grouping on typing)
*   `Ctrl-R`: Redo last undone editing step
*   `Ctrl-E`: Toggle the built-in File Explorer floating pane
*   `Ctrl-Space`: Open the command bar for pane commands

### File Explorer Navigation
When the File Explorer pane is active:
*   `Up` / `Down` arrows: Select directory entry
*   `Enter`: Open file or traverse directory
*   `Esc`: Close File Explorer

### Split Resizing
*   Click and drag on horizontal or vertical dividers between tiled panes to resize them dynamically.


## 🖥️ GUI (Tauri narrative studio)

The `yonro-gui` crate is the visual half of the studio over the same `.yonro/` workspace the TUI reads: **Write** (distraction-free drafting with `@mention` autocomplete), **Outline** (manuscript table plus alternate outline trees), **Graph** (inferred relationship map, hand-drawn canvases, and saved lenses: Cast, Story map, Act presence), **Timeline** (outline-ordered scenes plus continuity notes), and **Lore** (entity sheets, aliases, POV backlinks).

```bash
cargo run -p yonro-gui -- <workspace_dir>
```

New projects open on a ready-to-write skeleton (Act I / Chapter 1 / Scene 1). Paths resolve under your home folder (`Desktop/novel` works; `/Desktop` is the disk root and is refused). A sample novel for trying the graph views:

```bash
cargo run -p yonro-core --example seed -- /tmp/y --entities 250
```

Keymap: press `?` outside the editor for the shortcut sheet, or run palette `>keyboard shortcuts` (`mod+Shift+P`). The full map lives in `AGENTS.md` under KEYMAP.

Adapter seam: only `crates/yonro-gui/ui/core.js` touches `window.__TAURI__`. Every other UI file calls `core.<fn>()`. All narrative facts (word counts, mentions, graph, timeline, filters, lenses) are computed in `yonro-core`; the UI only renders. See `architecture.md` for the command list and `ui/` file layout.

## 🏛️ Architecture & Layered Design

Yonro is built in robust layers, separating terminal rendering from state management and asynchronous operations.

```mermaid
graph TD
    subgraph Synchronous Core Loop
        Term[crossterm Inputs] -->|Wait For Event| Loop[Editor::run Event Loop]
        Loop -->|Draw Screen| UI[BufferBar / StatusBar / Panes / CommandBar]
        Loop -->|Sync Dispatch| Dispatch[HandlerRegistry]
        Dispatch -->|Modify| State[EditorContext]
    end

    subgraph Asynchronous Worker
        Loop -->|MPSC Tx Event| PluginRuntime[Plugin Tokio Runtime]
        PluginRuntime -->|Process on Background Thread| Plugins[Built-in/Custom Plugins]
        Plugins -->|MPSC Tx Response| Loop
    end
```

### 📦 Core Components

*   **`Editor`**: Integrates layout, buffers, rendering, and coordinates with the asynchronous plugin runtime.
*   **`LayoutTree`**: A recursive binary tree structure managing horizontal and vertical pane splits.
*   **`PaneManager`**: Controls both tiled and floating panes, handles pane selection, and tracks pane depth/Z-index.
*   **`BufferManager` & `Buffer`**: Owns text data, tracks file paths, dirty flags, and performs grapheme-level file reads/writes.
*   **`View`**: The text-rendering viewport inside a text pane, managing line highlights, local cursors, and scrolling.



## 🔌 Asynchronous Plugin System

Yonro features a background plugin runtime that runs on an independent Tokio worker thread. This keeps key input latency at virtually `0ms` even if plugins perform expensive disk, LSP, or network operations.

```mermaid
sequenceDiagram
    autonumber
    participant Core as Core Event Loop
    participant Worker as Background Plugin Thread
    participant Plug as Plugin (e.g. FileExplorer)

    Core->>Worker: send(PluginMessage::Event { event, active_pane_id })
    Worker->>Plug: on_event(&event, active_pane_id)
    Note over Plug: Process action asynchronously
    Plug->>Worker: Return PluginResponse (e.g. OpenFloatingPane)
    Worker->>Core: mpsc tx channel
    Note over Core: Core processes response in next tick
```

### Message and Response Pipeline
*   **`PluginMessage`**: Propagates events (`Event { event, active_pane_id }`), buffer updates (`BufferChanged(BufferSnapshot)`), and notifications (`PaneOpened { plugin_name, pane_id }`) to the plugin runtime.
*   **`PluginResponse`**: Plugins return responses to open floating panes (`OpenFloatingPane`), close panes (`ClosePane`), shift selections (`MoveInPane`), or output logs (`UpdateMessage`).



## 🛠️ Command Bar & Window Management

Pressing **`Ctrl-Space`** opens the Pane Command Bar. The following commands can be executed:

*   **`focus <id>`** (or just **`<id>`**): Swaps active window focus to the specified pane.
*   **`close <id>`** (or just **`close`**): Closes the specified (or active) pane.
*   **`float`**: Un-tiles the active pane and turns it into a floating pane.
*   **`unfloat`**: Re-tiles a floating pane back into the Layout Tree.
*   **`explore`**: Spawns a tiled File Explorer pane on the side.



## 📄 License

Yonro is licensed under the MIT License.
inspired by hecto: https://www.flenker.blog/hecto/
