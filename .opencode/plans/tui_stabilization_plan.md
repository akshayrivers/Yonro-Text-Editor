# TUI Stabilization Plan — Phase 3.5 Extended

## Goals
1. **FileExplorer as permanent left sidebar** (toggleable with Ctrl+E)
2. **BufferBar shows panes with their buffers** — close button closes pane, click focuses pane
3. **Fix close/minimize button hit-testing** for all panes (tiled + floating)
4. **Word count plugin** (Ctrl+WW) as floating pane
5. **Closing buffer/pane updates BufferBar** correctly

---

## Architecture Changes

### 1. Sidebar System (`crates/yonro-tui/src/layout/sidebar.rs` — NEW)
- `Sidebar` struct managing left/right sidebar state
- `SidebarKind::{FileExplorer, WordCount, Custom}`
- Fixed width (configurable, default 30 cols)
- Toggle visibility (Ctrl+E for FileExplorer, Ctrl+WW for WordCount)
- Integrated into `LayoutTree` as a special non-splittable region

### 2. BufferBar → PaneBar (`crates/yonro-tui/src/uicomponents/panebar.rs` — REPLACE bufferbar.rs)
- Shows **pane tabs** (not buffer tabs)
- Each tab: `[pane_id: buffer_name]` with close button (✕)
- Active pane highlighted (reverse video)
- Click tab → focus that pane
- Click close → close that pane
- Minimized panes shown in separate section

### 3. Unified Pane Title Bar Rendering
- **Single source of truth** for title bar rendering + hit-testing
- `Pane::draw_title_bar()` handles all pane types (TextView, Plugin, Popup)
- Buttons drawn at consistent positions: `[─] [✕]` at `width-7` to `width-1`
- Component `render_content()` only draws INSIDE the border

### 4. Plugin System Enhancements
- `FileExplorerPlugin` → manages sidebar (not floating pane)
- New `WordCountPlugin` (Ctrl+WW) → floating pane
- Plugins register sidebar kind + toggle keybinding

---

## Detailed Task Breakdown

### Task 1: Create Sidebar System
**Files:** `crates/yonro-tui/src/layout/sidebar.rs` (new), `layout/mod.rs`, `editor.rs`
- `Sidebar { kind: SidebarKind, visible: bool, width: usize, pane_id: Option<usize> }`
- `SidebarKind::FileExplorer | WordCount | Custom(Box<dyn PluginComponent>)`
- LayoutTree reserves leftmost `width` columns when sidebar visible
- `Editor::toggle_sidebar(kind)` shows/hides and creates/removes pane

### Task 2: Replace BufferBar with PaneBar
**Files:** `crates/yonro-tui/src/uicomponents/panebar.rs` (new), `editor.rs`, `uicomponents/mod.rs`
- Renders pane tabs: `[0: main.rs] [1: Cargo.toml] [2: README.md]`
- Each tab has inline close button: `[0: main.rs ✕]`
- Hitboxes map to `pane_id` (not `buffer_id`)
- Minimized panes section unchanged
- `mark_all_panes_for_redraw` triggers PaneBar redraw

### Task 3: Unified Title Bar Rendering
**Files:** `crates/yonro-tui/src/layout/pane.rs`, `uicomponents/fileexplorer.rs`, `uicomponents/plugin_component.rs`
- `Pane::draw_title_bar(active: bool, is_floating: bool)` draws:
  - Pane ID + active indicator: `─ [0]* `
  - Buttons at fixed positions: `[─][✕]` at `col + width - 7` to `col + width - 1`
- `FileExplorer::draw_frame()` REMOVED — uses Pane's title bar
- `FileExplorer::render_content(rect)` draws ONLY file list (no border)
- PluginComponent trait: remove `render()` requirement, keep `render_content()`

### Task 4: Fix Hit-Testing
**Files:** `crates/yonro-tui/src/layout/pane.rs`, `command_dispatcher/handlers/mouse.rs`
- Single `is_on_close_button(pos)`, `is_on_min_button(pos)`, `is_on_title_bar(pos)` on Pane
- Uses `self.rect` (Pane's rect) — consistent for all pane types
- Floating panes: component doesn't draw buttons, Pane draws them
- Mouse handler: `pane.is_on_close_button(position)` works for all

### Task 5: FileExplorer as Sidebar
**Files:** `crates/yonro-tui/src/plugins/builtin/file_explorer_plugin.rs`, `editor.rs`
- `FileExplorerPlugin::on_event`:
  - Ctrl+E → `PluginResponse::ToggleSidebar { kind: SidebarKind::FileExplorer }`
  - No more floating pane creation
- `on_pane_opened` stores sidebar pane_id for focus management
- Enter key → `SelectInPane` (existing, works)
- Esc → close sidebar (toggle)

### Task 6: Word Count Plugin (Ctrl+WW)
**Files:** `crates/yonro-tui/src/plugins/builtin/word_count_plugin.rs` (new), `plugins/mod.rs`, `builtin/mod.rs`
- `WordCountPlugin` — floating pane, shows:
  - Words, Characters, Lines, Reading time
  - Updates on buffer change (via `on_buffer_change` with `Arc<Rope>` snapshot)
- Ctrl+WW → `PluginResponse::OpenFloatingPane` with word count component
- Component: simple `UIComponent` showing stats, no input handling needed

### Task 7: Buffer/Pane Close Sync
**Files:** `crates/yonro-tui/src/editor.rs`, `command_dispatcher/handlers/mouse.rs`
- `close_pane` already calls `mark_all_panes_for_redraw` → triggers PaneBar redraw
- When buffer has no panes viewing it → buffer can be GC'd (optional)
- PaneBar automatically reflects current panes on next render

---

## Implementation Order

| Phase | Task | Dependencies |
|-------|------|--------------|
| 1 | Sidebar system (layout) | — |
| 2 | PaneBar (replace BufferBar) | Sidebar (for minimized section) |
| 3 | Unified title bar + hit-testing | PaneBar (for consistent rendering) |
| 4 | FileExplorer as sidebar | Sidebar, unified title bar |
| 5 | Word count plugin | Unified title bar, plugin system |
| 6 | Integration testing | All above |

---

## Key Design Decisions

1. **Sidebar in LayoutTree**: Not a regular pane — reserved space, always on left, fixed width. Simpler than splitting.
2. **PaneBar shows panes**: Matches user mental model (close tab = close pane). Buffer switching via command palette or pane focus.
3. **Title bar on Pane**: Single rendering path. Components only render content. Eliminates floating vs tiled divergence.
4. **Word count as floating**: Transient info panel, not persistent UI.

---

## Testing Checklist

- [ ] Ctrl+E toggles FileExplorer sidebar (left, 30 cols)
- [ ] FileExplorer arrow keys navigate, Enter opens file in active pane
- [ ] PaneBar shows all panes with correct buffer names
- [ ] Click PaneBar tab → focuses that pane
- [ ] Click PaneBar close (✕) → closes that pane, Bar updates
- [ ] Close last pane in split → remaining pane expands
- [ ] Ctrl+WW opens word count floating pane with live stats
- [ ] Word count updates on typing/editing
- [ ] Floating pane drag respects boundaries (BufferBar, StatusBar, CommandBar)
- [ ] Minimize/maximize works for all pane types
- [ ] No visual glitches on resize/split/drag

---

## Files to Modify/Create

### New Files
- `crates/yonro-tui/src/layout/sidebar.rs`
- `crates/yonro-tui/src/uicomponents/panebar.rs`
- `crates/yonro-tui/src/plugins/builtin/word_count_plugin.rs`

### Modified Files
- `crates/yonro-tui/src/layout/mod.rs` — export sidebar
- `crates/yonro-tui/src/layout/layout_tree.rs` — reserve sidebar space
- `crates/yonro-tui/src/layout/pane.rs` — unified title bar, hit-testing
- `crates/yonro-tui/src/uicomponents/fileexplorer.rs` — remove draw_frame, use render_content
- `crates/yonro-tui/src/uicomponents/plugin_component.rs` — simplify trait
- `crates/yonro-tui/src/uicomponents/mod.rs` — export PaneBar, remove BufferBar
- `crates/yonro-tui/src/plugins/builtin/mod.rs` — export WordCountPlugin
- `crates/yonro-tui/src/plugins/builtin/file_explorer_plugin.rs` — sidebar integration
- `crates/yonro-tui/src/command_dispatcher/handlers/mouse.rs` — unified hit-testing
- `crates/yonro-tui/src/editor.rs` — sidebar toggle, PaneBar integration
- `crates/yonro-tui/src/main.rs` — help text updates