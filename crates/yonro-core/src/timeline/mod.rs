//! Narrative timeline (`PLAN.md Phase 5.3`, core half): outline-ordered
//! scene sequence plus travel/continuity checks.
//!
//! Story dates stay free-form (`SceneMeta::story_date`, per product
//! decision), so ordering follows the outline and continuity flags what the
//! data actually supports: a POV character in different settings across
//! adjacent scenes (did they travel off-page?), and scenes missing POV or
//! setting metadata.

use super::manuscript::{Manuscript, NodeId};

/// One scene placed on the story timeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineEntry {
    /// Outline position (0-based story order).
    pub index: usize,
    pub scene: NodeId,
    pub title: String,
    pub chapter: String,
    pub pov: String,
    pub setting: String,
    pub story_date: String,
    pub words: usize,
}

/// A continuity observation worth surfacing (soft warning, never an error).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuityNote {
    /// Human-readable description.
    pub message: String,
    /// Scenes involved (earlier first).
    pub scenes: Vec<NodeId>,
}

/// Outline-ordered scenes with chapter context.
#[derive(Debug, Clone, Default)]
pub struct Timeline {
    pub entries: Vec<TimelineEntry>,
}

impl Timeline {
    /// Build from the manuscript (outline order = story order).
    #[must_use]
    pub fn build(manuscript: &Manuscript) -> Self {
        let mut entries = Vec::new();
        let root = manuscript.root();
        let acts: Vec<usize> = manuscript.children(root).iter().map(|n| n.id).collect();
        for act in acts {
            let chapters: Vec<usize> = manuscript.children(act).iter().map(|n| n.id).collect();
            for chapter in chapters {
                let chapter_title = manuscript
                    .get(chapter)
                    .map_or(String::new(), |n| n.title.clone());
                for scene in manuscript.children(chapter) {
                    let (title, pov, setting, date, words) = match manuscript.get(scene.id) {
                        Some(node) => {
                            let meta = node.meta.as_ref();
                            (
                                node.title.clone(),
                                meta.map_or(String::new(), |m| m.pov.clone()),
                                meta.map_or(String::new(), |m| m.setting.clone()),
                                meta.map_or(String::new(), |m| m.story_date.clone()),
                                meta.map_or(0, |m| m.current_words),
                            )
                        }
                        None => continue,
                    };
                    entries.push(TimelineEntry {
                        index: entries.len(),
                        scene: scene.id,
                        title,
                        chapter: chapter_title.clone(),
                        pov,
                        setting,
                        story_date: date,
                        words,
                    });
                }
            }
        }
        Self { entries }
    }

    /// Continuity notes: POV setting jumps between adjacent scenes, plus
    /// scenes missing POV/setting metadata.
    #[must_use]
    pub fn continuity_notes(&self) -> Vec<ContinuityNote> {
        let mut notes = Vec::new();
        for entry in &self.entries {
            if entry.pov.trim().is_empty() {
                notes.push(ContinuityNote {
                    message: format!("“{}” has no POV character", entry.title),
                    scenes: vec![entry.scene],
                });
            }
            if entry.setting.trim().is_empty() {
                notes.push(ContinuityNote {
                    message: format!("“{}” has no setting", entry.title),
                    scenes: vec![entry.scene],
                });
            }
        }
        for pair in self.entries.windows(2) {
            let (prev, next) = (&pair[0], &pair[1]);
            if prev.pov.trim().is_empty() || next.pov.trim().is_empty() {
                continue;
            }
            if prev.pov.eq_ignore_ascii_case(&next.pov)
                && !prev.setting.trim().is_empty()
                && !next.setting.trim().is_empty()
                && !prev.setting.eq_ignore_ascii_case(&next.setting)
            {
                notes.push(ContinuityNote {
                    message: format!(
                        "{} travels {} → {} between “{}” and “{}” — on-page?",
                        prev.pov, prev.setting, next.setting, prev.title, next.title
                    ),
                    scenes: vec![prev.scene, next.scene],
                });
            }
        }
        notes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manuscript::SceneMeta;

    fn story() -> Manuscript {
        let mut ms = Manuscript::new("T");
        let act = ms.add_act("A").unwrap();
        let ch = ms.add_chapter(act, "C").unwrap();
        let s1 = ms.add_scene(ch, "Gate").unwrap();
        ms.set_meta(
            s1,
            SceneMeta {
                pov: "Mara".to_string(),
                setting: "Mill farm".to_string(),
                story_date: "Day 1".to_string(),
                current_words: 100,
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
                story_date: "Day 2".to_string(),
                current_words: 200,
                ..SceneMeta::default()
            },
        )
        .unwrap();
        ms
    }

    #[test]
    fn entries_follow_outline_with_metadata() {
        let timeline = Timeline::build(&story());
        assert_eq!(timeline.entries.len(), 2);
        assert_eq!(timeline.entries[0].title, "Gate");
        assert_eq!(timeline.entries[1].index, 1);
        assert_eq!(timeline.entries[1].chapter, "C");
        assert_eq!(timeline.entries[1].story_date, "Day 2");
        assert_eq!(timeline.entries[1].words, 200);
    }

    #[test]
    fn pov_setting_jump_flags_travel() {
        let notes = Timeline::build(&story()).continuity_notes();
        assert_eq!(notes.len(), 1);
        assert!(notes[0].message.contains("travels"));
        assert!(notes[0].message.contains("Mill farm"));
        assert_eq!(notes[0].scenes.len(), 2);
    }

    #[test]
    fn missing_metadata_is_flagged() {
        let mut ms = Manuscript::new("T");
        let act = ms.add_act("A").unwrap();
        let ch = ms.add_chapter(act, "C").unwrap();
        ms.add_scene(ch, "Draft").unwrap();
        let notes = Timeline::build(&ms).continuity_notes();
        assert_eq!(notes.len(), 2); // no POV + no setting
    }

    #[test]
    fn same_setting_no_note() {
        let mut ms = Manuscript::new("T");
        let act = ms.add_act("A").unwrap();
        let ch = ms.add_chapter(act, "C").unwrap();
        for title in ["One", "Two"] {
            let sc = ms.add_scene(ch, title).unwrap();
            ms.set_meta(
                sc,
                SceneMeta {
                    pov: "Mara".to_string(),
                    setting: "Mill farm".to_string(),
                    ..SceneMeta::default()
                },
            )
            .unwrap();
        }
        assert!(Timeline::build(&ms).continuity_notes().is_empty());
    }
}
