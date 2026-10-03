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
  } else if (e.key === 'F2') {
    if (id === null || id === undefined) return;
    e.preventDefault();
    binderBeginRename(id);
  } else if (e.key === 'Delete' || e.key === 'Backspace') {
    if (id === null || id === undefined) return;
    e.preventDefault();
    binderRemove(id);
  } else if (!e.ctrlKey && !e.metaKey && !e.altKey && (e.key === 'a' || e.key === 'c' || e.key === 's')) {
    e.preventDefault();
    const target = id !== null && id !== undefined && !Number.isNaN(id) ? id : binderFocusId;
    binderBeginAdd(e.key, target);
  }
});

/* Inline structure editing: F2 renames, Del removes, a/c/s add.
 * Commits go through core and reload from truth; Esc/blur cancels.
 */

function binderEditing() {
  const box = document.getElementById('binder-outline');
  return box ? box.querySelector('.inline-edit') : null;
}

function binderInlineInput(row, initial, placeholder, commit) {
  const label = row.querySelector('.label');
  if (!label) return;
  if (binderEditing()) return;
  label.style.display = 'none';
  const input = document.createElement('input');
  input.className = 'inline-edit';
  input.type = 'text';
  input.value = initial;
  input.placeholder = placeholder;
  input.setAttribute('aria-label', placeholder);
  let done = false;
  const cancel = () => {
    if (done) return;
    done = true;
    const adding = row.closest('.node.adding');
    if (adding) adding.remove();
    input.remove();
    label.style.display = '';
    const back = binderRowFor(binderFocusId);
    if (back) back.focus();
  };
  input.addEventListener('keydown', (ev) => {
    if (ev.key === 'Enter') {
      ev.preventDefault();
      if (done) return;
      done = true;
      const value = input.value;
      input.remove();
      label.style.display = '';
      commit(value);
    } else if (ev.key === 'Escape') {
      ev.preventDefault();
      ev.stopPropagation();
      cancel();
    } else if (ev.key === ' ' || ev.key === 'ArrowLeft' || ev.key === 'ArrowRight') {
      ev.stopPropagation();
    }
  });
  input.addEventListener('blur', cancel);
  row.appendChild(input);
  input.focus();
  input.select();
}

function binderNearestAncestor(id, kind) {
  let cursor = binderFind(id);
  while (cursor) {
    if (cursor.kind === kind) return cursor.id;
    const parent = binderParentOf(cursor.id);
    cursor = parent;
  }
  return null;
}

function binderBeginRename(id) {
  const node = binderFind(id);
  const row = binderRowFor(id);
  if (!node || !row) return;
  binderFocus(id, false);
  binderInlineInput(row, node.title, 'rename', async (value) => {
    const title = value.trim();
    if (!title) {
      setMessage('title cannot be empty', { error: true });
      binderBeginRename(id);
      return;
    }
    try {
      await core.renameNode(id, title);
      await loadBinder();
      binderFocus(id, true);
      binderSelect(id);
      setMessage(`renamed to ${title}`);
    } catch (err) {
      setMessage(`rename failed: ${err}`, { error: true });
      binderFocus(id, true);
    }
  });
}

function binderBeginAdd(which, focusId) {
  const kind = which === 'a' ? 'act' : which === 'c' ? 'chapter' : 'scene';
  let parent = null;
  if (kind === 'chapter') {
    parent = focusId === null ? null : binderNearestAncestor(focusId, 'act');
    if (parent === null) {
      setMessage('chapters live in acts. press a to add an act first.', { error: true });
      return;
    }
  } else if (kind === 'scene') {
    parent = focusId === null ? null : binderNearestAncestor(focusId, 'chapter');
    if (parent === null) {
      setMessage('scenes live in chapters.', { error: true });
      return;
    }
  }
  const box = document.getElementById('binder-outline');
  if (!box) return;
  if (binderEditing()) return;
  let container = box;
  if (parent !== null) {
    const wrap = box.querySelector(`.node[data-id="${parent}"]`);
    container = (wrap && wrap.querySelector('.children')) || box;
  }
  const temp = document.createElement('div');
  temp.className = 'node adding';
  const row = document.createElement('div');
  row.className = 'row';
  const label = document.createElement('span');
  label.className = 'label';
  label.style.display = 'none';
  row.appendChild(label);
  temp.appendChild(row);
  container.appendChild(temp);
  binderInlineInput(row, '', `new ${kind} title`, async (value) => {
    const title = value.trim();
    if (!title) {
      setMessage('title cannot be empty', { error: true });
      temp.remove();
      return;
    }
    try {
      const res = await core.addNode(parent, kind, title);
      binderTree = res.outline;
      const titleElm = document.getElementById('project-title');
      if (titleElm) titleElm.textContent = res.outline.title || 'untitled';
      renderBinderTree();
      binderFocus(res.new_id, true);
      binderSelect(res.new_id);
      setMessage(`added ${kind} ${title}`);
    } catch (err) {
      setMessage(`add failed: ${err}`, { error: true });
      temp.remove();
    }
  });
}

function binderCountScenes(node) {
  let count = node.kind === 'scene' ? 1 : 0;
  for (const child of node.children || []) count += binderCountScenes(child);
  return count;
}

async function binderRemove(id) {
  const node = binderFind(id);
  if (!node) return;
  const kids = (node.children || []).length;
  const needsConfirm = kids > 0 || (node.kind === 'scene' && node.words > 0);
  if (!needsConfirm) {
    await binderDoRemove(id);
    return;
  }
  const dlg = ensureDialog('binder-confirm', 'delete');
  dlg.innerHTML = '';
  const h = document.createElement('h2');
  h.textContent = 'delete';
  dlg.appendChild(h);
  const p = document.createElement('p');
  if (kids > 0) {
    p.textContent = `"${node.title}" holds ${kids} item${kids === 1 ? '' : 's'}. drafts stay on disk; snapshots go to history.`;
  } else {
    p.textContent = `"${node.title}" has ${node.words} words. the draft stays on disk; a snapshot goes to history.`;
  }
  dlg.appendChild(p);
  const row = document.createElement('div');
  const del = document.createElement('button');
  del.textContent = 'delete';
  const cancel = document.createElement('button');
  cancel.textContent = 'cancel';
  row.appendChild(del);
  row.appendChild(cancel);
  dlg.appendChild(row);
  del.addEventListener('click', async () => {
    dlg.close();
    await binderDoRemove(id);
  }, { once: true });
  cancel.addEventListener('click', () => dlg.close(), { once: true });
  dlg.addEventListener('cancel', () => {
    const back = binderRowFor(id);
    if (back) back.focus();
  }, { once: true });
  if (typeof dlg.showModal === 'function') dlg.showModal();
  cancel.focus();
}

async function binderDoRemove(id) {
  const parent = binderParentOf(id);
  try {
    const res = await core.removeNode(id);
    if (binderSelectedId === id) binderSelectedId = null;
    binderTree = res.outline;
    const titleElm = document.getElementById('project-title');
    if (titleElm) titleElm.textContent = res.outline.title || 'untitled';
    renderBinderTree();
    const next = parent ? parent.id : null;
    if (next !== null && binderFind(next)) {
      binderFocus(next, true);
      binderSelect(next);
    }
    setMessage(res.message);
  } catch (err) {
    setMessage(`delete failed: ${err}`, { error: true });
    binderFocus(id, true);
  }
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
