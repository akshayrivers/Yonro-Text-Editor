# 0004. Plain prose plus a format sidecar

Status: Accepted
Date: 2026-11-27

## Context

The recording rejected living in markdown. Obsidian's graph was useful. `**bold**` in the draft was not. The TUI still has to open the same scene file and read words.

Bold, italic, headings, lists, alignment, and one anchored note are document facts. They are not characters in the prose.

## Decision

The daily save is two files:

- `scene-<id>.md` holds plain words. Bold does not insert asterisks.
- `.yonro/format.json` holds marks, paragraph props, face, spacing, and the one note, keyed by scene file name.

Missing or corrupt `format.json` means no marks. It never crashes. Old scene files still open as plain prose. New fields are additive and use `#[serde(default)]`.

`EditLog` shifts mark and note ranges on insert and delete, and undo restores the format from the snapshot taken before that step.

Headings, alignment, lists, and indent are paragraph props. They paint. They do not rewrite the manuscript tree.

## Consequences

Both the TUI and the desktop page can read the prose. Only the page paints the sidecar.

A writer who edits the scene file by hand keeps the words and may leave marks pointing at the wrong graphemes until the next edit shifts them. That is accepted. The sidecar is not a second source of the words.
