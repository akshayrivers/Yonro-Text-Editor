# 0005. Docx is a door, not the daily file

Status: Accepted
Date: 2026-11-27

## Context

A `.docx` is a zip of XML. Rewriting it on every keystroke is slow and easy to corrupt. The TUI cannot treat it as the buffer. Word is still how a draft moves to and from other people.

Hand-writing an ECMA parser would make the OOXML crate the editor. The editor is the rope and the marks.

## Decision

Import reads a docx into the rope and the marks. Export writes a new docx. Neither call runs on the keystroke path. Export never overwrites the source file.

The pinned crates are `docx` `=1.1.2` (PoiScript/docx-rs, MIT) and `zip` `=0.5.13`, the same package reader `docx` already uses, so a drawing can be named when the document model skips it. Do not float them.

Map only the baseline: paragraphs, bold, italic, underline, strike, headings, lists, links, alignment, and size. Our export uses numbering id 1 for bullets and id 2 for numbers. Foreign numbering.xml is not read, so a foreign numbered list may import as bullets. Highlight has no field on this crate's run properties, so it does not round-trip. Any text color imports as swatch 1.

If the package has images, tables, or tracked changes, keep the original and say those parts were skipped. A drawing inside a run can fail the document model. Fall back to the plain text of `w:t` nodes. Still do not modify the file.

## Consequences

Devanagari survives import and export because it is just text in the rope. Images do not. The original docx remains the copy that still has them.

A second OOXML crate needs a new record before it is added.
