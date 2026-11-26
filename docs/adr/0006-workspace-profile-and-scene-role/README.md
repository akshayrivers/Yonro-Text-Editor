# 0006. Profile and role choose the desk

Status: Accepted
Date: 2026-11-27

## Context

One editor has to hold poems, stories, and novels. A haiku does not need a relationship graph. A novel does. Verse does not need POV and setting chrome while that scene is focused. The graph can stay for the rest of the book.

Poem subtypes (haiku, lyrical, ballad, narrative, blank verse) are labels, not engines. Plays, screenplays, and research papers are later presets on the same registry.

## Decision

`project.json` has `profile`: `novel` (default), `story`, or `pieces`. A missing or corrupt field loads as novel. Saving sessions must keep the profile.

`SceneMeta.role` is `prose` (default) or `verse`. Missing means prose. `NodeKind` does not change.

- Novel shows page, outline, graph, timeline, and lore.
- Story hides the graph.
- Pieces is the page and a flat list. The binder calls a scene a piece. Under the hood, pieces still stores one act and one chapter, and add-piece is still add-scene.

Verse focus hides the mention list. The inspector slot for that scene is the title.

Graph, timeline, and lore tabs may show the line "not on the page yet." They are not rebuilt in this pass.

A new workspace with no project yet offers `novel`, `story`, and `pieces` on one line.

## Consequences

Old projects open as novels. Old scenes open as prose. The manuscript tree shape is unchanged, so the TUI still loads it.

Hiding a tab does not delete graph or lore data. The files stay. The desk is quiet.
