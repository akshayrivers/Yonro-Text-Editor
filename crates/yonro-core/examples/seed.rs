//! Seed tool (`P0.6`): build a sample novel workspace for manual QA.
//!
//! ```sh
//! cargo run -p yonro-core --example seed -- <dir> [--title "Sample Novel"] [--entities 250] [--force]
//! ```
//!
//! Builds 3 acts / 9 chapters / 36 scenes with POV, settings, dates,
//! targets, and scene files whose prose contains `@mentions`: 8 hand-made
//! characters plus generated entities (default 250 total, for the >200-node
//! graph check). Uses the `Project` API only. Refuses a non-empty `<dir>`
//! unless `--force` is given.

use std::path::PathBuf;

use yonro_core::{Buffer, EntityKind, Project, SceneMeta};

const HANDMADE: [(&str, &str); 8] = [
    (
        "Mara",
        "Protagonist. Gatekeeper's daughter, restless and kind.",
    ),
    (
        "Joren",
        "Mara's brother. A cartographer who maps what he fears.",
    ),
    ("Sella", "Harbor merchant. Keeps every promise twice."),
    ("Bram", "Retired soldier. Tends the pine chapel garden."),
    ("Odell", "Salt tower keeper. Counts storms, forgets names."),
    (
        "Wren",
        "Fox hollow runner. Carries letters she never reads.",
    ),
    (
        "Halloran",
        "Fading faction lord. Collects other people's maps.",
    ),
    ("Isolde", "Chapel archivist. Remembers every date but one."),
];

const SETTINGS: [&str; 6] = [
    "Mill farm",
    "Old bridge",
    "Harbor market",
    "Pine chapel",
    "Salt tower",
    "Fox hollow",
];

const TIMES: [&str; 4] = ["dawn", "midday", "dusk", "night"];

fn usage() -> String {
    "usage: seed <dir> [--title \"Sample Novel\"] [--entities 250] [--force]".to_string()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut dir: Option<PathBuf> = None;
    let mut title = String::from("Sample Novel");
    let mut total_entities: usize = 250;
    let mut force = false;

    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if arg == "--title" {
            index = index.saturating_add(1);
            match args.get(index) {
                Some(value) => title = value.clone(),
                None => {
                    eprintln!("seed: --title needs a value\n{}", usage());
                    std::process::exit(1);
                }
            }
        } else if let Some(value) = arg.strip_prefix("--title=") {
            title = value.to_string();
        } else if arg == "--entities" {
            index = index.saturating_add(1);
            match args.get(index) {
                Some(value) => match value.parse::<usize>() {
                    Ok(count) => total_entities = count,
                    Err(_) => {
                        eprintln!("seed: --entities needs a number\n{}", usage());
                        std::process::exit(1);
                    }
                },
                None => {
                    eprintln!("seed: --entities needs a number\n{}", usage());
                    std::process::exit(1);
                }
            }
        } else if let Some(value) = arg.strip_prefix("--entities=") {
            match value.parse::<usize>() {
                Ok(count) => total_entities = count,
                Err(_) => {
                    eprintln!("seed: --entities needs a number\n{}", usage());
                    std::process::exit(1);
                }
            }
        } else if arg == "--force" {
            force = true;
        } else if arg == "--help" || arg == "-h" {
            eprintln!("{}", usage());
            return Ok(());
        } else if arg.starts_with("--") {
            eprintln!("seed: unknown flag {arg}\n{}", usage());
            std::process::exit(1);
        } else if dir.is_none() {
            dir = Some(PathBuf::from(arg));
        } else {
            eprintln!("seed: only one <dir> expected\n{}", usage());
            std::process::exit(1);
        }
        index = index.saturating_add(1);
    }

    let dir = match dir {
        Some(dir) => dir,
        None => {
            eprintln!("seed: missing <dir>\n{}", usage());
            std::process::exit(1);
        }
    };

    if total_entities < HANDMADE.len() {
        total_entities = HANDMADE.len();
    }

    if dir.exists() {
        let non_empty = std::fs::read_dir(&dir)
            .map(|mut entries| entries.any(|entry| entry.is_ok()))
            .unwrap_or(false);
        if non_empty && !force {
            eprintln!(
                "seed: {} is not empty (pass --force to overwrite)",
                dir.display()
            );
            std::process::exit(1);
        }
    }
    std::fs::create_dir_all(&dir)?;

    let mut project = Project {
        root: dir.clone(),
        manuscript: yonro_core::Manuscript::new(&title),
        lore: yonro_core::LoreBook::new(),
        warnings: Vec::new(),
    };

    for (name, sheet) in HANDMADE {
        let id = project.lore.add(EntityKind::Character, name)?;
        project.lore.set_sheet(id, sheet)?;
    }

    let generated = total_entities.saturating_sub(HANDMADE.len());
    for i in 0..generated {
        let name = format!("Entity{i:03}");
        let kind = match i % 5 {
            0 => EntityKind::Character,
            1 => EntityKind::Place,
            2 => EntityKind::Faction,
            3 => EntityKind::Item,
            _ => EntityKind::Lore,
        };
        let _ = project.lore.add(kind, &name);
    }
    let generated_names: Vec<String> = (0..generated).map(|i| format!("Entity{i:03}")).collect();

    let mut scene_index: usize = 0;
    for act_number in 0_usize..3_usize {
        let act = project
            .manuscript
            .add_act(&format!("Act {}", act_number.saturating_add(1)))?;
        for chapter_number in 0_usize..3_usize {
            let chapter_label = act_number
                .saturating_mul(3)
                .saturating_add(chapter_number)
                .saturating_add(1);
            let chapter = project
                .manuscript
                .add_chapter(act, &format!("Chapter {chapter_label}"))?;
            for _ in 0..4 {
                let handmade_index = scene_index % HANDMADE.len();
                let pov = HANDMADE[handmade_index].0.to_string();
                let setting = SETTINGS[scene_index % SETTINGS.len()].to_string();
                let story_date = format!("Day {}", scene_index.saturating_add(1));
                let story_time = TIMES[scene_index % TIMES.len()].to_string();
                let target = 800_usize.saturating_add((scene_index % 4).saturating_mul(200));
                let scene_number = scene_index.saturating_add(1);
                let scene = project
                    .manuscript
                    .add_scene(chapter, &format!("Scene {scene_number}"))?;
                project.manuscript.set_meta(
                    scene,
                    SceneMeta {
                        pov: pov.clone(),
                        setting: setting.clone(),
                        story_date: story_date.clone(),
                        story_time: story_time.clone(),
                        synopsis: format!("{pov} at {setting} ({story_date})."),
                        target_words: target,
                        current_words: 0,
                        file: None,
                    },
                )?;
                let path = project.scene_file(scene)?;
                let prose = scene_prose(
                    &pov,
                    &setting,
                    &story_date,
                    &story_time,
                    scene_index,
                    &generated_names,
                );
                std::fs::write(&path, &prose)?;
                let mut buffer = Buffer::default();
                buffer.set_text(&prose);
                let (words, _, _) = buffer.word_count_stats();
                let _ = project.sync_scene_words(&path, words);
                scene_index = scene_index.saturating_add(1);
            }
        }
    }

    project.save()?;
    eprintln!(
        "seed: {} — 3 acts, 9 chapters, 36 scenes, {} entities at {}",
        title,
        project.lore.live_count(),
        dir.display()
    );
    Ok(())
}

fn scene_prose(
    pov: &str,
    setting: &str,
    date: &str,
    time: &str,
    scene_index: usize,
    generated: &[String],
) -> String {
    // Clustered mentions: the cast is dealt into groups of ~8 and each
    // scene hosts one group, so members appear in 1-3 scenes together
    // (a real neighborhood) instead of scattered singles. The only hero
    // links are the POV and one handmade mention.
    let hand_a = HANDMADE[(scene_index.saturating_add(1)) % HANDMADE.len()].0;
    let members = cluster_members(scene_index, generated, hand_a);
    let list = members
        .iter()
        .map(|name| format!("@{name}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{pov} left {setting} at {time} on {date}, thinking of @{hand_a}.\n\
        The road was empty except for {list}, who argued about the {date} market.\n\
        \"Tell @{hand_a} I was here,\" {pov} said, \"and keep it quiet.\"\n\
        By nightfall the {date} entry was done.\n",
    )
}

/// Members of scene `scene_index`'s cluster (up to 8 names).
fn cluster_members(scene_index: usize, generated: &[String], fallback: &str) -> Vec<String> {
    if generated.is_empty() {
        return vec![fallback.to_string()];
    }
    const SIZE: usize = 8;
    let clusters = generated.len().div_ceil(SIZE).max(1);
    let which = scene_index % clusters;
    let start = which.saturating_mul(SIZE);
    let end = start.saturating_add(SIZE).min(generated.len());
    if start >= end {
        return vec![fallback.to_string()];
    }
    generated[start..end].to_vec()
}
