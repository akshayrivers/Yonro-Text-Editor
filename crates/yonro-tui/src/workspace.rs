//! Workspace persistence (`PLAN.md Phase 4.3` follow-up): the manuscript
//! tree and lore book live in `<cwd>/.yonro/` as pretty JSON, saved after
//! every structural change (best-effort; a missing/unwritable directory only
//! skips the save, it never blocks editing).

use yonro_core::{LoreBook, Manuscript, Project};

/// Save both files via `Project` (atomic: write `*.tmp`, sync, rename).
///
/// # Errors
/// When the working directory is missing/unwritable or serialization fails.
pub fn save(manuscript: &Manuscript, lore: &LoreBook) -> Result<(), String> {
    let root = std::env::current_dir().map_err(|err| err.to_string())?;
    let project = Project {
        root,
        manuscript: manuscript.clone(),
        lore: lore.clone(),
        warnings: Vec::new(),
    };
    project.save().map_err(|err| err.to_string())
}

/// Load both files (`None` when absent or corrupt — the caller keeps a fresh
/// manuscript instead of failing startup).
#[must_use]
pub fn load() -> Option<(Manuscript, LoreBook)> {
    let root = std::env::current_dir().ok()?;
    let project = Project::load(&root);
    if project.warnings.is_empty() {
        Some((project.manuscript, project.lore))
    } else {
        None
    }
}
