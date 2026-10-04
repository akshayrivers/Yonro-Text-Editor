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
use yonro_core::{api, Buffer, Project, Recents, SceneMetaFields, UndoStack};

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
            can_undo: !self.history.undo.is_empty(),
            can_redo: !self.history.redo.is_empty(),
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

    fn open_texts(&self) -> std::collections::BTreeMap<PathBuf, String> {
        let buffers = self.buffers.lock().unwrap_or_else(|e| e.into_inner());
        let mut open_texts = std::collections::BTreeMap::new();
        for buf in buffers.values() {
            if let Some(path) = buf.path.clone() {
                open_texts.insert(path, buf.buffer.text());
            }
        }
        open_texts
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
    can_undo: bool,
    can_redo: bool,
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
// Workspaces (P5.1): start screen + recents. Thin over core: `Recents` owns
// the file logic, `Project` owns load/create, this layer only swaps state,
// guards dirty buffers, and stamps `last_opened`.
// ---------------------------------------------------------------------------

fn config_recents_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .map(|dir| Recents::file_in(&dir))
        .ok()
}

/// Days since civil 1970-01-01 → (year, month, day).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days.saturating_add(719_468);
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = doe
        .saturating_sub(doe.div_euclid(1_460))
        .saturating_add(doe.div_euclid(36_524))
        .saturating_sub(doe.div_euclid(146_096))
        .div_euclid(365);
    let mut year = yoe.saturating_add(era.saturating_mul(400));
    let doy = doe.saturating_sub(
        yoe.saturating_mul(365)
            .saturating_add(yoe.div_euclid(4))
            .saturating_sub(yoe.div_euclid(100)),
    );
    let mp = doy.saturating_mul(5).saturating_add(2).div_euclid(153);
    let day = doy
        .saturating_sub(mp.saturating_mul(153).saturating_add(2).div_euclid(5))
        .saturating_add(1);
    let month = if mp < 10 {
        mp.saturating_add(3)
    } else {
        mp.saturating_sub(9)
    };
    year = year.saturating_add(i64::from(month <= 2));
    (
        year,
        u32::try_from(month).unwrap_or(1),
        u32::try_from(day).unwrap_or(1),
    )
}

/// `YYYY-MM-DD` in UTC (`"1970-01-01"` when the clock is unavailable).
fn today_stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|span| span.as_secs())
        .unwrap_or(0);
    let days_i64 = i64::try_from(secs.div_euclid(86_400)).unwrap_or(0);
    let (year, month, day) = civil_from_days(days_i64);
    format!("{year:04}-{month:02}-{day:02}")
}

fn absolute_workspace_path(raw: &str) -> Result<PathBuf, String> {
    let resolved = Project::resolve_workspace_path(raw).map_err(|err| err.to_string())?;
    if resolved.is_absolute() {
        Ok(resolved)
    } else {
        let cwd = std::env::current_dir().map_err(|err| format!("cannot resolve {raw}: {err}"))?;
        Ok(cwd.join(resolved))
    }
}

fn any_dirty(state: &AppState) -> bool {
    state
        .buffers
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .values()
        .any(|buf| buf.buffer.is_dirty())
}

fn touch_recents(recents_path: Option<&PathBuf>, dir: &std::path::Path, title: &str) {
    let Some(path) = recents_path else { return };
    let mut recents = Recents::load(path);
    recents.touch(&dir.to_string_lossy(), title, &today_stamp());
    let _ = recents.save(path);
}

fn get_workspace_impl(state: &AppState, recents_path: Option<PathBuf>) -> api::WorkspaceDto {
    let recents = recents_path.map_or_else(Recents::new, |path| Recents::load(&path));
    let project = state.project.lock().unwrap_or_else(|e| e.into_inner());
    api::workspace_dto(&project, &recents)
}

fn open_workspace_impl(
    state: &AppState,
    path: &str,
    force: bool,
    recents_path: Option<PathBuf>,
) -> Result<api::WorkspaceDto, String> {
    if !force && any_dirty(state) {
        return Err("unsaved changes: save or discard first, or retry with force".to_string());
    }
    let absolute = absolute_workspace_path(path)?;
    let title = Project::load(&absolute).manuscript.title().to_string();
    {
        let mut guard = state.project.lock().unwrap_or_else(|e| e.into_inner());
        *guard = Project::load(&absolute);
    }
    state
        .buffers
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clear();
    touch_recents(recents_path.as_ref(), &absolute, &title);
    Ok(get_workspace_impl(state, recents_path))
}

fn create_workspace_impl(
    state: &AppState,
    path: &str,
    title: &str,
    recents_path: Option<PathBuf>,
) -> Result<api::WorkspaceDto, String> {
    if any_dirty(state) {
        return Err("unsaved changes: save or discard first, then create".to_string());
    }
    let absolute = absolute_workspace_path(path)?;
    let project = Project::create(&absolute, title).map_err(|err| err.to_string())?;
    let saved_title = project.manuscript.title().to_string();
    {
        let mut guard = state.project.lock().unwrap_or_else(|e| e.into_inner());
        *guard = project;
    }
    state
        .buffers
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clear();
    touch_recents(recents_path.as_ref(), &absolute, &saved_title);
    Ok(get_workspace_impl(state, recents_path))
}

/// Start-screen payload: dir, title, project presence, recents, warnings.
#[tauri::command]
fn get_workspace(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> api::WorkspaceDto {
    get_workspace_impl(&state, config_recents_path(&app))
}

/// Switch workspace (drops all buffers). Dirty buffers block unless `force`.
#[tauri::command]
fn open_workspace(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    path: String,
    force: bool,
) -> Result<api::WorkspaceDto, String> {
    open_workspace_impl(&state, &path, force, config_recents_path(&app))
}

/// Create a workspace (mkdir + fresh project) and switch to it.
#[tauri::command]
fn create_workspace(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    path: String,
    title: String,
) -> Result<api::WorkspaceDto, String> {
    create_workspace_impl(&state, &path, &title, config_recents_path(&app))
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

fn resolve_save_target(
    root: &std::path::Path,
    requested: Option<String>,
    stored: Option<PathBuf>,
    buffer_id: usize,
    overwrite: bool,
) -> Result<PathBuf, String> {
    if let Some(raw) = requested {
        let trimmed = raw.trim().to_string();
        if trimmed.is_empty() {
            return Err(format!("cannot save {raw}: filename is empty"));
        }
        if trimmed.contains("..") {
            return Err(format!(
                "cannot save {trimmed}: must stay inside workspace (no \"..\")"
            ));
        }
        let lower = trimmed.to_lowercase();
        if !(lower.ends_with(".md") || lower.ends_with(".txt")) {
            return Err(format!("cannot save {trimmed}: must end with .md or .txt"));
        }
        let candidate = if PathBuf::from(&trimmed).is_absolute() {
            PathBuf::from(&trimmed)
        } else {
            root.join(&trimmed)
        };
        for component in candidate.components() {
            if matches!(component, std::path::Component::ParentDir) {
                return Err(format!(
                    "cannot save {trimmed}: must stay inside workspace (no \"..\")"
                ));
            }
        }
        let root_canon = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        if !candidate.starts_with(&root_canon) && !candidate.starts_with(root) {
            return Err(format!("cannot save {trimmed}: must stay inside workspace"));
        }
        let same_as_stored = stored.as_ref().is_some_and(|current| *current == candidate);
        if !same_as_stored && candidate.exists() && !overwrite {
            return Err(format!(
                "cannot save {}: file exists (tick overwrite to replace)",
                candidate.display()
            ));
        }
        if let Some(parent) = candidate.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("cannot save {}: {err}", candidate.display()))?;
        }
        Ok(candidate)
    } else if let Some(current) = stored {
        Ok(current)
    } else {
        let mut n = buffer_id;
        loop {
            let candidate = root.join(format!("untitled-{n}.md"));
            if !candidate.exists() {
                return Ok(candidate);
            }
            n = n.saturating_add(1);
        }
    }
}

fn recovery_dir(root: &std::path::Path) -> PathBuf {
    root.join(".yonro/recovery")
}

fn recovery_name_for(buffer_id: usize, path: Option<&PathBuf>) -> String {
    if let Some(path) = path {
        if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            return name.to_string();
        }
    }
    format!("untitled-{buffer_id}.md")
}

fn write_bytes_atomic(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("cannot write {}: {err}", path.display()))?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).map_err(|err| format!("cannot write {}: {err}", tmp.display()))?;
    let file = std::fs::File::open(&tmp)
        .map_err(|err| format!("cannot write {}: {err}", tmp.display()))?;
    file.sync_all()
        .map_err(|err| format!("cannot write {}: {err}", tmp.display()))?;
    drop(file);
    std::fs::rename(&tmp, path).map_err(|err| format!("cannot write {}: {err}", path.display()))?;
    Ok(())
}

fn clear_recovery_file(root: &std::path::Path, name: &str) {
    let path = recovery_dir(root).join(name);
    let _ = std::fs::remove_file(&path);
}

#[derive(Debug, Clone, Serialize)]
struct RecoveryDto {
    name: String,
    text: String,
    newer: bool,
}

fn check_recovery_impl(
    state: &AppState,
    path: Option<String>,
) -> Result<Option<RecoveryDto>, String> {
    let Some(raw) = path else {
        return Ok(None);
    };
    let original = if PathBuf::from(&raw).is_absolute() {
        PathBuf::from(&raw)
    } else {
        state.workspace_root().join(&raw)
    };
    let Some(file_name) = original
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let recovery = recovery_dir(&state.workspace_root()).join(&file_name);
    if !recovery.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&recovery)
        .map_err(|err| format!("cannot read {}: {err}", recovery.display()))?;
    let newer = match (
        std::fs::metadata(&recovery).and_then(|meta| meta.modified()),
        std::fs::metadata(&original).and_then(|meta| meta.modified()),
    ) {
        (_, Err(_)) => true,
        (Ok(recovery_time), Ok(original_time)) => recovery_time > original_time,
        (Err(_), Ok(_)) => false,
    };
    Ok(Some(RecoveryDto {
        name: file_name,
        text,
        newer,
    }))
}

fn discard_recovery_impl(state: &AppState, path: Option<String>) -> Result<(), String> {
    let Some(raw) = path else {
        return Ok(());
    };
    let original = if PathBuf::from(&raw).is_absolute() {
        PathBuf::from(&raw)
    } else {
        state.workspace_root().join(&raw)
    };
    let Some(file_name) = original
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
    else {
        return Ok(());
    };
    clear_recovery_file(&state.workspace_root(), &file_name);
    Ok(())
}

fn sweep_recovery_impl(state: &AppState) -> Result<Vec<String>, String> {
    let root = state.workspace_root();
    let pending: Vec<(usize, String, String)> = {
        let buffers = state.buffers.lock().unwrap_or_else(|e| e.into_inner());
        buffers
            .iter()
            .filter(|(_, buf)| buf.buffer.is_dirty())
            .map(|(id, buf)| {
                (
                    *id,
                    recovery_name_for(*id, buf.path.as_ref()),
                    buf.buffer.text(),
                )
            })
            .collect()
    };
    let mut written = Vec::new();
    for (buffer_id, name, text) in pending {
        let dest = recovery_dir(&root).join(&name);
        write_bytes_atomic(&dest, text.as_bytes())?;
        written.push(format!("{buffer_id}:{name}"));
        let _ = buffer_id;
    }
    Ok(written)
}

fn save_file_impl(
    state: &AppState,
    buffer_id: usize,
    path: Option<String>,
    overwrite: bool,
) -> Result<String, String> {
    let stored = {
        let buffers = state.buffers.lock().unwrap_or_else(|e| e.into_inner());
        buffers.get(&buffer_id).and_then(|buf| buf.path.clone())
    };
    let root = state.workspace_root();
    let target = resolve_save_target(&root, path, stored, buffer_id, overwrite)?;
    let target_string = target.to_string_lossy().to_string();
    let (saved, words) = with_buffer(state, buffer_id, |buf| {
        buf.buffer
            .save_as(&target_string)
            .map_err(|err| format!("cannot save {target_string}: {err}"))?;
        buf.path = Some(PathBuf::from(&target_string));
        Ok::<(String, usize), String>((target_string.clone(), buf.stats().words))
    })??;
    let saved_path = PathBuf::from(&saved);
    let mut project = state.project.lock().unwrap_or_else(|e| e.into_inner());
    let _ = project.sync_scene_words(&saved_path, words);
    project.save().map_err(|err| err.to_string())?;
    if let Some(name) = saved_path.file_name().and_then(|name| name.to_str()) {
        // NOTE: `root` (not `workspace_root()`) — the project lock above
        // is still held and `Mutex` is not reentrant.
        clear_recovery_file(&root, name);
    }
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
    overwrite: Option<bool>,
) -> Result<String, String> {
    save_file_impl(&state, buffer_id, path, overwrite.unwrap_or(false))
}

/// Write crash-recovery copies for every dirty buffer (atomic).
#[tauri::command]
fn sweep_recovery(state: tauri::State<'_, AppState>) -> Result<Vec<String>, String> {
    sweep_recovery_impl(&state)
}

/// When `path` has a newer recovery copy, return it so the UI can offer
/// [restore] [discard]. `None` means no recovery to consider.
#[tauri::command]
fn check_recovery(
    state: tauri::State<'_, AppState>,
    path: Option<String>,
) -> Result<Option<RecoveryDto>, String> {
    check_recovery_impl(&state, path)
}

/// Delete the recovery copy backing `path` (after restore or explicit discard).
#[tauri::command]
fn discard_recovery(state: tauri::State<'_, AppState>, path: Option<String>) -> Result<(), String> {
    discard_recovery_impl(&state, path)
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
    let project = state.project.lock().unwrap_or_else(|e| e.into_inner());
    let open_texts = state.open_texts();
    let scene_texts = api::gather_scene_texts(&project, &open_texts);
    api::graph_dto(&project.manuscript, &project.lore, &scene_texts)
}

/// Outline-ordered timeline plus continuity notes.
#[tauri::command]
fn get_timeline(state: tauri::State<'_, AppState>) -> api::TimelineDto {
    let project = state.project.lock().unwrap_or_else(|e| e.into_inner());
    api::timeline_dto(&project.manuscript, &project.lore)
}

// ---------------------------------------------------------------------------
// Structure editing (thin over `Project`; every mutation persists and
// returns the fresh outline so the UI re-renders from truth)
// ---------------------------------------------------------------------------

fn add_node_impl(
    state: &AppState,
    parent: Option<usize>,
    kind: String,
    title: String,
) -> Result<api::AddNodeDto, String> {
    let mut project = state.project.lock().unwrap_or_else(|e| e.into_inner());
    let new_id = project
        .add_node(parent, &kind, &title)
        .map_err(|err| err.to_string())?;
    Ok(api::AddNodeDto {
        outline: api::outline_dto(&project.manuscript),
        new_id,
    })
}

/// Add an act/chapter/scene; `parent: null` puts an act at the root.
#[tauri::command]
fn add_node(
    state: tauri::State<'_, AppState>,
    parent: Option<usize>,
    kind: String,
    title: String,
) -> Result<api::AddNodeDto, String> {
    add_node_impl(&state, parent, kind, title)
}

fn rename_node_impl(
    state: &AppState,
    id: usize,
    title: String,
) -> Result<api::OutlineNodeDto, String> {
    let mut project = state.project.lock().unwrap_or_else(|e| e.into_inner());
    project
        .rename_node(id, &title)
        .map_err(|err| err.to_string())?;
    Ok(api::outline_dto(&project.manuscript))
}

/// Rename any outline node.
#[tauri::command]
fn rename_node(
    state: tauri::State<'_, AppState>,
    id: usize,
    title: String,
) -> Result<api::OutlineNodeDto, String> {
    rename_node_impl(&state, id, title)
}

fn move_node_impl(
    state: &AppState,
    id: usize,
    new_parent: usize,
    index: Option<usize>,
) -> Result<api::OutlineNodeDto, String> {
    let mut project = state.project.lock().unwrap_or_else(|e| e.into_inner());
    project
        .move_node(id, new_parent, index)
        .map_err(|err| err.to_string())?;
    Ok(api::outline_dto(&project.manuscript))
}

/// Reparent `id` under `new_parent` (`index: null` appends).
#[tauri::command]
fn move_node(
    state: tauri::State<'_, AppState>,
    id: usize,
    new_parent: usize,
    index: Option<usize>,
) -> Result<api::OutlineNodeDto, String> {
    move_node_impl(&state, id, new_parent, index)
}

fn remove_node_impl(state: &AppState, id: usize) -> Result<api::RemoveNodeDto, String> {
    let mut project = state.project.lock().unwrap_or_else(|e| e.into_inner());
    let message = project.remove_node(id).map_err(|err| err.to_string())?;
    Ok(api::RemoveNodeDto {
        outline: api::outline_dto(&project.manuscript),
        message,
    })
}

/// Remove a node (scene drafts stay on disk; snapshots go to history).
#[tauri::command]
fn remove_node(state: tauri::State<'_, AppState>, id: usize) -> Result<api::RemoveNodeDto, String> {
    remove_node_impl(&state, id)
}

fn set_scene_meta_impl(
    state: &AppState,
    id: usize,
    meta: SceneMetaFields,
) -> Result<api::OutlineNodeDto, String> {
    let mut project = state.project.lock().unwrap_or_else(|e| e.into_inner());
    project
        .set_scene_meta(id, &meta)
        .map_err(|err| err.to_string())?;
    Ok(api::outline_dto(&project.manuscript))
}

/// Replace a scene's inspector-editable metadata (seeds lore like the TUI).
#[tauri::command]
fn set_scene_meta(
    state: tauri::State<'_, AppState>,
    id: usize,
    meta: SceneMetaFields,
) -> Result<api::OutlineNodeDto, String> {
    set_scene_meta_impl(&state, id, meta)
}

fn get_scene_impl(state: &AppState, id: usize) -> Result<api::SceneDetailDto, String> {
    let project = state.project.lock().unwrap_or_else(|e| e.into_inner());
    api::scene_detail_dto(&project, id).map_err(|err| err.to_string())
}

/// One scene's inspector payload (breadcrumb, meta, file, counts).
#[tauri::command]
fn get_scene(state: tauri::State<'_, AppState>, id: usize) -> Result<api::SceneDetailDto, String> {
    get_scene_impl(&state, id)
}

fn open_scene_impl(state: &AppState, id: usize) -> Result<OpenedDto, String> {
    let path = {
        let mut project = state.project.lock().unwrap_or_else(|e| e.into_inner());
        project.scene_file(id).map_err(|err| err.to_string())?
    };
    open_file_impl(state, Some(path.to_string_lossy().to_string()))
}

/// Materialize a scene's draft file, then open it (deduped like files).
#[tauri::command]
fn open_scene(state: tauri::State<'_, AppState>, id: usize) -> Result<OpenedDto, String> {
    open_scene_impl(&state, id)
}

fn list_files_impl(state: &AppState) -> Vec<String> {
    let project = state.project.lock().unwrap_or_else(|e| e.into_inner());
    project.list_files()
}

/// Scene-less `.md` files in the workspace root.
#[tauri::command]
fn list_files(state: tauri::State<'_, AppState>) -> Vec<String> {
    list_files_impl(&state)
}

#[derive(Debug, Clone, serde::Deserialize)]
struct UpdateEntityPatch {
    name: Option<String>,
    aliases: Option<Vec<String>>,
    sheet: Option<String>,
}

fn add_entity_impl(state: &AppState, kind: String, name: String) -> Result<api::EntityDto, String> {
    let mut project = state.project.lock().unwrap_or_else(|e| e.into_inner());
    let id = project
        .add_entity(&kind, &name)
        .map_err(|err| err.to_string())?;
    api::entity_dto(&project.manuscript, &project.lore, id)
        .ok_or_else(|| format!("unknown entity {id}"))
}

#[tauri::command]
fn add_entity(
    state: tauri::State<'_, AppState>,
    kind: String,
    name: String,
) -> Result<api::EntityDto, String> {
    add_entity_impl(&state, kind, name)
}

fn update_entity_impl(
    state: &AppState,
    id: usize,
    patch: UpdateEntityPatch,
) -> Result<api::EntityDto, String> {
    let mut project = state.project.lock().unwrap_or_else(|e| e.into_inner());
    project
        .update_entity(
            id,
            patch.name.as_deref(),
            patch.aliases.as_deref(),
            patch.sheet.as_deref(),
        )
        .map_err(|err| err.to_string())?;
    api::entity_dto(&project.manuscript, &project.lore, id)
        .ok_or_else(|| format!("unknown entity {id}"))
}

#[tauri::command]
fn update_entity(
    state: tauri::State<'_, AppState>,
    id: usize,
    patch: UpdateEntityPatch,
) -> Result<api::EntityDto, String> {
    update_entity_impl(&state, id, patch)
}

fn remove_entity_impl(state: &AppState, id: usize) -> Result<(), String> {
    let mut project = state.project.lock().unwrap_or_else(|e| e.into_inner());
    project.remove_entity(id).map_err(|err| err.to_string())
}

#[tauri::command]
fn remove_entity(state: tauri::State<'_, AppState>, id: usize) -> Result<(), String> {
    remove_entity_impl(&state, id)
}

fn lore_search_impl(
    state: &AppState,
    prefix: String,
    limit: Option<usize>,
) -> Vec<api::LoreSearchHitDto> {
    let project = state.project.lock().unwrap_or_else(|e| e.into_inner());
    api::lore_search(&project.lore, &prefix, limit.unwrap_or(8))
}

#[tauri::command]
fn lore_search(
    state: tauri::State<'_, AppState>,
    prefix: String,
    limit: Option<usize>,
) -> Vec<api::LoreSearchHitDto> {
    lore_search_impl(&state, prefix, limit)
}

fn get_entity_impl(state: &AppState, id: usize) -> Result<api::EntityDetailDto, String> {
    let project = state.project.lock().unwrap_or_else(|e| e.into_inner());
    let open_texts = state.open_texts();
    api::entity_detail_dto(&project, &open_texts, id).map_err(|err| err.to_string())
}

#[tauri::command]
fn get_entity(
    state: tauri::State<'_, AppState>,
    id: usize,
) -> Result<api::EntityDetailDto, String> {
    get_entity_impl(&state, id)
}

fn get_mentions_impl(
    state: &AppState,
    buffer_id: usize,
) -> Result<Vec<api::MentionSpanDto>, String> {
    let text = with_buffer(state, buffer_id, |buf| buf.buffer.text())?;
    let project = state.project.lock().unwrap_or_else(|e| e.into_inner());
    Ok(api::get_mentions_dto(&project.lore, &text))
}

#[tauri::command]
fn get_mentions(
    state: tauri::State<'_, AppState>,
    buffer_id: usize,
) -> Result<Vec<api::MentionSpanDto>, String> {
    get_mentions_impl(&state, buffer_id)
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
            get_workspace,
            open_workspace,
            create_workspace,
            open_file,
            set_text,
            save_file,
            close_buffer,
            undo_buffer,
            redo_buffer,
            get_graph,
            get_timeline,
            add_node,
            rename_node,
            move_node,
            remove_node,
            set_scene_meta,
            get_scene,
            open_scene,
            list_files,
            add_entity,
            update_entity,
            remove_entity,
            lore_search,
            get_entity,
            get_mentions,
            sweep_recovery,
            check_recovery,
            discard_recovery
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

    #[test]
    fn save_rejects_dotdot_and_bad_extension() {
        let (state, _path) = scene_workspace();
        let dir = state.workspace_root();
        let opened = open_file_impl(&state, None).unwrap();
        let dotdot = save_file_impl(
            &state,
            opened.buffer_id,
            Some("../evil.md".to_string()),
            false,
        );
        assert!(dotdot.is_err());
        assert!(dotdot.unwrap_err().contains("../evil.md"));
        let bad_ext = save_file_impl(
            &state,
            opened.buffer_id,
            Some("notes.pdf".to_string()),
            false,
        );
        assert!(bad_ext.is_err());
        assert!(bad_ext.unwrap_err().contains("notes.pdf"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_refuses_overwrite_without_flag() {
        let (state, _path) = scene_workspace();
        let dir = state.workspace_root();
        std::fs::write(dir.join("taken.md"), "existing").unwrap();
        let opened = open_file_impl(&state, None).unwrap();
        set_text_impl(&state, opened.buffer_id, "new words here".to_string()).unwrap();
        let refused = save_file_impl(
            &state,
            opened.buffer_id,
            Some("taken.md".to_string()),
            false,
        );
        assert!(refused.is_err());
        assert!(refused.unwrap_err().contains("taken.md"));
        let allowed =
            save_file_impl(&state, opened.buffer_id, Some("taken.md".to_string()), true).unwrap();
        assert!(allowed.ends_with("taken.md"));
        assert_eq!(std::fs::read_to_string(&allowed).unwrap(), "new words here");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_rejects_absolute_path_outside_workspace() {
        let (state, _path) = scene_workspace();
        let dir = state.workspace_root();
        let opened = open_file_impl(&state, None).unwrap();
        let outside = std::env::temp_dir().join("yonro-outside-gui.md");
        let err = save_file_impl(
            &state,
            opened.buffer_id,
            Some(outside.to_string_lossy().to_string()),
            true,
        );
        assert!(err.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stats_reports_can_undo_can_redo() {
        let (state, path) = scene_workspace();
        let dir = state.workspace_root();
        let opened = open_file_impl(&state, Some(path.to_string_lossy().to_string())).unwrap();
        assert!(!opened.stats.can_undo);
        set_text_impl(&state, opened.buffer_id, "burst one".to_string()).unwrap();
        let buffers = state.buffers.lock().unwrap_or_else(|e| e.into_inner());
        let buf = buffers.get(&opened.buffer_id).unwrap();
        assert!(buf.stats().can_undo);
        assert!(!buf.stats().can_redo);
        drop(buffers);
        undo_impl(&state, opened.buffer_id).unwrap();
        let buffers = state.buffers.lock().unwrap_or_else(|e| e.into_inner());
        let buf = buffers.get(&opened.buffer_id).unwrap();
        assert!(buf.stats().can_redo);
        drop(buffers);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recovery_sweep_check_and_clear() {
        let (state, path) = scene_workspace();
        let dir = state.workspace_root();
        let opened = open_file_impl(&state, Some(path.to_string_lossy().to_string())).unwrap();
        set_text_impl(&state, opened.buffer_id, "dirty recovery text".to_string()).unwrap();
        let written = sweep_recovery_impl(&state).unwrap();
        assert_eq!(written.len(), 1);
        let found = check_recovery_impl(&state, Some(path.to_string_lossy().to_string())).unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert!(found.newer);
        assert_eq!(found.text, "dirty recovery text");
        discard_recovery_impl(&state, Some(path.to_string_lossy().to_string())).unwrap();
        let gone = check_recovery_impl(&state, Some(path.to_string_lossy().to_string())).unwrap();
        assert!(gone.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_clears_recovery_copy() {
        let (state, _path) = scene_workspace();
        let dir = state.workspace_root();
        let opened = open_file_impl(&state, None).unwrap();
        set_text_impl(&state, opened.buffer_id, "fresh draft".to_string()).unwrap();
        sweep_recovery_impl(&state).unwrap();
        let saved = save_file_impl(
            &state,
            opened.buffer_id,
            Some("fresh.md".to_string()),
            false,
        )
        .unwrap();
        assert!(saved.ends_with("fresh.md"));
        assert!(!recovery_dir(&dir).join("fresh.md").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn empty_state() -> AppState {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(10_000);
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("yonro-gui-struct-{n}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        AppState::load(dir)
    }

    #[test]
    fn structure_impls_build_rename_move_and_open_scenes() {
        let state = empty_state();
        let dir = state.workspace_root();
        let act = add_node_impl(&state, None, "act".to_string(), "Act I".to_string())
            .unwrap()
            .new_id;
        let ch = add_node_impl(&state, Some(act), "chapter".to_string(), "Ch 1".to_string())
            .unwrap()
            .new_id;
        let sc = add_node_impl(&state, Some(ch), "scene".to_string(), "S1".to_string())
            .unwrap()
            .new_id;
        rename_node_impl(&state, sc, "The gate".to_string()).unwrap();
        let detail = get_scene_impl(&state, sc).unwrap();
        assert_eq!(detail.title, "The gate");
        assert_eq!(
            detail.breadcrumb,
            vec!["Untitled", "Act I", "Ch 1", "The gate"]
        );
        set_scene_meta_impl(
            &state,
            sc,
            SceneMetaFields {
                pov: "Mara".to_string(),
                setting: "Mill farm".to_string(),
                target_words: 100,
                ..SceneMetaFields::default()
            },
        )
        .unwrap();
        // POV/setting seeded lore exactly like the TUI.
        let project = state.project.lock().unwrap_or_else(|e| e.into_inner());
        assert!(project.lore.resolve("Mara").is_some());
        drop(project);
        // Second chapter + indexed move to its front.
        let ch2 = add_node_impl(&state, Some(act), "chapter".to_string(), "Ch 2".to_string())
            .unwrap()
            .new_id;
        let outline = move_node_impl(&state, sc, ch2, Some(0)).unwrap();
        assert_eq!(outline.children[0].children[1].children[0].id, sc);
        // Opening materializes the draft; stray files list separately.
        let opened = open_scene_impl(&state, sc).unwrap();
        assert!(opened
            .path
            .is_some_and(|p| p.contains(&format!("scene-{sc}.md"))));
        std::fs::write(dir.join("notes.md"), "stray").unwrap();
        let files = list_files_impl(&state);
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("notes.md"));
        // Reload keeps structure and meta.
        let reloaded = AppState::load(dir.clone());
        let detail = get_scene_impl(&reloaded, sc).unwrap();
        assert_eq!(detail.meta.pov, "Mara");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn structure_impls_name_failures_and_keep_drafts_on_remove() {
        let state = empty_state();
        let dir = state.workspace_root();
        let err = add_node_impl(&state, None, "scene".to_string(), "S".to_string()).unwrap_err();
        assert!(err.contains("scenes live in chapters"));
        let err = rename_node_impl(&state, 999, "X".to_string()).unwrap_err();
        assert!(err.contains("999"));
        let act = add_node_impl(&state, None, "act".to_string(), "A".to_string())
            .unwrap()
            .new_id;
        let ch = add_node_impl(&state, Some(act), "chapter".to_string(), "C".to_string())
            .unwrap()
            .new_id;
        let sc = add_node_impl(&state, Some(ch), "scene".to_string(), "S".to_string())
            .unwrap()
            .new_id;
        let opened = open_scene_impl(&state, sc).unwrap();
        let path = opened.path.unwrap();
        std::fs::write(&path, "keep me").unwrap();
        let removed = remove_node_impl(&state, sc).unwrap();
        assert!(removed.message.contains(&format!("scene-{sc}.md")));
        assert!(removed.message.contains("kept"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep me");
        assert!(get_scene_impl(&state, sc).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn lore_and_mention_commands_roundtrip() {
        let (state, path) = scene_workspace();
        let dir = state.workspace_root();

        // 1. Add entity
        let mara =
            add_entity_impl(&state, "character".to_string(), "Mara Stone".to_string()).unwrap();
        assert_eq!(mara.name, "Mara Stone");
        assert_eq!(mara.kind, "character");

        // Duplicate rejection error mapping
        let err =
            add_entity_impl(&state, "character".to_string(), "mara stone".to_string()).unwrap_err();
        assert_eq!(err, "name already in use: mara stone");

        // 2. Update entity
        let updated = update_entity_impl(
            &state,
            mara.id,
            UpdateEntityPatch {
                name: Some("Mara the Brave".to_string()),
                aliases: Some(vec!["Brave Mara".to_string()]),
                sheet: Some("Leader of the rebellion.".to_string()),
            },
        )
        .unwrap();
        assert_eq!(updated.name, "Mara the Brave");
        assert_eq!(updated.aliases, vec!["Brave Mara"]);
        assert_eq!(updated.sheet, "Leader of the rebellion.");

        // 3. Search
        let search = lore_search_impl(&state, "brave".to_string(), Some(5));
        assert_eq!(search.len(), 1);
        assert_eq!(search[0].name, "Mara the Brave");
        assert_eq!(search[0].matched_alias, Some("Brave Mara".to_string()));

        // 4. Open buffer and check get_entity mention counts + get_mentions
        let opened = open_file_impl(&state, Some(path.to_string_lossy().to_string())).unwrap();
        set_text_impl(
            &state,
            opened.buffer_id,
            "Talked with @Brave Mara today. @Brave Mara agreed.".to_string(),
        )
        .unwrap();

        let detail = get_entity_impl(&state, mara.id).unwrap();
        assert_eq!(detail.entity.name, "Mara the Brave");
        assert_eq!(detail.mention_scenes.len(), 1);
        assert_eq!(detail.mention_scenes[0].count, 2);

        let mentions = get_mentions_impl(&state, opened.buffer_id).unwrap();
        assert_eq!(mentions.len(), 2);
        assert_eq!(mentions[0].entity_id, Some(mara.id));
        assert_eq!(mentions[0].kind.as_deref(), Some("character"));

        // 5. Remove entity
        remove_entity_impl(&state, mara.id).unwrap();
        assert!(get_entity_impl(&state, mara.id).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn workspace_impls_guard_dirty_create_and_remember() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(50_000);
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let base = std::env::temp_dir().join(format!("yonro-gui-ws-{n}"));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let recents = base.join("recents.json");
        let first = base.join("first");
        std::fs::create_dir_all(&first).unwrap();
        let state = AppState::load(first.clone());

        // Fresh dir reports no project; creating flips it and stamps recents.
        let empty = get_workspace_impl(&state, Some(recents.clone()));
        assert!(!empty.has_project);
        let made = create_workspace_impl(
            &state,
            base.join("novel").to_string_lossy().as_ref(),
            "Novel",
            Some(recents.clone()),
        )
        .unwrap();
        assert!(made.has_project);
        assert_eq!(made.title, "Novel");
        assert_eq!(made.recent.len(), 1);
        assert_eq!(made.recent[0].last_opened.len(), 10);

        // A dirty buffer blocks a plain switch but yields to force.
        let opened = open_file_impl(&state, None).unwrap();
        set_text_impl(&state, opened.buffer_id, "unsaved words".to_string()).unwrap();
        assert!(open_workspace_impl(
            &state,
            &first.to_string_lossy(),
            false,
            Some(recents.clone())
        )
        .is_err());
        let back = open_workspace_impl(
            &state,
            &first.to_string_lossy(),
            true,
            Some(recents.clone()),
        )
        .unwrap();
        assert_eq!(back.dir, first.to_string_lossy().to_string());
        assert_eq!(back.recent.len(), 2);
        let _ = std::fs::remove_dir_all(&base);
    }
}
