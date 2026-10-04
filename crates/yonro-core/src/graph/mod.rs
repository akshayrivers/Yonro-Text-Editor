//! Character relationship graph (`PLAN.md Phase 5.2`, core half).
//!
//! This module *computes* relationships; frontends only render them
//! (adapter doctrine: the GUI/CLI never compute narrative facts).
//!
//! Signals (combined, per product decision):
//! * **Co-occurrence** — entities sharing a scene link (POV, setting, and
//!   `@mentioned` entities all count as "present"). Weight = shared scenes.
//! * **Mention** — a scene's POV character links to every entity `@mentioned`
//!   in that scene's text. Weight = mentioning scenes.
//!
//! Inputs are plain data (`&Manuscript`, `&LoreBook`, scene texts), so the
//! same computation serves the TUI, the Tauri GUI, and a future web build.

pub mod lens;
pub mod query;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use super::lore::{EntityId, LoreBook};
use super::manuscript::{Manuscript, NodeId};

/// How two entities are linked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EdgeKind {
    /// Present in the same scene(s).
    Cooccurrence,
    /// POV character `@mentioned` the other in scene text.
    Mention,
}

/// One undirected link with per-signal weights.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    pub a: EntityId,
    pub b: EntityId,
    /// Scenes shared (co-occurrence weight).
    pub shared_scenes: usize,
    /// Scenes where the POV mentioned the other.
    pub mention_scenes: usize,
}

impl Edge {
    /// Combined weight (sum of both signals).
    #[must_use]
    pub fn weight(&self) -> usize {
        self.shared_scenes.saturating_add(self.mention_scenes)
    }

    /// Which signals produced this edge.
    #[must_use]
    pub fn kinds(&self) -> Vec<EdgeKind> {
        let mut kinds = Vec::new();
        if self.shared_scenes > 0 {
            kinds.push(EdgeKind::Cooccurrence);
        }
        if self.mention_scenes > 0 {
            kinds.push(EdgeKind::Mention);
        }
        kinds
    }
}

/// Graph build failures (structural, never data-dependent panics).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphError {
    UnknownScene(NodeId),
}

impl fmt::Display for GraphError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownScene(id) => write!(formatter, "unknown scene node {id}"),
        }
    }
}

impl std::error::Error for GraphError {}

/// Entity presence: entity -> scenes present.
pub(crate) type Presence = BTreeMap<EntityId, BTreeSet<NodeId>>;
/// Directed mention pairs: (mentioner, mentioned) -> scenes.
pub(crate) type Mentions = BTreeMap<(EntityId, EntityId), BTreeSet<NodeId>>;

/// Computed relationship graph: nodes are lore entity ids.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Graph {
    /// lore entity ids present in at least one scene.
    pub nodes: BTreeSet<EntityId>,
    /// Undirected edges, canonical `(min, max)` ordering, sorted.
    pub edges: Vec<Edge>,
}

impl Graph {
    /// Per-scene evidence behind a graph: entity presence plus directed
    /// mention pairs, both as scene sets. Shared by `build` and filtered
    /// views (`query::apply`) so both see identical facts.
    pub(crate) fn evidence(
        manuscript: &Manuscript,
        lore: &LoreBook,
        scene_texts: &BTreeMap<NodeId, String>,
    ) -> (Presence, Mentions) {
        // entity -> scenes present; (pov, mentioned) -> scenes.
        let mut presence: BTreeMap<EntityId, BTreeSet<NodeId>> = BTreeMap::new();
        let mut mentions: BTreeMap<(EntityId, EntityId), BTreeSet<NodeId>> = BTreeMap::new();

        for scene_id in Self::scene_order(manuscript) {
            let Some(node) = manuscript.get(scene_id) else {
                continue;
            };
            let meta = node.meta.as_ref();
            // Present: POV + setting (as registered entities, if known).
            let mut present: BTreeSet<EntityId> = BTreeSet::new();
            if let Some(meta) = meta {
                for name in [&meta.pov, &meta.setting] {
                    if name.trim().is_empty() {
                        continue;
                    }
                    if let Some(entity) = lore.resolve(name) {
                        present.insert(entity.id);
                    }
                }
            }
            // Present + directing: @mentions in the draft text.
            let mut pov: Option<EntityId> = None;
            if let Some(meta) = meta {
                if !meta.pov.trim().is_empty() {
                    pov = lore.resolve(&meta.pov).map(|entity| entity.id);
                }
            }
            if let Some(text) = scene_texts.get(&scene_id) {
                for mention in lore.parse_mentions(text) {
                    if let Some(id) = mention.entity {
                        present.insert(id);
                        if let Some(pov_id) = pov {
                            if pov_id != id {
                                mentions
                                    .entry((pov_id.min(id), pov_id.max(id)))
                                    .or_default()
                                    .insert(scene_id);
                            }
                        }
                    }
                }
            }
            for id in &present {
                presence.entry(*id).or_default().insert(scene_id);
            }
        }
        (presence, mentions)
    }

    /// Build from manuscript structure + lore + draft texts.
    ///
    /// `scene_texts` maps scene [`NodeId`] to its current draft text
    /// (frontends sync this from live buffers; missing scenes simply
    /// contribute POV/setting presence without mention edges).
    /// Unknown/dead scenes in the map are ignored.
    #[must_use]
    pub fn build(
        manuscript: &Manuscript,
        lore: &LoreBook,
        scene_texts: &BTreeMap<NodeId, String>,
    ) -> Self {
        let (presence, mentions) = Self::evidence(manuscript, lore, scene_texts);

        // Co-occurrence: every pair sharing ≥1 scene.
        let mut cooccur: BTreeMap<(EntityId, EntityId), BTreeSet<NodeId>> = BTreeMap::new();
        // entity -> scenes, inverted to scene -> entities.
        let mut by_scene: BTreeMap<NodeId, Vec<EntityId>> = BTreeMap::new();
        for (entity, scenes) in &presence {
            for scene in scenes {
                by_scene.entry(*scene).or_default().push(*entity);
            }
        }
        for (scene, entities) in &by_scene {
            for (i, a) in entities.iter().enumerate() {
                for b in entities.iter().skip(i.saturating_add(1)) {
                    cooccur
                        .entry(((*a).min(*b), (*a).max(*b)))
                        .or_default()
                        .insert(*scene);
                }
            }
        }

        let mut nodes = BTreeSet::new();
        let mut edges: BTreeMap<(EntityId, EntityId), Edge> = BTreeMap::new();
        for ((a, b), scenes) in cooccur {
            nodes.insert(a);
            nodes.insert(b);
            edges.insert(
                (a, b),
                Edge {
                    a,
                    b,
                    shared_scenes: scenes.len(),
                    mention_scenes: 0,
                },
            );
        }
        for ((a, b), scenes) in mentions {
            nodes.insert(a);
            nodes.insert(b);
            edges
                .entry((a, b))
                .and_modify(|edge| {
                    edge.mention_scenes = edge.mention_scenes.saturating_add(scenes.len());
                })
                .or_insert(Edge {
                    a,
                    b,
                    shared_scenes: 0,
                    mention_scenes: scenes.len(),
                });
        }
        let mut edges: Vec<Edge> = edges.into_values().collect();
        edges.sort_by(|x, y| {
            y.weight()
                .cmp(&x.weight())
                .then_with(|| (x.a, x.b).cmp(&(y.a, y.b)))
        });
        Self { nodes, edges }
    }

    /// Scene ids in outline order (depth-first acts → chapters → scenes).
    #[must_use]
    pub fn scene_order(manuscript: &Manuscript) -> Vec<NodeId> {
        fn walk(manuscript: &Manuscript, id: NodeId, out: &mut Vec<NodeId>) {
            if let Some(node) = manuscript.get(id) {
                if node.kind == super::manuscript::NodeKind::Scene {
                    out.push(id);
                }
                for child in manuscript.children(id) {
                    walk(manuscript, child.id, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(manuscript, manuscript.root(), &mut out);
        out
    }

    /// Neighbors of `entity` with combined weights, strongest first.
    #[must_use]
    pub fn neighbors(&self, entity: EntityId) -> Vec<(EntityId, usize)> {
        let mut out = Vec::new();
        for edge in &self.edges {
            if edge.a == entity {
                out.push((edge.b, edge.weight()));
            } else if edge.b == entity {
                out.push((edge.a, edge.weight()));
            }
        }
        out.sort_by(|x, y| y.1.cmp(&x.1).then_with(|| x.0.cmp(&y.0)));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lore::EntityKind;
    use crate::manuscript::SceneMeta;

    fn story() -> (Manuscript, LoreBook, BTreeMap<NodeId, String>) {
        let mut ms = Manuscript::new("T");
        let act = ms.add_act("A").unwrap();
        let ch = ms.add_chapter(act, "C").unwrap();
        let s1 = ms.add_scene(ch, "Gate").unwrap();
        ms.set_meta(
            s1,
            SceneMeta {
                pov: "Mara".to_string(),
                setting: "Mill farm".to_string(),
                ..SceneMeta::default()
            },
        )
        .unwrap();
        let s2 = ms.add_scene(ch, "River").unwrap();
        ms.set_meta(
            s2,
            SceneMeta {
                pov: "Mara".to_string(),
                setting: "Old bridge".to_string(),
                ..SceneMeta::default()
            },
        )
        .unwrap();
        let mut lore = LoreBook::new();
        lore.add(EntityKind::Character, "Mara").unwrap();
        lore.add(EntityKind::Place, "Mill farm").unwrap();
        lore.add(EntityKind::Place, "Old bridge").unwrap();
        lore.add(EntityKind::Character, "Joren").unwrap();
        let mut texts = BTreeMap::new();
        texts.insert(s1, "Mara waved at @Joren by the mill.".to_string());
        texts.insert(s2, "The bridge was empty.".to_string());
        (ms, lore, texts)
    }

    #[test]
    fn presence_links_pov_setting_and_mentions() {
        let (ms, lore, texts) = story();
        let graph = Graph::build(&ms, &lore, &texts);
        let mara = lore.resolve("Mara").unwrap().id;
        let mill = lore.resolve("Mill farm").unwrap().id;
        let joren = lore.resolve("Joren").unwrap().id;
        // Mara + Mill farm share scene 1 (co-occurrence, no mention edge:
        // the text mentions Joren, not the farm).
        let edge = graph
            .edges
            .iter()
            .find(|e| (e.a, e.b) == (mara.min(mill), mara.max(mill)))
            .unwrap();
        assert_eq!(edge.shared_scenes, 1);
        assert_eq!(edge.mention_scenes, 0);
        // Mara → Joren mention edge from scene 1's text (plus co-presence:
        // Joren is present via the mention itself).
        let edge = graph
            .edges
            .iter()
            .find(|e| (e.a, e.b) == (mara.min(joren), mara.max(joren)))
            .unwrap();
        assert_eq!(edge.mention_scenes, 1);
        assert!(edge.shared_scenes >= 1);
        assert!(edge.kinds().contains(&EdgeKind::Mention));
    }

    #[test]
    fn unregistered_povs_contribute_nothing() {
        let (ms, mut lore, texts) = story();
        // Remove everyone: no nodes, no edges (no panics on empty lore).
        for id in [0, 1, 2, 3] {
            let _ = lore.remove(id);
        }
        let graph = Graph::build(&ms, &lore, &texts);
        assert!(graph.nodes.is_empty());
        assert!(graph.edges.is_empty());
    }

    #[test]
    fn scene_order_follows_outline() {
        let (ms, _, _) = story();
        assert_eq!(Graph::scene_order(&ms).len(), 2);
    }

    #[test]
    fn neighbors_rank_by_weight() {
        let (ms, lore, texts) = story();
        let graph = Graph::build(&ms, &lore, &texts);
        let mara = lore.resolve("Mara").unwrap().id;
        let neighbors = graph.neighbors(mara);
        assert!(!neighbors.is_empty());
        assert!(neighbors.windows(2).all(|w| w[0].1 >= w[1].1));
    }
}
