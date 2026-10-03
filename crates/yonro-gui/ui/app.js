/* Yonro GUI frontend — vanilla JS, no build step.
 *
 * ADAPTER SEAM (the important part): every backend read/write below goes
 * through `core`, a tiny wrapper over Tauri commands. A future pure-web
 * build keeps this whole file and reimplements `core` over WASM — nothing
 * else changes.
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

const esc = (s) =>
  String(s ?? '').replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));

/* ---------- nav ---------- */
const views = ['write', 'outline', 'graph', 'timeline', 'lore'];
function show(name) {
  for (const v of views) {
    document.getElementById(`view-${v}`).classList.toggle('hidden', v !== name);
  }
  for (const btn of document.querySelectorAll('#topnav [data-view]')) {
    btn.classList.toggle('active', btn.dataset.view === name);
  }
  if (name === 'outline') loadOutline();
  if (name === 'graph') loadGraph();
  if (name === 'timeline') loadTimeline();
  if (name === 'lore') loadLore();
}
for (const btn of document.querySelectorAll('#topnav [data-view]')) {
  btn.addEventListener('click', () => show(btn.dataset.view));
}
document.getElementById('zen-toggle').addEventListener('click', () => {
  document.body.classList.toggle('zen');
});

/* ---------- write view ---------- */
const docs = new Map(); // bufferId -> { path, dirty }
const docCache = new Map(); // bufferId -> last known text
let activeDoc = null;
let applyingRemote = false;

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
  for (const [id, doc] of docs) {
    const tab = document.createElement('span');
    tab.className = 'tab' + (id === activeDoc ? ' active' : '');
    tab.setAttribute('role', 'tab');
    tab.innerHTML = `${esc(shortName(doc.path))}${doc.dirty ? ' <span class="dirty">●</span>' : ''}`;
    tab.addEventListener('click', () => activateDoc(id));
    docTabs.appendChild(tab);
  }
}

function renderStatus(stats) {
  const doc = docs.get(activeDoc);
  docName.textContent = doc ? `${shortName(doc.path)}${doc.dirty ? ' ●' : ''}` : 'No document — open one from Outline, or start typing below.';
  docStatus.textContent = stats
    ? `${stats.words} words · ${stats.chars} chars · ${stats.lines} lines · ~${stats.reading_min} min${stats.dirty ? ' · modified' : ''}`
    : '';
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
  } catch (err) {
    docStatus.textContent = `Could not open: ${err}`;
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
  editor.focus();
}

editor.addEventListener('input', async () => {
  if (applyingRemote || activeDoc === null) return;
  const text = editor.value;
  docCache.set(activeDoc, text);
  try {
    const stats = await core.setText(activeDoc, text);
    const doc = docs.get(activeDoc);
    if (doc) doc.dirty = stats.dirty;
    renderTabs();
    renderStatus(stats);
  } catch (err) {
    docStatus.textContent = `Sync failed: ${err}`;
  }
});

editor.addEventListener('keydown', async (e) => {
  if (e.key === 'Tab') {
    e.preventDefault();
    const { selectionStart: s, selectionEnd: t } = editor;
    editor.setRangeText('\t', s, t, 'end');
    editor.dispatchEvent(new Event('input'));
  }
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') {
    e.preventDefault();
    await saveActive();
  }
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'z' && !e.shiftKey) {
    e.preventDefault();
    await historyStep('undo');
  }
  if ((e.ctrlKey || e.metaKey) && (e.key.toLowerCase() === 'y' || (e.key.toLowerCase() === 'z' && e.shiftKey))) {
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
    docStatus.textContent = `History failed: ${err}`;
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
    docStatus.textContent = `Saved ${saved}`;
  } catch (err) {
    docStatus.textContent = `Save failed: ${err}`;
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
    docStatus.textContent = `Close failed: ${err}`;
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
  }
  renderTabs();
  renderStatus(null);
});

/* ---------- outline section ---------- */
function renderNode(node, openScene) {
  const meta =
    node.kind === 'scene' && node.target > 0
      ? `<span class="meta">${node.words}/${node.target} words</span>`
      : node.kind === 'scene' && node.words > 0
        ? `<span class="meta">${node.words} words</span>`
        : '';
  const bar =
    node.kind !== 'project' && node.target > 0
      ? `<div class="pbar"><div style="width:${Math.min(100, Math.round((node.words / node.target) * 100))}%"></div></div>`
      : '';
  const kids = (node.children || []).map((k) => renderNode(k, openScene)).join('');
  const clickable = node.kind === 'scene' && node.file ? ` data-file="${esc(node.file)}"` : '';
  return `<div class="node" data-kind="${esc(node.kind)}"><div class="row"${clickable}>${esc(node.title)}${meta}${bar}</div>${
    kids ? `<div class="children">${kids}</div>` : ''
  }</div>`;
}

async function loadOutline() {
  try {
    const outline = await core.outline();
    const kids = (outline.children || []).map((k) => renderNode(k, true)).join('');
    const el = document.getElementById('outline');
    el.innerHTML =
      `<div class="node" data-kind="project"><div class="row">✎ ${esc(outline.title)}</div>` +
      (kids ? `<div class="children">${kids}</div>` : '') +
      `</div>`;
    el.querySelectorAll('[data-file]').forEach((row) => {
      row.addEventListener('click', () => openDoc(row.dataset.file));
    });
  } catch (err) {
    document.getElementById('outline').innerHTML = `<p class="muted">Outline unavailable: ${esc(err)}</p>`;
  }
}

/* ---------- graph section ---------- */
let graphSpotlight = null;

async function loadGraph() {
  try {
    const graph = await core.graph();
    renderGraph(graph);
  } catch (err) {
    document.getElementById('graph-legend').textContent = `Graph unavailable: ${err}`;
  }
}

function renderGraph(graph) {
  const svg = document.getElementById('graph');
  const W = 900;
  const H = 480;
  svg.setAttribute('viewBox', `0 0 ${W} ${H}`);
  const nodes = graph.nodes || [];
  const edges = graph.edges || [];
  if (!nodes.length) {
    svg.innerHTML = '';
    document.getElementById('graph-legend').textContent = 'No linked entities yet — set scene POVs and write @mentions.';
    return;
  }
  const cx = W / 2;
  const cy = H / 2;
  const radius = Math.min(W, H) / 2 - 60;
  const pos = {};
  nodes.forEach((n, i) => {
    const angle = (2 * Math.PI * i) / nodes.length - Math.PI / 2;
    pos[n.id] = { x: cx + radius * Math.cos(angle), y: cy + radius * Math.sin(angle) };
  });
  const neighborIds = new Set();
  if (graphSpotlight !== null) {
    neighborIds.add(graphSpotlight);
    for (const e of edges) {
      if (e.a === graphSpotlight) neighborIds.add(e.b);
      if (e.b === graphSpotlight) neighborIds.add(e.a);
    }
  }
  const dim = (id) => (graphSpotlight === null || neighborIds.has(id) ? '' : ' class="dim"');
  const dimEdge = (e) =>
    graphSpotlight === null || e.a === graphSpotlight || e.b === graphSpotlight ? '' : ' class="dim"';
  let html = '';
  for (const e of edges) {
    const a = pos[e.a];
    const b = pos[e.b];
    if (!a || !b) continue;
    const w = 1 + Math.min(6, e.weight);
    const color = (e.kinds || []).includes('mention') ? '#d6a35c' : '#785f3c';
    html += `<line x1="${a.x}" y1="${a.y}" x2="${b.x}" y2="${b.y}" stroke="${color}" stroke-width="${w}"${dimEdge(e)}><title>${e.shared} shared · ${e.mentions} mentions</title></line>`;
  }
  for (const n of nodes) {
    const p = pos[n.id];
    html += `<g data-id="${n.id}"${dim(n.id)} style="cursor:pointer"><circle cx="${p.x}" cy="${p.y}" r="16" fill="#292524" stroke="#d6a35c" stroke-width="2"/><text x="${p.x}" y="${p.y + 4}" text-anchor="middle">${esc(n.label.slice(0, 10))}</text><title>${esc(n.label)} (${esc(n.kind)})</title></g>`;
  }
  svg.innerHTML = html;
  svg.querySelectorAll('g[data-id]').forEach((g) => {
    g.addEventListener('click', () => {
      const id = Number(g.dataset.id);
      graphSpotlight = graphSpotlight === id ? null : id;
      renderGraph(graph);
    });
  });
  const shown = graphSpotlight === null ? edges.length : edges.filter((e) => e.a === graphSpotlight || e.b === graphSpotlight).length;
  document.getElementById('graph-legend').textContent =
    `${nodes.length} entities · ${edges.length} links (${shown} shown). Gold = @mention link, bronze = shared scene.`;
}

/* ---------- timeline section ---------- */
async function loadTimeline() {
  try {
    const timeline = await core.timeline();
    const notes = timeline.notes || [];
    document.getElementById('continuity').innerHTML = notes.length
      ? notes.map((n) => `<div class="note">⚠ ${esc(n.message)}</div>`).join('')
      : '<p class="muted">No continuity notes — every POV/setting transition reads clean.</p>';
    document.getElementById('timeline').innerHTML = (timeline.entries || [])
      .map(
        (e) =>
          `<li><strong>${esc(e.title)}</strong> <span class="when">#${e.index + 1} · ${esc(e.chapter)} · ${esc(e.pov) || 'no POV'} · ${esc(e.setting) || 'no setting'}${e.story_date ? ` · ${esc(e.story_date)}` : ''} · ${e.words}w</span></li>`,
      )
      .join('');
  } catch (err) {
    document.getElementById('timeline').innerHTML = `<p class="muted">Timeline unavailable: ${esc(err)}</p>`;
  }
}

/* ---------- lore section ---------- */
async function loadLore() {
  try {
    const entities = await core.lore();
    if (!entities.length) return; // keep the helpful placeholder
    document.getElementById('lore').innerHTML = entities
      .map(
        (e) => `<div class="entity"><h3>${esc(e.name)}<span class="kind">${esc(e.kind)}</span></h3>` +
          (e.aliases && e.aliases.length
            ? `<p class="aliases">Also known as: ${esc(e.aliases.join(', '))}</p>`
            : '') +
          (e.sheet ? `<p class="sheet">${esc(e.sheet)}</p>` : '') +
          (e.pov_scenes && e.pov_scenes.length
            ? `<p class="povs">POV in: ${esc(e.pov_scenes.join(', '))}</p>`
            : '') +
          `</div>`,
      )
      .join('');
  } catch (err) {
    document.getElementById('lore').innerHTML = `<p class="muted">Lore unavailable: ${esc(err)}</p>`;
  }
}
