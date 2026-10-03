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
  saveFile: (bufferId, path, overwrite) => core.invoke('save_file', { bufferId, path: path ?? null, overwrite: overwrite ?? false }),
  closeBuffer: (bufferId) => core.invoke('close_buffer', { bufferId }),
  undo: (bufferId) => core.invoke('undo_buffer', { bufferId }),
  redo: (bufferId) => core.invoke('redo_buffer', { bufferId }),
  sweepRecovery: () => core.invoke('sweep_recovery'),
  checkRecovery: (path) => core.invoke('check_recovery', { path: path ?? null }),
  discardRecovery: (path) => core.invoke('discard_recovery', { path: path ?? null }),
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
