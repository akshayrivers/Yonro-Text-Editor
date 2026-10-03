/* Yonro write view. Classic script; top-level fns are global for app.js.
 * Renders open buffers; never counts words (stats come from core).
 */

const docs = new Map(); // bufferId -> { path, dirty }
const docCache = new Map(); // bufferId -> last known text
let activeDoc = null;
let applyingRemote = false;
let lastStats = null;

const editor = document.getElementById('editor');
const docTabs = document.getElementById('doc-tabs');
const docName = document.getElementById('doc-name');
const docStatus = document.getElementById('doc-status');

function shortName(path) {
  if (!path) return 'untitled';
  const parts = path.split('/');
  return parts[parts.length - 1] || 'untitled';
}

function renderTabs() {
  docTabs.innerHTML = '';
  const ids = Array.from(docs.keys());
  ids.forEach((id, idx) => {
    const doc = docs.get(id);
    const tab = document.createElement('button');
    tab.className = 'tab' + (id === activeDoc ? ' active' : '');
    tab.setAttribute('role', 'tab');
    tab.setAttribute('aria-selected', id === activeDoc ? 'true' : 'false');
    tab.tabIndex = id === activeDoc ? 0 : -1;
    tab.dataset.doc = String(id);
    tab.innerHTML = `${esc(shortName(doc.path))}${doc.dirty ? ' <span class="dirty">●</span>' : ''}`;
    tab.setAttribute('aria-label', shortName(doc.path) + (doc.dirty ? ', unsaved' : ''));
    tab.addEventListener('click', () => activateDoc(id));
    tab.addEventListener('keydown', (e) => {
      if (e.key !== 'ArrowRight' && e.key !== 'ArrowLeft') return;
      e.preventDefault();
      const next = e.key === 'ArrowRight'
        ? ids[(idx + 1) % ids.length]
        : ids[(idx - 1 + ids.length) % ids.length];
      activateDoc(next);
      const btn = docTabs.querySelector(`[data-doc="${next}"]`);
      if (btn) btn.focus();
    });
    docTabs.appendChild(tab);
  });
  renderBinderFiles();
}

function renderBinderFiles() {
  const box = document.getElementById('binder-files');
  if (!box) return;
  box.innerHTML = '';
  if (docs.size === 0) {
    const p = document.createElement('p');
    p.className = 'muted';
    p.textContent = 'no open files. press new to start a draft.';
    box.appendChild(p);
    return;
  }
  for (const [id, doc] of docs) {
    const row = document.createElement('button');
    row.className = 'file-row' + (id === activeDoc ? ' active' : '');
    row.textContent = `${shortName(doc.path)}${doc.dirty ? ' ●' : ''}`;
    row.setAttribute('aria-label', shortName(doc.path));
    row.addEventListener('click', () => activateDoc(id));
    box.appendChild(row);
  }
}

function setStatusline(stats) {
  const stFile = document.getElementById('st-file');
  const stWords = document.getElementById('st-words');
  const stSession = document.getElementById('st-session');
  const stGoal = document.getElementById('st-goal');
  const stMin = document.getElementById('st-min');
  const doc = docs.get(activeDoc);
  if (stFile) stFile.textContent = doc ? `${shortName(doc.path)}${doc.dirty ? ' ●' : ''}` : '—';
  if (stWords) stWords.textContent = stats ? `${stats.words} w` : '—';
  if (stSession) stSession.textContent = '—';
  if (stGoal) stGoal.textContent = '—';
  if (stMin) stMin.textContent = stats ? `~${stats.reading_min} min` : '—';
  const zen = document.getElementById('zen-count');
  if (zen) zen.textContent = stats ? `${stats.words} words` : '';
}

function renderStatus(stats) {
  lastStats = stats || lastStats;
  const doc = docs.get(activeDoc);
  if (docName) {
    docName.textContent = doc
      ? `${shortName(doc.path)}${doc.dirty ? ' ●' : ''}`
      : 'No document — open one from Outline, or start typing below.';
  }
  if (docStatus) {
    docStatus.textContent = stats
      ? `${stats.words} words · ${stats.chars} chars · ${stats.lines} lines · ~${stats.reading_min} min${stats.dirty ? ' · modified' : ''}`
      : '';
  }
  setStatusline(stats || null);
  updateCaret();
}

function updateCaret() {
  const pos = document.getElementById('st-pos');
  if (!pos || !editor) return;
  try {
    const upto = editor.value.slice(0, editor.selectionStart ?? 0);
    const ln = upto.split('\n').length;
    const lastNl = upto.lastIndexOf('\n');
    const col = upto.length - lastNl;
    pos.textContent = `Ln ${ln}, Col ${col}`;
  } catch (err) {
    void err;
  }
}

async function openDoc(path) {
  try {
    const opened = await core.openFile(path);
    docs.set(opened.buffer_id, { path: opened.path, dirty: opened.stats.dirty });
    docCache.set(opened.buffer_id, opened.text);
    activeDoc = opened.buffer_id;
    applyingRemote = true;
    editor.value = opened.text;
    applyingRemote = false;
    renderTabs();
    renderStatus(opened.stats);
    show('write');
    if (typeof refreshBinder === 'function') refreshBinder();
  } catch (err) {
    setMessage(`could not open: ${err}`, { error: true });
  }
}

async function activateDoc(id) {
  if (!docs.has(id)) return;
  activeDoc = id;
  renderTabs();
  // Text was synced to the backend on every keystroke; the local cache is
  // the source of truth for instant tab switches.
  const cached = docCache.get(id);
  if (cached !== undefined) {
    applyingRemote = true;
    editor.value = cached;
    applyingRemote = false;
  }
  renderStatus(lastStats);
  const doc = docs.get(id);
  if (doc) {
    const stFile = document.getElementById('st-file');
    if (stFile) stFile.textContent = `${shortName(doc.path)}${doc.dirty ? ' ●' : ''}`;
  }
  editor.focus();
}

function cycleDoc(dir) {
  const ids = Array.from(docs.keys());
  if (ids.length < 2) return;
  const i = ids.indexOf(activeDoc);
  const next = ids[(i + dir + ids.length) % ids.length];
  activateDoc(next);
}

editor.addEventListener('input', async () => {
  if (applyingRemote || activeDoc === null) return;
  const text = editor.value;
  docCache.set(activeDoc, text);
  updateCaret();
  try {
    const stats = await core.setText(activeDoc, text);
    const doc = docs.get(activeDoc);
    if (doc) doc.dirty = stats.dirty;
    renderTabs();
    renderStatus(stats);
  } catch (err) {
    setMessage(`sync failed: ${err}`, { error: true });
  }
});

editor.addEventListener('click', updateCaret);
editor.addEventListener('keyup', updateCaret);
editor.addEventListener('select', updateCaret);

editor.addEventListener('keydown', async (e) => {
  if (e.key === 'Tab') {
    e.preventDefault();
    const { selectionStart: s, selectionEnd: t } = editor;
    editor.setRangeText('\t', s, t, 'end');
    editor.dispatchEvent(new Event('input'));
  }
  if (isMod(e) && e.key.toLowerCase() === 's') {
    e.preventDefault();
    await saveActive();
  }
  if (isMod(e) && e.key.toLowerCase() === 'z' && !e.shiftKey) {
    e.preventDefault();
    await historyStep('undo');
  }
  if (isMod(e) && (e.key.toLowerCase() === 'y' || (e.key.toLowerCase() === 'z' && e.shiftKey))) {
    e.preventDefault();
    await historyStep('redo');
  }
});

async function historyStep(which) {
  if (activeDoc === null) return;
  try {
    const res = await core[which](activeDoc);
    applyingRemote = true;
    editor.value = res.text;
    applyingRemote = false;
    docCache.set(activeDoc, res.text);
    const doc = docs.get(activeDoc);
    if (doc) doc.dirty = res.stats.dirty;
    renderTabs();
    renderStatus(res.stats);
  } catch (err) {
    setMessage(`history failed: ${err}`, { error: true });
  }
}

async function saveActive() {
  if (activeDoc === null) return;
  try {
    // No path argument: the backend saves in place, or auto-names
    // `untitled-<id>.md` inside the workspace for new drafts.
    const saved = await core.saveFile(activeDoc, null);
    const doc = docs.get(activeDoc);
    if (doc) {
      doc.path = saved;
      doc.dirty = false;
    }
    renderTabs();
    renderStatus(lastStats);
    setMessage(`saved ${shortName(saved)}`);
    if (typeof refreshBinder === 'function') refreshBinder();
  } catch (err) {
    setMessage(`save failed: ${err}`, { error: true });
  }
}

document.getElementById('btn-save').addEventListener('click', saveActive);
document.getElementById('btn-undo').addEventListener('click', () => historyStep('undo'));
document.getElementById('btn-redo').addEventListener('click', () => historyStep('redo'));
document.getElementById('btn-new').addEventListener('click', () => openDoc(null));
document.getElementById('btn-close').addEventListener('click', async () => {
  if (activeDoc === null) return;
  const doc = docs.get(activeDoc);
  if (doc && doc.dirty && !window.confirm('Close without saving?')) return;
  try {
    await core.closeBuffer(activeDoc);
  } catch (err) {
    setMessage(`close failed: ${err}`, { error: true });
    return;
  }
  docs.delete(activeDoc);
  docCache.delete(activeDoc);
  activeDoc = docs.size ? [...docs.keys()].pop() : null;
  if (activeDoc !== null) {
    applyingRemote = true;
    editor.value = docCache.get(activeDoc) ?? '';
    applyingRemote = false;
  } else {
    editor.value = '';
    lastStats = null;
  }
  renderTabs();
  renderStatus(null);
  if (typeof refreshBinder === 'function') refreshBinder();
});
