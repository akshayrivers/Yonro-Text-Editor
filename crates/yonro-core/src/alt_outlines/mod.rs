//! Alternate outline structures (second trees).
//!
//! The manuscript stays exactly one. Everything here is an extra
//! act/chapter/scene tree beside it (a restructured draft, a "what if"
//! ordering) with its own draft files, so word counts never mix: alt
//! scenes live in `scene-alt<outline>-<node>.md`, never `scene-<id>.md`.
//! The GUI owns rendering; core only stores, validates, and persists.

use std::fmt;
use std::path::{Path, PathBuf};

use super::manuscript::{Manuscript, NodeId, NodeKind, SceneMeta};

/// Opaque id of one alternate outline (stable while it lives in the store).
pub type AltOutlineId = usize;

/// One extra structure tree plus its title.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AltOutline {
    /// Outline id, unique inside the store.
    pub id: AltOutlineId,
    /// Display title.
    pub title: String,
    /// The tree itself (same shape as the manuscript).
    pub manuscript: Manuscript,
}

/// Every alternate outline in a project (`.yonro/alt_outlines.json`).
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AltOutlineStore {
    /// Outlines in creation order.
    #[serde(default)]
    pub outlines: Vec<AltOutline>,
    #[serde(default)]
    next_id: AltOutlineId,
}

/// Validation / persistence failures (messages name the outline/node/file
/// and the reason).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AltOutlineError {
    /// Blank outline title.
    EmptyTitle,
    /// Unknown outline id.
    UnknownOutline(AltOutlineId),
    /// Unknown node id inside a known outline.
    UnknownNode(AltOutlineId, NodeId),
    /// Node exists but is not a scene.
    NotAScene(AltOutlineId, NodeId),
    /// Hierarchy violation or other structural failure (message names it).
    Structure(String),
    /// `alt_outlines.json` or a draft cannot be written (names the file).
    Io(String),
}

impl fmt::Display for AltOutlineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyTitle => write!(formatter, "outline title cannot be empty"),
            Self::UnknownOutline(id) => write!(formatter, "unknown outline {id}"),
            Self::UnknownNode(outline, node) => {
                write!(formatter, "outline {outline} has no node {node}")
            }
            Self::NotAScene(outline, node) => {
                write!(formatter, "outline {outline} node {node} is not a scene")
            }
            Self::Structure(message) | Self::Io(message) => write!(formatter, "{message}"),
        }
    }
}

impl std::error::Error for AltOutlineError {}

impl AltOutlineStore {
    /// Empty store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            outlines: Vec::new(),
            next_id: 0,
        }
    }

    /// All outlines in creation order.
    #[must_use]
    pub fn list(&self) -> &[AltOutline] {
        &self.outlines
    }

    /// One outline by id.
    #[must_use]
    pub fn get(&self, id: AltOutlineId) -> Option<&AltOutline> {
        self.outlines.iter().find(|outline| outline.id == id)
    }

    /// One outline's tree for structure editing.
    ///
    /// # Errors
    /// `UnknownOutline` for a bad id.
    pub fn manuscript_mut(&mut self, id: AltOutlineId) -> Result<&mut Manuscript, AltOutlineError> {
        self.outlines
            .iter_mut()
            .find(|outline| outline.id == id)
            .map(|outline| &mut outline.manuscript)
            .ok_or(AltOutlineError::UnknownOutline(id))
    }

    /// Create an empty outline titled `title`.
    ///
    /// # Errors
    /// `EmptyTitle` for a blank title.
    pub fn create(&mut self, title: &str) -> Result<AltOutlineId, AltOutlineError> {
        let title = title.trim().to_string();
        if title.is_empty() {
            return Err(AltOutlineError::EmptyTitle);
        }
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.outlines.push(AltOutline {
            id,
            title: title.clone(),
            manuscript: Manuscript::new(&title),
        });
        Ok(id)
    }

    /// Rename an outline (its tree title follows).
    ///
    /// # Errors
    /// `UnknownOutline` for a bad id, `EmptyTitle` for a blank title.
    pub fn rename(&mut self, id: AltOutlineId, title: &str) -> Result<(), AltOutlineError> {
        let title = title.trim().to_string();
        if title.is_empty() {
            return Err(AltOutlineError::EmptyTitle);
        }
        let outline = self
            .outlines
            .iter_mut()
            .find(|outline| outline.id == id)
            .ok_or(AltOutlineError::UnknownOutline(id))?;
        outline.title.clone_from(&title);
        let root = outline.manuscript.root();
        outline
            .manuscript
            .rename(root, &title)
            .map_err(|err| AltOutlineError::Structure(err.to_string()))?;
        Ok(())
    }

    /// Remove an outline and its tree (draft files stay on disk).
    ///
    /// # Errors
    /// `UnknownOutline` for a bad id.
    pub fn remove(&mut self, id: AltOutlineId) -> Result<(), AltOutlineError> {
        let at = self
            .outlines
            .iter()
            .position(|outline| outline.id == id)
            .ok_or(AltOutlineError::UnknownOutline(id))?;
        self.outlines.remove(at);
        Ok(())
    }

    /// Draft file backing an alt scene, creating
    /// `root/scene-alt<outline>-<scene>.md` on first use (own namespace, so
    /// alt word counts never touch the manuscript's).
    ///
    /// # Errors
    /// `UnknownOutline` / `UnknownNode` for bad ids, `NotAScene` for
    /// non-scenes, or `Io` when the file cannot be written.
    pub fn scene_file(
        &mut self,
        outline: AltOutlineId,
        scene: NodeId,
        root: &Path,
    ) -> Result<PathBuf, AltOutlineError> {
        let manuscript = self.manuscript_mut(outline)?;
        let stored = match manuscript.get(scene) {
            None => return Err(AltOutlineError::UnknownNode(outline, scene)),
            Some(node) => {
                if node.kind != NodeKind::Scene {
                    return Err(AltOutlineError::NotAScene(outline, scene));
                }
                node.meta.as_ref().and_then(|meta| meta.file.clone())
            }
        };
        if let Some(path) = stored {
            if path.exists() {
                return Ok(path);
            }
            std::fs::write(&path, "")
                .map_err(|err| AltOutlineError::Io(format!("{}: {err}", path.display())))?;
            return Ok(path);
        }
        let path = root.join(format!("scene-alt{outline}-{scene}.md"));
        if !path.exists() {
            std::fs::write(&path, "")
                .map_err(|err| AltOutlineError::Io(format!("{}: {err}", path.display())))?;
        }
        let meta = match manuscript.get(scene).and_then(|node| node.meta.clone()) {
            Some(mut meta) => {
                meta.file = Some(path.clone());
                meta
            }
            None => SceneMeta {
                file: Some(path.clone()),
                ..SceneMeta::default()
            },
        };
        manuscript
            .set_meta(scene, meta)
            .map_err(|_| AltOutlineError::NotAScene(outline, scene))?;
        Ok(path)
    }

    /// `alt_outlines.json` inside a workspace `.yonro/` dir.
    #[must_use]
    pub fn file_in(dot_yonro: &Path) -> PathBuf {
        dot_yonro.join("alt_outlines.json")
    }

    /// Load `path`, falling back to an empty store on any failure.
    ///
    /// Missing files (old workspaces) and corrupt JSON yield zero outlines —
    /// never an error, never a crash.
    #[must_use]
    pub fn load(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::new();
        };
        serde_json::from_str(&text).unwrap_or_else(|_| Self::new())
    }

    /// Persist as pretty JSON (creates parent dirs; atomic tmp + rename).
    ///
    /// # Errors
    /// `Io` when the directory cannot be created or any write/sync/rename
    /// step fails.
    pub fn save(&self, path: &Path) -> Result<(), AltOutlineError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|err| AltOutlineError::Io(format!("{}: {err}", parent.display())))?;
            }
        }
        let json = serde_json::to_string_pretty(&self)
            .map_err(|err| AltOutlineError::Io(format!("{}: {err}", path.display())))?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, json.as_bytes())
            .map_err(|err| AltOutlineError::Io(format!("{}: {err}", tmp.display())))?;
        let handle = std::fs::File::open(&tmp)
            .map_err(|err| AltOutlineError::Io(format!("{}: {err}", tmp.display())))?;
        handle
            .sync_all()
            .map_err(|err| AltOutlineError::Io(format!("{}: {err}", tmp.display())))?;
        drop(handle);
        std::fs::rename(&tmp, path)
            .map_err(|err| AltOutlineError::Io(format!("{}: {err}", path.display())))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outline_trees_keep_separate_draft_files() {
        let dir = std::env::temp_dir().join(format!("yonro-alt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut store = AltOutlineStore::new();
        assert!(store.create("  ").is_err());
        let id = store.create("Restructure").unwrap();
        assert!(store.rename(999, "Nope").is_err());
        store.rename(id, "Second pass").unwrap();
        assert_eq!(store.get(id).unwrap().title.as_str(), "Second pass");
        // Structure editing reuses the Manuscript API through manuscript_mut.
        let scene = {
            let tree = store.manuscript_mut(id).unwrap();
            let act = tree.add_act("Act I").unwrap();
            let ch = tree.add_chapter(act, "Chapter 1").unwrap();
            tree.add_scene(ch, "The gate").unwrap()
        };
        assert!(store.manuscript_mut(999).is_err());
        let file = store.scene_file(id, scene, &dir).unwrap();
        assert_eq!(file, dir.join(format!("scene-alt{id}-{scene}.md")));
        assert!(file.is_file());
        // Missing file loads as empty; round-trip preserves everything.
        let path = AltOutlineStore::file_in(&dir.join(".yonro"));
        assert!(AltOutlineStore::load(&path).list().is_empty());
        store.save(&path).unwrap();
        let back = AltOutlineStore::load(&path);
        assert_eq!(store, back);
        // Corrupt file falls back to empty, never crashes.
        std::fs::write(&path, "broken{{").unwrap();
        assert!(AltOutlineStore::load(&path).list().is_empty());
        store.remove(id).unwrap();
        assert!(store.get(id).is_none());
        // Drafts stay on disk after removal.
        assert!(file.is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
