/* Yonro outline view (center column). Classic script; globals for app.js. */

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
    const element = document.getElementById('outline');
    element.innerHTML =
      `<div class="node" data-kind="project"><div class="row">✎ ${esc(outline.title)}</div>` +
      (kids ? `<div class="children">${kids}</div>` : '') +
      `</div>`;
    element.querySelectorAll('[data-file]').forEach((row) => {
      row.addEventListener('click', () => openDoc(row.dataset.file));
    });
  } catch (err) {
    document.getElementById('outline').innerHTML = `<p class="muted">Outline unavailable: ${esc(err)}</p>`;
  }
}

/* Binder tree (left column). Same OutlineNodeDto data as the Outline view.
 * DOM-built (textContent, no innerHTML). Roving tabindex; keyboard:
 * up/down move, left/right collapse/expand, Enter opens scenes.
 * F2/Del/a/c/s arrive in P3.2b; collapse + menu + moves in P3.2c.
 */

let binderTree = null;
let binderFocusId = null;
let binderSelectedId = null;

async function loadBinder() {
  const box = document.getElementById('binder-outline');
  try {
    binderTree = await core.outline();
    const title = document.getElementById('project-title');
    if (title) title.textContent = binderTree.title || 'untitled';
    renderBinderTree();
  } catch (err) {
    binderTree = null;
    if (box) {
      box.innerHTML = '';
      const p = document.createElement('p');
      p.className = 'muted';
      p.textContent = `binder unavailable: ${err}`;
      box.appendChild(p);
    }
  }
}

function binderFind(id, root) {
  const top = root || binderTree;
  if (!top) return null;
  if (top.id === id) return top;
  for (const child of top.children || []) {
    const hit = binderFind(id, child);
    if (hit) return hit;
  }
  return null;
}

function binderParentOf(id) {
  if (!binderTree || binderTree.id === id) return null;
  return binderFindParentIn(id, binderTree);
}

function binderFindParentIn(id, node) {
  for (const child of node.children || []) {
    if (child.id === id) return node;
    const hit = binderFindParentIn(id, child);
    if (hit) return hit;
  }
  return null;
}

function binderSiblingsOf(id) {
  if (!binderTree) return { parent: null, kids: [], index: -1 };
  if (binderTree.id === id) return { parent: null, kids: [binderTree], index: 0 };
  const parent = binderParentOf(id);
  const kids = (parent && parent.children) || [];
  return { parent, kids, index: kids.findIndex((k) => k.id === id) };
}

function binderVisibleIds() {
  const out = [];
  const walk = (node) => {
    out.push(node.id);
    for (const child of node.children || []) walk(child);
  };
  for (const child of (binderTree && binderTree.children) || []) walk(child);
  return out;
}

function binderDepthOf(id) {
  let depth = 0;
  let cursor = binderParentOf(id);
  while (cursor && binderTree && cursor.id !== binderTree.id) {
    depth += 1;
    cursor = binderParentOf(cursor.id);
  }
  return depth;
}

function renderBinderTree() {
  const box = document.getElementById('binder-outline');
  if (!box) return;
  box.innerHTML = '';
  box.setAttribute('role', 'tree');
  box.setAttribute('aria-label', 'manuscript outline');
  const kids = (binderTree && binderTree.children) || [];
  if (!kids.length) {
    box.removeAttribute('role');
    const p = document.createElement('p');
    p.className = 'muted';
    p.textContent = 'no manuscript yet. press a to add an act, then c, then s.';
    box.appendChild(p);
    return;
  }
  if (binderFocusId === null || !binderFind(binderFocusId)) {
    binderFocusId = kids[0].id;
  }
  for (const child of kids) box.appendChild(buildBinderNode(child));
}

function buildBinderNode(node) {
  const wrap = document.createElement('div');
  wrap.className = 'node';
  wrap.dataset.id = String(node.id);
  wrap.dataset.kind = node.kind;
  const row = document.createElement('div');
  row.className = 'row';
  row.dataset.id = String(node.id);
  row.setAttribute('role', 'treeitem');
  row.setAttribute('aria-level', String(binderDepthOf(node.id) + 1));
  row.tabIndex = node.id === binderFocusId ? 0 : -1;
  if (node.id === binderSelectedId) row.setAttribute('aria-selected', 'true');
  const hasKids = (node.children || []).length > 0;
  if (hasKids) row.setAttribute('aria-expanded', 'true');
  const twisty = document.createElement('span');
  twisty.className = 'twisty';
  twisty.setAttribute('aria-hidden', 'true');
  twisty.textContent = hasKids ? '▾' : '·';
  twisty.addEventListener('click', (e) => {
    e.stopPropagation();
    binderFocus(Number(row.dataset.id), true);
  });
  row.appendChild(twisty);
  const label = document.createElement('span');
  label.className = 'label';
  label.textContent = node.title;
  row.appendChild(label);
  if (node.kind === 'scene' || node.target > 0 || node.words > 0) {
    const meta = document.createElement('span');
    meta.className = 'meta';
    meta.textContent = node.target > 0 ? `${node.words}/${node.target}` : `${node.words} words`;
    row.appendChild(meta);
  }
  if (node.target > 0 && node.kind !== 'project') {
    const bar = document.createElement('div');
    bar.className = 'pbar';
    const fill = document.createElement('div');
    fill.style.width = `${Math.min(100, Math.round((node.words / node.target) * 100))}%`;
    bar.appendChild(fill);
    row.appendChild(bar);
  }
  row.addEventListener('click', () => {
    binderFocus(node.id, true);
    binderSelect(node.id);
    if (node.kind === 'scene') openSceneDoc(node.id);
  });
  wrap.appendChild(row);
  if (hasKids) {
    const group = document.createElement('div');
    group.className = 'children';
    group.setAttribute('role', 'group');
    for (const child of node.children) group.appendChild(buildBinderNode(child));
    wrap.appendChild(group);
  }
  return wrap;
}

function binderRowFor(id) {
  const box = document.getElementById('binder-outline');
  if (!box) return null;
  return box.querySelector(`.row[data-id="${id}"]`);
}

function binderFocus(id, steal) {
  binderFocusId = id;
  const box = document.getElementById('binder-outline');
  if (!box) return;
  for (const row of box.querySelectorAll('.row[tabindex="0"]')) row.tabIndex = -1;
  const row = binderRowFor(id);
  if (row) {
    row.tabIndex = 0;
    if (steal) row.focus({ preventScroll: true });
  }
}

function binderSelect(id) {
  binderSelectedId = id;
  const box = document.getElementById('binder-outline');
  if (!box) return;
  for (const row of box.querySelectorAll('.row[aria-selected="true"]')) {
    row.removeAttribute('aria-selected');
  }
  const row = binderRowFor(id);
  if (row) row.setAttribute('aria-selected', 'true');
  if (typeof showInspectorFor === 'function') showInspectorFor(id);
}

function binderFocusDelta(delta) {
  const ids = binderVisibleIds();
  if (!ids.length) return;
  let at = ids.indexOf(binderFocusId);
  if (at === -1) at = delta > 0 ? -1 : 0;
  const next = ids[Math.min(ids.length - 1, Math.max(0, at + delta))];
  binderFocus(next, true);
  binderSelect(next);
}

document.getElementById('binder-outline').addEventListener('keydown', (e) => {
  if (e.target && (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA')) return;
  const row = e.target && e.target.closest ? e.target.closest('.row') : null;
  const id = row && row.dataset.id !== undefined ? Number(row.dataset.id) : binderFocusId;
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    e.preventDefault();
    binderFocusDelta(e.key === 'ArrowDown' ? 1 : -1);
  } else if (e.key === 'Home') {
    e.preventDefault();
    const ids = binderVisibleIds();
    if (ids.length) {
      binderFocus(ids[0], true);
      binderSelect(ids[0]);
    }
  } else if (e.key === 'End') {
    e.preventDefault();
    const ids = binderVisibleIds();
    if (ids.length) {
      binderFocus(ids[ids.length - 1], true);
      binderSelect(ids[ids.length - 1]);
    }
  } else if (e.key === 'Enter') {
    if (id === null || id === undefined) return;
    e.preventDefault();
    const node = binderFind(id);
    if (!node) return;
    binderSelect(id);
    if (node.kind === 'scene') openSceneDoc(id);
  }
});

async function loadOutline() {
  try {
    const outline = await core.outline();
    const kids = (outline.children || []).map((k) => renderNode(k, true)).join('');
    const element = document.getElementById('outline');
    element.innerHTML =
      `<div class="node" data-kind="project"><div class="row">✎ ${esc(outline.title)}</div>` +
      (kids ? `<div class="children">${kids}</div>` : '') +
      `</div>`;
    element.querySelectorAll('[data-file]').forEach((row) => {
      row.addEventListener('click', () => openDoc(row.dataset.file));
    });
  } catch (err) {
    document.getElementById('outline').innerHTML = `<p class="muted">Outline unavailable: ${esc(err)}</p>`;
  }
}
