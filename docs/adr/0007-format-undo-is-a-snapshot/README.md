# 0007. Format undo is a snapshot

Status: Accepted
Date: 2026-11-27

## Context

Marks and paragraph props have to move with the text, and undo has to put them back. Inverse range math gets the live typing right and loses paragraph props when two lines join. A format-only change (bold, a heading, replace-all) is still one undo step even when the rope's shape is the part that is hard.

## Decision

On a new undo step, `EditLog` snapshots `Format` before it shifts ranges. A typing burst snapshots once, at the first character. Undo restores that snapshot. Redo restores the format from after the step. Live marks still shift so typing inside a mark grows it, and typing at the start edge does not pull the new graphemes in.

A format-only change is `EditOp::Format`. It records the caret and leaves the rope alone.

Replace-all is `EditOp::ReplaceAll`: every match, one snapshot, one mark shift, one undo. Matches are collected before the edit and applied from the end so earlier locations stay valid.

## Consequences

Undo of a joined line restores the dropped line's paragraph props, because they were in the snapshot.

The snapshot is the whole format, not a diff. A long scene with many marks copies that struct once per undo step. That cost is accepted at prototype size.

Mention accept is still delete plus insert, two undo steps. This record does not change that.
