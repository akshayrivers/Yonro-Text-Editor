//! Recent workspaces (`P5.1`): the `recents.json` list behind the start screen.
//!
//! * `Recents` owns the read/write logic; the GUI only resolves the file
//!   path (`app_config_dir/recents.json`) and passes opaque `last_opened`
//!   display strings (core never reads the clock, so tests need none).
//! * `load` is infallible: missing/corrupt JSON yields an empty list, never
//!   a crash. Entries whose directory vanished are dropped on load and by
//!   [`Recents::prune_missing`]. The list is capped at 10, most recent first.

use std::fmt;
use std::path::{Path, PathBuf};

/// Maximum remembered workspaces (most recent first).
pub const MAX_RECENTS: usize = 10;

/// One remembered workspace for the start screen.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RecentEntry {
    /// Workspace root, as given by the user (absolute or `~`-free).
    pub path: String,
    /// Project title at the time it was last opened.
    pub title: String,
    /// Opaque display string (`"2026-10-04 09:12"`, …); core never parses it.
    #[serde(default)]
    pub last_opened: String,
}

/// On-disk shape: `{ "recent": [...] }` (missing key reads as empty).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct RecentsFile {
    #[serde(default)]
    recent: Vec<RecentEntry>,
}

/// Most-recent-first workspace list (already pruned + capped).
#[derive(Debug, Clone, Default)]
pub struct Recents {
    entries: Vec<RecentEntry>,
}

/// Persistence failures (message already names the file).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecentsError {
    /// Directory creation, write, sync, or rename failed.
    Io(String),
}

impl fmt::Display for RecentsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(message) => write!(formatter, "{message}"),
        }
    }
}

impl std::error::Error for RecentsError {}

impl Recents {
    /// Empty list.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Entries, most recent first.
    #[must_use]
    pub fn list(&self) -> &[RecentEntry] {
        &self.entries
    }

    /// Number of remembered workspaces.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the list is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `recents.json` inside the Tauri app config dir.
    #[must_use]
    pub fn file_in(config_dir: &Path) -> PathBuf {
        config_dir.join("recents.json")
    }

    /// Load `path`, falling back to empty on any failure.
    ///
    /// Corrupt JSON, missing files, and entries whose directory no longer
    /// exists all yield fewer (possibly zero) entries — never an error.
    #[must_use]
    pub fn load(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::new();
        };
        let file: RecentsFile = match serde_json::from_str(&text) {
            Ok(file) => file,
            Err(_) => return Self::new(),
        };
        let mut recent = Self {
            entries: file.recent,
        };
        recent.prune_missing();
        recent.truncate();
        recent
    }

    /// Persist as pretty JSON (creates parent dirs; atomic tmp + rename).
    ///
    /// # Errors
    /// `Io` when the directory cannot be created or any write/sync/rename
    /// step fails.
    pub fn save(&self, path: &Path) -> Result<(), RecentsError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|err| RecentsError::Io(format!("{}: {err}", parent.display())))?;
            }
        }
        let file = RecentsFile {
            recent: self.entries.clone(),
        };
        let json = serde_json::to_string_pretty(&file)
            .map_err(|err| RecentsError::Io(format!("{}: {err}", path.display())))?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, json.as_bytes())
            .map_err(|err| RecentsError::Io(format!("{}: {err}", tmp.display())))?;
        let handle = std::fs::File::open(&tmp)
            .map_err(|err| RecentsError::Io(format!("{}: {err}", tmp.display())))?;
        handle
            .sync_all()
            .map_err(|err| RecentsError::Io(format!("{}: {err}", tmp.display())))?;
        drop(handle);
        std::fs::rename(&tmp, path)
            .map_err(|err| RecentsError::Io(format!("{}: {err}", path.display())))?;
        Ok(())
    }

    /// Remember `path` as most recent (upsert; refreshes title + timestamp).
    pub fn touch(&mut self, path: &str, title: &str, last_opened: &str) {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return;
        }
        self.entries.retain(|entry| entry.path != trimmed);
        self.entries.insert(
            0,
            RecentEntry {
                path: trimmed.to_string(),
                title: title.to_string(),
                last_opened: last_opened.to_string(),
            },
        );
        self.truncate();
    }

    /// Forget `path` (no-op when absent).
    pub fn remove(&mut self, path: &str) {
        self.entries.retain(|entry| entry.path != path);
    }

    /// Drop entries whose directory no longer exists.
    pub fn prune_missing(&mut self) {
        self.entries.retain(|entry| Path::new(&entry.path).is_dir());
    }

    fn truncate(&mut self) {
        self.entries.truncate(MAX_RECENTS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_dir(name: &str) -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("yonro-recents-{name}-{n}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn workspace_with(dir: &Path, name: &str) -> String {
        let root = dir.join(name);
        std::fs::create_dir_all(&root).unwrap();
        root.to_string_lossy().to_string()
    }

    #[test]
    fn missing_file_loads_empty() {
        let dir = unique_dir("missing");
        let recent = Recents::load(&dir.join("recents.json"));
        assert!(recent.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_file_loads_empty() {
        let dir = unique_dir("corrupt");
        std::fs::write(dir.join("recents.json"), "not json{{").unwrap();
        assert!(Recents::load(&dir.join("recents.json")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn touch_upserts_to_front_and_roundtrips() {
        let dir = unique_dir("touch");
        let first = workspace_with(&dir, "first");
        let second = workspace_with(&dir, "second");
        let mut recent = Recents::new();
        recent.touch(&first, "First", "2026-10-01");
        recent.touch(&second, "Second", "2026-10-02");
        assert_eq!(recent.len(), 2);
        assert_eq!(recent.list()[0].path, second);
        // Re-touching moves to front and refreshes the title + timestamp.
        recent.touch(&first, "First v2", "2026-10-03");
        assert_eq!(recent.list()[0].path, first);
        assert_eq!(recent.list()[0].title, "First v2");
        assert_eq!(recent.list()[0].last_opened, "2026-10-03");
        recent.save(&dir.join("recents.json")).unwrap();
        let loaded = Recents::load(&dir.join("recents.json"));
        assert_eq!(loaded.list(), recent.list());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn caps_at_ten_most_recent() {
        let dir = unique_dir("cap");
        let mut recent = Recents::new();
        for index in 0..12 {
            let path = workspace_with(&dir, &format!("ws-{index}"));
            recent.touch(&path, &format!("W{index}"), "2026-10-04");
        }
        assert_eq!(recent.len(), MAX_RECENTS);
        assert_eq!(recent.list()[0].title, "W11");
        assert!(!recent.list().iter().any(|e| e.title == "W0"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn drops_entries_whose_dir_vanished() {
        let dir = unique_dir("prune");
        let kept = workspace_with(&dir, "kept");
        let gone = dir.join("gone").to_string_lossy().to_string();
        let mut recent = Recents::new();
        recent.touch(&kept, "Kept", "2026-10-04");
        recent.touch(&gone, "Gone", "2026-10-04");
        assert_eq!(recent.len(), 2);
        recent.prune_missing();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent.list()[0].path, kept);
        // Load prunes too: persist without the check, delete, reload.
        let mut raw = Recents::new();
        raw.entries.push(RecentEntry {
            path: kept.clone(),
            title: "Kept".to_string(),
            last_opened: String::new(),
        });
        raw.entries.push(RecentEntry {
            path: gone.clone(),
            title: "Gone".to_string(),
            last_opened: String::new(),
        });
        let path = dir.join("recents.json");
        raw.save(&path).unwrap();
        let loaded = Recents::load(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded.list()[0].path, kept);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ignores_blank_paths() {
        let mut recent = Recents::new();
        recent.touch("   ", "Blank", "2026-10-04");
        assert!(recent.is_empty());
    }
}
