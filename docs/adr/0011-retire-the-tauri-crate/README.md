# 0011. The Tauri crate is retired

Status: Accepted
Date: 2026-11-27

## Context

`yonro-gui` was the Tauri shell. It stayed in the tree until the new window could open a scene, take a caret, undo, and save. Porting `editor.js` or `graph.js` would have kept two desktops alive.

Cutover was allowed only after that page was green. The format baseline was still required before the app was called done, and it landed before the crate was deleted.

## Decision

`crates/yonro-gui` is removed from the workspace. The dev loop is:

```bash
cargo run -p yonro-app -- <workspace_dir>
```

The working protocol no longer has the JavaScript gates: `node --check`, the `__TAURI__` seam, and the hex grep over `ui/`. Hex colors for the page live in `crates/yonro-app/src/theme.rs`.

`yonro-core` and `yonro-tui` stay. On-disk files stay readable by both.

## Consequences

There is one desktop binary. CI no longer sets up Node or WebKitGTK for this repo.

The GitHub token used for the cutover pull request could not update `.github/workflows/rust.yml` without the workflow scope. The workflow edit that drops the JavaScript steps may still be local until that scope is granted. The crate deletion does not depend on it.

Do not resurrect the Tauri crate to reach a feature. Add the feature to `yonro-app` and core.
