//! Filtered graph views (Obsidian-style panel, computed in core).
//!
//! Pipeline order (fixed):
//! 1. `scope` narrows the scene set (act/chapter/scene, else whole book).
//! 2. `pov` keeps entities appearing in scenes with that POV.
//! 3. `kinds`/`text` keep matching entities.
//! 4. Edges rebuild over the surviving scenes; `min_weight` drops weak ones.
//! 5. `focus` BFS (`depth` hops) keeps the local neighborhood.
//! 6. `min_degree`/`hide_orphans` drop poorly linked nodes.
//! 7. `max_nodes` keeps the top nodes by degree.
//!
//! Degrees always derive from the surviving edges, so they are recomputed
//! after every stage by construction. An empty query returns the input
//! graph unchanged.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::{Edge, Graph};
use crate::lore::{EntityId, EntityKind, LoreBook};
use crate::manuscript::{Manuscript, NodeId, NodeKind};

/// Filter set for one graph view. Every field unset/zero means no
/// filtering there; a fully empty query returns its input unchanged.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct GraphQuery {
    /// Entity kind labels (`"character"`, …); empty = all. Unknown labels
    /// match nothing.
    #[serde(default)]
    pub kinds: Vec<String>,
    /// Case-insensitive substring on name or alias; empty = all.
    #[serde(default)]
    pub text: String,
    /// Act/chapter/scene node: build over its scenes only. `None`, the
    /// project root, or an unknown id = whole book.
    #[serde(default)]
    pub scope: Option<NodeId>,
    /// Keep entities appearing in scenes with this POV
    /// (case-insensitive); `None`/blank = all.
    #[serde(default)]
    pub pov: Option<String>,
    /// Drop edges below this combined weight.
    #[serde(default)]
    pub min_weight: usize,
    /// Drop nodes with fewer surviving links.
    #[serde(default)]
    pub min_degree: usize,
    /// Drop link-less nodes.
    #[serde(default)]
    pub hide_orphans: bool,
    /// Local graph around this entity.
    #[serde(default)]
    pub focus: Option<EntityId>,
    /// Focus hops, clamped to 1..=3.
    #[serde(default)]
    pub depth: u8,
    /// Keep the top N by degree (ties by id); 0 = no cap.
    #[serde(default)]
    pub max_nodes: usize,
}

impl GraphQuery {
    /// Whether no stage filters anything.
    fn is_empty(&self) -> bool {
        self.kinds.is_empty()
            && self.text.trim().is_empty()
            && self.scope.is_none()
            && self.pov.as_ref().is_none_or(|pov| pov.trim().is_empty())
            && self.min_weight == 0
            && self.min_degree == 0
            && !self.hide_orphans
            && self.focus.is_none()
            && self.max_nodes == 0
    }
}

/// Scenes under `id`: the scene itself, a chapter's scenes, an act's
/// scenes, or the whole book for the project root / unknown ids.
fn expand_scope(manuscript: &Manuscript, id: NodeId) -> BTreeSet<NodeId> {
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
    let mut out = BTreeSet::new();
    match manuscript.get(id) {
        None => Graph::scene_order(manuscript).into_iter().collect(),
        Some(node) => match node.kind {
            NodeKind::Project => Graph::scene_order(manuscript).into_iter().collect(),
            NodeKind::Scene => {
                out.insert(id);
                out
            }
            NodeKind::Act | NodeKind::Chapter => {
                scenes_under(manuscript, id, &mut out);
                out
            }
        },
    }
}

/// Filtered copy of `graph` (same edge sort order).
///
/// Scope/pov filter on scenes, so the build inputs travel along; the
/// returned graph only ever contains nodes and edges the unfiltered
/// build would produce.
#[must_use]
pub fn apply(
    graph: &Graph,
    manuscript: &Manuscript,
    lore: &LoreBook,
    scene_texts: &BTreeMap<NodeId, String>,
    query: &GraphQuery,
) -> Graph {
    if query.is_empty() {
        return graph.clone();
    }
    let (presence, mentions) = Graph::evidence(manuscript, lore, scene_texts);
    let edge_scenes = edge_scenes(manuscript, query);
    let mut kept = keep_entities(&presence, lore, &edge_scenes, query);
    let mut pairs = rebuild_edges(&presence, &mentions, &kept, &edge_scenes, query.min_weight);

    // 5. Local graph: BFS over the surviving edges.
    if query.focus.is_some() {
        kept = focus_keep(&pairs, &kept, query);
        pairs.retain(|(a, b), _| kept.contains(a) && kept.contains(b));
        if kept.is_empty() {
            return Graph::default();
        }
    }

    // 6. Degrees derive from the surviving edges.
    let degree = drop_weak(&mut pairs, &mut kept, query);

    // 7. Cap: top nodes by degree, ties by id (deterministic).
    if query.max_nodes > 0 && kept.len() > query.max_nodes {
        let mut ranked: Vec<EntityId> = kept.into_iter().collect();
        ranked.sort_by(|x, y| {
            degree
                .get(y)
                .copied()
                .unwrap_or(0)
                .cmp(&degree.get(x).copied().unwrap_or(0))
                .then_with(|| x.cmp(y))
        });
        kept = ranked.into_iter().take(query.max_nodes).collect();
        pairs.retain(|(a, b), _| kept.contains(a) && kept.contains(b));
    }

    let mut edges: Vec<Edge> = pairs.into_values().collect();
    edges.sort_by(|x, y| {
        y.weight()
            .cmp(&x.weight())
            .then_with(|| (x.a, x.b).cmp(&(y.a, y.b)))
    });
    Graph { nodes: kept, edges }
}

/// Stages 1-2: scenes the edges may use (scope narrowed, POV intersected).
fn edge_scenes(manuscript: &Manuscript, query: &GraphQuery) -> BTreeSet<NodeId> {
    let scope_scenes: BTreeSet<NodeId> = match query.scope {
        None => Graph::scene_order(manuscript).into_iter().collect(),
        Some(id) => expand_scope(manuscript, id),
    };
    let pov = query.pov.as_deref().map_or("", str::trim);
    if pov.is_empty() {
        return scope_scenes;
    }
    let pov_scenes: BTreeSet<NodeId> = Graph::scene_order(manuscript)
        .into_iter()
        .filter(|scene| {
            manuscript.get(*scene).is_some_and(|node| {
                node.meta
                    .as_ref()
                    .is_some_and(|meta| meta.pov.trim().eq_ignore_ascii_case(pov))
            })
        })
        .collect();
    scope_scenes.intersection(&pov_scenes).copied().collect()
}

/// Stages 2-3: entities present in the surviving scenes, matching
/// kinds/text. A non-empty kinds list filters (unknown labels match nothing).
fn keep_entities(
    presence: &BTreeMap<EntityId, BTreeSet<NodeId>>,
    lore: &LoreBook,
    edge_scenes: &BTreeSet<NodeId>,
    query: &GraphQuery,
) -> BTreeSet<EntityId> {
    let kinds: Vec<EntityKind> = query
        .kinds
        .iter()
        .filter_map(|kind| EntityKind::parse(kind))
        .collect();
    let filter_kinds = !query.kinds.is_empty();
    let needle = query.text.trim().to_lowercase();
    let mut kept: BTreeSet<EntityId> = BTreeSet::new();
    for (entity, scenes) in presence {
        if scenes.intersection(edge_scenes).next().is_none() {
            continue;
        }
        let Some(detail) = lore.get(*entity) else {
            continue;
        };
        if filter_kinds && !kinds.contains(&detail.kind) {
            continue;
        }
        if !needle.is_empty()
            && !detail.name.to_lowercase().contains(&needle)
            && !detail
                .aliases
                .iter()
                .any(|alias| alias.to_lowercase().contains(&needle))
        {
            continue;
        }
        kept.insert(*entity);
    }
    kept
}

/// Stage 4: edges over the surviving scenes; weights count those scenes
/// only, then `min_weight` drops weak ones.
fn rebuild_edges(
    presence: &BTreeMap<EntityId, BTreeSet<NodeId>>,
    mentions: &BTreeMap<(EntityId, EntityId), BTreeSet<NodeId>>,
    kept: &BTreeSet<EntityId>,
    edge_scenes: &BTreeSet<NodeId>,
    min_weight: usize,
) -> BTreeMap<(EntityId, EntityId), Edge> {
    let mut by_scene: BTreeMap<NodeId, Vec<EntityId>> = BTreeMap::new();
    for entity in kept {
        if let Some(scenes) = presence.get(entity) {
            for scene in scenes.intersection(edge_scenes) {
                by_scene.entry(*scene).or_default().push(*entity);
            }
        }
    }
    let mut shared: BTreeMap<(EntityId, EntityId), usize> = BTreeMap::new();
    for entities in by_scene.values() {
        for (i, a) in entities.iter().enumerate() {
            for b in entities.iter().skip(i.saturating_add(1)) {
                let key = ((*a).min(*b), (*a).max(*b));
                shared
                    .entry(key)
                    .and_modify(|count| *count = count.saturating_add(1))
                    .or_insert(1);
            }
        }
    }
    let mut mentioned: BTreeMap<(EntityId, EntityId), usize> = BTreeMap::new();
    for ((a, b), scenes) in mentions {
        if !kept.contains(a) || !kept.contains(b) {
            continue;
        }
        let count = scenes.intersection(edge_scenes).count();
        if count > 0 {
            mentioned.insert((*a, *b), count);
        }
    }
    let mut pairs: BTreeMap<(EntityId, EntityId), Edge> = BTreeMap::new();
    for ((a, b), count) in shared {
        pairs.insert(
            (a, b),
            Edge {
                a,
                b,
                shared_scenes: count,
                mention_scenes: 0,
            },
        );
    }
    for ((a, b), count) in mentioned {
        pairs
            .entry((a, b))
            .and_modify(|edge| {
                edge.mention_scenes = edge.mention_scenes.saturating_add(count);
            })
            .or_insert(Edge {
                a,
                b,
                shared_scenes: 0,
                mention_scenes: count,
            });
    }
    pairs.retain(|_, edge| edge.weight() >= min_weight);
    pairs
}

/// Stage 5: BFS hops around the focus over surviving edges (depth clamped
/// 1..=3). Unknown focus keeps nothing.
fn focus_keep(
    pairs: &BTreeMap<(EntityId, EntityId), Edge>,
    kept: &BTreeSet<EntityId>,
    query: &GraphQuery,
) -> BTreeSet<EntityId> {
    let Some(focus) = query.focus else {
        return kept.clone();
    };
    if !kept.contains(&focus) {
        return BTreeSet::new();
    }
    let mut seen: BTreeSet<EntityId> = BTreeSet::from([focus]);
    let mut frontier = VecDeque::from([focus]);
    for _ in 0..query.depth.clamp(1, 3) {
        let mut next = Vec::new();
        while let Some(current) = frontier.pop_front() {
            for (a, b) in pairs.keys() {
                let other = if *a == current {
                    Some(*b)
                } else if *b == current {
                    Some(*a)
                } else {
                    None
                };
                if let Some(other) = other {
                    if seen.insert(other) {
                        next.push(other);
                    }
                }
            }
        }
        frontier.extend(next);
    }
    seen
}

/// Stage 6: drop nodes below `min_degree` (and orphans when asked),
/// then edges touching them. Returns degrees of the survivors.
fn drop_weak(
    pairs: &mut BTreeMap<(EntityId, EntityId), Edge>,
    kept: &mut BTreeSet<EntityId>,
    query: &GraphQuery,
) -> BTreeMap<EntityId, usize> {
    let mut degree: BTreeMap<EntityId, usize> = BTreeMap::new();
    for (a, b) in pairs.keys() {
        degree
            .entry(*a)
            .and_modify(|count| *count = count.saturating_add(1))
            .or_insert(1);
        degree
            .entry(*b)
            .and_modify(|count| *count = count.saturating_add(1))
            .or_insert(1);
    }
    kept.retain(|entity| {
        let links = degree.get(entity).copied().unwrap_or(0);
        links >= query.min_degree && (!query.hide_orphans || links > 0)
    });
    pairs.retain(|(a, b), _| kept.contains(a) && kept.contains(b));
    degree
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lore::EntityKind;
    use crate::manuscript::SceneMeta;

    /// Two chapters: ch1 is Mara's (Mill farm, Joren mentioned), ch2 is
    /// Joren's (Old bridge, Wren mentioned).
    fn two_chapters() -> (
        Manuscript,
        LoreBook,
        BTreeMap<NodeId, String>,
        NodeId,
        NodeId,
    ) {
        let mut ms = Manuscript::new("T");
        let act = ms.add_act("A").unwrap();
        let ch1 = ms.add_chapter(act, "C1").unwrap();
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
        let s2 = ms.add_scene(ch1, "Yard").unwrap();
        ms.set_meta(
            s2,
            SceneMeta {
                pov: "Mara".to_string(),
                setting: "Mill farm".to_string(),
                ..SceneMeta::default()
            },
        )
        .unwrap();
        let ch2 = ms.add_chapter(act, "C2").unwrap();
        let s3 = ms.add_scene(ch2, "Bridge").unwrap();
        ms.set_meta(
            s3,
            SceneMeta {
                pov: "Joren".to_string(),
                setting: "Old bridge".to_string(),
                ..SceneMeta::default()
            },
        )
        .unwrap();
        let mut lore = LoreBook::new();
        lore.add(EntityKind::Character, "Mara").unwrap();
        let joren = lore.add(EntityKind::Character, "Joren").unwrap();
        lore.set_aliases(joren, &["Jojo".to_string()]).unwrap();
        lore.add(EntityKind::Place, "Mill farm").unwrap();
        lore.add(EntityKind::Place, "Old bridge").unwrap();
        lore.add(EntityKind::Character, "Wren").unwrap();
        let mut texts = BTreeMap::new();
        texts.insert(s1, "Mara waved at @Joren.".to_string());
        texts.insert(s2, "The yard was quiet.".to_string());
        texts.insert(s3, "Joren saw @Wren there.".to_string());
        (ms, lore, texts, ch1, ch2)
    }

    fn names(lore: &LoreBook, graph: &Graph) -> Vec<String> {
        let mut out: Vec<String> = graph
            .nodes
            .iter()
            .map(|id| lore.get(*id).unwrap().name.clone())
            .collect();
        out.sort();
        out
    }

    #[test]
    fn empty_query_returns_input_unchanged() {
        let (ms, lore, texts, _, _) = two_chapters();
        let graph = Graph::build(&ms, &lore, &texts);
        assert_eq!(
            apply(&graph, &ms, &lore, &texts, &GraphQuery::default()),
            graph
        );
    }

    #[test]
    fn scope_chapter_excludes_other_chapters() {
        let (ms, lore, texts, _, ch2) = two_chapters();
        let graph = Graph::build(&ms, &lore, &texts);
        let query = GraphQuery {
            scope: Some(ch2),
            ..GraphQuery::default()
        };
        let filtered = apply(&graph, &ms, &lore, &texts, &query);
        assert_eq!(names(&lore, &filtered), vec!["Joren", "Old bridge", "Wren"]);
        assert_eq!(filtered.edges.len(), 3);
    }

    #[test]
    fn pov_keeps_entities_in_matching_scenes() {
        let (ms, lore, texts, _, _) = two_chapters();
        let graph = Graph::build(&ms, &lore, &texts);
        let query = GraphQuery {
            pov: Some("joren".to_string()),
            ..GraphQuery::default()
        };
        let filtered = apply(&graph, &ms, &lore, &texts, &query);
        assert_eq!(names(&lore, &filtered), vec!["Joren", "Old bridge", "Wren"]);
    }

    #[test]
    fn kinds_and_text_narrow_to_matches() {
        let (ms, lore, texts, _, _) = two_chapters();
        let graph = Graph::build(&ms, &lore, &texts);
        let places = apply(
            &graph,
            &ms,
            &lore,
            &texts,
            &GraphQuery {
                kinds: vec!["place".to_string()],
                ..GraphQuery::default()
            },
        );
        assert_eq!(names(&lore, &places), vec!["Mill farm", "Old bridge"]);
        let alias = apply(
            &graph,
            &ms,
            &lore,
            &texts,
            &GraphQuery {
                text: "jojo".to_string(),
                ..GraphQuery::default()
            },
        );
        assert_eq!(names(&lore, &alias), vec!["Joren"]);
        // Unknown kinds match nothing.
        let none = apply(
            &graph,
            &ms,
            &lore,
            &texts,
            &GraphQuery {
                kinds: vec!["dragon".to_string()],
                ..GraphQuery::default()
            },
        );
        assert!(none.nodes.is_empty());
    }

    #[test]
    fn focus_depth_grows_the_neighborhood() {
        let (ms, lore, texts, _, _) = two_chapters();
        let graph = Graph::build(&ms, &lore, &texts);
        let mill = lore.resolve("Mill farm").unwrap().id;
        let one = apply(
            &graph,
            &ms,
            &lore,
            &texts,
            &GraphQuery {
                focus: Some(mill),
                depth: 1,
                ..GraphQuery::default()
            },
        );
        assert_eq!(names(&lore, &one), vec!["Joren", "Mara", "Mill farm"]);
        let two = apply(
            &graph,
            &ms,
            &lore,
            &texts,
            &GraphQuery {
                focus: Some(mill),
                depth: 2,
                ..GraphQuery::default()
            },
        );
        assert_eq!(
            names(&lore, &two),
            vec!["Joren", "Mara", "Mill farm", "Old bridge", "Wren"]
        );
    }

    #[test]
    fn min_weight_orphans_drop_only_when_asked() {
        let (ms, lore, texts, _, _) = two_chapters();
        let graph = Graph::build(&ms, &lore, &texts);
        let heavy = GraphQuery {
            min_weight: 2,
            ..GraphQuery::default()
        };
        let kept = apply(&graph, &ms, &lore, &texts, &heavy);
        // Bridge and Wren survive with degree 0 until orphans hide.
        assert_eq!(
            names(&lore, &kept),
            vec!["Joren", "Mara", "Mill farm", "Old bridge", "Wren"]
        );
        assert!(kept.edges.iter().all(|edge| edge.weight() >= 2));
        let hidden = apply(
            &graph,
            &ms,
            &lore,
            &texts,
            &GraphQuery {
                hide_orphans: true,
                ..GraphQuery::default()
            },
        );
        // Nothing is orphaned without the weight cut, so nothing hides.
        assert_eq!(hidden.nodes.len(), graph.nodes.len());
        let both = GraphQuery {
            min_weight: 2,
            hide_orphans: true,
            ..GraphQuery::default()
        };
        let cut = apply(&graph, &ms, &lore, &texts, &both);
        assert_eq!(
            names(&lore, &cut),
            vec!["Joren", "Mara", "Mill farm", "Wren"]
        );
    }

    #[test]
    fn max_nodes_keeps_highest_degree() {
        let (ms, lore, texts, _, _) = two_chapters();
        let graph = Graph::build(&ms, &lore, &texts);
        let query = GraphQuery {
            max_nodes: 2,
            ..GraphQuery::default()
        };
        let capped = apply(&graph, &ms, &lore, &texts, &query);
        // Joren links 4; every other node links 2, ties break by id (Mara).
        assert_eq!(names(&lore, &capped), vec!["Joren", "Mara"]);
        assert_eq!(capped.edges.len(), 1);
    }
}
