//! Workspace project (`P0.1`): one workspace directory plus the two JSON
//! documents the TUI writes today (`manuscript.json`, `lore.json`) and the
//! `scene-<id>.md` draft files.
//!
//! * `Project::load` is infallible: missing/corrupt JSON falls back to
//!   defaults and records a warning naming the file and the reason.
//! * `Project::save` is atomic per file: write `*.tmp`, `sync_all`, rename.
//! * JSON shapes are unchanged from what `yonro-tui/src/workspace.rs` writes
//!   today (`serde_json::to_string_pretty`), so TUI and GUI stay compatible.

use std::fmt;
use std::path::{Path, PathBuf};

use super::lore::LoreBook;
use super::manuscript::{Manuscript, NodeId};

/// One workspace: root directory plus in-memory documents.
#[derive(Debug, Clone)]
pub struct Project {
    /// Workspace root (contains `.yonro/` and `scene-<id>.md` files).
    pub root: PathBuf,
    /// Manuscript tree.
    pub manuscript: Manuscript,
    /// Lore book.
    pub lore: LoreBook,
    /// Load warnings (`"<file>: <reason>"`), empty on a clean load.
    pub warnings: Vec<String>,
}

/// Persistence / lookup failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectError {
    /// Unknown manuscript node.
    UnknownScene(NodeId),
    /// Node exists but is not a scene.
    NotAScene(NodeId),
    /// Filesystem or serialization failure (message already includes the file).
    Io(String),
}

impl fmt::Display for ProjectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownScene(id) => write!(formatter, "unknown scene node {id}"),
            Self::NotAScene(id) => write!(formatter, "node {id} is not a scene"),
            Self::Io(message) => write!(formatter, "{message}"),
        }
    }
}

impl std::error::Error for ProjectError {}

impl Project {
    /// Load `<root>/.yonro/{manuscript.json,lore.json}`.
    ///
    /// Never fails: missing/corrupt files fall back to
    /// `Manuscript::new("Untitled")` / `LoreBook::default()` with one warning
    /// per bad file naming the file and the reason.
    #[must_use]
    pub fn load(root: &Path) -> Self {
        let mut warnings = Vec::new();
        let manuscript_path = root.join(".yonro/manuscript.json");
        let lore_path = root.join(".yonro/lore.json");

        let manuscript = match std::fs::read_to_string(&manuscript_path) {
            Err(err) => {
                warnings.push(format!("manuscript.json: {err}"));
                Manuscript::new("Untitled")
            }
            Ok(text) => match serde_json::from_str::<Manuscript>(&text) {
                Ok(manuscript) => manuscript,
                Err(err) => {
                    warnings.push(format!("manuscript.json: {err}"));
                    Manuscript::new("Untitled")
                }
            },
        };

        let lore = match std::fs::read_to_string(&lore_path) {
            Err(err) => {
                warnings.push(format!("lore.json: {err}"));
                LoreBook::default()
            }
            Ok(text) => match serde_json::from_str::<LoreBook>(&text) {
                Ok(lore) => lore,
                Err(err) => {
                    warnings.push(format!("lore.json: {err}"));
                    LoreBook::default()
                }
            },
        };

        Self {
            root: root.to_path_buf(),
            manuscript,
            lore,
            warnings,
        }
    }

    /// Save both JSON files atomically (write `*.tmp`, `sync_all`, rename).
    ///
    /// Creates `.yonro/` as needed. JSON encoding matches what the TUI writes
    /// today (pretty `serde_json`), so both frontends stay compatible.
    ///
    /// # Errors
    /// When the directory cannot be created, serialization fails, or any
    /// write/sync/rename step fails.
    pub fn save(&self) -> Result<(), ProjectError> {
        let dir = self.root.join(".yonro");
        std::fs::create_dir_all(&dir).map_err(|err| ProjectError::Io(format!(".yonro: {err}")))?;
        let manuscript_json = serde_json::to_string_pretty(&self.manuscript)
            .map_err(|err| ProjectError::Io(format!("manuscript.json: {err}")))?;
        write_atomic(&dir.join("manuscript.json"), manuscript_json.as_bytes())?;
        let lore_json = serde_json::to_string_pretty(&self.lore)
            .map_err(|err| ProjectError::Io(format!("lore.json: {err}")))?;
        write_atomic(&dir.join("lore.json"), lore_json.as_bytes())?;
        Ok(())
    }

    /// Draft file backing `scene`, creating `root/scene-<id>.md` on first use.
    ///
    /// When `meta.file` is `None`, creates an empty file at
    /// `root/scene-<id>.md` (leaving existing files untouched), points the
    /// scene at it, and saves the project so the link persists. Otherwise
    /// returns the stored path unchanged.
    ///
    /// # Errors
    /// `UnknownScene` for a bad id, `NotAScene` for non-scenes, or `Io` when
    /// the file cannot be created or the project cannot be saved.
    pub fn scene_file(&mut self, scene: NodeId) -> Result<PathBuf, ProjectError> {
        let stored = match self.manuscript.get(scene) {
            None => return Err(ProjectError::UnknownScene(scene)),
            Some(node) => {
                if node.kind != super::manuscript::NodeKind::Scene {
                    return Err(ProjectError::NotAScene(scene));
                }
                node.meta.as_ref().and_then(|meta| meta.file.clone())
            }
        };
        if let Some(path) = stored {
            return Ok(path);
        }
        let path = self.root.join(format!("scene-{scene}.md"));
        if !path.exists() {
            std::fs::write(&path, "")
                .map_err(|err| ProjectError::Io(format!("{}: {err}", path.display())))?;
        }
        let meta = match self
            .manuscript
            .get(scene)
            .and_then(|node| node.meta.clone())
        {
            Some(mut meta) => {
                meta.file = Some(path.clone());
                meta
            }
            None => super::manuscript::SceneMeta {
                file: Some(path.clone()),
                ..super::manuscript::SceneMeta::default()
            },
        };
        self.manuscript
            .set_meta(scene, meta)
            .map_err(|_| ProjectError::NotAScene(scene))?;
        self.save()?;
        Ok(path)
    }

    /// Sync a buffer's word count into the scene backed by `file`.
    ///
    /// Finds the scene whose `meta.file` equals `file` and records `words`
    /// via `set_scene_words`. Returns `true` when a scene matched.
    #[must_use]
    pub fn sync_scene_words(&mut self, file: &Path, words: usize) -> bool {
        let target = self.find_scene_by_file(file);
        match target {
            None => false,
            Some(id) => self.manuscript.set_scene_words(id, words).is_ok(),
        }
    }

    fn find_scene_by_file(&self, file: &Path) -> Option<NodeId> {
        let mut stack = vec![self.manuscript.root()];
        while let Some(id) = stack.pop() {
            let node = self.manuscript.get(id)?;
            let is_match = node.kind == super::manuscript::NodeKind::Scene
                && node
                    .meta
                    .as_ref()
                    .and_then(|meta| meta.file.as_deref())
                    .is_some_and(|path| path == file);
            if is_match {
                return Some(id);
            }
            for child in self.manuscript.children(id) {
                stack.push(child.id);
            }
        }
        None
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), ProjectError> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)
        .map_err(|err| ProjectError::Io(format!("{}: {err}", tmp.display())))?;
    let file = std::fs::File::open(&tmp)
        .map_err(|err| ProjectError::Io(format!("{}: {err}", tmp.display())))?;
    file.sync_all()
        .map_err(|err| ProjectError::Io(format!("{}: {err}", tmp.display())))?;
    drop(file);
    std::fs::rename(&tmp, path)
        .map_err(|err| ProjectError::Io(format!("{}: {err}", path.display())))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manuscript::SceneMeta;

    fn unique_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("yonro-project-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_project(dir: &Path) -> Project {
        let mut project = Project {
            root: dir.to_path_buf(),
            manuscript: Manuscript::new("Probe"),
            lore: LoreBook::new(),
            warnings: Vec::new(),
        };
        let act = project.manuscript.add_act("Act I").unwrap();
        let ch = project.manuscript.add_chapter(act, "Chapter 1").unwrap();
        let sc = project.manuscript.add_scene(ch, "The gate").unwrap();
        project
            .manuscript
            .set_meta(
                sc,
                SceneMeta {
                    pov: "Mara".to_string(),
                    current_words: 7,
                    ..SceneMeta::default()
                },
            )
            .unwrap();
        project
            .lore
            .add(crate::lore::EntityKind::Character, "Mara")
            .unwrap();
        project
    }

    #[test]
    fn roundtrip_keeps_manuscript_and_lore() {
        let dir = unique_dir("roundtrip");
        let project = sample_project(&dir);
        project.save().unwrap();
        let loaded = Project::load(&dir);
        assert!(loaded.warnings.is_empty());
        assert_eq!(loaded.manuscript.title(), "Probe");
        assert_eq!(loaded.manuscript.subtree_words(loaded.manuscript.root()), 7);
        assert_eq!(loaded.lore.live_count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_json_falls_back_with_warning_naming_file() {
        let dir = unique_dir("corrupt");
        std::fs::create_dir_all(dir.join(".yonro")).unwrap();
        std::fs::write(dir.join(".yonro/manuscript.json"), "not json{{").unwrap();
        std::fs::write(dir.join(".yonro/lore.json"), "{}").unwrap();
        let loaded = Project::load(&dir);
        assert_eq!(loaded.manuscript.title(), "Untitled");
        assert_eq!(loaded.lore.live_count(), 0);
        assert_eq!(loaded.warnings.len(), 2);
        assert!(loaded.warnings[0].contains("manuscript.json"));
        assert!(loaded.warnings[1].contains("lore.json"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_files_fall_back_with_warnings() {
        let dir = unique_dir("missing");
        let loaded = Project::load(&dir);
        assert_eq!(loaded.manuscript.title(), "Untitled");
        assert_eq!(loaded.warnings.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_save_leaves_no_tmp_files() {
        let dir = unique_dir("atomic");
        let project = sample_project(&dir);
        project.save().unwrap();
        let entries: Vec<_> = std::fs::read_dir(dir.join(".yonro"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert!(entries.iter().any(|p| p.ends_with("manuscript.json")));
        assert!(entries.iter().any(|p| p.ends_with("lore.json")));
        assert!(!entries
            .iter()
            .any(|p| p.extension().is_some_and(|ext| ext == "tmp")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scene_file_is_idempotent_and_preserves_prose() {
        let dir = unique_dir("scenefile");
        let mut project = sample_project(&dir);
        project.save().unwrap();
        let scene = {
            let root = project.manuscript.root();
            let act = project.manuscript.children(root)[0].id;
            let ch = project.manuscript.children(act)[0].id;
            project.manuscript.children(ch)[0].id
        };
        let first = project.scene_file(scene).unwrap();
        assert_eq!(first, dir.join(format!("scene-{scene}.md")));
        std::fs::write(&first, "hello world").unwrap();
        let second = project.scene_file(scene).unwrap();
        assert_eq!(first, second);
        assert_eq!(std::fs::read_to_string(&second).unwrap(), "hello world");
        // Reload keeps the link.
        let reloaded = Project::load(&dir);
        let file = reloaded
            .manuscript
            .get(scene)
            .and_then(|node| node.meta.as_ref())
            .and_then(|meta| meta.file.clone())
            .unwrap();
        assert_eq!(file, first);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sync_scene_words_updates_only_matching_scene() {
        let dir = unique_dir("sync");
        let mut project = sample_project(&dir);
        project.save().unwrap();
        let scene = {
            let root = project.manuscript.root();
            let act = project.manuscript.children(root)[0].id;
            let ch = project.manuscript.children(act)[0].id;
            project.manuscript.children(ch)[0].id
        };
        let path = project.scene_file(scene).unwrap();
        assert!(project.sync_scene_words(&path, 42));
        assert_eq!(
            project.manuscript.subtree_words(project.manuscript.root()),
            42
        );
        assert!(!project.sync_scene_words(&dir.join("other.md"), 99));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
