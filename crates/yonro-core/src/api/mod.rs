//! GUI adapter DTOs (`P0.2`): plain serializable shapes for the web UI.
//!
//! Moved from `yonro-gui/src/main.rs` so a future WASM build reuses the same
//! computation. Frontends render; they never compute narrative facts. Field
//! names and JSON shapes are unchanged from the GUI originals.

use std::collections::BTreeMap;
use std::path::PathBuf;

use super::lore::LoreBook;
use super::manuscript::{Manuscript, NodeId};
use super::project::Project;

// ---------------------------------------------------------------------------
// DTOs (plain shapes for the web UI — core types stay canonical in Rust)
// ---------------------------------------------------------------------------

/// One manuscript tree node with rollups.
#[derive(Debug, Clone, serde::Serialize)]
pub struct OutlineNodeDto {
    pub id: usize,
    pub kind: String,
    pub title: String,
    pub words: usize,
    pub target: usize,
    pub progress: f64,
    pub file: Option<String>,
    /// Scene POV, setting, and story date (empty for structural nodes).
    pub pov: String,
    /// Scene setting, if any.
    pub setting: String,
    /// Scene story date, if any.
    pub story_date: String,
    pub children: Vec<OutlineNodeDto>,
}

/// Project totals for the dashboard cards.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StatsDto {
    pub project: String,
    pub words: usize,
    pub target: usize,
    pub progress: f64,
    pub acts: usize,
    pub chapters: usize,
    pub scenes: usize,
    pub entities: usize,
}

/// One lore entity with POV backlinks.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EntityDto {
    pub id: usize,
    pub kind: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub sheet: String,
    pub pov_scenes: Vec<String>,
}

/// One ranked neighbor in the graph.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct GraphNeighborDto {
    pub id: usize,
    pub name: String,
    pub weight: usize,
}

/// One graph node (a lore entity present in at least one scene).
#[derive(Debug, Clone, serde::Serialize)]
pub struct GraphNodeDto {
    pub id: usize,
    pub label: String,
    pub kind: String,
    pub degree: usize,
    pub neighbors: Vec<GraphNeighborDto>,
}

/// Search hit for entity autocomplete / palette.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct LoreSearchHitDto {
    pub id: usize,
    pub name: String,
    pub kind: String,
    pub matched_alias: Option<String>,
}

/// One scene mention occurrence.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MentionSceneDto {
    pub scene_id: usize,
    pub title: String,
    pub count: usize,
}

/// Reference to a scene where an entity appears as POV.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SceneRefDto {
    pub scene_id: usize,
    pub title: String,
}

/// Detailed entity info for the lore view and inspector.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EntityDetailDto {
    #[serde(flatten)]
    pub entity: EntityDto,
    pub pov_scene_links: Vec<SceneRefDto>,
    pub mention_scenes: Vec<MentionSceneDto>,
}

/// One mention span inside buffer text with UTF-16 code unit offsets.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MentionSpanDto {
    pub start: usize,
    pub end: usize,
    pub entity_id: Option<usize>,
    pub kind: Option<String>,
}

/// One undirected relationship edge.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GraphEdgeDto {
    pub a: usize,
    pub b: usize,
    pub shared: usize,
    pub mentions: usize,
    pub weight: usize,
    pub kinds: Vec<String>,
}

/// Relationship graph.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GraphDto {
    pub nodes: Vec<GraphNodeDto>,
    pub edges: Vec<GraphEdgeDto>,
}

/// One scene placed on the timeline.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TimelineEntryDto {
    pub index: usize,
    pub scene: usize,
    pub title: String,
    pub chapter: String,
    pub pov: String,
    /// Lore kind of the POV (`"character"`, `"place"`, …), if registered.
    pub pov_kind: Option<String>,
    pub setting: String,
    pub story_date: String,
    pub words: usize,
}

/// One continuity observation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ContinuityNoteDto {
    pub message: String,
    pub scenes: Vec<usize>,
    /// Titles parallel to `scenes` (same order) for note-card buttons.
    pub scene_titles: Vec<String>,
}

/// Outline-ordered timeline plus continuity notes.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TimelineDto {
    pub entries: Vec<TimelineEntryDto>,
    pub notes: Vec<ContinuityNoteDto>,
}

/// Scene metadata for the inspector form.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SceneMetaDto {
    pub pov: String,
    pub setting: String,
    pub story_date: String,
    pub story_time: String,
    pub synopsis: String,
    pub target_words: usize,
}

/// One scene with everything the inspector shows.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SceneDetailDto {
    pub id: usize,
    pub title: String,
    pub breadcrumb: Vec<String>,
    pub meta: SceneMetaDto,
    pub file: Option<String>,
    pub words: usize,
    pub target: usize,
}

/// `add_node` result: the fresh tree plus the new node's id.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AddNodeDto {
    pub outline: OutlineNodeDto,
    pub new_id: usize,
}

/// `remove_node` result: the fresh tree plus the kept-on-disk message.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RemoveNodeDto {
    pub outline: OutlineNodeDto,
    pub message: String,
}

fn kind_label(kind: super::manuscript::NodeKind) -> &'static str {
    match kind {
        super::manuscript::NodeKind::Project => "project",
        super::manuscript::NodeKind::Act => "act",
        super::manuscript::NodeKind::Chapter => "chapter",
        super::manuscript::NodeKind::Scene => "scene",
    }
}

fn entity_kind_label(kind: super::lore::EntityKind) -> &'static str {
    match kind {
        super::lore::EntityKind::Character => "character",
        super::lore::EntityKind::Place => "place",
        super::lore::EntityKind::Faction => "faction",
        super::lore::EntityKind::Item => "item",
        super::lore::EntityKind::Lore => "lore",
    }
}

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// Full manuscript tree with per-node rollups.
#[must_use]
pub fn outline_dto(manuscript: &Manuscript) -> OutlineNodeDto {
    fn build(manuscript: &Manuscript, id: usize) -> Option<OutlineNodeDto> {
        let node = manuscript.get(id)?;
        let meta = node.meta.clone().unwrap_or_default();
        Some(OutlineNodeDto {
            id,
            kind: kind_label(node.kind).to_string(),
            title: node.title.clone(),
            words: manuscript.subtree_words(id),
            target: manuscript.subtree_target(id),
            progress: manuscript.progress(id),
            file: node
                .meta
                .as_ref()
                .and_then(|meta| meta.file.clone())
                .map(|path| path.to_string_lossy().to_string()),
            pov: meta.pov.clone(),
            setting: meta.setting.clone(),
            story_date: meta.story_date.clone(),
            children: manuscript
                .children(id)
                .iter()
                .filter_map(|child| build(manuscript, child.id))
                .collect(),
        })
    }
    // Root always exists (fresh manuscripts start with a project node).
    build(manuscript, manuscript.root()).unwrap_or(OutlineNodeDto {
        id: 0,
        kind: "project".to_string(),
        title: manuscript.title().to_string(),
        words: 0,
        target: 0,
        progress: 0.0,
        file: None,
        pov: String::new(),
        setting: String::new(),
        story_date: String::new(),
        children: Vec::new(),
    })
}

/// Project totals for the dashboard cards.
#[must_use]
pub fn stats_dto(manuscript: &Manuscript, lore: &LoreBook) -> StatsDto {
    let root = manuscript.root();
    let count = |kind: super::manuscript::NodeKind| {
        fn walk(
            manuscript: &Manuscript,
            id: usize,
            kind: super::manuscript::NodeKind,
            acc: &mut usize,
        ) {
            if let Some(node) = manuscript.get(id) {
                if node.kind == kind {
                    *acc = acc.saturating_add(1);
                }
                for child in manuscript.children(id) {
                    walk(manuscript, child.id, kind, acc);
                }
            }
        }
        let mut acc = 0;
        walk(manuscript, root, kind, &mut acc);
        acc
    };
    StatsDto {
        project: manuscript.title().to_string(),
        words: manuscript.subtree_words(root),
        target: manuscript.subtree_target(root),
        progress: manuscript.progress(root),
        acts: count(super::manuscript::NodeKind::Act),
        chapters: count(super::manuscript::NodeKind::Chapter),
        scenes: count(super::manuscript::NodeKind::Scene),
        entities: lore.live_count(),
    }
}

/// Every lore entity with POV backlinks, sorted by name.
#[must_use]
pub fn lore_dtos(manuscript: &Manuscript, lore: &LoreBook) -> Vec<EntityDto> {
    let mut pov_scenes: Vec<(String, String)> = Vec::new();
    for act in manuscript.children(manuscript.root()) {
        for chapter in manuscript.children(act.id) {
            for scene in manuscript.children(chapter.id) {
                if let Some(meta) = scene.meta.as_ref() {
                    if !meta.pov.trim().is_empty() {
                        pov_scenes.push((meta.pov.clone(), scene.title.clone()));
                    }
                }
            }
        }
    }
    let mut entities: Vec<EntityDto> = Vec::new();
    // NOTE: `LoreBook` has no public iterator yet; resolve via prefix "".
    for entity in lore.find_by_prefix("") {
        let mut scenes: Vec<String> = pov_scenes
            .iter()
            .filter(|(pov, _)| {
                pov.eq_ignore_ascii_case(&entity.name)
                    || entity
                        .aliases
                        .iter()
                        .any(|alias| pov.eq_ignore_ascii_case(alias))
            })
            .map(|(_, title)| title.clone())
            .collect();
        scenes.sort();
        scenes.dedup();
        entities.push(EntityDto {
            id: entity.id,
            kind: entity_kind_label(entity.kind).to_string(),
            name: entity.name.clone(),
            aliases: entity.aliases.clone(),
            sheet: entity.sheet.clone(),
            pov_scenes: scenes,
        });
    }
    entities.sort_by_key(|a| a.name.to_lowercase());
    entities
}

/// Scene texts for the graph: open buffers win over disk.
#[must_use]
pub fn gather_scene_texts(
    project: &Project,
    open_texts: &BTreeMap<PathBuf, String>,
) -> BTreeMap<NodeId, String> {
    let manuscript = &project.manuscript;
    let mut scene_texts: BTreeMap<NodeId, String> = BTreeMap::new();
    let root = manuscript.root();
    let acts: Vec<usize> = manuscript.children(root).iter().map(|n| n.id).collect();
    for act in acts {
        for chapter in manuscript.children(act).iter().map(|n| n.id) {
            for scene in manuscript.children(chapter) {
                let file = scene.meta.as_ref().and_then(|meta| meta.file.clone());
                if let Some(file) = file {
                    let text = open_texts
                        .get(&file)
                        .cloned()
                        .unwrap_or_else(|| std::fs::read_to_string(&file).unwrap_or_default());
                    scene_texts.insert(scene.id, text);
                }
            }
        }
    }
    scene_texts
}

/// Single entity DTO with POV scenes.
#[must_use]
pub fn entity_dto(manuscript: &Manuscript, lore: &LoreBook, id: usize) -> Option<EntityDto> {
    let entity = lore.get(id)?;
    let mut pov_scenes: Vec<String> = Vec::new();
    for act in manuscript.children(manuscript.root()) {
        for chapter in manuscript.children(act.id) {
            for scene in manuscript.children(chapter.id) {
                if let Some(meta) = scene.meta.as_ref() {
                    let pov = meta.pov.trim();
                    if !pov.is_empty()
                        && (pov.eq_ignore_ascii_case(&entity.name)
                            || entity
                                .aliases
                                .iter()
                                .any(|alias| pov.eq_ignore_ascii_case(alias)))
                    {
                        pov_scenes.push(scene.title.clone());
                    }
                }
            }
        }
    }
    pov_scenes.sort();
    pov_scenes.dedup();
    Some(EntityDto {
        id: entity.id,
        kind: entity_kind_label(entity.kind).to_string(),
        name: entity.name.clone(),
        aliases: entity.aliases.clone(),
        sheet: entity.sheet.clone(),
        pov_scenes,
    })
}

/// Search live entities by prefix (case-insensitive), names before aliases.
#[must_use]
pub fn lore_search(lore: &LoreBook, prefix: &str, limit: usize) -> Vec<LoreSearchHitDto> {
    let needle = prefix.trim().to_lowercase();
    let mut name_hits = Vec::new();
    let mut alias_hits = Vec::new();

    for entity in lore.find_by_prefix("") {
        let name_lower = entity.name.to_lowercase();
        if name_lower.starts_with(&needle) {
            name_hits.push(LoreSearchHitDto {
                id: entity.id,
                name: entity.name.clone(),
                kind: entity_kind_label(entity.kind).to_string(),
                matched_alias: None,
            });
        } else if let Some(matched) = entity
            .aliases
            .iter()
            .find(|alias| alias.to_lowercase().starts_with(&needle))
        {
            alias_hits.push(LoreSearchHitDto {
                id: entity.id,
                name: entity.name.clone(),
                kind: entity_kind_label(entity.kind).to_string(),
                matched_alias: Some(matched.clone()),
            });
        }
    }

    name_hits.sort_by_key(|h| h.name.to_lowercase());
    alias_hits.sort_by_key(|h| h.name.to_lowercase());
    name_hits.extend(alias_hits);
    if limit > 0 {
        name_hits.truncate(limit);
    }
    name_hits
}

/// Detailed entity info with POV backlinks and mention scene counts.
///
/// # Errors
/// `Lore` if entity not found or inactive.
pub fn entity_detail_dto(
    project: &Project,
    open_texts: &BTreeMap<PathBuf, String>,
    id: usize,
) -> Result<EntityDetailDto, super::project::ProjectError> {
    let entity = project
        .lore
        .get(id)
        .ok_or_else(|| super::project::ProjectError::Lore(format!("unknown lore entity {id}")))?;
    let entity_name = entity.name.clone();
    let entity_aliases = entity.aliases.clone();

    let mut pov_scene_links: Vec<SceneRefDto> = Vec::new();
    let mut pov_scenes: Vec<String> = Vec::new();
    let root = project.manuscript.root();
    for act in project.manuscript.children(root) {
        for chapter in project.manuscript.children(act.id) {
            for scene in project.manuscript.children(chapter.id) {
                if let Some(meta) = scene.meta.as_ref() {
                    let pov = meta.pov.trim();
                    if !pov.is_empty()
                        && (pov.eq_ignore_ascii_case(&entity_name)
                            || entity_aliases
                                .iter()
                                .any(|alias| pov.eq_ignore_ascii_case(alias)))
                    {
                        pov_scenes.push(scene.title.clone());
                        pov_scene_links.push(SceneRefDto {
                            scene_id: scene.id,
                            title: scene.title.clone(),
                        });
                    }
                }
            }
        }
    }
    pov_scenes.sort();
    pov_scenes.dedup();
    pov_scene_links.sort_by(|a, b| a.title.cmp(&b.title));
    pov_scene_links.dedup_by(|a, b| a.scene_id == b.scene_id);

    let scene_texts = gather_scene_texts(project, open_texts);
    let mut mention_scenes: Vec<MentionSceneDto> = Vec::new();
    for scene_id in super::graph::Graph::scene_order(&project.manuscript) {
        if let Some(text) = scene_texts.get(&scene_id) {
            let mentions = project.lore.parse_mentions(text);
            let count = mentions.iter().filter(|m| m.entity == Some(id)).count();
            if count > 0 {
                let title = project
                    .manuscript
                    .get(scene_id)
                    .map_or_else(|| format!("scene-{scene_id}"), |n| n.title.clone());
                mention_scenes.push(MentionSceneDto {
                    scene_id,
                    title,
                    count,
                });
            }
        }
    }

    Ok(EntityDetailDto {
        entity: EntityDto {
            id: entity.id,
            kind: entity_kind_label(entity.kind).to_string(),
            name: entity_name,
            aliases: entity_aliases,
            sheet: entity.sheet.clone(),
            pov_scenes,
        },
        pov_scene_links,
        mention_scenes,
    })
}

/// Parse mentions in `text` and return spans with UTF-16 code unit offsets.
#[must_use]
pub fn get_mentions_dto(lore: &LoreBook, text: &str) -> Vec<MentionSpanDto> {
    let raw_mentions = lore.parse_mentions(text);
    if raw_mentions.is_empty() {
        return Vec::new();
    }

    let mut result = Vec::with_capacity(raw_mentions.len());
    let mut mention_idx = 0;
    let mut current_start_u16: Option<usize> = None;
    let mut u16_offset = 0usize;

    for (byte_idx, ch) in text.char_indices() {
        while mention_idx < raw_mentions.len() {
            let mention = &raw_mentions[mention_idx];
            if current_start_u16.is_none() && byte_idx == mention.byte_range.start {
                current_start_u16 = Some(u16_offset);
            }
            if let Some(start_u16) = current_start_u16 {
                if byte_idx == mention.byte_range.end {
                    let kind = mention
                        .entity
                        .and_then(|id| lore.get(id))
                        .map(|e| entity_kind_label(e.kind).to_string());
                    result.push(MentionSpanDto {
                        start: start_u16,
                        end: u16_offset,
                        entity_id: mention.entity,
                        kind,
                    });
                    current_start_u16 = None;
                    mention_idx = mention_idx.saturating_add(1);
                    continue;
                }
            }
            break;
        }
        u16_offset = u16_offset.saturating_add(ch.len_utf16());
    }

    if mention_idx < raw_mentions.len() {
        let mention = &raw_mentions[mention_idx];
        if let Some(start_u16) = current_start_u16 {
            if mention.byte_range.end == text.len() {
                let kind = mention
                    .entity
                    .and_then(|id| lore.get(id))
                    .map(|e| entity_kind_label(e.kind).to_string());
                result.push(MentionSpanDto {
                    start: start_u16,
                    end: u16_offset,
                    entity_id: mention.entity,
                    kind,
                });
            }
        }
    }

    result
}

/// Relationship graph DTO from pre-gathered scene texts.
#[must_use]
pub fn graph_dto(
    manuscript: &Manuscript,
    lore: &LoreBook,
    scene_texts: &BTreeMap<usize, String>,
) -> GraphDto {
    let graph = super::graph::Graph::build(manuscript, lore, scene_texts);
    let nodes = graph
        .nodes
        .iter()
        .filter_map(|id| lore.get(*id))
        .map(|entity| {
            let ranked = graph.neighbors(entity.id);
            let degree = ranked.len();
            let neighbors = ranked
                .into_iter()
                .filter_map(|(nid, weight)| {
                    lore.get(nid).map(|n| GraphNeighborDto {
                        id: nid,
                        name: n.name.clone(),
                        weight,
                    })
                })
                .collect();
            GraphNodeDto {
                id: entity.id,
                label: entity.name.clone(),
                kind: entity_kind_label(entity.kind).to_string(),
                degree,
                neighbors,
            }
        })
        .collect();
    let edges = graph
        .edges
        .iter()
        .map(|edge| GraphEdgeDto {
            a: edge.a,
            b: edge.b,
            shared: edge.shared_scenes,
            mentions: edge.mention_scenes,
            weight: edge.weight(),
            kinds: edge
                .kinds()
                .iter()
                .map(|kind| {
                    match kind {
                        super::graph::EdgeKind::Cooccurrence => "shared scene",
                        super::graph::EdgeKind::Mention => "mention",
                    }
                    .to_string()
                })
                .collect(),
        })
        .collect();
    GraphDto { nodes, edges }
}

/// Outline-ordered timeline plus continuity notes.
///
/// Scene titles for each note are resolved here (server-side) so the UI
/// only renders buttons; POV kinds come from the lore book so chip colors
/// stay a render concern driven by core facts.
#[must_use]
pub fn timeline_dto(manuscript: &Manuscript, lore: &LoreBook) -> TimelineDto {
    let timeline = super::timeline::Timeline::build(manuscript);
    let title_of = |id: usize| {
        manuscript
            .get(id)
            .map_or_else(|| format!("scene-{id}"), |node| node.title.clone())
    };
    TimelineDto {
        entries: timeline
            .entries
            .iter()
            .map(|entry| TimelineEntryDto {
                index: entry.index,
                scene: entry.scene,
                title: entry.title.clone(),
                chapter: entry.chapter.clone(),
                pov: entry.pov.clone(),
                pov_kind: lore
                    .resolve(&entry.pov)
                    .map(|entity| entity_kind_label(entity.kind).to_string()),
                setting: entry.setting.clone(),
                story_date: entry.story_date.clone(),
                words: entry.words,
            })
            .collect(),
        notes: timeline
            .continuity_notes()
            .iter()
            .map(|note| ContinuityNoteDto {
                message: note.message.clone(),
                scenes: note.scenes.clone(),
                scene_titles: note.scenes.iter().map(|id| title_of(*id)).collect(),
            })
            .collect(),
    }
}

/// Scene detail for the inspector: breadcrumb, meta, file, live counts.
///
/// # Errors
/// `UnknownNode` for a bad id, `NotAScene` for structural nodes.
pub fn scene_detail_dto(
    project: &Project,
    id: NodeId,
) -> Result<SceneDetailDto, super::project::ProjectError> {
    use super::project::ProjectError;
    let node = project
        .manuscript
        .get(id)
        .ok_or(ProjectError::UnknownNode(id))?;
    if node.kind != super::manuscript::NodeKind::Scene {
        return Err(ProjectError::NotAScene(id));
    }
    let meta = node.meta.clone().unwrap_or_default();
    Ok(SceneDetailDto {
        id,
        title: node.title.clone(),
        breadcrumb: project.manuscript.breadcrumb(id).unwrap_or_default(),
        meta: SceneMetaDto {
            pov: meta.pov.clone(),
            setting: meta.setting.clone(),
            story_date: meta.story_date.clone(),
            story_time: meta.story_time.clone(),
            synopsis: meta.synopsis.clone(),
            target_words: meta.target_words,
        },
        file: meta.file.map(|path| path.to_string_lossy().to_string()),
        words: project.manuscript.subtree_words(id),
        target: project.manuscript.subtree_target(id),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EntityKind, SceneMeta};

    fn seed_story() -> (Manuscript, LoreBook) {
        let mut ms = Manuscript::new("Probe");
        let act = ms.add_act("Act I").unwrap();
        let ch = ms.add_chapter(act, "Chapter 1").unwrap();
        let sc = ms.add_scene(ch, "The gate").unwrap();
        ms.set_meta(
            sc,
            SceneMeta {
                pov: "Mara".to_string(),
                target_words: 1000,
                current_words: 250,
                ..SceneMeta::default()
            },
        )
        .unwrap();
        let mut lore = LoreBook::new();
        lore.add(EntityKind::Character, "Mara").unwrap();
        (ms, lore)
    }

    #[test]
    fn outline_dto_mirrors_tree_with_rollups() {
        let (ms, _) = seed_story();
        let dto = outline_dto(&ms);
        assert_eq!(dto.title, "Probe");
        assert_eq!(dto.words, 250);
        assert_eq!(dto.target, 1000);
        assert_eq!(dto.children.len(), 1);
        let scene = &dto.children[0].children[0].children[0];
        assert_eq!(scene.title, "The gate");
        assert_eq!(scene.kind, "scene");
        assert_eq!(scene.pov, "Mara");
        assert_eq!(scene.setting, "");
        assert_eq!(dto.children[0].pov, "");
    }

    #[test]
    fn stats_dto_counts_levels() {
        let (ms, lore) = seed_story();
        let stats = stats_dto(&ms, &lore);
        assert_eq!(stats.project, "Probe");
        assert_eq!((stats.acts, stats.chapters, stats.scenes), (1, 1, 1));
        assert_eq!(stats.entities, 1);
        assert!((stats.progress - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn lore_dtos_carry_pov_backlinks_sorted() {
        let (ms, lore) = seed_story();
        let dtos = lore_dtos(&ms, &lore);
        assert_eq!(dtos.len(), 1);
        assert_eq!(dtos[0].name, "Mara");
        assert_eq!(dtos[0].pov_scenes, vec!["The gate".to_string()]);
    }

    #[test]
    fn graph_dto_links_mention_edges() {
        let (ms, mut lore) = seed_story();
        lore.add(EntityKind::Character, "Joren").unwrap();
        let scene = ms.children(ms.children(ms.root())[0].id)[0].id;
        let scene = ms.children(scene)[0].id;
        let mut texts = BTreeMap::new();
        texts.insert(scene, "Mara waved at @Joren.".to_string());
        let dto = graph_dto(&ms, &lore, &texts);
        assert_eq!(dto.nodes.len(), 2);
        assert_eq!(dto.edges.len(), 1);
        assert_eq!(dto.edges[0].mentions, 1);
        assert!(dto.edges[0].kinds.iter().any(|k| k == "mention"));
    }

    #[test]
    fn timeline_dto_orders_entries_and_notes() {
        let (ms, lore) = seed_story();
        let dto = timeline_dto(&ms, &lore);
        assert_eq!(dto.entries.len(), 1);
        assert_eq!(dto.entries[0].title, "The gate");
        assert_eq!(dto.entries[0].pov_kind.as_deref(), Some("character"));
        // Seed scene has a POV but no setting → exactly one note.
        assert_eq!(dto.notes.len(), 1);
        assert!(dto.notes[0].message.contains("no setting"));
        assert_eq!(dto.notes[0].scenes.len(), 1);
        assert_eq!(dto.notes[0].scene_titles, vec!["The gate".to_string()]);
    }

    #[test]
    fn timeline_dto_resolves_note_titles_and_unknown_pov_kind() {
        let mut ms = Manuscript::new("Probe");
        let act = ms.add_act("Act I").unwrap();
        let ch = ms.add_chapter(act, "Chapter 1").unwrap();
        for title in ["Gate", "River"] {
            let sc = ms.add_scene(ch, title).unwrap();
            ms.set_meta(
                sc,
                SceneMeta {
                    pov: "Mara".to_string(),
                    setting: if title == "Gate" {
                        "Mill farm".to_string()
                    } else {
                        "Old bridge".to_string()
                    },
                    ..SceneMeta::default()
                },
            )
            .unwrap();
        }
        // "Mara" is unregistered here → pov_kind is None.
        let lore = LoreBook::new();
        let dto = timeline_dto(&ms, &lore);
        assert_eq!(dto.entries.len(), 2);
        assert_eq!(dto.entries[0].pov_kind, None);
        let travel = dto
            .notes
            .iter()
            .find(|n| n.message.contains("travels"))
            .unwrap();
        assert_eq!(travel.scenes.len(), 2);
        assert_eq!(
            travel.scene_titles,
            vec!["Gate".to_string(), "River".to_string()]
        );
    }

    #[test]
    fn gather_scene_texts_prefers_open_buffers() {
        use crate::manuscript::SceneMeta;
        let dir = std::env::temp_dir().join(format!("yonro-api-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut project = Project {
            root: dir.clone(),
            manuscript: Manuscript::new("Probe"),
            lore: LoreBook::new(),
            warnings: Vec::new(),
        };
        let act = project.manuscript.add_act("A").unwrap();
        let ch = project.manuscript.add_chapter(act, "C").unwrap();
        let sc = project.manuscript.add_scene(ch, "S").unwrap();
        let path = dir.join("scene-x.md");
        std::fs::write(&path, "disk text").unwrap();
        project
            .manuscript
            .set_meta(
                sc,
                SceneMeta {
                    file: Some(path.clone()),
                    ..SceneMeta::default()
                },
            )
            .unwrap();
        // Disk fallback.
        let empty: BTreeMap<PathBuf, String> = BTreeMap::new();
        let texts = gather_scene_texts(&project, &empty);
        assert_eq!(texts.get(&sc).map(String::as_str), Some("disk text"));
        // Open buffer wins.
        let mut open = BTreeMap::new();
        open.insert(path.clone(), "draft text".to_string());
        let texts = gather_scene_texts(&project, &open);
        assert_eq!(texts.get(&sc).map(String::as_str), Some("draft text"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn detail_project() -> (Project, usize) {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("yonro-api-detail-{}-{id}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (ms, lore) = seed_story();
        let scene = ms.children(ms.children(ms.root())[0].id)[0].id;
        let scene = ms.children(scene)[0].id;
        let project = Project {
            root: dir,
            manuscript: ms,
            lore,
            warnings: Vec::new(),
        };
        (project, scene)
    }

    #[test]
    fn scene_detail_carries_breadcrumb_meta_and_counts() {
        let (project, scene) = detail_project();
        let dto = scene_detail_dto(&project, scene).unwrap();
        assert_eq!(dto.id, scene);
        assert_eq!(dto.title, "The gate");
        assert_eq!(
            dto.breadcrumb,
            vec!["Probe", "Act I", "Chapter 1", "The gate"]
        );
        assert_eq!(dto.meta.pov, "Mara");
        assert_eq!(dto.meta.target_words, 1000);
        assert_eq!(dto.words, 250);
        assert_eq!(dto.target, 1000);
        assert_eq!(dto.file, None);
        let _ = std::fs::remove_dir_all(&project.root);
    }

    #[test]
    fn scene_detail_rejects_structural_and_unknown_ids() {
        use crate::ProjectError;
        let (project, _) = detail_project();
        let root = project.manuscript.root();
        assert_eq!(
            scene_detail_dto(&project, root).unwrap_err(),
            ProjectError::NotAScene(root)
        );
        assert_eq!(
            scene_detail_dto(&project, 999).unwrap_err(),
            ProjectError::UnknownNode(999)
        );
        let _ = std::fs::remove_dir_all(&project.root);
    }

    #[test]
    fn lore_search_ranks_names_before_aliases_and_limits() {
        let mut lore = LoreBook::new();
        let mara = lore.add(EntityKind::Character, "Mara Stone").unwrap();
        lore.add_alias(mara, "Red Wolf").unwrap();
        let red_keep = lore.add(EntityKind::Place, "Red Keep").unwrap();
        let _ = red_keep;

        // Prefix "red": "Red Keep" is a name match, "Mara Stone" is an alias match
        let hits = lore_search(&lore, "red", 10);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].name, "Red Keep");
        assert_eq!(hits[0].matched_alias, None);
        assert_eq!(hits[1].name, "Mara Stone");
        assert_eq!(hits[1].matched_alias, Some("Red Wolf".to_string()));

        // Limit works
        let limited = lore_search(&lore, "red", 1);
        assert_eq!(limited.len(), 1);
        assert_eq!(limited[0].name, "Red Keep");
    }

    #[test]
    fn entity_detail_carries_pov_and_mention_counts() {
        let (project, scene_id) = detail_project();
        let mara_id = project.lore.resolve("Mara").unwrap().id;

        let mut open_texts = BTreeMap::new();
        // Add mention in an open buffer
        let dummy_path = project.root.join("scene-gate.md");
        let mut project = project;
        project
            .manuscript
            .set_meta(
                scene_id,
                SceneMeta {
                    pov: "Mara".to_string(),
                    file: Some(dummy_path.clone()),
                    ..SceneMeta::default()
                },
            )
            .unwrap();
        open_texts.insert(
            dummy_path,
            "Met @Mara Stone at noon. Saw @Mara again.".to_string(),
        );

        let detail = entity_detail_dto(&project, &open_texts, mara_id).unwrap();
        assert_eq!(detail.entity.name, "Mara");
        assert_eq!(detail.entity.pov_scenes, vec!["The gate".to_string()]);
        assert_eq!(detail.pov_scene_links.len(), 1);
        assert_eq!(detail.pov_scene_links[0].scene_id, scene_id);
        assert_eq!(detail.mention_scenes.len(), 1);
        assert_eq!(detail.mention_scenes[0].scene_id, scene_id);
        assert_eq!(detail.mention_scenes[0].count, 2);

        let _ = std::fs::remove_dir_all(&project.root);
    }

    #[test]
    fn graph_dto_includes_degree_and_ranked_neighbors() {
        let (ms, mut lore) = seed_story();
        let joren = lore.add(EntityKind::Character, "Joren").unwrap();
        let mara = lore.resolve("Mara").unwrap().id;
        let scene = ms.children(ms.children(ms.root())[0].id)[0].id;
        let scene = ms.children(scene)[0].id;
        let mut texts = BTreeMap::new();
        texts.insert(scene, "Mara waved at @Joren.".to_string());
        let dto = graph_dto(&ms, &lore, &texts);

        let mara_node = dto.nodes.iter().find(|n| n.id == mara).unwrap();
        assert_eq!(mara_node.degree, 1);
        assert_eq!(mara_node.neighbors.len(), 1);
        assert_eq!(mara_node.neighbors[0].id, joren);
        assert_eq!(mara_node.neighbors[0].name, "Joren");
    }

    #[test]
    fn get_mentions_dto_utf16_offsets_with_emoji_and_umlauts() {
        let mut lore = LoreBook::new();
        let mara = lore.add(EntityKind::Character, "Mara").unwrap();

        // 1. Basic ASCII
        let text1 = "Hello @Mara world";
        let spans1 = get_mentions_dto(&lore, text1);
        assert_eq!(spans1.len(), 1);
        assert_eq!(spans1[0].start, 6);
        assert_eq!(spans1[0].end, 11);
        assert_eq!(spans1[0].entity_id, Some(mara));
        assert_eq!(spans1[0].kind.as_deref(), Some("character"));
        // Check slice matches exactly
        let u16_vec: Vec<u16> = text1.encode_utf16().collect();
        let slice = String::from_utf16(&u16_vec[spans1[0].start..spans1[0].end]).unwrap();
        assert_eq!(slice, "@Mara");

        // 2. Emoji (surrogate pairs in UTF-16, 4 bytes in UTF-8)
        let text2 = "👋 Schön @Mara 👋!";
        let spans2 = get_mentions_dto(&lore, text2);
        assert_eq!(spans2.len(), 1);
        let u16_vec2: Vec<u16> = text2.encode_utf16().collect();
        let slice2 = String::from_utf16(&u16_vec2[spans2[0].start..spans2[0].end]).unwrap();
        assert_eq!(slice2, "@Mara");

        // 3. Umlauts inside and before mention
        let joren = lore.add(EntityKind::Character, "Jörën").unwrap();
        let text3 = "Äpfel Über @Jörën. Ende";
        let spans3 = get_mentions_dto(&lore, text3);
        assert_eq!(spans3.len(), 1);
        assert_eq!(spans3[0].entity_id, Some(joren));
        let u16_vec3: Vec<u16> = text3.encode_utf16().collect();
        let slice3 = String::from_utf16(&u16_vec3[spans3[0].start..spans3[0].end]).unwrap();
        assert_eq!(slice3, "@Jörën");

        // 4. Mention extending to the very end of text
        let text4 = "Hi @Mara";
        let spans4 = get_mentions_dto(&lore, text4);
        assert_eq!(spans4.len(), 1);
        let u16_vec4: Vec<u16> = text4.encode_utf16().collect();
        let slice4 = String::from_utf16(&u16_vec4[spans4[0].start..spans4[0].end]).unwrap();
        assert_eq!(slice4, "@Mara");
    }
}
