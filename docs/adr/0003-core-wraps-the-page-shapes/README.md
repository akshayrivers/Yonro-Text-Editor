# 0003. Core wraps, the page shapes

Status: Accepted
Date: 2026-11-27

## Context

Word counts, mention parsing, wrap widths, and the caret have to agree. If the page wraps with the shaper and core wraps with column widths, a click and a word count land on different graphemes.

Pixels are a view concern. `Location` and columns are a core concern.

## Decision

Core produces visual rows (`Line::wrap_segments`, about 70 columns). GPUI shapes the glyphs of a row core already produced. It does not re-segment graphemes. `unicode-segmentation` stays in core.

A click picks the row from `y`, then the shaper's closest glyph boundary inside that row, then core turns that byte into a grapheme `Location`. Caret column in the status line stays `unicode-width`.

The app does not count words. `Buffer::word_count_stats` does.

## Consequences

Proportional Georgia can place a caret the column count would miss. Hit testing uses the shaped line. The status column can still disagree with the painted x of a proportional glyph. That split is intentional.

Do not virtualize rows until a real scene janks.
