# 0009. Zoom is view state

Status: Accepted
Date: 2026-11-27

## Context

Word and Docs let a writer zoom the page from about 80% to 200%. Zoom changes how large the glyphs are. It does not change the draft, the marks, or the dirty dot.

The desktop app has no `localStorage`. Manuscript data must not be written there. A zoom value is a preference, not a scene fact.

## Decision

Zoom is a field on the page, clamped from 0.8 to 2.0. It scales paint and hit testing. It does not call `EditLog`, it does not set the buffer dirty, and it is not written to `.yonro/`.

`cmd+shift+=` zooms in. `cmd+shift+-` zooms out. `cmd+0` returns to 100%. The status line shows the percent when it is not 100.

Face and line spacing (compact, roomy, airy) are document facts. They live in the format sidecar. Zoom does not.

## Consequences

Quit and reopen, and the zoom is 100% again. Persisting it is a later preference file, still outside the manuscript.

Font size on a selection is a mark (`MarkKind::Size`). It is not zoom.
