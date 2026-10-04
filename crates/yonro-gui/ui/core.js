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
  graph: (query, lens) => core.invoke('get_graph', { query: query ?? null, lens: lens ?? null }),
  lenses: () => core.invoke('list_lenses'),
  presence: (lens) => core.invoke('get_presence', { lens: lens ?? null }),
  saveLens: (name, lens) => core.invoke('save_lens', { name, lens }),
  deleteLens: (name) => core.invoke('delete_lens', { name }),
  timeline: () => core.invoke('get_timeline'),
  customGraphs: () => core.invoke('list_custom_graphs'),
  createCustomGraph: (title) => core.invoke('create_custom_graph', { title }),
  renameCustomGraph: (id, title) => core.invoke('rename_custom_graph', { id, title }),
  deleteCustomGraph: (id) => core.invoke('delete_custom_graph', { id }),
  addGraphNode: (id, label, x, y) => core.invoke('add_graph_node', { id, label, x, y }),
  moveGraphNode: (id, node, x, y) => core.invoke('move_graph_node', { id, node, x, y }),
  renameGraphNode: (id, node, label) => core.invoke('rename_graph_node', { id, node, label }),
  removeGraphNode: (id, node) => core.invoke('remove_graph_node', { id, node }),
  addGraphEdge: (id, a, b, label) => core.invoke('add_graph_edge', { id, a, b, label }),
  removeGraphEdge: (id, edge) => core.invoke('remove_graph_edge', { id, edge }),
  altOutlines: () => core.invoke('list_alt_outlines'),
  createAltOutline: (title) => core.invoke('create_alt_outline', { title }),
  renameAltOutline: (id, title) => core.invoke('rename_alt_outline', { id, title }),
  deleteAltOutline: (id) => core.invoke('delete_alt_outline', { id }),
  altAddNode: (outline, parent, kind, title) => core.invoke('alt_add_node', { outline, parent: parent ?? null, kind, title }),
  altRenameNode: (outline, id, title) => core.invoke('alt_rename_node', { outline, id, title }),
  altMoveNode: (outline, id, newParent, index) => core.invoke('alt_move_node', { outline, id, newParent, index: index ?? null }),
  altRemoveNode: (outline, id) => core.invoke('alt_remove_node', { outline, id }),
  altOpenScene: (outline, id) => core.invoke('alt_open_scene', { outline, id }),
  altOutline: (outline) => core.invoke('alt_get_outline', { outline }),
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
