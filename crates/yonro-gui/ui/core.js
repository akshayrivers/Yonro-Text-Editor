/* Yonro GUI adapter seam.
 *
 * The ONLY file in ui/ allowed to touch window.__TAURI__. Every other
 * file calls `core.<fn>()`. A future pure-web build reimplements this
 * file over WASM and nothing else changes.
 */
const core = {
  async invoke(cmd, args = {}) {
    // Tauri v2 exposes the API globally; the npm package is just typing.
    return window.__TAURI__.core.invoke(cmd, args);
  },
  outline: () => core.invoke('get_outline'),
  stats: () => core.invoke('get_stats'),
  lore: () => core.invoke('get_lore'),
  workspace: () => core.invoke('get_workspace'),
  home: () => core.invoke('get_home'),
  openWorkspace: (path, force) => core.invoke('open_workspace', { path, force: Boolean(force) }),
  createWorkspace: (path, title) => core.invoke('create_workspace', { path, title }),
  graph: () => core.invoke('get_graph'),
  timeline: () => core.invoke('get_timeline'),
  customGraphs: () => core.invoke('list_custom_graphs'),
  createCustomGraph: (title) => core.invoke('create_custom_graph', { title }),
  renameCustomGraph: (id, title) => core.invoke('rename_custom_graph', { id, title }),
  deleteCustomGraph: (id) => core.invoke('delete_custom_graph', { id }),
  openFile: (path) => core.invoke('open_file', { path: path ?? null }),
  setText: (bufferId, text) => core.invoke('set_text', { bufferId, text }),
  saveFile: (bufferId, path, overwrite) => core.invoke('save_file', { bufferId, path: path ?? null, overwrite: overwrite ?? false }),
  closeBuffer: (bufferId) => core.invoke('close_buffer', { bufferId }),
  undo: (bufferId) => core.invoke('undo_buffer', { bufferId }),
  redo: (bufferId) => core.invoke('redo_buffer', { bufferId }),
  sweepRecovery: () => core.invoke('sweep_recovery'),
  checkRecovery: (path) => core.invoke('check_recovery', { path: path ?? null }),
  discardRecovery: (path) => core.invoke('discard_recovery', { path: path ?? null }),
  addNode: (parent, kind, title) => core.invoke('add_node', { parent: parent ?? null, kind, title }),
  renameNode: (id, title) => core.invoke('rename_node', { id, title }),
  moveNode: (id, newParent, index) => core.invoke('move_node', { id, newParent, index: index ?? null }),
  removeNode: (id) => core.invoke('remove_node', { id }),
  setSceneMeta: (id, meta) => core.invoke('set_scene_meta', { id, meta }),
  getScene: (id) => core.invoke('get_scene', { id }),
  openScene: (id) => core.invoke('open_scene', { id }),
  listFiles: () => core.invoke('list_files'),
  addEntity: (kind, name) => core.invoke('add_entity', { kind, name }),
  updateEntity: (id, patch) => core.invoke('update_entity', { id, patch: patch ?? {} }),
  removeEntity: (id) => core.invoke('remove_entity', { id }),
  loreSearch: (prefix, limit) => core.invoke('lore_search', { prefix, limit: limit ?? null }),
  getEntity: (id) => core.invoke('get_entity', { id }),
  getMentions: (bufferId) => core.invoke('get_mentions', { bufferId }),
  searchBuffer: (bufferId, query, caseSensitive) => core.invoke('search_buffer', { bufferId, query, caseSensitive: Boolean(caseSensitive) }),
  searchProject: (query, caseSensitive) => core.invoke('search_project', { query, caseSensitive: Boolean(caseSensitive) }),
  session: () => core.invoke('get_session'),
  setGoal: (words) => core.invoke('set_goal', { words }),
  exportManuscript: (format, path) => core.invoke('export_manuscript', { format, path: path ?? null }),
  history: (id) => core.invoke('get_history', { id }),
  restoreSnapshot: (id, name) => core.invoke('restore_snapshot', { id, name }),
  onCloseRequested: (handler) => {
    try {
      const api = window.__TAURI__ && window.__TAURI__.window;
      const win = api && typeof api.getCurrentWindow === 'function' ? api.getCurrentWindow() : null;
      if (win && typeof win.onCloseRequested === 'function') {
        return win.onCloseRequested((event) => handler(event));
      }
    } catch (err) {
      void err;
    }
    window.addEventListener('beforeunload', handler);
    return Promise.resolve(() => window.removeEventListener('beforeunload', handler));
  },
};
