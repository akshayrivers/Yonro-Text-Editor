//! Workspace persistence (`PLAN.md Phase 4.3` follow-up): the manuscript
//! tree and lore book live in `<cwd>/.yonro/` as pretty JSON, saved after
//! every structural change (best-effort; a missing/unwritable directory only
//! skips the save, it never blocks editing).

use std::path::PathBuf;
use yonro_core::{LoreBook, Manuscript};

fn dir() -> Option<PathBuf> {
    std::env::current_dir().ok().map(|cwd| cwd.join(".yonro"))
}

/// Save both files, creating `.yonro/` as needed.
///
/// # Errors
/// When the working directory is missing/unwritable or serialization fails.
pub fn save(manuscript: &Manuscript, lore: &LoreBook) -> Result<(), String> {
    let dir = dir().ok_or_else(|| "no working directory".to_string())?;
    std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let manuscript_json =
        serde_json::to_string_pretty(manuscript).map_err(|err| err.to_string())?;
    std::fs::write(dir.join("manuscript.json"), manuscript_json).map_err(|err| err.to_string())?;
    let lore_json = serde_json::to_string_pretty(lore).map_err(|err| err.to_string())?;
    std::fs::write(dir.join("lore.json"), lore_json).map_err(|err| err.to_string())?;
    Ok(())
}

/// Load both files (`None` when absent or corrupt — the caller keeps a fresh
/// manuscript instead of failing startup).
#[must_use]
pub fn load() -> Option<(Manuscript, LoreBook)> {
    let dir = dir()?;
    let manuscript: Manuscript =
        serde_json::from_str(&std::fs::read_to_string(dir.join("manuscript.json")).ok()?).ok()?;
    let lore: LoreBook =
        serde_json::from_str(&std::fs::read_to_string(dir.join("lore.json")).ok()?).ok()?;
    Some((manuscript, lore))
}
