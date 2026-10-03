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
  graph: () => core.invoke('get_graph'),
  timeline: () => core.invoke('get_timeline'),
  openFile: (path) => core.invoke('open_file', { path: path ?? null }),
  setText: (bufferId, text) => core.invoke('set_text', { bufferId, text }),
  saveFile: (bufferId, path) => core.invoke('save_file', { bufferId, path: path ?? null }),
  closeBuffer: (bufferId) => core.invoke('close_buffer', { bufferId }),
  undo: (bufferId) => core.invoke('undo_buffer', { bufferId }),
  redo: (bufferId) => core.invoke('redo_buffer', { bufferId }),
};
