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

/// One graph node (a lore entity present in at least one scene).
#[derive(Debug, Clone, serde::Serialize)]
pub struct GraphNodeDto {
    pub id: usize,
    pub label: String,
    pub kind: String,
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
    pub setting: String,
    pub story_date: String,
    pub words: usize,
}

/// One continuity observation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ContinuityNoteDto {
    pub message: String,
    pub scenes: Vec<usize>,
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
        .map(|entity| GraphNodeDto {
            id: entity.id,
            label: entity.name.clone(),
            kind: entity_kind_label(entity.kind).to_string(),
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
#[must_use]
pub fn timeline_dto(manuscript: &Manuscript) -> TimelineDto {
    let timeline = super::timeline::Timeline::build(manuscript);
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
        let (ms, _) = seed_story();
        let dto = timeline_dto(&ms);
        assert_eq!(dto.entries.len(), 1);
        assert_eq!(dto.entries[0].title, "The gate");
        // Seed scene has a POV but no setting → exactly one note.
        assert_eq!(dto.notes.len(), 1);
        assert!(dto.notes[0].message.contains("no setting"));
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
        let dir = std::env::temp_dir().join(format!("yonro-api-detail-{}", std::process::id()));
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
}
