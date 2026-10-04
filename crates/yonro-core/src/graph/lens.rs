//! Lenses: named ways of looking at the relationship graph.
//!
//! The inferred entity graph stays exactly one. A lens reframes it:
//! - [`Engine::Entity`] keeps edges with one endpoint on each of two
//!   kind-sides (an empty B side mirrors A, so both endpoints share it;
//!   both sides empty keeps everything).
//! - [`Engine::Scene`] flips the graph over: nodes are scenes in outline
//!   order, edges link scenes sharing entities (weight = shared count).
//! - [`Engine::Presence`] returns an entity × act/chapter matrix instead
//!   of a graph.
//!
//! Frontends render; all facts here are computed in core.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

use super::Graph;
use crate::lore::{EntityId, EntityKind, LoreBook};
use crate::manuscript::{Manuscript, NodeId, NodeKind};

/// Maximum saved (non-builtin) lenses per workspace.
pub const MAX_LENSES: usize = 24;

/// Which shape a lens produces.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    /// Entity graph with the pair rule applied.
    #[default]
    Entity,
    /// Scene nodes linked by shared entities.
    Scene,
    /// Entity × act/chapter presence matrix.
    Presence,
}

/// One named view over the graph.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Lens {
    /// Display name (unique across builtins and saved lenses).
    pub name: String,
    /// Which shape this lens produces.
    #[serde(default)]
    pub engine: Engine,
    /// Entity-kind labels for side A (`"character"`, …); empty = all.
    #[serde(default)]
    pub kinds_a: Vec<String>,
    /// Entity-kind labels for side B; empty mirrors side A.
    #[serde(default)]
    pub kinds_b: Vec<String>,
}

/// One scene node for the story map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneNode {
    /// Scene [`NodeId`] (doubles as the DTO node id).
    pub scene: NodeId,
    /// Scene title.
    pub title: String,
}

/// One scene link: weight = shared-entity count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneEdge {
    /// First scene.
    pub a: NodeId,
    /// Second scene.
    pub b: NodeId,
    /// Entities present in both scenes.
    pub shared: usize,
}

/// One presence-matrix row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresenceRow {
    /// Entity id.
    pub id: EntityId,
    /// Entity name.
    pub name: String,
    /// Kind label (`"character"`, …).
    pub kind: String,
    /// Scenes present per column, parallel to the column list.
    pub counts: Vec<usize>,
}

/// One presence-matrix column (an act or a chapter).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresenceColumn {
    /// Act/chapter [`NodeId`].
    pub id: NodeId,
    /// Act/chapter title.
    pub title: String,
    /// `"act"` or `"chapter"`.
    pub kind: String,
}

/// Lens storage failures (messages name the lens/file and the reason).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LensError {
    /// Blank lens name.
    EmptyName,
    /// A lens with that name already exists (builtins included).
    Duplicate(String),
    /// More than [`MAX_LENSES`] saved lenses.
    Full,
    /// No saved lens with that name.
    Unknown(String),
    /// Builtin lenses live in code and cannot be deleted.
    Builtin(String),
    /// `lenses.json` cannot be written (message names the file).
    Io(String),
}

impl fmt::Display for LensError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyName => write!(formatter, "lens name cannot be empty"),
            Self::Duplicate(name) => write!(formatter, "a lens named {name:?} already exists"),
            Self::Full => write!(formatter, "at most {MAX_LENSES} saved lenses"),
            Self::Unknown(name) => write!(formatter, "unknown lens {name:?}"),
            Self::Builtin(name) => write!(
                formatter,
                "{name:?} is a builtin lens and cannot be deleted"
            ),
            Self::Io(message) => write!(formatter, "{message}"),
        }
    }
}

impl std::error::Error for LensError {}

fn kind_label(kind: EntityKind) -> &'static str {
    match kind {
        EntityKind::Character => "character",
        EntityKind::Place => "place",
        EntityKind::Faction => "faction",
        EntityKind::Item => "item",
        EntityKind::Lore => "lore",
    }
}

fn parse_kinds(labels: &[String]) -> Vec<EntityKind> {
    labels
        .iter()
        .filter_map(|label| EntityKind::parse(label.as_str()))
        .collect()
}

impl Lens {
    fn named(name: &str, engine: Engine, kinds_a: &[&str], kinds_b: &[&str]) -> Self {
        Self {
            name: name.to_string(),
            engine,
            kinds_a: kinds_a.iter().map(ToString::to_string).collect(),
            kinds_b: kinds_b.iter().map(ToString::to_string).collect(),
        }
    }

    /// Effective pair sides (parsed kinds). An empty B mirrors A; both
    /// empty keep everything. A non-empty list that parses to nothing
    /// matches nothing.
    fn sides(&self) -> (Vec<EntityKind>, Vec<EntityKind>) {
        let a = parse_kinds(&self.kinds_a);
        let b = if self.kinds_b.is_empty() {
            a.clone()
        } else {
            parse_kinds(&self.kinds_b)
        };
        (a, b)
    }

    /// Whether this lens divides entities into two distinct sides
    /// (bipartite layout) rather than one pool.
    #[must_use]
    pub fn is_split(&self) -> bool {
        if self.engine != Engine::Entity || self.kinds_b.is_empty() {
            return false;
        }
        let (a, b) = self.sides();
        !a.is_empty() && !b.is_empty() && a != b
    }

    /// Pair rule over an entity graph: keep edges with one endpoint on
    /// each side, then drop nodes left with degree 0. Same edge order.
    #[must_use]
    pub fn pair_filter(&self, graph: &Graph, lore: &LoreBook) -> Graph {
        if self.engine != Engine::Entity {
            return graph.clone();
        }
        let both_open = self.kinds_a.is_empty() && self.kinds_b.is_empty();
        let (a, b) = self.sides();
        let mut edges = Vec::new();
        for edge in &graph.edges {
            if both_open {
                edges.push(edge.clone());
                continue;
            }
            let ka = lore.get(edge.a).map(|entity| entity.kind);
            let kb = lore.get(edge.b).map(|entity| entity.kind);
            let cross = match (ka, kb) {
                (Some(ka), Some(kb)) => {
                    (a.contains(&ka) && b.contains(&kb)) || (a.contains(&kb) && b.contains(&ka))
                }
                _ => false,
            };
            if cross {
                edges.push(edge.clone());
            }
        }
        let mut kept = BTreeSet::new();
        for edge in &edges {
            kept.insert(edge.a);
            kept.insert(edge.b);
        }
        Graph { nodes: kept, edges }
    }
}

/// The six builtin lenses (always first, never deletable).
#[must_use]
pub fn builtin_lenses() -> Vec<Lens> {
    vec![
        Lens::named("Cast", Engine::Entity, &["character"], &[]),
        Lens::named("Cast x Places", Engine::Entity, &["character"], &["place"]),
        Lens::named("Cast x Items", Engine::Entity, &["character"], &["item"]),
        Lens::named("Factions", Engine::Entity, &["faction"], &[]),
        Lens::named("Story map", Engine::Scene, &[], &[]),
        Lens::named("Act presence", Engine::Presence, &[], &[]),
    ]
}

/// Scenes for the story map: scope expansion plus POV and title-text
/// filters. Kinds and entity focus do not apply to scenes.
#[must_use]
pub fn scene_set(
    manuscript: &Manuscript,
    scope: Option<NodeId>,
    pov: Option<&str>,
    title_text: &str,
) -> BTreeSet<NodeId> {
    let mut scenes: BTreeSet<NodeId> = match scope {
        None => Graph::scene_order(manuscript).into_iter().collect(),
        Some(id) => super::query::expand_scope(manuscript, id),
    };
    let pov = pov.unwrap_or("").trim();
    if !pov.is_empty() {
        scenes.retain(|scene| {
            manuscript.get(*scene).is_some_and(|node| {
                node.meta
                    .as_ref()
                    .is_some_and(|meta| meta.pov.trim().eq_ignore_ascii_case(pov))
            })
        });
    }
    let needle = title_text.trim().to_lowercase();
    if !needle.is_empty() {
        scenes.retain(|scene| {
            manuscript
                .get(*scene)
                .is_some_and(|node| node.title.to_lowercase().contains(&needle))
        });
    }
    scenes
}

/// Story map over `scenes`: nodes in outline order, edges linking scenes
/// that share entities (weight = shared-entity count).
#[must_use]
pub fn scene_view(
    manuscript: &Manuscript,
    lore: &LoreBook,
    scene_texts: &BTreeMap<NodeId, String>,
    scenes: &BTreeSet<NodeId>,
) -> (Vec<SceneNode>, Vec<SceneEdge>) {
    let (presence, _) = Graph::evidence(manuscript, lore, scene_texts);
    let mut present: BTreeMap<NodeId, BTreeSet<EntityId>> = BTreeMap::new();
    for (entity, at) in &presence {
        for scene in at {
            if scenes.contains(scene) {
                present.entry(*scene).or_default().insert(*entity);
            }
        }
    }
    let mut nodes = Vec::new();
    for scene in Graph::scene_order(manuscript) {
        if !scenes.contains(&scene) {
            continue;
        }
        let title = manuscript
            .get(scene)
            .map_or_else(|| format!("scene-{scene}"), |node| node.title.clone());
        nodes.push(SceneNode { scene, title });
    }
    let mut edges = Vec::new();
    for (i, a) in nodes.iter().enumerate() {
        for b in nodes.iter().skip(i.saturating_add(1)) {
            let empty = BTreeSet::new();
            let shared = present
                .get(&a.scene)
                .unwrap_or(&empty)
                .intersection(present.get(&b.scene).unwrap_or(&empty))
                .count();
            if shared > 0 {
                edges.push(SceneEdge {
                    a: a.scene,
                    b: b.scene,
                    shared,
                });
            }
        }
    }
    edges.sort_by(|x, y| {
        y.shared
            .cmp(&x.shared)
            .then_with(|| (x.a, x.b).cmp(&(y.a, y.b)))
    });
    (nodes, edges)
}

/// Presence matrix: entities present in at least one scene, counted per
/// act/chapter column (acts interleaved with their chapters, outline
/// order). Rows sort by total desc, then name.
#[must_use]
pub fn presence_matrix(
    manuscript: &Manuscript,
    lore: &LoreBook,
    scene_texts: &BTreeMap<NodeId, String>,
) -> (Vec<PresenceRow>, Vec<PresenceColumn>) {
    fn scenes_under(manuscript: &Manuscript, id: NodeId, out: &mut BTreeSet<NodeId>) {
        let Some(node) = manuscript.get(id) else {
            return;
        };
        if node.kind == NodeKind::Scene {
            out.insert(id);
        }
        for child in manuscript.children(id) {
            scenes_under(manuscript, child.id, out);
        }
    }
    let mut columns = Vec::new();
    for act in manuscript.children(manuscript.root()) {
        if act.kind != NodeKind::Act {
            continue;
        }
        let mut act_scenes = BTreeSet::new();
        scenes_under(manuscript, act.id, &mut act_scenes);
        columns.push((
            PresenceColumn {
                id: act.id,
                title: act.title.clone(),
                kind: "act".to_string(),
            },
            act_scenes,
        ));
        for chapter in manuscript.children(act.id) {
            if chapter.kind != NodeKind::Chapter {
                continue;
            }
            let mut chapter_scenes = BTreeSet::new();
            scenes_under(manuscript, chapter.id, &mut chapter_scenes);
            columns.push((
                PresenceColumn {
                    id: chapter.id,
                    title: chapter.title.clone(),
                    kind: "chapter".to_string(),
                },
                chapter_scenes,
            ));
        }
    }
    let (presence, _) = Graph::evidence(manuscript, lore, scene_texts);
    let mut rows = Vec::new();
    for (entity, at) in &presence {
        let Some(detail) = lore.get(*entity) else {
            continue;
        };
        if at.is_empty() {
            continue;
        }
        let counts = columns
            .iter()
            .map(|(_, scenes)| at.intersection(scenes).count())
            .collect::<Vec<_>>();
        rows.push(PresenceRow {
            id: *entity,
            name: detail.name.clone(),
            kind: kind_label(detail.kind).to_string(),
            counts,
        });
    }
    rows.sort_by(|x, y| {
        let xt: usize = x.counts.iter().sum();
        let yt: usize = y.counts.iter().sum();
        yt.cmp(&xt).then_with(|| {
            x.name
                .to_lowercase()
                .cmp(&y.name.to_lowercase())
                .then_with(|| x.id.cmp(&y.id))
        })
    });
    let columns = columns.into_iter().map(|(column, _)| column).collect();
    (rows, columns)
}

/// Saved (non-builtin) lenses in one workspace (`.yonro/lenses.json`).
#[derive(Debug, Clone, Default)]
pub struct LensStore {
    customs: Vec<Lens>,
}

impl LensStore {
    /// Empty store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            customs: Vec::new(),
        }
    }

    /// Saved lenses in creation order.
    #[must_use]
    pub fn customs(&self) -> &[Lens] {
        &self.customs
    }

    /// `lenses.json` inside a workspace `.yonro/` dir.
    #[must_use]
    pub fn file_in(dot_yonro: &Path) -> PathBuf {
        dot_yonro.join("lenses.json")
    }

    /// Load `path`, falling back to empty on any failure (missing files
    /// from old workspaces and corrupt JSON never crash, never error).
    #[must_use]
    pub fn load(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::new();
        };
        let customs: Vec<Lens> = serde_json::from_str(&text).unwrap_or_default();
        Self {
            customs: customs.into_iter().take(MAX_LENSES).collect(),
        }
    }

    /// Persist as pretty JSON (creates parent dirs; atomic tmp + rename).
    ///
    /// # Errors
    /// `Io` when the directory cannot be created or any write/sync/rename
    /// step fails.
    pub fn save(&self, path: &Path) -> Result<(), LensError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|err| LensError::Io(format!("{}: {err}", parent.display())))?;
            }
        }
        let json = serde_json::to_string_pretty(&self.customs)
            .map_err(|err| LensError::Io(format!("{}: {err}", path.display())))?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, json.as_bytes())
            .map_err(|err| LensError::Io(format!("{}: {err}", tmp.display())))?;
        let handle = std::fs::File::open(&tmp)
            .map_err(|err| LensError::Io(format!("{}: {err}", tmp.display())))?;
        handle
            .sync_all()
            .map_err(|err| LensError::Io(format!("{}: {err}", tmp.display())))?;
        drop(handle);
        std::fs::rename(&tmp, path)
            .map_err(|err| LensError::Io(format!("{}: {err}", path.display())))?;
        Ok(())
    }

    fn taken(&self, name: &str) -> bool {
        builtin_lenses()
            .iter()
            .chain(self.customs.iter())
            .any(|lens| lens.name.eq_ignore_ascii_case(name))
    }

    /// Save a validated lens (name stored trimmed).
    ///
    /// # Errors
    /// `EmptyName` for a blank name, `Duplicate` naming the clash
    /// (builtins included), `Full` past [`MAX_LENSES`] saved lenses.
    pub fn save_lens(&mut self, mut lens: Lens) -> Result<(), LensError> {
        lens.name = lens.name.trim().to_string();
        if lens.name.is_empty() {
            return Err(LensError::EmptyName);
        }
        if self.taken(&lens.name) {
            return Err(LensError::Duplicate(lens.name));
        }
        if self.customs.len() >= MAX_LENSES {
            return Err(LensError::Full);
        }
        self.customs.push(lens);
        Ok(())
    }

    /// Delete a saved lens (drafts are unaffected; there are none).
    ///
    /// # Errors
    /// `Builtin` for the six builtins, `Unknown` for anything else absent.
    pub fn delete_lens(&mut self, name: &str) -> Result<(), LensError> {
        if builtin_lenses()
            .iter()
            .any(|lens| lens.name.eq_ignore_ascii_case(name))
        {
            return Err(LensError::Builtin(name.trim().to_string()));
        }
        let at = self
            .customs
            .iter()
            .position(|lens| lens.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| LensError::Unknown(name.trim().to_string()))?;
        self.customs.remove(at);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lore::EntityKind;
    use crate::manuscript::SceneMeta;

    /// Two acts: Act 1 holds Mara + Mill farm (+ Joren mentioned once);
    /// Act 2 holds Joren + Old bridge.
    fn two_acts() -> (
        Manuscript,
        LoreBook,
        BTreeMap<NodeId, String>,
        NodeId,
        NodeId,
    ) {
        let mut ms = Manuscript::new("T");
        let act1 = ms.add_act("Act 1").unwrap();
        let ch1 = ms.add_chapter(act1, "C1").unwrap();
        let s1 = ms.add_scene(ch1, "Gate").unwrap();
        ms.set_meta(
            s1,
            SceneMeta {
                pov: "Mara".to_string(),
                setting: "Mill farm".to_string(),
                ..SceneMeta::default()
            },
        )
        .unwrap();
        let act2 = ms.add_act("Act 2").unwrap();
        let ch2 = ms.add_chapter(act2, "C2").unwrap();
        let s2 = ms.add_scene(ch2, "Bridge").unwrap();
        ms.set_meta(
            s2,
            SceneMeta {
                pov: "Joren".to_string(),
                setting: "Old bridge".to_string(),
                ..SceneMeta::default()
            },
        )
        .unwrap();
        let mut lore = LoreBook::new();
        lore.add(EntityKind::Character, "Mara").unwrap();
        lore.add(EntityKind::Character, "Joren").unwrap();
        lore.add(EntityKind::Place, "Mill farm").unwrap();
        lore.add(EntityKind::Place, "Old bridge").unwrap();
        let mut texts = BTreeMap::new();
        texts.insert(s1, "Mara waved at @Joren.".to_string());
        texts.insert(s2, "The bridge was empty.".to_string());
        (ms, lore, texts, act1, act2)
    }

    #[test]
    fn pair_filter_excludes_char_char_edges() {
        let (ms, lore, texts, _, _) = two_acts();
        let graph = Graph::build(&ms, &lore, &texts);
        assert!(graph.edges.len() >= 2);
        let lens = builtin_lenses()
            .into_iter()
            .find(|lens| lens.name == "Cast x Places")
            .unwrap();
        let filtered = lens.pair_filter(&graph, &lore);
        // Every surviving edge links a character with a place.
        assert!(!filtered.edges.is_empty());
        for edge in &filtered.edges {
            let kinds = [edge.a, edge.b].map(|id| lore.get(id).unwrap().kind);
            assert!(kinds.contains(&EntityKind::Character));
            assert!(kinds.contains(&EntityKind::Place));
        }
        // The Cast lens instead keeps same-side (character) edges.
        let cast = builtin_lenses()
            .into_iter()
            .find(|lens| lens.name == "Cast")
            .unwrap();
        assert!(!cast.is_split());
        let same = cast.pair_filter(&graph, &lore);
        for edge in &same.edges {
            for id in [edge.a, edge.b] {
                assert_eq!(lore.get(id).unwrap().kind, EntityKind::Character);
            }
        }
        // Cast x Places splits; Cast does not.
        assert!(lens.is_split());
        // Non-entity engines skip the pair rule untouched.
        let map = builtin_lenses()
            .into_iter()
            .find(|lens| lens.name == "Story map")
            .unwrap();
        assert_eq!(map.pair_filter(&graph, &lore), graph);
    }

    #[test]
    fn scene_weight_counts_shared_entities() {
        let (ms, lore, texts, _, _) = two_acts();
        // Joren is mentioned in Act 1 and POV in Act 2: the only bridge.
        let all: BTreeSet<NodeId> = Graph::scene_order(&ms).into_iter().collect();
        let (nodes, edges) = scene_view(&ms, &lore, &texts, &all);
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].title.as_str(), "Gate");
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].shared, 1);
    }

    #[test]
    fn presence_counts_per_act_match() {
        let (ms, lore, texts, _, _) = two_acts();
        let (rows, columns) = presence_matrix(&ms, &lore, &texts);
        assert_eq!(columns.len(), 4);
        assert_eq!(columns[0].title.as_str(), "Act 1");
        assert_eq!(columns[0].kind.as_str(), "act");
        assert_eq!(columns[1].title.as_str(), "C1");
        let by_name: BTreeMap<&str, &[usize]> = rows
            .iter()
            .map(|row| (row.name.as_str(), row.counts.as_slice()))
            .collect();
        // Mara: Act 1 scene only.
        assert_eq!(by_name["Mara"], &[1, 1, 0, 0]);
        // Joren: mentioned in Act 1, POV in Act 2.
        assert_eq!(by_name["Joren"], &[1, 1, 1, 1]);
        // Mill farm: Act 1 only.
        assert_eq!(by_name["Mill farm"], &[1, 1, 0, 0]);
        // Rows sort by total desc.
        let totals: Vec<usize> = rows.iter().map(|row| row.counts.iter().sum()).collect();
        assert!(totals.windows(2).all(|w| w[0] >= w[1]));
    }

    #[test]
    fn empty_manuscript_stays_empty_without_panic() {
        let ms = Manuscript::new("Empty");
        let lore = LoreBook::new();
        let texts = BTreeMap::new();
        let graph = Graph::build(&ms, &lore, &texts);
        assert!(graph.nodes.is_empty());
        for lens in builtin_lenses() {
            let filtered = lens.pair_filter(&graph, &lore);
            assert!(filtered.nodes.is_empty());
            assert!(filtered.edges.is_empty());
        }
        let all: BTreeSet<NodeId> = Graph::scene_order(&ms).into_iter().collect();
        let (nodes, edges) = scene_view(&ms, &lore, &texts, &all);
        assert!(nodes.is_empty());
        assert!(edges.is_empty());
        let (rows, columns) = presence_matrix(&ms, &lore, &texts);
        assert!(rows.is_empty());
        assert!(columns.is_empty());
    }

    #[test]
    fn store_rejects_duplicates_overflow_and_builtins() {
        let mut store = LensStore::new();
        assert!(store
            .save_lens(Lens {
                name: "  ".to_string(),
                engine: Engine::Entity,
                kinds_a: vec![],
                kinds_b: vec![],
            })
            .is_err());
        store
            .save_lens(Lens {
                name: "Mine".to_string(),
                engine: Engine::Entity,
                kinds_a: vec!["character".to_string()],
                kinds_b: vec![],
            })
            .unwrap();
        // Duplicates clash case-insensitively, builtins included.
        assert!(store
            .save_lens(Lens {
                name: "mine".to_string(),
                engine: Engine::Entity,
                kinds_a: vec![],
                kinds_b: vec![],
            })
            .is_err());
        assert!(store
            .save_lens(Lens {
                name: "cast".to_string(),
                engine: Engine::Entity,
                kinds_a: vec![],
                kinds_b: vec![],
            })
            .is_err());
        assert!(store.delete_lens("Cast").is_err());
        assert!(store.delete_lens("Ghost").is_err());
        store.delete_lens("MINE").unwrap();
        assert!(store.customs().is_empty());
        // Cap at MAX_LENSES.
        for i in 0..MAX_LENSES {
            store
                .save_lens(Lens {
                    name: format!("Lens{i}"),
                    engine: Engine::Entity,
                    kinds_a: vec![],
                    kinds_b: vec![],
                })
                .unwrap();
        }
        assert!(store
            .save_lens(Lens {
                name: "Overflow".to_string(),
                engine: Engine::Entity,
                kinds_a: vec![],
                kinds_b: vec![],
            })
            .is_err());
        // Round trip through disk.
        let dir = std::env::temp_dir().join(format!("yonro-lens-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = LensStore::file_in(&dir.join(".yonro"));
        assert!(LensStore::load(&path).customs().is_empty());
        store.save(&path).unwrap();
        let back = LensStore::load(&path);
        assert_eq!(back.customs().len(), MAX_LENSES);
        std::fs::write(&path, "broken{{").unwrap();
        assert!(LensStore::load(&path).customs().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
