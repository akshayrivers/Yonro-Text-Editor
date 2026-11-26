# 0002. The editor stays ours

Status: Accepted
Date: 2026-11-27

## Context

Yonro already stores prose in a rope and moves by grapheme. Hindi, Urdu, and emoji have to move, delete, and wrap as one visible character. GPUI's input handler speaks UTF-16. A borrowed editor would hide that boundary and make the caret someone else's type.

The point of the desktop page is to learn how an editor works, in Rust, in pieces a senior can read, and then to write in it.

## Decision

The caret stored in the app is a `Location` (`grapheme_idx`, `line_idx`). UTF-16 exists only inside `EntityInputHandler`. `location_to_utf16` and `utf16_to_location` are the boundary. A UTF-16 index inside a multi-unit grapheme stays on that grapheme.

Typing, delete, paste, and newline go through core `EditLog`. A typing burst within 800ms is one undo step. A paste is one step. The TUI keeps its own `EditOperation` until a later unification.

Preedit uses `marked_text_range` and `replace_and_mark_text_in_range`. Commit replaces the marked span. There is no hidden HTML field.

## Consequences

Left and Backspace move one grapheme. The status column uses `unicode-width`, so a wide glyph advances the column by two.

Dead-key `é`, one non-BMP character, and a Devanagari cluster such as `क्ष` are tests of this boundary, not special cases in the view.

Autocomplete names still come from `lore_search`. The popup is only a view.
