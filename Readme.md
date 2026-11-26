# Yonro — a narrative studio for fiction writers

Yonro is a **prototype** novel-writing studio: one headless narrative engine
(`yonro-core`) over the same workspace on disk. `yonro-tui` is the terminal.
`yonro-app` is the native page. `yonro-gui` (Tauri v2) is the older desktop
studio: outline, relationship graph, timeline, and lore. It started life
as a Hecto-tutorial terminal editor; the manuscript, lore, graph, and session
features are what it is now. The author is writing a novel inside it.


## demos

### page — yonro-app
![page](assets/page.png)

Warm paper, a seeded scene, underlined `@mentions`, and the format strip.
Captured from `yonro-app` opened on a sample novel.

### terminal — a scene, the outline, find
![tui demo](assets/tui.gif)

Seeded scene text (Mara, Mill farm, `@mentions`), the outline column, and
find. One frame of the recording prints a raw escape from the pty driver.

### terminal — split panes
![pane demo](assets/pane.gif)

Three panes, a divider drag, and a short typed line. This is the earlier
editor chrome, not the outline or zen pass. Recorded with
[asciinema](https://docs.asciinema.org/) (`assets/tui.cast` is the raw take
for the scene demo above).

### desktop — outline, graph, lore, write
![gui demo](assets/gui.gif)

`yonro-gui` on a seeded novel: the outline table, the graph lens strip, a
lore sheet, and the empty write view. A system privacy prompt sits along the
bottom of the recording. The graph canvas in this take is empty.


## what each crate does

### `yonro-core` — the engine (no UI)
Headless Rust library with zero UI dependencies. It computes every narrative
fact; frontends only render. Contents:

- manuscript tree: acts, chapters, scenes, word targets, progress rollups
- rope text buffers (`ropey`) with grapheme-correct word counts and undo
- lore book: entities, aliases, `@mention` parsing
- relationship graph from mention co-occurrence, plus saved lenses and an
  act-presence table
- timeline plus continuity notes
- project-wide search, manuscript export, history snapshots and recovery
- session log: words per day, streaks, goals
- alternate outline trees and hand-drawn custom graph canvases

### `yonro-tui` — the terminal frontend
A `crossterm` event loop over `yonro-core`: outline/binder sidebar (`Ctrl-O`),
file-explorer sidebar (`Ctrl-E`), document tabs, `@mention` autocomplete with
lore sheets, find, zen/typewriter mode, live word counts, mouse support, split
and floating panes with a command bar, and an async plugin runtime that hosts
the sidebars without blocking key input.

### `yonro-app` — the page
A native window. GPUI shapes the glyphs; the rope, the caret, and the marks
stay in our code. Open a workspace with
`cargo run -p yonro-app -- <workspace_dir>`.

### `yonro-gui` — the desktop frontend (Tauri v2)
The same core and the same files, behind five views: **Write**
(drafting with `@mention` autocomplete), **Outline** (manuscript table plus
alternate outlines), **Graph** (inferred relationships, custom canvases,
lenses), **Timeline** (scene order plus continuity notes), and **Lore**
(entity sheets, aliases, backlinks) — plus command palette, inspector,
session bar, and export. Only `crates/yonro-gui/ui/core.js` touches
`window.__TAURI__`; the rest is dependency-free static JS (no npm, no
bundler, no framework).


## on-disk format

The frontends read and write the same workspace:

```text
<root>/.yonro/{manuscript.json, lore.json, ...}
<root>/scene-<id>.md
```

New files under `.yonro/` may appear; existing JSON shapes only gain optional
fields. Missing or corrupt JSON falls back to Untitled/empty — never a crash.


## quick start

Requirements: stable Rust. On Linux, `yonro-gui` needs the WebKitGTK
libraries and `yonro-app` needs the X11 and Wayland client libraries.

```bash
git clone https://github.com/akshayrivers/Yonro-Text-Editor.git
cd Yonro-Text-Editor

# terminal editor (reads the .yonro workspace in <cwd>)
cargo run -p yonro-tui

# native page
cargo run -p yonro-app -- <workspace_dir>

# older desktop studio (paths resolve under your home folder)
cargo run -p yonro-gui -- <workspace_dir>

# sample novel (3 acts, 36 scenes, @mentions) for trying the views/graph
cargo run -p yonro-core --example seed -- /tmp/y --entities 250
```

New projects open on a ready-to-write skeleton (Act I / Chapter 1 / Scene 1).
The GUI keymap: press `?` outside the editor for the shortcut sheet, or run
palette `>keyboard shortcuts`. The full map lives in `AGENTS.md` under KEYMAP.


## keybindings (tui)

- `Ctrl-S` save · `Ctrl-Q` quit (repeats to force when dirty) · `Ctrl-F` find
- `Ctrl-Z` undo · `Ctrl-R` redo · `Ctrl-C`/`X`/`V` clipboard
- `Ctrl-E` explorer sidebar · `Ctrl-O` outline sidebar · `Ctrl-H` split pane
- `Ctrl-Space` command bar (`focus <id>`, `close`, `float`, `unfloat`, `explore`)
- `F11` (or double `Ctrl-Z`) zen/typewriter mode · `Esc` dismiss or close panel

With the outline focused: arrows navigate · `Enter` opens the scene in a new
tab · `a` / `c` / `s` add act / chapter / scene · `r` rename · `p` POV ·
`t` word target · `Delete` removes the node (scene files are kept). Typing
`@` in the editor offers lore autocomplete (`Enter` inserts, `Tab` shows the
lore sheet, `Esc` dismisses). Splits also resize by dragging dividers.


## architecture

```mermaid
graph TD
    Core[yonro-core: manuscript, lore, graph, timeline, search, export, sessions]
    TUI[yonro-tui: crossterm loop, sidebars, tabs, panes, plugin runtime]
    APP[yonro-app: native page]
    GUI[yonro-gui: Tauri commands + static ui, five views]
    Disk[.yonro JSON + scene .md files]
    Core --- Disk
    TUI -->|calls| Core
    APP -->|calls| Core
    GUI -->|calls| Core
```

`yonro-core` never touches a screen. The frontends never derive a narrative
fact. Details live in `architecture.md` (TUI loop, plugin messages, layout
tree, and the full Tauri command list behind `ui/core.js`).


## license

MIT. Started from the Hecto tutorial: https://www.flenker.blog/hecto/
