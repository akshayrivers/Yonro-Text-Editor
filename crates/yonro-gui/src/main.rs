// `yonro-gui`: Tauri v2 desktop shell over `yonro-core` (`PLAN.md Phase 5.1`).
//
// Architecture contract (keeps a future pure-web build possible): ALL
// narrative reads go through the `#[tauri::command]`s below — the single
// adapter seam. The frontend (`ui/app.js`) renders; it never computes.
// A browser port would reimplement exactly these commands over WASM.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use serde::Serialize;
use tauri::Manager;
use yonro_core::{api, Buffer, Project, UndoStack};

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

struct AppState {
    project: Mutex<Project>,
    buffers: Mutex<HashMap<usize, GuiBuffer>>,
    next_buffer: Mutex<usize>,
}

/// One open document: rope buffer plus coalescing whole-text undo.
struct GuiBuffer {
    path: Option<PathBuf>,
    buffer: Buffer,
    history: UndoStack,
}

impl GuiBuffer {
    fn stats(&self) -> TextStats {
        let (words, graphemes, lines) = self.buffer.word_count_stats();
        TextStats {
            words,
            chars: graphemes,
            lines,
            reading_min: if words == 0 { 0 } else { words.div_ceil(200) },
            dirty: self.buffer.is_dirty(),
        }
    }
}

impl AppState {
    fn load(workspace_dir: PathBuf) -> Self {
        let project = Project::load(&workspace_dir);
        Self {
            project: Mutex::new(project),
            buffers: Mutex::new(HashMap::new()),
            next_buffer: Mutex::new(0),
        }
    }

    fn alloc_buffer(&self, buf: GuiBuffer) -> usize {
        let mut next = self.next_buffer.lock().unwrap_or_else(|e| e.into_inner());
        let id = *next;
        *next = next.saturating_add(1);
        self.buffers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, buf);
        id
    }

    fn workspace_root(&self) -> PathBuf {
        self.project
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .root
            .clone()
    }
}

fn canonical_key(path: &PathBuf) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.clone())
}

// ---------------------------------------------------------------------------
// DTOs (plain shapes for the web UI — core types stay canonical in Rust)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
struct TextStats {
    words: usize,
    chars: usize,
    lines: usize,
    reading_min: usize,
    dirty: bool,
}

#[derive(Debug, Clone, Serialize)]
struct OpenedDto {
    buffer_id: usize,
    path: Option<String>,
    text: String,
    stats: TextStats,
}

// ---------------------------------------------------------------------------
// Commands (the adapter seam — see module docs; thin wrappers over core)
// ---------------------------------------------------------------------------

/// Full manuscript tree with per-node rollups.
#[tauri::command]
fn get_outline(state: tauri::State<'_, AppState>) -> api::OutlineNodeDto {
    let project = state.project.lock().unwrap_or_else(|e| e.into_inner());
    api::outline_dto(&project.manuscript)
}

/// Project totals for the dashboard cards.
#[tauri::command]
fn get_stats(state: tauri::State<'_, AppState>) -> api::StatsDto {
    let project = state.project.lock().unwrap_or_else(|e| e.into_inner());
    api::stats_dto(&project.manuscript, &project.lore)
}

/// Every lore entity with POV backlinks.
#[tauri::command]
fn get_lore(state: tauri::State<'_, AppState>) -> Vec<api::EntityDto> {
    let project = state.project.lock().unwrap_or_else(|e| e.into_inner());
    api::lore_dtos(&project.manuscript, &project.lore)
}

/// Workspace directory the GUI was opened in (for the header).
#[tauri::command]
fn get_workspace_dir(state: tauri::State<'_, AppState>) -> String {
    state.workspace_root().to_string_lossy().to_string()
}

// ---------------------------------------------------------------------------
// Editing (whole-text sync; single user, sequential invokes — no races)
// ---------------------------------------------------------------------------

fn with_buffer<T>(
    state: &AppState,
    buffer_id: usize,
    f: impl FnOnce(&mut GuiBuffer) -> T,
) -> Result<T, String> {
    let mut buffers = state.buffers.lock().unwrap_or_else(|e| e.into_inner());
    match buffers.get_mut(&buffer_id) {
        Some(buf) => Ok(f(buf)),
        None => Err(format!("unknown buffer {buffer_id}")),
    }
}

fn open_file_impl(state: &AppState, path: Option<String>) -> Result<OpenedDto, String> {
    if let Some(ref requested) = path {
        let requested_path = PathBuf::from(requested);
        let wanted = canonical_key(&requested_path);
        let buffers = state.buffers.lock().unwrap_or_else(|e| e.into_inner());
        for (id, buf) in buffers.iter() {
            if let Some(existing) = buf.path.as_ref() {
                if canonical_key(existing) == wanted {
                    return Ok(OpenedDto {
                        buffer_id: *id,
                        path: Some(existing.to_string_lossy().to_string()),
                        text: buf.buffer.text(),
                        stats: buf.stats(),
                    });
                }
            }
        }
    }
    let buffer = match &path {
        Some(path) => Buffer::load(path).map_err(|err| format!("cannot open {path}: {err}"))?,
        None => Buffer::default(),
    };
    let gui = GuiBuffer {
        path: path.clone().map(PathBuf::from),
        buffer,
        history: UndoStack::new(),
    };
    let dto = OpenedDto {
        buffer_id: 0, // filled in below
        path,
        text: gui.buffer.text(),
        stats: gui.stats(),
    };
    let buffer_id = state.alloc_buffer(gui);
    Ok(OpenedDto { buffer_id, ..dto })
}

/// Open a file (or an empty draft with `path: null`) for editing.
#[tauri::command]
fn open_file(state: tauri::State<'_, AppState>, path: Option<String>) -> Result<OpenedDto, String> {
    open_file_impl(&state, path)
}

fn set_text_impl(state: &AppState, buffer_id: usize, text: String) -> Result<TextStats, String> {
    let (stats, path) = with_buffer(state, buffer_id, |buf| {
        let current = buf.buffer.text();
        if current != text {
            buf.history.record(&current, Instant::now());
            buf.buffer.set_text(&text);
        }
        let stats = buf.stats();
        (stats.clone(), buf.path.clone())
    })?;
    if let Some(path) = path {
        let words = stats.words;
        let mut project = state.project.lock().unwrap_or_else(|e| e.into_inner());
        let _ = project.sync_scene_words(&path, words);
    }
    Ok(stats)
}

/// Replace a buffer's whole text (coalescing undo; syncs outline words).
#[tauri::command]
fn set_text(
    state: tauri::State<'_, AppState>,
    buffer_id: usize,
    text: String,
) -> Result<TextStats, String> {
    set_text_impl(&state, buffer_id, text)
}

fn save_file_impl(
    state: &AppState,
    buffer_id: usize,
    path: Option<String>,
) -> Result<String, String> {
    let stored = {
        let buffers = state.buffers.lock().unwrap_or_else(|e| e.into_inner());
        buffers
            .get(&buffer_id)
            .and_then(|buf| buf.path.clone())
            .map(|path| path.to_string_lossy().to_string())
    };
    let root = state.workspace_root();
    let target = path.or(stored).unwrap_or_else(|| {
        let mut n = buffer_id;
        loop {
            let candidate = root.join(format!("untitled-{n}.md"));
            if !candidate.exists() {
                break candidate.to_string_lossy().to_string();
            }
            n = n.saturating_add(1);
        }
    });
    let (saved, words) = with_buffer(state, buffer_id, |buf| {
        buf.buffer
            .save_as(&target)
            .map_err(|err| format!("cannot save {target}: {err}"))?;
        buf.path = Some(PathBuf::from(&target));
        Ok::<(String, usize), String>((target.clone(), buf.stats().words))
    })??;
    let saved_path = PathBuf::from(&saved);
    let mut project = state.project.lock().unwrap_or_else(|e| e.into_inner());
    let _ = project.sync_scene_words(&saved_path, words);
    project.save().map_err(|err| err.to_string())?;
    Ok(saved)
}

/// Save a buffer. No path anywhere → deterministic `untitled-<id>.md` in
/// the workspace (native Save-As dialogs need an extra plugin; the message
/// tells the user the exact file).
#[tauri::command]
fn save_file(
    state: tauri::State<'_, AppState>,
    buffer_id: usize,
    path: Option<String>,
) -> Result<String, String> {
    save_file_impl(&state, buffer_id, path)
}

fn close_buffer_impl(state: &AppState, buffer_id: usize) -> Result<(), String> {
    {
        let mut buffers = state.buffers.lock().unwrap_or_else(|e| e.into_inner());
        buffers
            .remove(&buffer_id)
            .map(|_| ())
            .ok_or_else(|| format!("unknown buffer {buffer_id}"))?;
    }
    let project = state.project.lock().unwrap_or_else(|e| e.into_inner());
    project.save().map_err(|err| err.to_string())?;
    Ok(())
}

/// Drop a buffer from the session.
#[tauri::command]
fn close_buffer(state: tauri::State<'_, AppState>, buffer_id: usize) -> Result<(), String> {
    close_buffer_impl(&state, buffer_id)
}

fn undo_impl(state: &AppState, buffer_id: usize) -> Result<EditDto, String> {
    let (text, stats, path, words) = with_buffer(state, buffer_id, |buf| {
        let current = buf.buffer.text();
        if let Some(prev) = buf.history.undo(&current) {
            buf.buffer.set_text(&prev);
        }
        let stats = buf.stats();
        let words = stats.words;
        (buf.buffer.text(), stats, buf.path.clone(), words)
    })?;
    if let Some(path) = path {
        let mut project = state.project.lock().unwrap_or_else(|e| e.into_inner());
        let _ = project.sync_scene_words(&path, words);
    }
    Ok(EditDto { text, stats })
}

/// Undo via coalescing snapshots.
#[tauri::command]
fn undo_buffer(state: tauri::State<'_, AppState>, buffer_id: usize) -> Result<EditDto, String> {
    undo_impl(&state, buffer_id)
}

fn redo_impl(state: &AppState, buffer_id: usize) -> Result<EditDto, String> {
    let (text, stats, path, words) = with_buffer(state, buffer_id, |buf| {
        let current = buf.buffer.text();
        if let Some(next) = buf.history.redo(&current) {
            buf.buffer.set_text(&next);
        }
        let stats = buf.stats();
        let words = stats.words;
        (buf.buffer.text(), stats, buf.path.clone(), words)
    })?;
    if let Some(path) = path {
        let mut project = state.project.lock().unwrap_or_else(|e| e.into_inner());
        let _ = project.sync_scene_words(&path, words);
    }
    Ok(EditDto { text, stats })
}

#[tauri::command]
fn redo_buffer(state: tauri::State<'_, AppState>, buffer_id: usize) -> Result<EditDto, String> {
    redo_impl(&state, buffer_id)
}

#[derive(Debug, Clone, Serialize)]
struct EditDto {
    text: String,
    stats: TextStats,
}

// ---------------------------------------------------------------------------
// Graph + timeline (computed in core, rendered in JS)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Graph + timeline (computed in core, rendered in JS)
// ---------------------------------------------------------------------------

/// Relationship graph: scene texts come from open buffers (path-matched to
/// scene files) so mentions in *unsaved* drafts still link.
#[tauri::command]
fn get_graph(state: tauri::State<'_, AppState>) -> api::GraphDto {
    use std::collections::BTreeMap;
    let project = state.project.lock().unwrap_or_else(|e| e.into_inner());
    let buffers = state.buffers.lock().unwrap_or_else(|e| e.into_inner());
    let mut open_texts: BTreeMap<PathBuf, String> = BTreeMap::new();
    for buf in buffers.values() {
        if let Some(path) = buf.path.clone() {
            open_texts.insert(path, buf.buffer.text());
        }
    }
    let scene_texts = api::gather_scene_texts(&project, &open_texts);
    api::graph_dto(&project.manuscript, &project.lore, &scene_texts)
}

/// Outline-ordered timeline plus continuity notes.
#[tauri::command]
fn get_timeline(state: tauri::State<'_, AppState>) -> api::TimelineDto {
    let project = state.project.lock().unwrap_or_else(|e| e.into_inner());
    api::timeline_dto(&project.manuscript)
}

fn save_project_on_exit(window: &tauri::Window) {
    let state = window.state::<AppState>();
    let project = state.project.lock().unwrap_or_else(|e| e.into_inner());
    let _ = project.save();
}

fn main() {
    let workspace_dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    tauri::Builder::default()
        .manage(AppState::load(workspace_dir))
        .on_window_event(|window, event| {
            if matches!(
                event,
                tauri::WindowEvent::CloseRequested { .. } | tauri::WindowEvent::Destroyed
            ) {
                save_project_on_exit(window);
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_outline,
            get_stats,
            get_lore,
            get_workspace_dir,
            open_file,
            set_text,
            save_file,
            close_buffer,
            undo_buffer,
            redo_buffer,
            get_graph,
            get_timeline
        ])
        .run(tauri::generate_context!())
        .expect("error running yonro gui");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed_workspace() -> PathBuf {
        use yonro_core::{LoreBook, Manuscript};
        let dir = std::env::temp_dir().join(format!("yonro-gui-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".yonro")).unwrap();
        let manuscript = Manuscript::new("Probe");
        std::fs::write(
            dir.join(".yonro/manuscript.json"),
            serde_json::to_string(&manuscript).unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.join(".yonro/lore.json"),
            serde_json::to_string(&LoreBook::default()).unwrap(),
        )
        .unwrap();
        dir
    }

    fn project_title(state: &AppState) -> String {
        state
            .project
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .manuscript
            .title()
            .to_string()
    }

    #[test]
    fn loads_workspace_and_reports_empty_stats() {
        let dir = seed_workspace();
        let state = AppState::load(dir.clone());
        let project = state.project.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(project.root, dir);
        assert_eq!(project.manuscript.title(), "Probe");
        drop(project);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_workspace_falls_back_to_untitled() {
        let dir = std::env::temp_dir().join(format!("yonro-gui-missing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let state = AppState::load(dir);
        assert_eq!(project_title(&state), "Untitled");
    }

    #[test]
    fn corrupt_workspace_falls_back_safely() {
        let dir = std::env::temp_dir().join(format!("yonro-gui-corrupt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".yonro")).unwrap();
        std::fs::write(dir.join(".yonro/manuscript.json"), "not json{{").unwrap();
        std::fs::write(dir.join(".yonro/lore.json"), "{}").unwrap();
        let state = AppState::load(dir.clone());
        assert_eq!(project_title(&state), "Untitled");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn scene_workspace() -> (AppState, PathBuf) {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use yonro_core::{Manuscript, Project, SceneMeta};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("yonro-gui-scene-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut project = Project::load(&dir);
        project.manuscript = Manuscript::new("Probe");
        let act = project.manuscript.add_act("Act I").unwrap();
        let ch = project.manuscript.add_chapter(act, "Chapter 1").unwrap();
        let sc = project.manuscript.add_scene(ch, "The gate").unwrap();
        let path = dir.join("scene-gate.md");
        std::fs::write(&path, "Mara walked home").unwrap();
        project
            .manuscript
            .set_meta(
                sc,
                SceneMeta {
                    pov: "Mara".to_string(),
                    target_words: 100,
                    current_words: 0,
                    file: Some(path.clone()),
                    ..SceneMeta::default()
                },
            )
            .unwrap();
        project.save().unwrap();
        let state = AppState::load(dir.clone());
        (state, path)
    }

    #[test]
    fn set_text_syncs_scene_words_into_outline() {
        let (state, path) = scene_workspace();
        let dir = state.workspace_root();
        let opened = open_file_impl(&state, Some(path.to_string_lossy().to_string())).unwrap();
        let stats = set_text_impl(
            &state,
            opened.buffer_id,
            "one two three four five six".to_string(),
        )
        .unwrap();
        assert_eq!(stats.words, 6);
        let project = state.project.lock().unwrap_or_else(|e| e.into_inner());
        let outline = api::outline_dto(&project.manuscript);
        assert_eq!(outline.words, 6);
        assert!((outline.progress - 0.06).abs() < f64::EPSILON);
        drop(project);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_file_dedupes_canonical_path_with_live_text() {
        let (state, path) = scene_workspace();
        let dir = state.workspace_root();
        let first = open_file_impl(&state, Some(path.to_string_lossy().to_string())).unwrap();
        set_text_impl(
            &state,
            first.buffer_id,
            "draft unsaved text here".to_string(),
        )
        .unwrap();
        let second = open_file_impl(&state, Some(path.to_string_lossy().to_string())).unwrap();
        assert_eq!(second.buffer_id, first.buffer_id);
        assert_eq!(second.text, "draft unsaved text here");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
