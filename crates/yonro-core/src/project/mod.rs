//! Workspace project (`P0.1`): one workspace directory plus the two JSON
//! documents the TUI writes today (`manuscript.json`, `lore.json`) and the
//! `scene-<id>.md` draft files.
//!
//! * `Project::load` is infallible: missing/corrupt JSON falls back to
//!   defaults and records a warning naming the file and the reason.
//! * `Project::save` is atomic per file: write `*.tmp`, `sync_all`, rename.
//! * JSON shapes are unchanged from what `yonro-tui/src/workspace.rs` writes
//!   today (`serde_json::to_string_pretty`), so TUI and GUI stay compatible.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use super::lore::LoreBook;
use super::manuscript::{Manuscript, NodeId, NodeKind};
use super::session::{DayRecord, SessionLog};

/// Scene metadata fields the GUI inspector edits.
///
/// `file` and `current_words` are backend-owned and never overwritten
/// through this shape. Additive `serde(default)` fields keep the on-disk
/// format compatible.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SceneMetaFields {
    /// Point-of-view character.
    #[serde(default)]
    pub pov: String,
    /// Where the scene happens.
    #[serde(default)]
    pub setting: String,
    /// In-story date, free-form.
    #[serde(default)]
    pub story_date: String,
    /// In-story time, free-form.
    #[serde(default)]
    pub story_time: String,
    /// One-line reminder of what happens.
    #[serde(default)]
    pub synopsis: String,
    /// Draft word-count goal (`0` = no target).
    #[serde(default)]
    pub target_words: usize,
}

/// `project.json` shape: workspace-level settings (additive over time).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct ProjectFile {
    /// Daily word goal (`0` = none).
    #[serde(default)]
    daily_goal: usize,
}

/// One history snapshot: file name plus draft words at the time.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HistoryEntry {
    /// Snapshot file name (`<unix-ts>.md`, `-n` suffixed on collisions).
    pub name: String,
    /// Draft words in the snapshot.
    pub words: usize,
}

/// Snapshots kept per scene (oldest pruned first).
pub const MAX_SNAPSHOTS_PER_SCENE: usize = 50;

/// Minimum age of the newest snapshot before `save` takes another.
pub const SNAPSHOT_THROTTLE_SECS: u64 = 600;

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
    /// Unknown manuscript node (structure editing).
    UnknownNode(NodeId),
    /// `kind` was not `"act"`, `"chapter"`, or `"scene"`.
    BadKind(String),
    /// Title trimmed to empty.
    EmptyTitle,
    /// Hierarchy violation or other structural failure (message names it).
    Structure(String),
    /// Filesystem or serialization failure (message already includes the file).
    Io(String),
    /// Lore entity failure (readable error message).
    Lore(String),
}

impl fmt::Display for ProjectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownScene(id) => write!(formatter, "unknown scene node {id}"),
            Self::NotAScene(id) => write!(formatter, "node {id} is not a scene"),
            Self::UnknownNode(id) => write!(formatter, "unknown manuscript node {id}"),
            Self::BadKind(kind) => write!(
                formatter,
                "unknown node kind {kind:?}: expected act, chapter, or scene"
            ),
            Self::EmptyTitle => write!(formatter, "title cannot be empty"),
            Self::Structure(message) | Self::Io(message) | Self::Lore(message) => {
                write!(formatter, "{message}")
            }
        }
    }
}

impl std::error::Error for ProjectError {}

impl Project {
    /// Whether `root` already holds a project (`manuscript.json` exists).
    ///
    /// Corrupt JSON still counts as a project (load warns but opens it);
    /// only a missing file means "no project yet" for the start screen.
    #[must_use]
    pub fn has_project(root: &Path) -> bool {
        root.join(".yonro/manuscript.json").is_file()
    }

    /// Resolve a UI-typed workspace path (`~` expanded, trimmed).
    ///
    /// # Errors
    /// `Structure` when the trimmed input is empty.
    pub fn resolve_workspace_path(raw: &str) -> Result<PathBuf, ProjectError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(ProjectError::Structure("path cannot be empty".to_string()));
        }
        if trimmed == "~" || trimmed.starts_with("~/") {
            if let Ok(home) = std::env::var("HOME") {
                let rest = trimmed.strip_prefix('~').unwrap_or(trimmed);
                return Ok(PathBuf::from(home + rest));
            }
        }
        Ok(PathBuf::from(trimmed))
    }

    /// Create a fresh workspace at `root` titled `title`.
    ///
    /// Runs `mkdir -p` for `root`, writes empty `.yonro/` documents, and
    /// returns the loaded-feeling project (no warnings on a fresh create).
    ///
    /// # Errors
    /// `EmptyTitle` for a blank title, or `Io` when the directory or the
    /// initial documents cannot be written.
    pub fn create(root: &Path, title: &str) -> Result<Self, ProjectError> {
        let title = title.trim();
        if title.is_empty() {
            return Err(ProjectError::EmptyTitle);
        }
        std::fs::create_dir_all(root)
            .map_err(|err| ProjectError::Io(format!("{}: {err}", root.display())))?;
        let project = Self {
            root: root.to_path_buf(),
            manuscript: Manuscript::new(title),
            lore: LoreBook::default(),
            warnings: Vec::new(),
        };
        project.save()?;
        Ok(project)
    }

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

    /// Parse a GUI `kind` string (`"act"`, `"chapter"`, `"scene"`;
    /// case-insensitive, surrounding whitespace ignored).
    ///
    /// # Errors
    /// `BadKind` naming the offending value.
    pub fn parse_node_kind(kind: &str) -> Result<NodeKind, ProjectError> {
        match kind.trim().to_lowercase().as_str() {
            "act" => Ok(NodeKind::Act),
            "chapter" => Ok(NodeKind::Chapter),
            "scene" => Ok(NodeKind::Scene),
            _ => Err(ProjectError::BadKind(kind.to_string())),
        }
    }

    /// Add a structural node and persist immediately.
    ///
    /// `parent: None` places an act under the project root; chapters and
    /// scenes need an explicit parent (the error says so). Titles are
    /// trimmed and must not be empty.
    ///
    /// # Errors
    /// `BadKind`, `EmptyTitle`, `UnknownNode` for a bad parent, `Structure`
    /// for hierarchy violations, or `Io` when the save fails.
    pub fn add_node(
        &mut self,
        parent: Option<NodeId>,
        kind: &str,
        title: &str,
    ) -> Result<NodeId, ProjectError> {
        let kind = Self::parse_node_kind(kind)?;
        let title = title.trim();
        if title.is_empty() {
            return Err(ProjectError::EmptyTitle);
        }
        let parent = match (parent, kind) {
            (Some(id), _) => {
                if self.manuscript.get(id).is_none() {
                    return Err(ProjectError::UnknownNode(id));
                }
                id
            }
            (None, NodeKind::Act) => self.manuscript.root(),
            (None, NodeKind::Chapter) => {
                return Err(ProjectError::Structure(format!(
                    "cannot add chapter {title:?}: select an act first"
                )));
            }
            (None, NodeKind::Scene) => {
                return Err(ProjectError::Structure(format!(
                    "cannot add scene {title:?}: scenes live in chapters"
                )));
            }
            (None, NodeKind::Project) => {
                return Err(ProjectError::Structure(
                    "cannot add another project: one project per manuscript".to_string(),
                ));
            }
        };
        let id = match kind {
            NodeKind::Act => self.manuscript.add_act(title),
            NodeKind::Chapter => self.manuscript.add_chapter(parent, title),
            NodeKind::Scene => self.manuscript.add_scene(parent, title),
            NodeKind::Project => {
                return Err(ProjectError::Structure(
                    "cannot add another project: one project per manuscript".to_string(),
                ));
            }
        }
        .map_err(|err| ProjectError::Structure(err.to_string()))?;
        self.lore.seed_from_manuscript(&self.manuscript);
        self.save()?;
        Ok(id)
    }

    /// Rename any node and persist immediately. Titles are trimmed.
    ///
    /// # Errors
    /// `UnknownNode`, `EmptyTitle`, `Structure`, or `Io` on save failure.
    pub fn rename_node(&mut self, id: NodeId, title: &str) -> Result<(), ProjectError> {
        let title = title.trim();
        if title.is_empty() {
            return Err(ProjectError::EmptyTitle);
        }
        if self.manuscript.get(id).is_none() {
            return Err(ProjectError::UnknownNode(id));
        }
        self.manuscript
            .rename(id, title)
            .map_err(|err| ProjectError::Structure(err.to_string()))?;
        self.save()?;
        Ok(())
    }

    /// Reparent `id` under `new_parent` at sibling position `index`
    /// (`None` appends) and persist immediately.
    ///
    /// # Errors
    /// `UnknownNode`, `Structure` (wrong level or cycle), or `Io` on save.
    pub fn move_node(
        &mut self,
        id: NodeId,
        new_parent: NodeId,
        index: Option<usize>,
    ) -> Result<(), ProjectError> {
        if self.manuscript.get(id).is_none() {
            return Err(ProjectError::UnknownNode(id));
        }
        if self.manuscript.get(new_parent).is_none() {
            return Err(ProjectError::UnknownNode(new_parent));
        }
        self.manuscript
            .move_node_at(id, new_parent, index)
            .map_err(|err| ProjectError::Structure(err.to_string()))?;
        self.save()?;
        Ok(())
    }

    /// Remove `id` and its subtree, persisting immediately.
    ///
    /// Scene draft files are never deleted: every scene with a file under
    /// `id` is first copied to
    /// `.yonro/history/<scene-id>/<unix-ts>.md` (best-effort — a failed
    /// snapshot never blocks the removal). Returns a human message naming
    /// what was kept, e.g.
    /// `removed scene "The gate" (kept scene-7.md on disk)`.
    ///
    /// # Errors
    /// `UnknownNode`, `Structure` for the project root, or `Io` on save.
    pub fn remove_node(&mut self, id: NodeId) -> Result<String, ProjectError> {
        let node = self
            .manuscript
            .get(id)
            .ok_or(ProjectError::UnknownNode(id))?;
        if id == self.manuscript.root() {
            return Err(ProjectError::Structure(
                "cannot remove the project node".to_string(),
            ));
        }
        let title = node.title.clone();
        let kind_label = match node.kind {
            NodeKind::Project => "project",
            NodeKind::Act => "act",
            NodeKind::Chapter => "chapter",
            NodeKind::Scene => "scene",
        };
        let mut kept: Vec<String> = Vec::new();
        for (scene, path) in self.scene_files_under(id) {
            if !path.exists() {
                continue;
            }
            if let Some(name) = path
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string)
            {
                // Best-effort: the original file stays either way.
                let _ = self.snapshot_scene_file(scene, &path);
                kept.push(name);
            }
        }
        self.manuscript
            .remove(id)
            .map_err(|err| ProjectError::Structure(err.to_string()))?;
        self.save()?;
        if kept.is_empty() {
            Ok(format!("removed {kind_label} {title:?}"))
        } else {
            Ok(format!(
                "removed {kind_label} {title:?} (kept {} on disk)",
                kept.join(", ")
            ))
        }
    }

    /// Replace a scene's inspector-editable metadata and persist.
    ///
    /// Trims free-form strings; `file` and `current_words` are
    /// backend-owned and preserved. New POVs/settings seed the lore book
    /// exactly like the TUI (deduped `@`-completable entities).
    ///
    /// # Errors
    /// `UnknownNode`, `NotAScene`, or `Io` on save failure.
    pub fn set_scene_meta(
        &mut self,
        id: NodeId,
        fields: &SceneMetaFields,
    ) -> Result<(), ProjectError> {
        let node = self
            .manuscript
            .get(id)
            .ok_or(ProjectError::UnknownNode(id))?;
        if node.kind != NodeKind::Scene {
            return Err(ProjectError::NotAScene(id));
        }
        let mut meta = node.meta.clone().unwrap_or_default();
        meta.pov = fields.pov.trim().to_string();
        meta.setting = fields.setting.trim().to_string();
        meta.story_date = fields.story_date.trim().to_string();
        meta.story_time = fields.story_time.trim().to_string();
        meta.synopsis = fields.synopsis.trim().to_string();
        meta.target_words = fields.target_words;
        self.manuscript
            .set_meta(id, meta)
            .map_err(|err| ProjectError::Structure(err.to_string()))?;
        self.lore.seed_from_manuscript(&self.manuscript);
        self.save()?;
        Ok(())
    }

    /// `(scene id, draft file)` pairs in the subtree under `id`.
    fn scene_files_under(&self, id: NodeId) -> Vec<(NodeId, PathBuf)> {
        let mut out = Vec::new();
        let mut stack = vec![id];
        while let Some(current) = stack.pop() {
            let Some(node) = self.manuscript.get(current) else {
                continue;
            };
            if node.kind == NodeKind::Scene {
                if let Some(path) = node.meta.as_ref().and_then(|meta| meta.file.clone()) {
                    out.push((current, path));
                }
            }
            for child in self.manuscript.children(current) {
                stack.push(child.id);
            }
        }
        out
    }

    /// Snapshot `file` after a save when the newest snapshot is older than
    /// ten minutes (cap 50 per scene, oldest pruned first).
    ///
    /// Returns `false` (no snapshot) when `file` backs no scene or the
    /// throttle has not elapsed. Best-effort for callers: failures surface
    /// as `Io`, but a failed snapshot must never block the save itself.
    ///
    /// # Errors
    /// `Io` when the draft cannot be read or the snapshot cannot be written.
    pub fn maybe_snapshot(&self, file: &Path) -> Result<bool, ProjectError> {
        let Some(scene) = self.find_scene_by_file(file) else {
            return Ok(false);
        };
        let dir = self.root.join(format!(".yonro/history/{scene}"));
        if let Some(newest) = latest_snapshot_time(&dir) {
            let elapsed = std::time::SystemTime::now()
                .duration_since(newest)
                .unwrap_or(std::time::Duration::ZERO);
            if elapsed.as_secs() < SNAPSHOT_THROTTLE_SECS {
                return Ok(false);
            }
        }
        self.snapshot_scene_file(scene, file)?;
        prune_history(&dir)?;
        Ok(true)
    }

    /// Snapshot list for `scene`, newest first (name plus words).
    ///
    /// # Errors
    /// `UnknownNode` for a bad id, `NotAScene` for structural nodes.
    pub fn scene_history(&self, scene: NodeId) -> Result<Vec<HistoryEntry>, ProjectError> {
        let node = self
            .manuscript
            .get(scene)
            .ok_or(ProjectError::UnknownNode(scene))?;
        if node.kind != NodeKind::Scene {
            return Err(ProjectError::NotAScene(scene));
        }
        let dir = self.root.join(format!(".yonro/history/{scene}"));
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Ok(Vec::new());
        };
        let mut names: Vec<String> = entries
            .filter_map(std::result::Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file()
                    && path
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
            })
            .filter_map(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_string)
            })
            .collect();
        names.sort();
        names.reverse();
        let mut out = Vec::with_capacity(names.len());
        for name in names {
            let bytes = std::fs::read(dir.join(&name))
                .map_err(|err| ProjectError::Io(format!("{}: {err}", dir.join(&name).display())))?;
            let text = String::from_utf8_lossy(&bytes);
            out.push(HistoryEntry {
                name,
                words: super::export::count_words(&text),
            });
        }
        Ok(out)
    }

    /// Restore snapshot `name` over a scene's draft file, returning its text.
    ///
    /// The current draft is snapshotted first (unthrottled), so a restore
    /// never loses prose.
    ///
    /// # Errors
    /// `UnknownNode`/`NotAScene` for bad ids, `Structure` for bad names or
    /// scenes with no draft file, `Io` on filesystem failures.
    pub fn restore_snapshot(&self, scene: NodeId, name: &str) -> Result<String, ProjectError> {
        if name.is_empty()
            || name.contains('/')
            || name.contains('\\')
            || !Path::new(name)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        {
            return Err(ProjectError::Structure(format!(
                "bad snapshot name {name:?}"
            )));
        }
        let node = self
            .manuscript
            .get(scene)
            .ok_or(ProjectError::UnknownNode(scene))?;
        if node.kind != NodeKind::Scene {
            return Err(ProjectError::NotAScene(scene));
        }
        let draft = node
            .meta
            .as_ref()
            .and_then(|meta| meta.file.clone())
            .ok_or_else(|| ProjectError::Structure(format!("scene {scene} has no draft file")))?;
        let source = self.root.join(format!(".yonro/history/{scene}")).join(name);
        if !source.is_file() {
            return Err(ProjectError::Structure(format!(
                "snapshot {name:?} not found for scene {scene}"
            )));
        }
        // Snapshot the present before overwriting (prune keeps the cap).
        if draft.exists() {
            let dir = self.root.join(format!(".yonro/history/{scene}"));
            let _ = self.snapshot_scene_file(scene, &draft);
            prune_history(&dir)?;
        }
        let bytes = std::fs::read(&source)
            .map_err(|err| ProjectError::Io(format!("{}: {err}", source.display())))?;
        write_atomic(&draft, &bytes)
            .map_err(|err| ProjectError::Io(format!("{}: {err}", draft.display())))?;
        String::from_utf8(bytes)
            .map_err(|err| ProjectError::Io(format!("{}: {err}", source.display())))
    }

    /// Copy a scene draft to `.yonro/history/<scene>/<unix-ts>.md`.
    ///
    /// Same-second collisions gain a `-n` suffix.
    ///
    /// # Errors
    /// `Io` when the file cannot be read or the snapshot cannot be written.
    fn snapshot_scene_file(&self, scene: NodeId, path: &Path) -> Result<(), ProjectError> {
        let bytes = std::fs::read(path)
            .map_err(|err| ProjectError::Io(format!("{}: {err}", path.display())))?;
        let dir = self.root.join(format!(".yonro/history/{scene}"));
        std::fs::create_dir_all(&dir)
            .map_err(|err| ProjectError::Io(format!("{}: {err}", dir.display())))?;
        let stamp = unix_timestamp();
        let mut suffix: u32 = 0;
        loop {
            let name = if suffix == 0 {
                format!("{stamp}.md")
            } else {
                format!("{stamp}-{suffix}.md")
            };
            let dest = dir.join(&name);
            if dest.exists() {
                suffix = suffix.saturating_add(1);
                if suffix > 1000 {
                    return Err(ProjectError::Io(format!(
                        "{}: too many snapshots",
                        dir.display()
                    )));
                }
                continue;
            }
            std::fs::write(&dest, &bytes)
                .map_err(|err| ProjectError::Io(format!("{}: {err}", dest.display())))?;
            return Ok(());
        }
    }

    /// Scene-less `.md` files directly under the workspace root, sorted.
    ///
    /// Scene drafts linked from the manuscript are excluded; `.yonro/` is
    /// never scanned. Missing/unreadable roots yield an empty list, never
    /// an error (frontends render an empty files section).
    #[must_use]
    pub fn list_files(&self) -> Vec<String> {
        let mut linked: Vec<String> = Vec::new();
        let mut stack = vec![self.manuscript.root()];
        while let Some(id) = stack.pop() {
            let Some(node) = self.manuscript.get(id) else {
                continue;
            };
            if node.kind == NodeKind::Scene {
                if let Some(name) = node
                    .meta
                    .as_ref()
                    .and_then(|meta| meta.file.as_ref())
                    .and_then(|path| path.file_name())
                    .and_then(|name| name.to_str())
                {
                    linked.push(name.to_string());
                }
            }
            for child in self.manuscript.children(id) {
                stack.push(child.id);
            }
        }
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut files: Vec<String> = entries
            .filter_map(std::result::Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file()
                    && path
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
                    && !path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| linked.iter().any(|taken| taken == name))
            })
            .map(|path| path.to_string_lossy().to_string())
            .collect();
        files.sort();
        files
    }

    /// Load session days plus the daily goal for this workspace.
    ///
    /// Reads `<root>/.yonro/sessions.json` (`{ "YYYY-MM-DD": ... }`) and
    /// `<root>/.yonro/project.json` (`{ "daily_goal": n }`). Missing files
    /// are a fresh workspace (silent); corrupt ones fall back to empty/zero
    /// with one warning each naming the file and the reason.
    #[must_use]
    pub fn load_sessions(&self) -> (SessionLog, Vec<String>) {
        let mut warnings = Vec::new();
        let sessions_path = self.root.join(".yonro/sessions.json");
        let days: BTreeMap<String, DayRecord> = match std::fs::read_to_string(&sessions_path) {
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(err) => {
                warnings.push(format!("sessions.json: {err}"));
                BTreeMap::new()
            }
            Ok(text) => match serde_json::from_str(&text) {
                Ok(days) => days,
                Err(err) => {
                    warnings.push(format!("sessions.json: {err}"));
                    BTreeMap::new()
                }
            },
        };
        let project_path = self.root.join(".yonro/project.json");
        let goal: usize = match std::fs::read_to_string(&project_path) {
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => 0,
            Err(err) => {
                warnings.push(format!("project.json: {err}"));
                0
            }
            Ok(text) => match serde_json::from_str::<ProjectFile>(&text) {
                Ok(file) => file.daily_goal,
                Err(err) => {
                    warnings.push(format!("project.json: {err}"));
                    0
                }
            },
        };
        (SessionLog::from_parts(days, goal), warnings)
    }

    /// Persist session days plus the daily goal (atomic per file).
    ///
    /// Call after `save()` on save/exit; `set_text` paths only observe
    /// in-memory via [`Project::observe_session_words`].
    ///
    /// # Errors
    /// `Io` when the directory cannot be created or any write/sync/rename
    /// step fails.
    pub fn save_sessions(&self, log: &SessionLog) -> Result<(), ProjectError> {
        let dir = self.root.join(".yonro");
        std::fs::create_dir_all(&dir).map_err(|err| ProjectError::Io(format!(".yonro: {err}")))?;
        let sessions_json = serde_json::to_string_pretty(log.days())
            .map_err(|err| ProjectError::Io(format!("sessions.json: {err}")))?;
        write_atomic(&dir.join("sessions.json"), sessions_json.as_bytes())?;
        let file = ProjectFile {
            daily_goal: log.goal(),
        };
        let project_json = serde_json::to_string_pretty(&file)
            .map_err(|err| ProjectError::Io(format!("project.json: {err}")))?;
        write_atomic(&dir.join("project.json"), project_json.as_bytes())?;
        Ok(())
    }

    /// Record the current manuscript total into `log` for `today`.
    ///
    /// Cheap and in-memory: callers persist later via
    /// [`Project::save_sessions`].
    pub fn observe_session_words(&self, log: &mut SessionLog, today: &str) {
        log.record(self.manuscript.subtree_words(self.manuscript.root()), today);
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

    /// Add a lore entity and persist immediately.
    ///
    /// # Errors
    /// `BadKind`, `Lore` on duplicate/empty name, or `Io` on save.
    pub fn add_entity(&mut self, kind: &str, name: &str) -> Result<usize, ProjectError> {
        let parsed_kind = super::lore::EntityKind::parse(kind)
            .ok_or_else(|| ProjectError::BadKind(kind.to_string()))?;
        let id = self
            .lore
            .add(parsed_kind, name)
            .map_err(|err| ProjectError::Lore(super::lore::lore_error_message(&err)))?;
        self.save()?;
        Ok(id)
    }

    /// Update a lore entity's name, aliases, and/or sheet and persist immediately.
    ///
    /// # Errors
    /// `Lore` on duplicate name/alias or missing entity, or `Io` on save.
    pub fn update_entity(
        &mut self,
        id: usize,
        name: Option<&str>,
        aliases: Option<&[String]>,
        sheet: Option<&str>,
    ) -> Result<(), ProjectError> {
        self.lore
            .update(id, name, aliases, sheet)
            .map_err(|err| ProjectError::Lore(super::lore::lore_error_message(&err)))?;
        self.save()?;
        Ok(())
    }

    /// Remove a lore entity and persist immediately.
    ///
    /// # Errors
    /// `Lore` on unknown/inactive entity, or `Io` on save.
    pub fn remove_entity(&mut self, id: usize) -> Result<(), ProjectError> {
        self.lore
            .remove(id)
            .map_err(|err| ProjectError::Lore(super::lore::lore_error_message(&err)))?;
        self.save()?;
        Ok(())
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

/// Newest snapshot mtime in `dir` (`None` when empty/unreadable).
fn latest_snapshot_time(dir: &Path) -> Option<std::time::SystemTime> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut newest: Option<std::time::SystemTime> = None;
    for entry in entries.filter_map(std::result::Result::ok) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if let Ok(meta) = std::fs::metadata(&path) {
            if let Ok(modified) = meta.modified() {
                let is_newer = newest.is_none_or(|known| modified > known);
                if is_newer {
                    newest = Some(modified);
                }
            }
        }
    }
    newest
}

/// Keep the newest [`MAX_SNAPSHOTS_PER_SCENE`] snapshots (name order).
///
/// # Errors
/// `Io` when the history dir cannot be listed or a stale file cannot be removed.
fn prune_history(dir: &Path) -> Result<(), ProjectError> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    let mut names: Vec<String> = entries
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        })
        .filter_map(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string)
        })
        .collect();
    names.sort();
    while names.len() > MAX_SNAPSHOTS_PER_SCENE {
        let oldest = names.remove(0);
        std::fs::remove_file(dir.join(&oldest))
            .map_err(|err| ProjectError::Io(format!("{}: {err}", dir.join(&oldest).display())))?;
    }
    Ok(())
}

/// Seconds since the Unix epoch (`0` when the clock is unavailable).
fn unix_timestamp() -> u64 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(span) => span.as_secs(),
        Err(_) => 0,
    }
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

    #[test]
    fn add_node_builds_act_chapter_scene_and_persists() {
        let dir = unique_dir("addnode");
        let mut project = Project {
            root: dir.clone(),
            manuscript: Manuscript::new("Probe"),
            lore: LoreBook::new(),
            warnings: Vec::new(),
        };
        let act = project.add_node(None, "act", "Act I").unwrap();
        let ch = project.add_node(Some(act), "chapter", "Ch 1").unwrap();
        let sc = project.add_node(Some(ch), "scene", "S1").unwrap();
        assert_eq!(project.manuscript.live_count(), 4);
        // Reload from disk keeps the structure.
        let loaded = Project::load(&dir);
        assert!(loaded.warnings.is_empty());
        assert_eq!(loaded.manuscript.get(sc).unwrap().title, "S1");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn add_node_rejects_bad_kind_parent_and_title() {
        let dir = unique_dir("addnode-err");
        let mut project = Project {
            root: dir.clone(),
            manuscript: Manuscript::new("Probe"),
            lore: LoreBook::new(),
            warnings: Vec::new(),
        };
        assert!(matches!(
            project.add_node(None, "volume", "V"),
            Err(ProjectError::BadKind(_))
        ));
        let err = project.add_node(None, "scene", "S").unwrap_err();
        assert!(err.to_string().contains("scenes live in chapters"));
        let err = project.add_node(None, "chapter", "C").unwrap_err();
        assert!(err.to_string().contains("select an act"));
        assert_eq!(
            Project::create(&dir, "   ").unwrap_err(),
            ProjectError::EmptyTitle
        );
        assert_eq!(
            project.add_node(Some(99), "act", "A"),
            Err(ProjectError::UnknownNode(99))
        );
        // Scene directly under an act names the hierarchy rule.
        let act = project.add_node(None, "act", "A").unwrap();
        let err = project.add_node(Some(act), "scene", "S").unwrap_err();
        assert!(err.to_string().contains("Chapter"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rename_and_move_nodes_persist() {
        let dir = unique_dir("renamemove");
        let mut project = sample_project(&dir);
        project.save().unwrap();
        let scene = {
            let root = project.manuscript.root();
            let act = project.manuscript.children(root)[0].id;
            let ch = project.manuscript.children(act)[0].id;
            project.manuscript.children(ch)[0].id
        };
        project.rename_node(scene, "The window").unwrap();
        let before: Vec<usize> = project
            .manuscript
            .children(project.manuscript.root())
            .iter()
            .map(|n| n.id)
            .collect();
        let act2 = project.add_node(None, "act", "Act II").unwrap();
        assert_ne!(act2, before[0]);
        let ch2 = project.add_node(Some(act2), "chapter", "Ch 2").unwrap();
        project.move_node(scene, ch2, Some(0)).unwrap();
        let loaded = Project::load(&dir);
        assert_eq!(loaded.manuscript.get(scene).unwrap().title, "The window");
        assert_eq!(loaded.manuscript.children(ch2)[0].id, scene);
        assert!(project.rename_node(scene, "  ").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_files_excludes_scene_drafts_and_sorts() {
        let dir = unique_dir("listfiles");
        let mut project = sample_project(&dir);
        project.save().unwrap();
        let scene = {
            let root = project.manuscript.root();
            let act = project.manuscript.children(root)[0].id;
            let ch = project.manuscript.children(act)[0].id;
            project.manuscript.children(ch)[0].id
        };
        let draft = project.scene_file(scene).unwrap();
        std::fs::write(dir.join("notes.md"), "a").unwrap();
        std::fs::write(dir.join("diary.md"), "b").unwrap();
        std::fs::write(dir.join("sketch.txt"), "c").unwrap();
        let files = project.list_files();
        assert_eq!(files.len(), 2);
        assert!(files[0].ends_with("diary.md"));
        assert!(files[1].ends_with("notes.md"));
        assert!(!files.iter().any(|f| *f == draft.to_string_lossy()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn set_scene_meta_saves_fields_seeds_lore_and_keeps_file() {
        let dir = unique_dir("setmeta");
        let mut project = sample_project(&dir);
        project.save().unwrap();
        let scene = {
            let root = project.manuscript.root();
            let act = project.manuscript.children(root)[0].id;
            let ch = project.manuscript.children(act)[0].id;
            project.manuscript.children(ch)[0].id
        };
        let path = project.scene_file(scene).unwrap();
        assert!(project.sync_scene_words(&path, 33));
        project
            .set_scene_meta(
                scene,
                &SceneMetaFields {
                    pov: "  Joren ".to_string(),
                    setting: "Harbor".to_string(),
                    story_date: "Day 4".to_string(),
                    story_time: "dawn".to_string(),
                    synopsis: "Joren sails.".to_string(),
                    target_words: 500,
                },
            )
            .unwrap();
        let meta = project
            .manuscript
            .get(scene)
            .and_then(|node| node.meta.clone())
            .unwrap();
        assert_eq!(meta.pov, "Joren");
        assert_eq!(meta.setting, "Harbor");
        assert_eq!(meta.target_words, 500);
        assert_eq!(meta.file, Some(path));
        assert_eq!(meta.current_words, 33);
        // POV + setting seeded exactly like the TUI (Mara was already there).
        assert!(project.lore.resolve("Joren").is_some());
        assert!(project.lore.resolve("Harbor").is_some());
        // Reload persists everything.
        let loaded = Project::load(&dir);
        let meta = loaded
            .manuscript
            .get(scene)
            .and_then(|node| node.meta.clone())
            .unwrap();
        assert_eq!(meta.pov, "Joren");
        assert_eq!(loaded.lore.live_count(), 3);
        // Non-scenes and unknown ids are named.
        let act = loaded.manuscript.root();
        assert_eq!(
            project.set_scene_meta(act, &SceneMetaFields::default()),
            Err(ProjectError::NotAScene(act))
        );
        assert_eq!(
            project.set_scene_meta(999, &SceneMetaFields::default()),
            Err(ProjectError::UnknownNode(999))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remove_scene_keeps_md_and_snapshots_history() {
        let dir = unique_dir("rmscene");
        let mut project = sample_project(&dir);
        project.save().unwrap();
        let scene = {
            let root = project.manuscript.root();
            let act = project.manuscript.children(root)[0].id;
            let ch = project.manuscript.children(act)[0].id;
            project.manuscript.children(ch)[0].id
        };
        let path = project.scene_file(scene).unwrap();
        std::fs::write(&path, "dear draft").unwrap();
        let message = project.remove_node(scene).unwrap();
        assert!(
            message.contains(&format!("kept scene-{scene}.md on disk")),
            "message was: {message}"
        );
        // Original file untouched; snapshot holds the same prose.
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "dear draft");
        let history: Vec<_> = std::fs::read_dir(dir.join(format!(".yonro/history/{scene}")))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(history.len(), 1);
        assert_eq!(std::fs::read_to_string(&history[0]).unwrap(), "dear draft");
        assert!(project.manuscript.get(scene).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remove_chapter_snapshots_every_scene_and_names_root() {
        let dir = unique_dir("rmchapter");
        let mut project = sample_project(&dir);
        project.save().unwrap();
        let (chapter, first, second) = {
            let root = project.manuscript.root();
            let act = project.manuscript.children(root)[0].id;
            let ch = project.manuscript.children(act)[0].id;
            let first = project.manuscript.children(ch)[0].id;
            let extra = project.manuscript.add_scene(ch, "Second").unwrap();
            (ch, first, extra)
        };
        for scene in [first, second] {
            let path = project.scene_file(scene).unwrap();
            std::fs::write(&path, format!("words {scene}")).unwrap();
        }
        let message = project.remove_node(chapter).unwrap();
        assert!(message.contains("kept"), "message was: {message}");
        assert!(message.contains("scene-"));
        for scene in [first, second] {
            let snaps: Vec<_> = std::fs::read_dir(dir.join(format!(".yonro/history/{scene}")))
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .collect();
            assert_eq!(snaps.len(), 1);
            assert_eq!(
                std::fs::read_to_string(&snaps[0]).unwrap(),
                format!("words {scene}")
            );
            // Drafts stay on disk even though the nodes are gone.
            assert!(dir.join(format!("scene-{scene}.md")).exists());
        }
        let root = project.manuscript.root();
        assert_eq!(
            project.remove_node(root).unwrap_err().to_string(),
            "cannot remove the project node"
        );
        assert_eq!(
            project.remove_node(999),
            Err(ProjectError::UnknownNode(999))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn lore_entity_mutations_persist_and_validate() {
        let dir = unique_dir("lore_mutations");
        let mut project = sample_project(&dir);
        project.save().unwrap();

        // Add
        let id = project.add_entity("character", "Corin").unwrap();
        assert_eq!(project.lore.get(id).unwrap().name, "Corin");

        // Duplicate rejection
        assert_eq!(
            project
                .add_entity("character", "corin")
                .unwrap_err()
                .to_string(),
            "name already in use: corin"
        );
        // Bad kind rejection
        assert_eq!(
            project.add_entity("spaceship", "Apollo").unwrap_err(),
            ProjectError::BadKind("spaceship".to_string())
        );

        // Update
        project
            .update_entity(
                id,
                Some("Corin the Bold"),
                Some(&["Bold Corin".to_string()]),
                Some("Protagonist."),
            )
            .unwrap();
        assert_eq!(project.lore.get(id).unwrap().name, "Corin the Bold");
        assert_eq!(project.lore.get(id).unwrap().aliases, vec!["Bold Corin"]);
        assert_eq!(project.lore.get(id).unwrap().sheet, "Protagonist.");

        // Reload from disk to verify persistence
        let reloaded = Project::load(&dir);
        let reloaded_entity = reloaded.lore.get(id).unwrap();
        assert_eq!(reloaded_entity.name, "Corin the Bold");
        assert_eq!(reloaded_entity.aliases, vec!["Bold Corin"]);
        assert_eq!(reloaded_entity.sheet, "Protagonist.");

        // Remove
        project.remove_entity(id).unwrap();
        assert!(project.lore.get(id).is_none());

        let reloaded2 = Project::load(&dir);
        assert!(reloaded2.lore.get(id).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sessions_roundtrip_goal_and_corrupt_fallback() {
        let dir = unique_dir("sessions");
        let project = Project {
            root: dir.clone(),
            manuscript: Manuscript::new("Probe"),
            lore: LoreBook::new(),
            warnings: Vec::new(),
        };
        // Missing files are a fresh workspace: silent, empty, goalless.
        let (mut log, warnings) = project.load_sessions();
        assert!(warnings.is_empty());
        assert_eq!(log.goal(), 0);
        log.set_goal(500);
        project.observe_session_words(&mut log, "2026-10-04");
        assert_eq!(log.words_today("2026-10-04"), 0);
        project.save_sessions(&log).unwrap();
        let (loaded, warnings) = project.load_sessions();
        assert!(warnings.is_empty());
        assert_eq!(loaded.goal(), 500);
        // Corrupt sessions.json falls back with a warning naming the file.
        std::fs::write(dir.join(".yonro/sessions.json"), "broken{{").unwrap();
        let (fallback, warnings) = project.load_sessions();
        assert_eq!(fallback.words_today("2026-10-04"), 0);
        assert!(warnings
            .iter()
            .any(|warning| warning.contains("sessions.json")));
        assert_eq!(fallback.goal(), 500);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn snapshots_throttle_list_and_restore() {
        let dir = unique_dir("snapshots");
        let mut project = sample_project(&dir);
        project.save().unwrap();
        let scene = {
            let root = project.manuscript.root();
            let act = project.manuscript.children(root)[0].id;
            let ch = project.manuscript.children(act)[0].id;
            project.manuscript.children(ch)[0].id
        };
        let path = project.scene_file(scene).unwrap();
        // Loose files never snapshot.
        let loose = dir.join("notes.md");
        std::fs::write(&loose, "stray").unwrap();
        assert!(!project.maybe_snapshot(&loose).unwrap());
        // First save snapshots; the immediate next one throttles.
        std::fs::write(&path, "first words here").unwrap();
        assert!(project.maybe_snapshot(&path).unwrap());
        assert!(!project.maybe_snapshot(&path).unwrap());
        let history = project.scene_history(scene).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].words, 3);
        // Restore snapshots the present first, then swaps the text.
        std::fs::write(&path, "second version live").unwrap();
        let restored = project.restore_snapshot(scene, &history[0].name).unwrap();
        assert_eq!(restored, "first words here");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first words here");
        let history = project.scene_history(scene).unwrap();
        assert_eq!(history.len(), 2);
        assert!(project.restore_snapshot(scene, "../evil.md").is_err());
        assert_eq!(
            project.restore_snapshot(999, "x.md"),
            Err(ProjectError::UnknownNode(999))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn create_has_project_and_resolve_path() {
        let base = unique_dir("create-ws");
        let dir = base.join("novel");
        assert!(!Project::has_project(&dir));
        let project = Project::create(&dir, "  Sample Novel ").unwrap();
        assert_eq!(project.manuscript.title(), "Sample Novel");
        assert!(project.warnings.is_empty());
        assert!(Project::has_project(&dir));
        let loaded = Project::load(&dir);
        assert!(loaded.warnings.is_empty());
        assert_eq!(loaded.manuscript.title(), "Sample Novel");
        assert_eq!(
            Project::create(&dir, "   ").unwrap_err(),
            ProjectError::EmptyTitle
        );
        assert!(Project::resolve_workspace_path("   ").is_err());
        assert_eq!(
            Project::resolve_workspace_path("  /tmp/x  ").unwrap(),
            PathBuf::from("/tmp/x")
        );
        let _ = std::fs::remove_dir_all(&base);
    }
}
