//! Manuscript tree (`PLAN.md Phase 4.3`): the structural backbone of a story.
//!
//! ```text
//! Project "The Pink Dog"
//! └── Act "Act I — Departure"
//!     └── Chapter "Chapter 1 — Muddy Paws"
//!         └── Scene "The gate" (POV: Mara, setting: farm, target: 1200w)
//! ```
//!
//! * Hierarchy is enforced: `Act`s live under the `Project`, `Chapter`s
//!   under an `Act`, `Scene`s under a `Chapter`.
//! * Node ids are arena indices and **never reused** (removals leave
//!   tombstones), so UI selection stays valid across edits.
//! * Word counts are attached per scene (`current_words`, typically synced
//!   from live buffers by the frontend); rollups and target progress derive
//!   from the tree. No I/O here — persistence is a later phase.

use std::fmt;
use std::path::PathBuf;

pub type NodeId = usize;

/// Structural level of a manuscript node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum NodeKind {
    Project,
    Act,
    Chapter,
    Scene,
}

impl NodeKind {
    /// The only legal parent kind (`None` for the project root itself).
    #[must_use]
    pub const fn parent_kind(self) -> Option<Self> {
        match self {
            Self::Project => None,
            Self::Act => Some(Self::Project),
            Self::Chapter => Some(Self::Act),
            Self::Scene => Some(Self::Chapter),
        }
    }
}

/// Per-scene metadata (`PLAN.md Phase 4.3`: POV, setting, story date/time,
/// target word count). Free-form strings — writers mean many things by
/// "Day 3" or "dusk", and parsing that is a later feature, not this one.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SceneMeta {
    /// Point-of-view character (e.g. `"Mara"`).
    pub pov: String,
    /// Where the scene happens (e.g. `"Mill farm"`).
    pub setting: String,
    /// In-story date, free-form (e.g. `"Day 3"`, `"1492-03-04"`).
    pub story_date: String,
    /// In-story time, free-form (e.g. `"dusk"`).
    pub story_time: String,
    /// One-line reminder of what happens.
    pub synopsis: String,
    /// Draft word-count goal (`0` = no target).
    pub target_words: usize,
    /// Current draft size (synced from the scene's buffer by the frontend).
    pub current_words: usize,
    /// Draft file backing this scene, if any.
    pub file: Option<PathBuf>,
}

/// One node of the manuscript tree.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    pub title: String,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    /// `Some` for scenes, `None` for structural nodes.
    pub meta: Option<SceneMeta>,
    /// `false` after removal (tombstone keeps ids stable).
    pub alive: bool,
}

/// Hierarchy / lookup failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManuscriptError {
    UnknownNode(NodeId),
    InactiveNode(NodeId),
    /// `Project` root cannot be removed.
    CannotRemoveProject,
    /// Child kind does not belong under the given parent kind.
    InvalidParent {
        child: NodeKind,
        parent: NodeKind,
    },
    /// Only scenes carry metadata.
    NotAScene(NodeId),
    /// Repointing a node under its own descendant.
    Cycle(NodeId),
}

impl fmt::Display for ManuscriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownNode(id) => write!(formatter, "unknown manuscript node {id}"),
            Self::InactiveNode(id) => write!(formatter, "removed manuscript node {id}"),
            Self::CannotRemoveProject => write!(formatter, "cannot remove the project node"),
            Self::InvalidParent { child, parent } => write!(
                formatter,
                "{child:?} does not belong under {parent:?} (Project → Act → Chapter → Scene)"
            ),
            Self::NotAScene(id) => write!(formatter, "node {id} is not a scene"),
            Self::Cycle(id) => write!(formatter, "cannot move node {id} under its own descendant"),
        }
    }
}

impl std::error::Error for ManuscriptError {}

/// An in-memory manuscript: one project tree plus structural operations.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Manuscript {
    nodes: Vec<Node>,
    root: NodeId,
}

impl Manuscript {
    /// New manuscript with a single `Project` node titled `title`.
    #[must_use]
    pub fn new(title: &str) -> Self {
        let root = Node {
            id: 0,
            kind: NodeKind::Project,
            title: title.to_string(),
            parent: None,
            children: Vec::new(),
            meta: None,
            alive: true,
        };
        Self {
            nodes: vec![root],
            root: 0,
        }
    }

    /// Project root id.
    #[must_use]
    pub const fn root(&self) -> NodeId {
        self.root
    }

    /// Project title.
    #[must_use]
    pub fn title(&self) -> &str {
        self.nodes
            .get(self.root)
            .map_or("", |node| node.title.as_str())
    }

    /// Live node lookup (`None` for unknown or removed ids).
    #[must_use]
    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id).filter(|node| node.alive)
    }

    fn get_mut(&mut self, id: NodeId) -> Result<&mut Node, ManuscriptError> {
        match self.nodes.get_mut(id) {
            Some(node) if node.alive => Ok(node),
            Some(_) => Err(ManuscriptError::InactiveNode(id)),
            None => Err(ManuscriptError::UnknownNode(id)),
        }
    }

    /// Live children of `id`, in insertion order.
    #[must_use]
    pub fn children(&self, id: NodeId) -> Vec<&Node> {
        self.get(id).map_or_else(Vec::new, |node| {
            node.children
                .iter()
                .filter_map(|child| self.get(*child))
                .collect()
        })
    }

    /// Titles from the project root down to `id` (`None` if unknown/removed).
    #[must_use]
    pub fn breadcrumb(&self, id: NodeId) -> Option<Vec<String>> {
        let mut trail = Vec::new();
        let mut cursor = self.get(id)?;
        loop {
            trail.push(cursor.title.clone());
            match cursor.parent {
                Some(parent) => cursor = self.get(parent)?,
                None => break,
            }
        }
        trail.reverse();
        Some(trail)
    }

    fn push_child(
        &mut self,
        parent: NodeId,
        kind: NodeKind,
        title: &str,
    ) -> Result<NodeId, ManuscriptError> {
        let parent_kind = self.get_mut(parent)?.kind;
        if kind.parent_kind() != Some(parent_kind) {
            return Err(ManuscriptError::InvalidParent {
                child: kind,
                parent: parent_kind,
            });
        }
        let id = self.nodes.len();
        self.nodes.push(Node {
            id,
            kind,
            title: title.to_string(),
            parent: Some(parent),
            children: Vec::new(),
            meta: (kind == NodeKind::Scene).then(SceneMeta::default),
            alive: true,
        });
        self.get_mut(parent)?.children.push(id);
        Ok(id)
    }

    /// Add an act to the project.
    ///
    /// # Errors
    /// Never fails for a live manuscript (root always exists).
    pub fn add_act(&mut self, title: &str) -> Result<NodeId, ManuscriptError> {
        let root = self.root;
        self.push_child(root, NodeKind::Act, title)
    }

    /// Add a chapter under `act_id`.
    ///
    /// # Errors
    /// `UnknownNode`/`InactiveNode` for a bad act, `InvalidParent` otherwise.
    pub fn add_chapter(&mut self, act_id: NodeId, title: &str) -> Result<NodeId, ManuscriptError> {
        self.push_child(act_id, NodeKind::Chapter, title)
    }

    /// Add a scene under `chapter_id`.
    ///
    /// # Errors
    /// `UnknownNode`/`InactiveNode` for a bad chapter, `InvalidParent` otherwise.
    pub fn add_scene(
        &mut self,
        chapter_id: NodeId,
        title: &str,
    ) -> Result<NodeId, ManuscriptError> {
        self.push_child(chapter_id, NodeKind::Scene, title)
    }

    /// Rename any live node.
    ///
    /// # Errors
    /// `UnknownNode`/`InactiveNode` for a bad id.
    pub fn rename(&mut self, id: NodeId, title: &str) -> Result<(), ManuscriptError> {
        self.get_mut(id)?.title = title.to_string();
        Ok(())
    }

    /// Replace a scene's metadata (acts/chapters reject with `NotAScene`).
    ///
    /// # Errors
    /// `UnknownNode`/`InactiveNode`/`NotAScene` as applicable.
    pub fn set_meta(&mut self, id: NodeId, meta: SceneMeta) -> Result<(), ManuscriptError> {
        let node = self.get_mut(id)?;
        if node.kind != NodeKind::Scene {
            return Err(ManuscriptError::NotAScene(id));
        }
        node.meta = Some(meta);
        Ok(())
    }

    /// Record a scene's current draft size (frontend sync hook).
    ///
    /// # Errors
    /// `UnknownNode`/`InactiveNode`/`NotAScene` as applicable.
    pub fn set_scene_words(&mut self, id: NodeId, words: usize) -> Result<(), ManuscriptError> {
        let node = self.get_mut(id)?;
        match node.meta.as_mut() {
            Some(meta) if node.kind == NodeKind::Scene => {
                meta.current_words = words;
                Ok(())
            }
            _ => Err(ManuscriptError::NotAScene(id)),
        }
    }

    /// Reparent `id` under `new_parent` (same hierarchy rules as insertion,
    /// plus cycle rejection). Appends to the end of the new sibling list.
    ///
    /// # Errors
    /// `UnknownNode`/`InactiveNode`/`InvalidParent`/`Cycle`/`CannotRemoveProject`.
    pub fn move_node(&mut self, id: NodeId, new_parent: NodeId) -> Result<(), ManuscriptError> {
        self.move_node_at(id, new_parent, None)
    }

    /// Reparent `id` under `new_parent` at sibling position `index`
    /// (`None` appends; out-of-range clamps to the end). Used by outline
    /// drag-and-drop and move up/down so the UI can place the node exactly.
    ///
    /// # Errors
    /// `UnknownNode`/`InactiveNode`/`InvalidParent`/`Cycle`/`CannotRemoveProject`.
    pub fn move_node_at(
        &mut self,
        id: NodeId,
        new_parent: NodeId,
        index: Option<usize>,
    ) -> Result<(), ManuscriptError> {
        let kind = self.get_mut(id)?.kind;
        if id == self.root {
            return Err(ManuscriptError::CannotRemoveProject);
        }
        let parent_kind = self.get_mut(new_parent)?.kind;
        if kind.parent_kind() != Some(parent_kind) {
            return Err(ManuscriptError::InvalidParent {
                child: kind,
                parent: parent_kind,
            });
        }
        // Cycle: new parent must not sit inside `id`'s own subtree.
        let mut cursor = Some(new_parent);
        while let Some(current) = cursor {
            if current == id {
                return Err(ManuscriptError::Cycle(id));
            }
            cursor = self.get(current).and_then(|node| node.parent);
        }
        let old_parent = self.get_mut(id)?.parent;
        if let Some(old) = old_parent {
            self.get_mut(old)?.children.retain(|child| *child != id);
        }
        self.get_mut(id)?.parent = Some(new_parent);
        let siblings = &mut self.get_mut(new_parent)?.children;
        match index {
            Some(at) => {
                let clamped = at.min(siblings.len());
                siblings.insert(clamped, id);
            }
            None => siblings.push(id),
        }
        Ok(())
    }

    /// Remove `id` and its whole subtree (tombstones; ids stay stable).
    ///
    /// # Errors
    /// `UnknownNode`/`InactiveNode`, or `CannotRemoveProject` for the root.
    pub fn remove(&mut self, id: NodeId) -> Result<(), ManuscriptError> {
        if id == self.root {
            return Err(ManuscriptError::CannotRemoveProject);
        }
        self.get_mut(id)?; // existence check
        let mut stack = vec![id];
        while let Some(current) = stack.pop() {
            if let Some(node) = self.nodes.get_mut(current) {
                node.alive = false;
                stack.extend(node.children.iter().copied());
            }
        }
        if let Some(parent) = self.nodes.get(id).and_then(|node| node.parent) {
            if let Ok(parent_node) = self.get_mut(parent) {
                parent_node.children.retain(|child| *child != id);
            }
        }
        Ok(())
    }

    /// Total current words under `id` (scene itself or whole subtree).
    #[must_use]
    pub fn subtree_words(&self, id: NodeId) -> usize {
        self.get(id).map_or(0, |node| {
            let own = node.meta.as_ref().map_or(0, |meta| meta.current_words);
            node.children
                .iter()
                .filter_map(|child| self.get(*child))
                .map(|child| self.subtree_words(child.id))
                .fold(own, usize::saturating_add)
        })
    }

    /// Total target words under `id`.
    #[must_use]
    pub fn subtree_target(&self, id: NodeId) -> usize {
        self.get(id).map_or(0, |node| {
            let own = node.meta.as_ref().map_or(0, |meta| meta.target_words);
            node.children
                .iter()
                .filter_map(|child| self.get(*child))
                .map(|child| self.subtree_target(child.id))
                .fold(own, usize::saturating_add)
        })
    }

    /// Draft progress under `id`: `words / target` (`0.0` when untargeted).
    #[must_use]
    #[allow(clippy::cast_precision_loss, clippy::as_conversions)]
    pub fn progress(&self, id: NodeId) -> f64 {
        let target = self.subtree_target(id);
        if target == 0 {
            0.0
        } else {
            self.subtree_words(id) as f64 / target as f64
        }
    }

    /// Number of live nodes (tombstones excluded).
    #[must_use]
    pub fn live_count(&self) -> usize {
        self.nodes.iter().filter(|node| node.alive).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Manuscript {
        let mut ms = Manuscript::new("The Pink Dog");
        let act = ms.add_act("Act I").unwrap();
        let ch = ms.add_chapter(act, "Chapter 1").unwrap();
        let sc = ms.add_scene(ch, "The gate").unwrap();
        ms.set_meta(
            sc,
            SceneMeta {
                pov: "Mara".to_string(),
                setting: "Mill farm".to_string(),
                story_date: "Day 3".to_string(),
                story_time: "dusk".to_string(),
                synopsis: "Mara opens the gate.".to_string(),
                target_words: 1000,
                current_words: 250,
                file: None,
            },
        )
        .unwrap();
        ms
    }

    #[test]
    fn hierarchy_is_enforced() {
        let mut ms = Manuscript::new("T");
        let act = ms.add_act("A").unwrap();
        // Chapter directly under project: rejected.
        assert_eq!(
            ms.push_child(ms.root(), NodeKind::Chapter, "X"),
            Err(ManuscriptError::InvalidParent {
                child: NodeKind::Chapter,
                parent: NodeKind::Project,
            })
        );
        // Scene directly under act: rejected.
        assert!(ms.push_child(act, NodeKind::Scene, "X").is_err());
        // Unknown parent: rejected.
        assert_eq!(
            ms.add_chapter(99, "X"),
            Err(ManuscriptError::UnknownNode(99))
        );
    }

    #[test]
    fn breadcrumb_traces_root_to_node() {
        let ms = sample();
        let scene = ms.children(ms.children(ms.root())[0].id)[0].id;
        let scene = ms.children(scene)[0].id;
        assert_eq!(
            ms.breadcrumb(scene).unwrap(),
            vec!["The Pink Dog", "Act I", "Chapter 1", "The gate"]
        );
        assert!(ms.breadcrumb(99).is_none());
    }

    #[test]
    fn rollup_and_progress_cover_subtrees() {
        let ms = sample();
        let root = ms.root();
        assert_eq!(ms.subtree_words(root), 250);
        assert_eq!(ms.subtree_target(root), 1000);
        assert!((ms.progress(root) - 0.25).abs() < f64::EPSILON);
        assert_eq!(ms.progress(99), 0.0);
    }

    #[test]
    fn metadata_only_on_scenes() {
        let mut ms = Manuscript::new("T");
        let act = ms.add_act("A").unwrap();
        assert_eq!(
            ms.set_meta(act, SceneMeta::default()),
            Err(ManuscriptError::NotAScene(act))
        );
        assert_eq!(
            ms.set_scene_words(act, 10),
            Err(ManuscriptError::NotAScene(act))
        );
    }

    #[test]
    fn remove_keeps_ids_stable_and_prunes_subtree() {
        let mut ms = sample();
        let act = ms.children(ms.root())[0].id;
        let before = ms.live_count();
        ms.remove(act).unwrap();
        // Act + chapter + scene gone, project stays; other ids untouched.
        assert_eq!(ms.live_count(), before.saturating_sub(3));
        assert_eq!(ms.subtree_words(ms.root()), 0);
        assert!(ms.get(act).is_none());
        assert_eq!(
            ms.remove(ms.root()),
            Err(ManuscriptError::CannotRemoveProject)
        );
    }

    #[test]
    fn move_rejects_wrong_levels() {
        let mut ms = sample();
        let act = ms.children(ms.root())[0].id;
        let ch = ms.children(act)[0].id;
        let sc = ms.children(ch)[0].id;
        // Strict levels (Project → Act → Chapter → Scene) make true cycles
        // unreachable; misuse surfaces as InvalidParent instead.
        assert_eq!(
            ms.move_node(act, sc),
            Err(ManuscriptError::InvalidParent {
                child: NodeKind::Act,
                parent: NodeKind::Scene,
            })
        );
        assert_eq!(
            ms.move_node(ms.root(), act),
            Err(ManuscriptError::CannotRemoveProject)
        );
    }

    #[test]
    fn move_chapter_between_acts() {
        let mut ms = sample();
        let act2 = ms.add_act("Act II").unwrap();
        let act1 = ms.children(ms.root())[0].id;
        let ch = ms.children(act1)[0].id;
        ms.move_node(ch, act2).unwrap();
        assert_eq!(ms.children(act2).len(), 1);
        assert!(ms.children(act1).is_empty());
        assert_eq!(
            ms.breadcrumb(ch).unwrap(),
            vec!["The Pink Dog", "Act II", "Chapter 1"]
        );
    }

    #[test]
    fn move_at_inserts_at_index_and_clamps() {
        let mut ms = Manuscript::new("T");
        let act = ms.add_act("A").unwrap();
        let c1 = ms.add_chapter(act, "C1").unwrap();
        let c2 = ms.add_chapter(act, "C2").unwrap();
        // Reorder within the same parent: C2 to the front.
        ms.move_node_at(c2, act, Some(0)).unwrap();
        let order: Vec<usize> = ms.children(act).iter().map(|n| n.id).collect();
        assert_eq!(order, vec![c2, c1]);
        // Out-of-range index clamps to the end.
        ms.move_node_at(c2, act, Some(99)).unwrap();
        let order: Vec<usize> = ms.children(act).iter().map(|n| n.id).collect();
        assert_eq!(order, vec![c1, c2]);
        // `None` appends, preserving the old `move_node` behaviour.
        ms.move_node_at(c1, act, None).unwrap();
        let order: Vec<usize> = ms.children(act).iter().map(|n| n.id).collect();
        assert_eq!(order, vec![c2, c1]);
    }

    #[test]
    fn rename_and_title() {
        let mut ms = Manuscript::new("Old");
        assert_eq!(ms.title(), "Old");
        ms.rename(ms.root(), "New").unwrap();
        assert_eq!(ms.title(), "New");
        assert!(ms.rename(99, "X").is_err());
    }
}
