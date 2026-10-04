/* Yonro command palette (P5.2). Classic script; loaded before app.js.
 * <dialog>-based, mod+P / mod+Shift+P. Prefix modes: (none) scenes+files,
 * '>' commands, '@' entities, '/' project search (P5.3 fills in results).
 * Fuzzy match on labels is UI-only (subsequence + prefix boost).
 */

let paletteState = null;
let paletteSeq = 0;
let paletteFilesCache = [];

function paletteModeOf(value) {
  const first = (value || '').charAt(0);
  if (first === '>' || first === '@' || first === '/') {
    return { mode: first, query: (value || '').slice(1) };
  }
  return { mode: '', query: value || '' };
}

function paletteFooterText(mode) {
  if (mode === '>') return 'commands · ↑↓ move · Enter run · Esc close';
  if (mode === '@') return 'entities · ↑↓ move · Enter inspect · Esc close';
  if (mode === '/') return 'project search · ↑↓ move · Enter jump · Esc close';
  return 'scenes + files · ↑↓ move · Enter open · Esc close';
}

/* UI-only fuzzy: subsequence match, prefix and adjacency boosted. -1 = no. */
function fuzzyScore(label, query) {
  const q = (query || '').trim().toLowerCase();
  if (!q) return 0;
  const l = (label || '').toLowerCase();
  if (l.startsWith(q)) return 1000 + q.length;
  let score = 0;
  let at = 0;
  for (const ch of q) {
    const found = l.indexOf(ch, at);
    if (found === -1) return -1;
    score += found === at ? 2 : 1;
    at = found + 1;
  }
  return score;
}

function ensurePalette() {
  let dlg = document.getElementById('palette');
  if (dlg) return dlg;
  dlg = document.createElement('dialog');
  dlg.id = 'palette';
  dlg.setAttribute('aria-label', 'command palette');
  const input = document.createElement('input');
  input.id = 'palette-input';
  input.type = 'text';
  input.setAttribute('aria-label', 'palette input');
  input.setAttribute('aria-controls', 'palette-list');
  input.setAttribute('role', 'combobox');
  input.setAttribute('aria-expanded', 'true');
  input.setAttribute('aria-autocomplete', 'list');
  dlg.appendChild(input);
  const list = document.createElement('div');
  list.id = 'palette-list';
  list.setAttribute('role', 'listbox');
  list.setAttribute('aria-label', 'palette results');
  dlg.appendChild(list);
  const foot = document.createElement('div');
  foot.id = 'palette-foot';
  foot.className = 'muted';
  dlg.appendChild(foot);
  input.addEventListener('input', () => paletteRefresh());
  input.addEventListener('keydown', (e) => {
    if (!paletteState) return;
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      paletteMove(e.key === 'ArrowDown' ? 1 : -1);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      paletteCommit();
    }
  });
  dlg.addEventListener('cancel', () => {
    paletteState = null;
  });
  dlg.addEventListener('close', () => {
    paletteState = null;
  });
  document.body.appendChild(dlg);
  return dlg;
}

function openPalette(mode) {
  const dlg = ensurePalette();
  const input = document.getElementById('palette-input');
  input.value = mode || '';
  paletteState = { mode: mode || '', items: [], active: 0 };
  if (typeof dlg.showModal === 'function' && !dlg.open) dlg.showModal();
  input.focus();
  paletteFilesCache = [];
  if (typeof core.listFiles === 'function') {
    core.listFiles().then((files) => {
      paletteFilesCache = Array.isArray(files) ? files : [];
      if (paletteState && paletteState.mode === '') paletteRefresh();
    }).catch(() => {
      paletteFilesCache = [];
    });
  }
  paletteRefresh();
}

function paletteSceneItems() {
  const out = [];
  const walk = (node, trail) => {
    const path = trail ? `${trail} / ${node.title}` : node.title;
    if (node.kind === 'scene') {
      out.push({
        label: node.title,
        sub: trail,
        run: () => {
          if (typeof binderSelect === 'function') binderSelect(node.id);
          if (typeof openSceneDoc === 'function') openSceneDoc(node.id);
        },
      });
    }
    for (const child of node.children || []) {
      walk(child, node.kind === 'project' ? '' : path);
    }
  };
  if (typeof binderTree !== 'undefined' && binderTree) walk(binderTree, '');
  for (const file of paletteFilesCache) {
    const name = typeof shortName === 'function' ? shortName(file) : file;
    out.push({
      label: name,
      sub: 'file',
      run: () => {
        if (typeof openDoc === 'function') openDoc(file);
      },
    });
  }
  return out;
}

function paletteCommandItems() {
  return (typeof COMMANDS !== 'undefined' ? COMMANDS : []).map((cmd) => ({
    label: cmd.title,
    sub: cmd.keys,
    run: cmd.run,
  }));
}

async function paletteEntityItems(query) {
  let hits = [];
  try {
    hits = await core.loreSearch(query, 20);
  } catch (err) {
    void err;
    hits = [];
  }
  return (hits || []).map((hit) => ({
    label: `@${hit.name}`,
    sub: hit.matched_alias ? `${hit.kind} · ${hit.matched_alias}` : hit.kind,
    run: () => {
      if (typeof showEntityInspector === 'function') showEntityInspector(hit.id);
    },
  }));
}

async function paletteSearchItems(query) {
  if (typeof core.searchProject !== 'function') return null;
  let hits = [];
  try {
    hits = await core.searchProject(query, false);
  } catch (err) {
    void err;
    hits = [];
  }
  return (hits || []).map((hit) => ({
    label: hit.excerpt,
    sub: `${hit.title} · ${hit.line}:${hit.col_start}`,
    run: () => {
      if (typeof openSceneAndSelectRange === 'function') {
        openSceneAndSelectRange(hit.scene_id, hit.line, hit.col_start, hit.col_end);
      } else if (typeof openSceneDoc === 'function') {
        openSceneDoc(hit.scene_id);
      }
    },
  }));
}

async function paletteRefresh() {
  const dlg = document.getElementById('palette');
  const input = document.getElementById('palette-input');
  if (!dlg || !input || !paletteState) return;
  const parsed = paletteModeOf(input.value);
  paletteState.mode = parsed.mode;
  const foot = document.getElementById('palette-foot');
  if (foot) foot.textContent = paletteFooterText(parsed.mode);
  const seq = ++paletteSeq;
  let items = [];
  if (parsed.mode === '>') {
    items = paletteCommandItems();
  } else if (parsed.mode === '@') {
    items = await paletteEntityItems(parsed.query);
    if (seq !== paletteSeq || !paletteState) return;
    paletteState.items = items.slice(0, 20);
    paletteState.active = 0;
    paletteRender();
    return;
  } else if (parsed.mode === '/') {
    if (!parsed.query.trim()) {
      paletteState.items = [];
      paletteState.active = 0;
      paletteRenderHint('type to search the project.');
      return;
    }
    const found = await paletteSearchItems(parsed.query);
    if (seq !== paletteSeq || !paletteState) return;
    if (found === null) {
      paletteState.items = [];
      paletteState.active = 0;
      paletteRenderHint('project search lands in P5.3.');
      return;
    }
    paletteState.items = found.slice(0, 20);
    paletteState.active = 0;
    paletteRender();
    return;
  } else {
    items = paletteSceneItems();
  }
  if (seq !== paletteSeq || !paletteState) return;
  const scored = [];
  for (const item of items) {
    const score = fuzzyScore(item.label, parsed.query);
    if (score >= 0) scored.push({ item, score });
  }
  scored.sort((a, b) => b.score - a.score);
  paletteState.items = scored.slice(0, 20).map((s) => s.item);
  paletteState.active = 0;
  paletteRender();
}

function paletteRenderHint(text) {
  const list = document.getElementById('palette-list');
  if (!list) return;
  list.innerHTML = '';
  const p = document.createElement('p');
  p.className = 'muted';
  p.textContent = text;
  list.appendChild(p);
}

function paletteRender() {
  const list = document.getElementById('palette-list');
  const input = document.getElementById('palette-input');
  if (!list || !paletteState) return;
  list.innerHTML = '';
  if (!paletteState.items.length) {
    paletteRenderHint('no matches.');
    if (input) input.removeAttribute('aria-activedescendant');
    return;
  }
  paletteState.items.forEach((item, i) => {
    const row = document.createElement('div');
    row.id = `palette-opt-${i}`;
    row.setAttribute('role', 'option');
    row.setAttribute('aria-selected', i === paletteState.active ? 'true' : 'false');
    row.className = 'palette-opt' + (i === paletteState.active ? ' active' : '');
    const name = document.createElement('span');
    name.textContent = item.label;
    row.appendChild(name);
    if (item.sub) {
      const sub = document.createElement('span');
      sub.className = 'muted';
      sub.textContent = ` ${item.sub}`;
      row.appendChild(sub);
    }
    row.addEventListener('click', () => {
      paletteState.active = i;
      paletteCommit();
    });
    list.appendChild(row);
  });
  if (input) input.setAttribute('aria-activedescendant', `palette-opt-${paletteState.active}`);
}

function paletteMove(dir) {
  if (!paletteState || !paletteState.items.length) return;
  const n = paletteState.items.length;
  paletteState.active = (paletteState.active + dir + n) % n;
  paletteRender();
}

function paletteCommit() {
  if (!paletteState || !paletteState.items.length) return;
  const item = paletteState.items[paletteState.active];
  const dlg = document.getElementById('palette');
  if (dlg && dlg.open) dlg.close();
  paletteState = null;
  if (item && typeof item.run === 'function') item.run();
}
