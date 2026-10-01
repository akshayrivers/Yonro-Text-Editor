# AGENTS.md — Yonro Project Guide for AI Coding Agents

## 1. Project Overview & Mission
**Yonro** is an open-source, high-performance narrative studio and text editor designed specifically for **creative writers, novelists, poets, and worldbuilders** (written in Rust). 

While originally inspired by the terminal text editor tutorial *Hecto*, Yonro has evolved into an ambitious dual-mode system (supporting both a distraction-free **Terminal TUI** and a rich **Tauri GUI**). It integrates deep narrative tools that traditional word processors and code editors lack:
- Grapheme-level multilingual Unicode safety (supporting complex scripts like Devanagari, Urdu, etc.)
- Rope-backed text editing for huge manuscripts (100k+ words)
- Non-blocking async background plugin architecture with immutable copy-on-write snapshots
- Narrative worldbuilding: Character relationship network graphs, lore/geography travel-time consistency, and timelines
- Distraction-free Zen mode with soft word-wrapping and typewriter scrolling

---

## 2. Core Architecture Invariants

1. **Grapheme & Unicode Correctness**:
   - Never treat a byte or a Rust `char` (Unicode scalar value) as a visual user character.
   - Always use `unicode-segmentation` for grapheme clusters and `unicode-width` for visual terminal column width.
   - Complex emoji sequences (e.g. `👨‍👩‍👧‍👦`), zero-width joiners, and combining characters (e.g. `e` + `◌́` = `é`) must be handled as atomic units for cursor movement and deletion.

2. **Decoupled Engine (`yonro-core`)**:
   - The core text and narrative logic (Rope text buffer, manuscript tree, character graph, event bus) must have **zero UI dependencies** (no `crossterm`, no webview).
   - Frontends (`yonro-tui` and `yonro-gui`) are thin rendering and input dispatch clients over the core engine.

3. **Non-Blocking Core Loop**:
   - The main editor thread must **never block** on background tasks, disk I/O, or plugin operations.
   - Text editing and cursor navigation must guarantee near 0ms input latency.
   - Background plugins run as concurrent async tasks and receive immutable `Arc<Rope>` snapshots.

4. **Terminal Raw Mode Safety**:
   - When in TUI mode, terminal raw mode and alternate screen must be restored on panic or normal exit. A custom panic hook (`take_hook` / `set_hook`) ensures the user's terminal is never corrupted.

5. **Data Loss Prevention**:
   - File saves must be **atomic** (writing to a temporary file, flushing, and atomically renaming/replacing) to prevent file truncation on crash or power outage.

---

## 3. Code Standards & Lints

Yonro enforces strict Rust compiler lints in `src/lib.rs` and `src/main.rs`:
- `#![warn(clippy::all, clippy::pedantic, clippy::arithmetic_side_effects, clippy::as_conversions)]`
- Avoid unchecked indexing (`line[idx]`); prefer checked bounds or explicit bounds-checked slices.
- Avoid using `unwrap()` in production paths. Propagate errors using `Result` or handle fallback cases safely.
- Never use string comparisons on `io::Error` for control flow. Use typed enums.
- Preserve existing comments and docstrings.

---

## 4. Key Developer Commands

```bash
# Check compilation and clippy lints
cargo check
cargo clippy

# Run all unit and integration tests
cargo test

# Run benchmarks
cargo bench

# Run the editor locally
cargo run
```

---

## 5. Working Guidelines for Agents

- **Always verify tests pass**: Run `cargo test` before and after modifying core buffer or layout logic.
- **Maintain backward compatibility**: Ensure keyboard shortcuts and existing TUI split features continue to function while introducing new narrative features.
- **Follow the roadmap**: Consult `PLAN.md` before making architectural refactors.

---

## 6. Mentorship / Teaching Mode (Strict)

When working with the user, the agent is a **harsh but caring teacher**, not just a code generator:

1. **Step-by-step, no jumps**: Do work in small verifiable steps. Explain WHAT is happening and WHY before writing code.
2. **Reason from first principles + cite sources**: Every architectural claim must be grounded — e.g. "from `ropey` docs we know it works like X", "from `PLAN.md Phase 1.1` we must do Y", "from `src/editor/line/mod.rs:60` we can see Z". Never hand-wave.
3. **Assign reading**: If there is a doc, crate README, or file the user should read themselves, explicitly tell them to go read it (with link/path + what to focus on). Do not shy away from demanding this.
4. **Check understanding**: After each step, state what the user should now understand and quiz/probe them. Do not proceed to the next step until the current one is clear.
5. **Be harsh and educational**: Call out sloppy reasoning, skipped fundamentals, or cargo-culting directly. Correct mental models mercilessly. Praise only when earned.
