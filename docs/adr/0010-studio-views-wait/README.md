# 0010. Studio views wait on the page

Status: Accepted
Date: 2026-11-27

## Context

The reason to build this, rather than use Word, is the studio beside the page: cast, timeline, lore, and later geography, transport, resources, economics, magic, and a plot clock. Those views are why a novel is not a haiku.

The current graph is static co-occurrence. It is not the final cast model. A relationship that changes over story time is a later plan.

Rebuilding graph, timeline, and lore in GPUI before a paragraph can be written would repeat the Tauri mistake: chrome around a weak page.

## Decision

The page ships first. Graph, timeline, and lore stay computed in core. Their desks, when the profile shows them, render one line: "not on the page yet."

Not in this pass, and not in the near list: characters that write the story, a personal language model, plot-hole detection, readability scores, images, tables, track changes, live collaboration, headers, footers, page numbers, and print pagination.

Zen hides the extra tools. The page remains. F11 or cmd+. toggles it and scrolls the caret near 45% of the window.

## Consequences

The core graph, timeline, and lore APIs stay. The TUI can keep using them. The desktop page does not port `graph.js`.

The next plan starts when the page, the baseline marks, and the docx door are real.
