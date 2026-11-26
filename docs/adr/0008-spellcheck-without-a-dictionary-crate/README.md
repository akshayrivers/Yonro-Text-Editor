# 0008. Spellcheck stays inside core

Status: Accepted
Date: 2026-11-27

## Context

The page should paint a squiggle under a misspelling. The check has to run in core, with no network and no account. A dictionary crate would be a new dependency, and a large word list would flag invented names, which is most of a novel.

## Decision

`misspelled_bytes` lives in `yonro-core`. No dictionary crate was added.

A word is a misspelling when it is in a short list of known slips (`teh`, `recieve`, `seperate`, and a few more) or when two adjacent letters of a common word are swapped. A word with an uppercase letter is left alone. The page paints those byte ranges as a wavy underline.

## Consequences

Ordinary words and names stay clean. Most real typos are not caught. That is the prototype boundary.

A dictionary crate, if one is added later, needs its own record and a one-paragraph justification before the dependency line.
