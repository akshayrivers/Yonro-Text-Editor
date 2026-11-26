# 0001. GPUI is the desktop window

Status: Accepted
Date: 2026-11-27

## Context

The first desktop shell was Tauri around a textarea. Every keystroke sent the whole draft through JavaScript. A caret, a selection, and an input method cannot grow on that path without a second text engine in the browser.

Writing a GPU toolkit from scratch would teach shaders and delay the first typed sentence. CodeMirror and ProseMirror would replace the rope, the grapheme caret, and the undo log we already have.

## Decision

`yonro-app` is a native window on GPUI 0.2.2, pinned with `=0.2.2`. GPUI owns the window, glyph shaping, and the IME protocol (`EntityInputHandler`). The crate is pre-1.0. We do not float the version.

The license of that pin was read before the dependency landed. The Zed application is copyleft. The library crate is Apache-2.0. We do not vendor Zed's editor.

Official GPUI getting-started text says macOS or Linux. This machine is macOS.

## Consequences

A click updates the rope in-process. There is no webview and no bundler on the desktop path.

GPUI upgrades are a deliberate pin bump. Linux CI links the X11 and Wayland client libraries the window needs.

Raw wgpu stays out until a sentence can be written and formatted.
