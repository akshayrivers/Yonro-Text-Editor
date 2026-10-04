/* Yonro write view. Classic script; top-level fns are global for app.js.
 * Renders open buffers; never counts words (stats come from core).
 */

const docs = new Map(); // bufferId -> { path, dirty }
const docCache = new Map(); // bufferId -> last known text
const docStats = new Map(); // bufferId -> last TextStats
let activeDoc = null;
let applyingRemote = false;
let lastStats = null;

/* Debounced sync: local dirty first, backend trailing 120ms. */
let syncTimer = null;
let syncPending = false;
let typewriterRaf = 0;
let autosaveTimer = null;
let lastEditAt = 0;

function getAutosave() {
  try {
    const v = localStorage.getItem('yonro.autosave');
    if (v === null) return true;
    return v !== '0';
  } catch (err) {
    void err;
    return true;
  }
}

function getTypewriter() {
  try {
    const v = localStorage.getItem('yonro.typewriter');
    if (v !== null) return v === '1';
  } catch (err) {
    void err;
  }
  return document.body.classList.contains('zen');
}

function setTypewriter(on) {
  try {
    localStorage.setItem('yonro.typewriter', on ? '1' : '0');
  } catch (err) {
    void err;
  }
  refreshTypewriterToggle();
  centerCaretSoon();
}

function fullPathOf(doc) {
  return (doc && doc.path) ? doc.path : 'untitled';
}

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
    const label = shortName(doc.path);
    const tab = document.createElement('button');
    tab.className = 'tab' + (id === activeDoc ? ' active' : '');
    tab.setAttribute('role', 'tab');
    tab.setAttribute('aria-selected', id === activeDoc ? 'true' : 'false');
    tab.tabIndex = id === activeDoc ? 0 : -1;
    tab.dataset.doc = String(id);
    tab.title = fullPathOf(doc);
    const name = document.createElement('span');
    name.textContent = label;
    tab.appendChild(name);
    if (doc.dirty) {
      const dot = document.createElement('span');
      dot.className = 'dirty';
      dot.textContent = ' ●';
      dot.setAttribute('aria-hidden', 'true');
      tab.appendChild(dot);
    }
    const close = document.createElement('span');
    close.className = 'tab-x';
    close.textContent = ' ×';
    close.setAttribute('role', 'button');
    close.setAttribute('aria-label', `close ${label}`);
    close.tabIndex = -1;
    close.addEventListener('click', (e) => {
      e.stopPropagation();
      closeDoc(id);
    });
    tab.appendChild(close);
    tab.setAttribute('aria-label', label + (doc.dirty ? ', unsaved' : ''));
    tab.addEventListener('click', () => activateDoc(id));
    tab.addEventListener('auxclick', (e) => {
      if (e.button === 1) {
        e.preventDefault();
        closeDoc(id);
      }
    });
    tab.addEventListener('keydown', (e) => {
      if (e.key === 'Delete' || e.key === 'Backspace') {
        e.preventDefault();
        closeDoc(id);
        return;
      }
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
}

function isDocDirty(path) {
  if (!path) return false;
  for (const doc of docs.values()) {
    if (doc.path === path && doc.dirty) return true;
  }
  return false;
}

const scheduleBinderRefresh = debounce(() => {
  if (typeof refreshBinder === 'function') refreshBinder();
}, 800);

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
  updateUndoRedoButtons(stats);
}

function updateUndoRedoButtons(stats) {
  const undo = document.getElementById('btn-undo');
  const redo = document.getElementById('btn-redo');
  const s = stats || (activeDoc !== null ? docStats.get(activeDoc) : null);
  if (undo) undo.disabled = !(s && s.can_undo);
  if (redo) redo.disabled = !(s && s.can_redo);
}

function renderStatus(stats) {
  if (stats) {
    lastStats = stats;
    if (activeDoc !== null) docStats.set(activeDoc, stats);
  }
  const effective = stats || (activeDoc !== null ? docStats.get(activeDoc) : null) || lastStats;
  const doc = docs.get(activeDoc);
  if (docName) {
    docName.textContent = doc
      ? `${shortName(doc.path)}${doc.dirty ? ' ●' : ''}`
      : 'no document open.';
  }
  if (docStatus) {
    if (!doc) {
      docStatus.textContent = '';
    } else if (!doc.path) {
      docStatus.textContent = 'untitled · unsaved · mod+S';
    } else if (effective) {
      docStatus.textContent = `${effective.words} words · ${effective.chars} chars · ${effective.lines} lines · ~${effective.reading_min} min${doc.dirty ? ' · modified' : ''}`;
    } else {
      docStatus.textContent = doc.dirty ? 'modified' : '';
    }
  }
  setStatusline(effective || null);
  updateCaret();
  updateEmptyState();
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
  centerCaretSoon();
}

function updateEmptyState() {
  const empty = document.getElementById('empty-state');
  if (!empty || !editor) return;
  const hasDoc = activeDoc !== null && docs.has(activeDoc);
  empty.hidden = hasDoc;
  const wrap = document.getElementById('editor-wrap');
  if (wrap) wrap.style.display = hasDoc ? '' : 'none';
  const bar = document.getElementById('editor-bar');
  if (bar) bar.style.display = hasDoc ? '' : 'none';
  const tabs = document.getElementById('doc-tabs');
  if (tabs) tabs.style.display = docs.size ? '' : 'none';
}

function ensureEmptyState() {
  if (document.getElementById('empty-state')) return;
  const view = document.getElementById('view-write');
  if (!view || !editor) return;
  const card = document.createElement('div');
  card.id = 'empty-state';
  card.style.maxWidth = '36ch';
  card.style.margin = '12vh auto';
  card.style.textAlign = 'center';
  const p = document.createElement('p');
  p.textContent = 'no document open.';
  card.appendChild(p);
  const row = document.createElement('div');
  row.style.display = 'flex';
  row.style.gap = '8px';
  row.style.justifyContent = 'center';
  const openBtn = document.createElement('button');
  openBtn.textContent = 'open a scene — mod+O';
  openBtn.addEventListener('click', () => toggleBinder());
  const newBtn = document.createElement('button');
  newBtn.textContent = 'new draft — mod+N';
  newBtn.addEventListener('click', () => openDoc(null));
  row.appendChild(openBtn);
  row.appendChild(newBtn);
  card.appendChild(row);
  const wrap = document.getElementById('editor-wrap');
  view.insertBefore(card, wrap || editor);
}

/* Typewriter: keep the caret line near 45% of the textarea height. */
function ensureMirror() {
  let mirror = document.getElementById('typewriter-mirror');
  if (mirror) return mirror;
  mirror = document.createElement('div');
  mirror.id = 'typewriter-mirror';
  mirror.setAttribute('aria-hidden', 'true');
  const cs = getComputedStyle(editor);
  mirror.style.position = 'absolute';
  mirror.style.visibility = 'hidden';
  mirror.style.whiteSpace = 'pre-wrap';
  mirror.style.wordBreak = 'break-word';
  mirror.style.overflowWrap = 'break-word';
  document.body.appendChild(mirror);
  return mirror;
}

function syncMirrorStyle(mirror) {
  const cs = getComputedStyle(editor);
  mirror.style.width = `${editor.clientWidth}px`;
  mirror.style.font = cs.font;
  mirror.style.lineHeight = cs.lineHeight;
  mirror.style.padding = cs.padding;
  mirror.style.border = cs.border;
  mirror.style.letterSpacing = cs.letterSpacing;
  mirror.style.textTransform = cs.textTransform;
}

function centerCaretSoon() {
  if (typewriterRaf) return;
  typewriterRaf = requestAnimationFrame(() => {
    typewriterRaf = 0;
    centerCaretNow();
  });
}

function centerCaretNow() {
  try {
    if (!getTypewriter()) return;
    const wrap = document.getElementById('editor-wrap');
    if (activeDoc === null || (wrap && wrap.style.display === 'none')) return;
    if (document.activeElement !== editor) return;
    const mirror = ensureMirror();
    syncMirrorStyle(mirror);
    mirror.textContent = '';
    const before = editor.value.slice(0, editor.selectionStart ?? 0);
    mirror.appendChild(document.createTextNode(before));
    const span = document.createElement('span');
    span.textContent = 'x';
    mirror.appendChild(span);
    const caretTop = span.offsetTop;
    const target = Math.round(editor.clientHeight * 0.45);
    editor.scrollTop = Math.max(0, caretTop - target);
  } catch (err) {
    void err;
  }
}

function refreshTypewriterToggle() {
  const btn = document.getElementById('typewriter-toggle');
  if (!btn) return;
  const on = getTypewriter();
  btn.textContent = on ? 'typewriter: on' : 'typewriter: off';
  btn.setAttribute('aria-pressed', on ? 'true' : 'false');
}

function ensureTypewriterToggle() {
  if (document.getElementById('typewriter-toggle')) return;
  const bar = document.getElementById('statusline');
  if (!bar) return;
  const btn = document.createElement('button');
  btn.id = 'typewriter-toggle';
  btn.title = 'typewriter mode: keep caret centered';
  btn.addEventListener('click', () => setTypewriter(!getTypewriter()));
  bar.appendChild(btn);
  refreshTypewriterToggle();
}

function markDirtyLocal(id) {
  const doc = docs.get(id);
  if (!doc || doc.dirty) return false;
  doc.dirty = true;
  renderTabs();
  renderStatus(null);
  return true;
}

/* @mention highlight (P4.5): spans are core-computed with UTF-16 offsets
 * (JS string indices); JS only slices and wraps them in <mark>. Refresh is
 * debounced 200ms after typing stops; marks hide via .stale while typing.
 */
let mentionSpans = [];
let mentionSeq = 0;
let mentionRefreshFn = null;

function editorBackdrop() {
  return document.getElementById('editor-backdrop');
}

function syncBackdropScroll() {
  const bd = editorBackdrop();
  if (bd && editor) {
    bd.scrollTop = editor.scrollTop;
    bd.scrollLeft = editor.scrollLeft;
  }
}

function markMentionsStale() {
  const bd = editorBackdrop();
  if (bd) bd.classList.add('stale');
  if (!mentionRefreshFn) mentionRefreshFn = debounce(refreshMentionsNow, 200);
  mentionRefreshFn();
}

async function refreshMentionsNow() {
  if (activeDoc === null || !editor) {
    renderMentions([]);
    return;
  }
  const id = activeDoc;
  const seq = ++mentionSeq;
  let spans = [];
  try {
    spans = await core.getMentions(id);
  } catch (err) {
    void err;
    spans = [];
  }
  if (seq !== mentionSeq || id !== activeDoc) return;
  renderMentions(spans);
}

function renderMentions(spans) {
  mentionSpans = Array.isArray(spans) ? spans : [];
  const bd = editorBackdrop();
  if (!bd || !editor) return;
  bd.classList.remove('stale');
  const text = editor.value;
  const sorted = mentionSpans.slice().sort((a, b) => a.start - b.start);
  let html = '';
  let at = 0;
  for (const m of sorted) {
    const start = Math.max(0, Math.min(m.start, text.length));
    const end = Math.max(start, Math.min(m.end, text.length));
    if (start < at) continue;
    html += esc(text.slice(at, start));
    const kind = String(m.kind || 'lore').toLowerCase();
    const ent = (m.entity_id === null || m.entity_id === undefined)
      ? ''
      : ` data-entity="${m.entity_id}"`;
    html += `<mark class="mention k-${esc(kind)}"${ent}>${esc(text.slice(start, end))}</mark>`;
    at = end;
  }
  html += esc(text.slice(at));
  if (text.endsWith('\n')) html += ' ';
  bd.innerHTML = html;
  syncBackdropScroll();
}

function mentionAtCaret() {
  if (!editor || activeDoc === null) return null;
  const caret = editor.selectionStart;
  if (caret === null || caret === undefined || editor.selectionEnd !== caret) return null;
  for (const m of mentionSpans) {
    if (m.entity_id !== null && m.entity_id !== undefined && caret >= m.start && caret < m.end) {
      return m;
    }
  }
  return null;
}

function showMentionAtCaret() {
  const m = mentionAtCaret();
  if (m && typeof showEntityInspector === 'function') showEntityInspector(m.entity_id);
}

/* @mention autocomplete (P4.5b): `@` + chars before the caret (UI-only
 * regex approximating core is_mention_char) → core.loreSearch(prefix, 8).
 * Popup anchors near the caret via the typewriter mirror; fallback pins it
 * to the bottom-left of the editor wrap (above the statusline).
 */
let mentionPopup = null;
let mentionSearchSeq = 0;

function mentionQueryAtCaret() {
  if (!editor || activeDoc === null) return null;
  const caret = editor.selectionStart;
  if (caret === null || caret === undefined || editor.selectionEnd !== caret) return null;
  const before = editor.value.slice(0, caret);
  const m = before.match(/@([\p{L}\p{N}_'.\-]*)$/u);
  if (!m) return null;
  return { prefix: m[1], start: caret - m[0].length };
}

function ensureMentionPopup() {
  let pop = document.getElementById('mention-popup');
  if (pop) return pop;
  pop = document.createElement('div');
  pop.id = 'mention-popup';
  pop.setAttribute('role', 'listbox');
  pop.setAttribute('aria-label', 'mention suggestions');
  pop.hidden = true;
  const wrap = document.getElementById('editor-wrap');
  (wrap || document.body).appendChild(pop);
  return pop;
}

function hideMentionPopup() {
  const pop = document.getElementById('mention-popup');
  if (pop) pop.hidden = true;
  mentionPopup = null;
  if (editor) editor.removeAttribute('aria-activedescendant');
}

function caretPopupAnchor() {
  try {
    const mirror = ensureMirror();
    syncMirrorStyle(mirror);
    mirror.textContent = '';
    mirror.appendChild(document.createTextNode(editor.value.slice(0, editor.selectionStart ?? 0)));
    const span = document.createElement('span');
    span.textContent = 'x';
    mirror.appendChild(span);
    const cs = getComputedStyle(editor);
    const parsed = parseFloat(cs.lineHeight);
    const lineH = Number.isFinite(parsed) ? parsed : 24;
    return { x: span.offsetLeft, y: span.offsetTop, h: span.offsetHeight || lineH, lineH };
  } catch (err) {
    void err;
    return null;
  }
}

async function maybeShowMentionPopup() {
  const q = mentionQueryAtCaret();
  if (!q) {
    hideMentionPopup();
    return;
  }
  const seq = ++mentionSearchSeq;
  let items = [];
  try {
    items = await core.loreSearch(q.prefix, 8);
  } catch (err) {
    void err;
    hideMentionPopup();
    return;
  }
  if (seq !== mentionSearchSeq) return;
  const now = mentionQueryAtCaret();
  if (!now || now.prefix !== q.prefix) return;
  if (!items || !items.length) {
    hideMentionPopup();
    return;
  }
  mentionPopup = { prefix: q.prefix, start: q.start, items, active: 0 };
  renderMentionPopup();
}

function renderMentionPopup() {
  const pop = ensureMentionPopup();
  if (!mentionPopup || !mentionPopup.items.length) {
    hideMentionPopup();
    return;
  }
  pop.innerHTML = '';
  mentionPopup.items.forEach((item, i) => {
    const opt = document.createElement('div');
    opt.id = `mention-opt-${i}`;
    opt.setAttribute('role', 'option');
    opt.setAttribute('aria-selected', i === mentionPopup.active ? 'true' : 'false');
    opt.className = 'mention-opt' + (i === mentionPopup.active ? ' active' : '');
    const name = document.createElement('span');
    name.textContent = `@${item.name}`;
    opt.appendChild(name);
    const kind = document.createElement('span');
    kind.className = 'muted';
    kind.textContent = ` ${item.kind}`;
    opt.appendChild(kind);
    if (item.matched_alias) {
      const alias = document.createElement('span');
      alias.className = 'muted';
      alias.textContent = ` · ${item.matched_alias}`;
      opt.appendChild(alias);
    }
    opt.addEventListener('mousedown', (e) => {
      e.preventDefault();
      if (mentionPopup) mentionPopup.active = i;
      commitMentionPopup();
    });
    pop.appendChild(opt);
  });
  editor.setAttribute('aria-activedescendant', `mention-opt-${mentionPopup.active}`);
  const wrap = document.getElementById('editor-wrap');
  const wrapW = wrap ? wrap.clientWidth : 600;
  const wrapH = wrap ? wrap.clientHeight : 400;
  const anchor = caretPopupAnchor();
  if (anchor) {
    const x = Math.max(8, Math.min(anchor.x - editor.scrollLeft, wrapW - 240));
    let y = anchor.y - editor.scrollTop + anchor.lineH + 4;
    if (y > wrapH - 60) y = Math.max(8, anchor.y - editor.scrollTop - 150);
    pop.style.left = `${x}px`;
    pop.style.top = `${y}px`;
  } else {
    pop.style.left = '8px';
    pop.style.top = `${Math.max(8, wrapH - 160)}px`;
  }
  pop.hidden = false;
}

function commitMentionPopup() {
  if (!mentionPopup || !editor) return;
  const item = mentionPopup.items[mentionPopup.active];
  if (!item) {
    hideMentionPopup();
    return;
  }
  const caret = editor.selectionStart ?? 0;
  editor.setRangeText(`@${item.name}`, mentionPopup.start, caret, 'end');
  hideMentionPopup();
  editor.dispatchEvent(new Event('input'));
  editor.focus();
}

/* Popup-first keys. Returns true when the key was consumed. */
function handleMentionPopupKey(e) {
  if (!mentionPopup) return false;
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    e.preventDefault();
    e.stopPropagation();
    const n = mentionPopup.items.length;
    mentionPopup.active = e.key === 'ArrowDown'
      ? (mentionPopup.active + 1) % n
      : (mentionPopup.active - 1 + n) % n;
    renderMentionPopup();
    return true;
  }
  if (e.key === 'Enter') {
    e.preventDefault();
    e.stopPropagation();
    commitMentionPopup();
    return true;
  }
  if (e.key === 'Tab') {
    e.preventDefault();
    e.stopPropagation();
    const item = mentionPopup.items[mentionPopup.active];
    if (item && typeof showEntityInspector === 'function') showEntityInspector(item.id);
    return true;
  }
  if (e.key === 'Escape') {
    e.preventDefault();
    e.stopPropagation();
    hideMentionPopup();
    return true;
  }
  return false;
}

function scheduleSync() {
  syncPending = true;
  lastEditAt = Date.now();
  if (syncTimer !== null) clearTimeout(syncTimer);
  syncTimer = setTimeout(() => {
    syncTimer = null;
    flushSync();
  }, 120);
  scheduleAutosave();
}

async function flushSync() {
  if (syncTimer !== null) {
    clearTimeout(syncTimer);
    syncTimer = null;
  }
  if (!syncPending || activeDoc === null) {
    syncPending = false;
    return;
  }
  const id = activeDoc;
  const text = docCache.get(id) ?? editor.value;
  syncPending = false;
  try {
    const stats = await core.setText(id, text);
    const doc = docs.get(id);
    if (doc) {
      const flipped = doc.dirty !== stats.dirty;
      doc.dirty = stats.dirty;
      if (flipped) renderTabs();
    }
    renderStatus(stats);
    scheduleBinderRefresh();
  } catch (err) {
    setMessage(`sync failed: ${err}`, { error: true });
  }
}

function scheduleAutosave() {
  if (autosaveTimer !== null) {
    clearTimeout(autosaveTimer);
    autosaveTimer = null;
  }
  if (!getAutosave()) return;
  autosaveTimer = setTimeout(() => {
    autosaveTimer = null;
    autosaveTick();
  }, 3000);
}

async function autosaveTick() {
  if (!getAutosave() || activeDoc === null) return;
  if (syncPending) await flushSync();
  const doc = docs.get(activeDoc);
  if (!doc || !doc.dirty || !doc.path) return;
  try {
    const saved = await core.saveFile(activeDoc, doc.path, false);
    doc.path = saved;
    doc.dirty = false;
    renderTabs();
    renderStatus(null);
    const now = new Date();
    const hh = String(now.getHours()).padStart(2, '0');
    const mm = String(now.getMinutes()).padStart(2, '0');
    setMessage(`autosaved ${shortName(saved)} · ${hh}:${mm}`);
    if (typeof refreshBinder === 'function') refreshBinder();
  } catch (err) {
    setMessage(`autosave failed ${shortName(doc.path)}: ${err}`, { error: true });
  }
}

async function sweepRecoveryTick() {
  try {
    await core.sweepRecovery();
  } catch (err) {
    void err;
  }
}

function adoptOpened(opened) {
  docs.set(opened.buffer_id, { path: opened.path, dirty: opened.stats.dirty });
  docCache.set(opened.buffer_id, opened.text);
  docStats.set(opened.buffer_id, opened.stats);
  activeDoc = opened.buffer_id;
  applyingRemote = true;
  editor.value = opened.text;
  applyingRemote = false;
  syncPending = false;
  renderTabs();
  renderStatus(opened.stats);
  show('write');
  hideMentionPopup();
  refreshMentionsNow();
  if (typeof refreshBinder === 'function') refreshBinder();
  if (opened.path) maybeOfferRecovery(opened.buffer_id, opened.path);
}

async function openDoc(path) {
  try {
    await flushSync();
    const opened = await core.openFile(path);
    adoptOpened(opened);
  } catch (err) {
    setMessage(`could not open: ${err}`, { error: true });
  }
}

async function openSceneDoc(id) {
  try {
    await flushSync();
    const opened = await core.openScene(id);
    adoptOpened(opened);
  } catch (err) {
    setMessage(`could not open scene: ${err}`, { error: true });
  }
}

async function activateDoc(id) {
  if (!docs.has(id)) return;
  if (id !== activeDoc) await flushSync();
  activeDoc = id;
  renderTabs();
  // Local cache is instant; the debounced backend sync follows on next edit.
  const cached = docCache.get(id);
  if (cached !== undefined) {
    applyingRemote = true;
    editor.value = cached;
    applyingRemote = false;
  }
  syncPending = false;
  renderStatus(docStats.get(id) || lastStats);
  hideMentionPopup();
  refreshMentionsNow();
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

editor.addEventListener('input', () => {
  if (applyingRemote || activeDoc === null) return;
  const text = editor.value;
  docCache.set(activeDoc, text);
  markDirtyLocal(activeDoc);
  updateCaret();
  scheduleSync();
  markMentionsStale();
  maybeShowMentionPopup();
});

editor.addEventListener('click', () => {
  updateCaret();
  showMentionAtCaret();
});
editor.addEventListener('keyup', (e) => {
  updateCaret();
  if (e && e.key && (e.key.indexOf('Arrow') === 0 || e.key === 'Home' || e.key === 'End')) {
    showMentionAtCaret();
    if (!mentionPopup) maybeShowMentionPopup();
  }
});
editor.addEventListener('scroll', syncBackdropScroll, { passive: true });
editor.addEventListener('select', updateCaret);
document.addEventListener('selectionchange', () => {
  if (document.activeElement === editor) centerCaretSoon();
});

editor.addEventListener('keydown', async (e) => {
  if (mentionPopup && handleMentionPopupKey(e)) return;
  if (e.key === 'Tab') {
    e.preventDefault();
    const { selectionStart: s, selectionEnd: t } = editor;
    editor.setRangeText('\t', s, t, 'end');
    editor.dispatchEvent(new Event('input'));
  }
  if (isMod(e) && e.key.toLowerCase() === 's') {
    e.preventDefault();
    if (e.shiftKey) await saveAsFlow();
    else await saveActive();
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
  await flushSync();
  try {
    const res = await core[which](activeDoc);
    applyingRemote = true;
    editor.value = res.text;
    applyingRemote = false;
    docCache.set(activeDoc, res.text);
    syncPending = false;
    const doc = docs.get(activeDoc);
    if (doc) doc.dirty = res.stats.dirty;
    renderTabs();
    renderStatus(res.stats);
    refreshMentionsNow();
    scheduleBinderRefresh();
  } catch (err) {
    setMessage(`history failed: ${err}`, { error: true });
  }
}

function stampNow() {
  const now = new Date();
  const hh = String(now.getHours()).padStart(2, '0');
  const mm = String(now.getMinutes()).padStart(2, '0');
  return `${hh}:${mm}`;
}

async function saveActive() {
  if (activeDoc === null) return;
  const doc = docs.get(activeDoc);
  if (doc && !doc.path) {
    await saveAsFlow();
    return;
  }
  await flushSync();
  try {
    const saved = await core.saveFile(activeDoc, doc ? doc.path : null, false);
    if (doc) {
      doc.path = saved;
      doc.dirty = false;
    }
    renderTabs();
    renderStatus(null);
    setMessage(`saved ${shortName(saved)} · ${stampNow()}`);
    if (typeof refreshBinder === 'function') refreshBinder();
  } catch (err) {
    setMessage(`save failed: ${err}`, { error: true });
  }
}

function ensureDialog(id, title) {
  let dlg = document.getElementById(id);
  if (dlg) return dlg;
  dlg = document.createElement('dialog');
  dlg.id = id;
  const h = document.createElement('h2');
  h.textContent = title;
  dlg.appendChild(h);
  document.body.appendChild(dlg);
  return dlg;
}

async function saveAsFlow() {
  if (activeDoc === null) return false;
  const targetId = activeDoc;
  const doc = docs.get(targetId);
  await flushSync();
  const dlg = ensureDialog('saveas-dialog', 'save as');
  dlg.innerHTML = '';
  const h = document.createElement('h2');
  h.textContent = 'save as';
  dlg.appendChild(h);
  const label = document.createElement('label');
  label.textContent = 'filename inside workspace (.md/.txt)';
  const input = document.createElement('input');
  input.type = 'text';
  input.value = doc && doc.path ? shortName(doc.path) : `untitled-${targetId}.md`;
  label.appendChild(input);
  dlg.appendChild(label);
  const owLabel = document.createElement('label');
  const ow = document.createElement('input');
  ow.type = 'checkbox';
  owLabel.appendChild(ow);
  owLabel.appendChild(document.createTextNode('overwrite existing file'));
  dlg.appendChild(owLabel);
  const err = document.createElement('p');
  err.className = 'muted';
  err.setAttribute('aria-live', 'polite');
  dlg.appendChild(err);
  const row = document.createElement('div');
  const saveBtn = document.createElement('button');
  saveBtn.textContent = 'save';
  const cancelBtn = document.createElement('button');
  cancelBtn.textContent = 'cancel';
  row.appendChild(saveBtn);
  row.appendChild(cancelBtn);
  dlg.appendChild(row);
  return new Promise((resolve) => {
    cancelBtn.addEventListener('click', () => { dlg.close(); resolve(false); }, { once: true });
    dlg.addEventListener('cancel', () => resolve(false), { once: true });
    saveBtn.addEventListener('click', async () => {
      const name = input.value.trim();
      try {
        const saved = await core.saveFile(targetId, name, ow.checked);
        const d = docs.get(targetId);
        if (d) {
          d.path = saved;
          d.dirty = false;
        }
        if (targetId === activeDoc) {
          renderTabs();
          renderStatus(null);
        }
        setMessage(`saved ${shortName(saved)} · ${stampNow()}`);
        if (typeof refreshBinder === 'function') refreshBinder();
        dlg.close();
        resolve(true);
      } catch (e) {
        err.textContent = `cannot save ${name}: ${e}`;
      }
    });
    if (typeof dlg.showModal === 'function') dlg.showModal();
    input.focus();
    input.select();
  });
}

function confirmCloseDoc(id) {
  const doc = docs.get(id);
  if (!doc || !doc.dirty) return Promise.resolve('discard');
  const dlg = ensureDialog('close-dialog', 'unsaved changes');
  dlg.innerHTML = '';
  const h = document.createElement('h2');
  h.textContent = 'unsaved changes';
  dlg.appendChild(h);
  const p = document.createElement('p');
  p.textContent = `${shortName(doc.path)} has unsaved changes.`;
  dlg.appendChild(p);
  const row = document.createElement('div');
  const saveBtn = document.createElement('button');
  saveBtn.textContent = 'save';
  const discardBtn = document.createElement('button');
  discardBtn.textContent = 'discard';
  const cancelBtn = document.createElement('button');
  cancelBtn.textContent = 'cancel';
  row.appendChild(saveBtn);
  row.appendChild(discardBtn);
  row.appendChild(cancelBtn);
  dlg.appendChild(row);
  return new Promise((resolve) => {
    saveBtn.addEventListener('click', () => { dlg.close(); resolve('save'); }, { once: true });
    discardBtn.addEventListener('click', () => { dlg.close(); resolve('discard'); }, { once: true });
    cancelBtn.addEventListener('click', () => { dlg.close(); resolve('cancel'); }, { once: true });
    dlg.addEventListener('cancel', () => resolve('cancel'), { once: true });
    if (typeof dlg.showModal === 'function') dlg.showModal();
    saveBtn.focus();
  });
}

async function closeDoc(id) {
  if (!docs.has(id)) return;
  const wasActive = id === activeDoc;
  if (id === activeDoc) await flushSync();
  const choice = await confirmCloseDoc(id);
  if (choice === 'cancel') return;
  if (choice === 'save') {
    const doc = docs.get(id);
    try {
      if (doc && !doc.path) {
        const prev = activeDoc;
        activeDoc = id;
        applyingRemote = true;
        const cached = docCache.get(id);
        if (cached !== undefined) editor.value = cached;
        applyingRemote = false;
        const saved = await saveAsFlow();
        if (prev !== id && docs.has(prev)) {
          activeDoc = prev;
          const back = docCache.get(prev);
          applyingRemote = true;
          editor.value = back ?? '';
          applyingRemote = false;
        }
        if (!saved) return;
        if (docs.get(id) && docs.get(id).dirty) return;
      } else {
        await core.saveFile(id, doc ? doc.path : null, false);
        if (doc) doc.dirty = false;
      }
    } catch (err) {
      setMessage(`save failed: ${err}`, { error: true });
      return;
    }
  }
  try {
    await core.closeBuffer(id);
  } catch (err) {
    setMessage(`close failed: ${err}`, { error: true });
    return;
  }
  docs.delete(id);
  docCache.delete(id);
  docStats.delete(id);
  if (wasActive) {
    activeDoc = docs.size ? [...docs.keys()].pop() : null;
    if (activeDoc !== null) {
      applyingRemote = true;
      editor.value = docCache.get(activeDoc) ?? '';
      applyingRemote = false;
    } else {
      editor.value = '';
      lastStats = null;
      renderMentions([]);
    }
    syncPending = false;
  }
  renderTabs();
  renderStatus(activeDoc !== null ? docStats.get(activeDoc) || null : null);
  if (typeof refreshBinder === 'function') refreshBinder();
}

async function maybeOfferRecovery(bufferId, path) {
  let info = null;
  try {
    info = await core.checkRecovery(path);
  } catch (err) {
    void err;
    return;
  }
  if (!info || !info.newer) return;
  const dlg = ensureDialog('recovery-dialog', 'crash recovery');
  dlg.innerHTML = '';
  const h = document.createElement('h2');
  h.textContent = 'crash recovery';
  dlg.appendChild(h);
  const p = document.createElement('p');
  p.textContent = `a newer autosave copy of ${shortName(path)} exists. restore it?`;
  dlg.appendChild(p);
  const row = document.createElement('div');
  const restoreBtn = document.createElement('button');
  restoreBtn.textContent = 'restore';
  const discardBtn = document.createElement('button');
  discardBtn.textContent = 'discard';
  row.appendChild(restoreBtn);
  row.appendChild(discardBtn);
  dlg.appendChild(row);
  restoreBtn.addEventListener('click', async () => {
    applyingRemote = true;
    editor.value = info.text;
    applyingRemote = false;
    docCache.set(bufferId, info.text);
    if (bufferId === activeDoc) {
      markDirtyLocal(bufferId);
      scheduleSync();
    } else {
      const d = docs.get(bufferId);
      if (d) d.dirty = true;
    }
    renderTabs();
    updateCaret();
    dlg.close();
  }, { once: true });
  discardBtn.addEventListener('click', async () => {
    try {
      await core.discardRecovery(path);
    } catch (err) {
      void err;
    }
    dlg.close();
  }, { once: true });
  if (typeof dlg.showModal === 'function') dlg.showModal();
  restoreBtn.focus();
}

function dirtyList() {
  const out = [];
  for (const [id, doc] of docs) {
    if (doc.dirty) out.push({ id, path: fullPathOf(doc) });
  }
  return out;
}

async function saveAllQuit() {
  for (const { id } of dirtyList()) {
    const doc = docs.get(id);
    if (!doc) continue;
    if (!doc.path) continue;
    try {
      const saved = await core.saveFile(id, doc.path, false);
      doc.path = saved;
      doc.dirty = false;
    } catch (err) {
      setMessage(`save failed ${shortName(doc.path)}: ${err}`, { error: true });
      return false;
    }
  }
  return true;
}

function ensureQuitDialog() {
  const dlg = ensureDialog('quit-dialog', 'unsaved changes');
  return dlg;
}

function handleWindowClose(event) {
  const dirty = dirtyList();
  if (dirty.length === 0) return;
  if (event && typeof event.preventDefault === 'function') event.preventDefault();
  const dlg = ensureQuitDialog();
  dlg.innerHTML = '';
  const h = document.createElement('h2');
  h.textContent = 'unsaved changes';
  dlg.appendChild(h);
  const p = document.createElement('p');
  p.textContent = `${dirty.length} file(s) have unsaved changes:`;
  dlg.appendChild(p);
  const list = document.createElement('ul');
  for (const d of dirty.slice(0, 8)) {
    const li = document.createElement('li');
    li.textContent = shortName(d.path);
    li.title = d.path;
    list.appendChild(li);
  }
  dlg.appendChild(list);
  const row = document.createElement('div');
  const saveBtn = document.createElement('button');
  saveBtn.textContent = 'save all & quit';
  const discardBtn = document.createElement('button');
  discardBtn.textContent = 'discard & quit';
  const cancelBtn = document.createElement('button');
  cancelBtn.textContent = 'cancel';
  row.appendChild(saveBtn);
  row.appendChild(discardBtn);
  row.appendChild(cancelBtn);
  dlg.appendChild(row);
  saveBtn.addEventListener('click', async () => {
    dlg.close();
    if (await saveAllQuit()) window.close();
  }, { once: true });
  discardBtn.addEventListener('click', () => {
    dlg.close();
    window.close();
  }, { once: true });
  cancelBtn.addEventListener('click', () => dlg.close(), { once: true });
  if (typeof dlg.showModal === 'function') dlg.showModal();
  saveBtn.focus();
}

document.getElementById('btn-save').addEventListener('click', saveActive);
document.getElementById('btn-undo').addEventListener('click', () => historyStep('undo'));
document.getElementById('btn-redo').addEventListener('click', () => historyStep('redo'));
document.getElementById('btn-new').addEventListener('click', () => openDoc(null));
document.getElementById('btn-close').addEventListener('click', () => closeDoc(activeDoc));
window.addEventListener('blur', () => { flushSync(); });
window.addEventListener('beforeunload', () => { flushSync(); });

ensureEmptyState();
ensureTypewriterToggle();
renderTabs();
renderStatus(null);
setInterval(sweepRecoveryTick, 30000);
if (typeof core.onCloseRequested === 'function') {
  try {
    core.onCloseRequested(handleWindowClose);
  } catch (err) {
    void err;
    window.addEventListener('beforeunload', handleWindowClose);
  }
} else {
  window.addEventListener('beforeunload', handleWindowClose);
}
