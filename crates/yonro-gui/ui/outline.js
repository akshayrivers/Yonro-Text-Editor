/* Yonro outline view (center column): a table in tree order, same data
 * as the binder. Click selects (inspector updates), double-click/Enter
 * opens scenes in Write. Totals come from get_stats.
 */

function outlineFlatRows(outline) {
  const rows = [];
  const walk = (node, depth) => {
    if (node.kind !== 'project') rows.push({ node, depth });
    for (const child of node.children || []) walk(child, depth + 1);
  };
  walk(outline, -1);
  return rows;
}

function outlineStatus(node) {
  if (node.target > 0 && node.words >= node.target) return { cls: 'st-done', label: 'complete' };
  if (node.words > 0) return { cls: 'st-draft', label: 'drafting' };
  return { cls: 'st-empty', label: 'empty' };
}

function outlineRow(entry) {
  const { node, depth } = entry;
  const inAlt = altOutlineId !== null;
  const tr = document.createElement('tr');
  tr.dataset.id = String(node.id);
  tr.dataset.kind = node.kind;
  tr.tabIndex = 0;
  const selected = inAlt ? node.id === altSelectedId : node.id === binderSelectedId;
  if (selected) tr.setAttribute('aria-selected', 'true');
  const cells = [
    { text: node.title, pad: 8 + depth * 18 },
    { text: node.kind === 'scene' && node.pov ? node.pov : '' },
    { text: node.kind === 'scene' && node.setting ? node.setting : '' },
    { text: node.kind === 'scene' && node.story_date ? node.story_date : '' },
    { text: node.target > 0 ? `${node.words}/${node.target}` : `${node.words}` },
  ];
  for (const cell of cells) {
    const td = document.createElement('td');
    if (cell.pad !== undefined) td.style.paddingLeft = `${cell.pad}px`;
    td.textContent = cell.text;
    tr.appendChild(td);
  }
  const dotCell = document.createElement('td');
  const status = outlineStatus(node);
  const dot = document.createElement('span');
  dot.className = `status-dot ${status.cls}`;
  dot.textContent = '●';
  dot.title = status.label;
  dot.setAttribute('role', 'img');
  dot.setAttribute('aria-label', status.label);
  dotCell.appendChild(dot);
  tr.appendChild(dotCell);
  tr.addEventListener('click', () => {
    if (altOutlineId !== null) {
      altSelectedId = node.id;
      outlineMarkSelected(node.id);
      return;
    }
    binderSelect(node.id);
    outlineMarkSelected(node.id);
  });
  tr.addEventListener('dblclick', () => {
    if (node.kind !== 'scene') return;
    if (altOutlineId !== null) openAltSceneDoc(altOutlineId, node.id);
    else openSceneDoc(node.id);
  });
  tr.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      if (altOutlineId !== null) {
        altSelectedId = node.id;
        outlineMarkSelected(node.id);
        if (node.kind === 'scene') openAltSceneDoc(altOutlineId, node.id);
      } else {
        binderSelect(node.id);
        outlineMarkSelected(node.id);
        if (node.kind === 'scene') openSceneDoc(node.id);
      }
      return;
    }
    if (altOutlineId === null) return;
    if (e.key === 'F2') {
      e.preventDefault();
      altRenameFlow(node.id);
    } else if (e.key === 'Delete' || e.key === 'Backspace') {
      e.preventDefault();
      altRemoveFlow(node.id);
    } else if (!e.ctrlKey && !e.metaKey && !e.altKey && (e.key === 'a' || e.key === 'c' || e.key === 's')) {
      e.preventDefault();
      altBeginAdd(e.key);
    }
  });
  return tr;
}

async function loadOutline() {
  const element = document.getElementById('outline');
  try {
    const [outline, stats, alts] = await Promise.all([
      core.outline(),
      core.stats(),
      core.altOutlines().catch(() => []),
    ]);
    binderTree = outline;
    renderBinderTree();
    altOutlines = Array.isArray(alts) ? alts : [];
    if (altOutlineId !== null && !altOutlines.some((o) => o.id === altOutlineId)) {
      altOutlineId = null;
    }
    renderOutlineTabs();
    if (altOutlineId === null) {
      renderOutlineTable(element, outline, stats);
    } else {
      await loadAltOutlineTable(element);
    }
  } catch (err) {
    element.innerHTML = '';
    const p = document.createElement('p');
    p.className = 'muted';
    p.textContent = `could not load outline: ${errText(err)}`;
    element.appendChild(p);
  }
}

/* Alternate outlines: the manuscript tab plus one tab per extra tree.
 * The binder and inspector stay manuscript-bound; alt rows select locally
 * (altSelectedId) and scenes open straight into Write.
 */
let altOutlineId = null;
let altOutlines = [];
let altSelectedId = null;
let altTree = null;

async function loadAltOutlineTable(element) {
  try {
    const dto = await core.altOutline(altOutlineId);
    altTree = dto;
    renderAltOutlineTable(element, dto);
  } catch (err) {
    element.innerHTML = '';
    const p = document.createElement('p');
    p.className = 'muted';
    p.textContent = `could not load outline: ${errText(err)}`;
    element.appendChild(p);
  }
}

function renderAltOutlineTable(element, dto) {
  if (!(dto.children || []).length) {
    element.innerHTML = '';
    const p = document.createElement('p');
    p.className = 'muted';
    p.textContent = 'empty outline. add an act with the + act button below.';
    element.appendChild(p);
    return;
  }
  let scenes = 0;
  const walk = (node) => {
    if (node.kind === 'scene') scenes += 1;
    for (const child of node.children || []) walk(child);
  };
  walk(dto);
  renderOutlineTable(element, dto, {
    scenes,
    words: dto.words || 0,
    target: dto.target || 0,
  });
}

function renderOutlineTabs() {
  const bar = document.getElementById('outline-tabs');
  if (bar) {
    bar.innerHTML = '';
    const mk = (key, label, selected, hint) => {
      const b = document.createElement('button');
      b.className = 'tab';
      b.textContent = label;
      b.title = hint || label;
      b.setAttribute('role', 'tab');
      b.setAttribute('aria-selected', selected ? 'true' : 'false');
      b.addEventListener('click', () => switchOutlineView(key));
      bar.appendChild(b);
      return b;
    };
    mk(null, 'manuscript', altOutlineId === null, 'the main manuscript tree');
    for (const o of altOutlines) {
      const tab = mk(o.id, o.title, o.id === altOutlineId, 'double-click renames, Del removes');
      tab.addEventListener('dblclick', (e) => {
        e.stopPropagation();
        renameAltOutlineFlow(o.id);
      });
      tab.addEventListener('keydown', (e) => {
        if (e.key === 'Delete' || e.key === 'Backspace') {
          e.preventDefault();
          deleteAltOutlineFlow(o.id);
        }
      });
    }
    const add = document.createElement('button');
    add.className = 'tab';
    add.textContent = '+ new';
    add.title = 'new alternate outline';
    add.addEventListener('click', () => createAltOutlineFlow());
    bar.appendChild(add);
  }
  renderAltToolbar();
}

function renderAltToolbar() {
  const bar = document.getElementById('alt-toolbar');
  if (!bar) return;
  bar.innerHTML = '';
  if (altOutlineId === null) {
    bar.hidden = true;
    return;
  }
  bar.hidden = false;
  for (const [key, label] of [['a', '+ act'], ['c', '+ chapter'], ['s', '+ scene']]) {
    const b = document.createElement('button');
    b.textContent = label;
    b.title = `add ${label.slice(2)} to this outline`;
    b.addEventListener('click', () => altBeginAdd(key));
    bar.appendChild(b);
  }
}

function switchOutlineView(key) {
  altOutlineId = key;
  altSelectedId = null;
  altTree = null;
  if (typeof show === 'function') show('outline');
  else loadOutline();
}

async function openAltSceneDoc(outlineId, id) {
  try {
    if (typeof flushSync === 'function') await flushSync();
    const opened = await core.altOpenScene(outlineId, id);
    adoptOpened(opened);
  } catch (err) {
    setMessage(`could not open scene ${id}: ${errText(err)}`, { error: true });
  }
}

function altFindWithParent(id, node, parent) {
  if (!node) return null;
  if (node.id === id) return { node, parent };
  for (const child of node.children || []) {
    const hit = altFindWithParent(id, child, node);
    if (hit) return hit;
  }
  return null;
}

function altNearestAncestor(id, kind) {
  let cursor = altFindWithParent(id, altTree);
  while (cursor && cursor.parent) {
    if (cursor.parent.kind === kind) return cursor.parent.id;
    cursor = altFindWithParent(cursor.parent.id, altTree);
  }
  return null;
}

function altTitleDialog(heading, initial, confirmLabel) {
  const dlg = ensureDialog('alt-outline-dialog', heading);
  dlg.innerHTML = '';
  const h = document.createElement('h2');
  h.textContent = heading;
  dlg.appendChild(h);
  const label = document.createElement('label');
  label.textContent = 'title';
  const input = document.createElement('input');
  input.type = 'text';
  input.value = initial || '';
  input.setAttribute('aria-label', 'title');
  label.appendChild(input);
  dlg.appendChild(label);
  const err = document.createElement('p');
  err.className = 'field-error';
  err.setAttribute('aria-live', 'polite');
  err.hidden = true;
  dlg.appendChild(err);
  const row = document.createElement('div');
  const goBtn = document.createElement('button');
  goBtn.textContent = confirmLabel;
  const cancelBtn = document.createElement('button');
  cancelBtn.textContent = 'cancel';
  row.appendChild(goBtn);
  row.appendChild(cancelBtn);
  dlg.appendChild(row);
  return { dlg, input, err, goBtn, cancelBtn };
}

function altBeginAdd(which) {
  if (altOutlineId === null || !altTree) return;
  const kind = which === 'a' ? 'act' : which === 'c' ? 'chapter' : 'scene';
  let parent = null;
  if (kind === 'chapter') {
    parent = altSelectedId === null ? null : altNearestAncestor(altSelectedId, 'act');
    if (parent === null) {
      setMessage('chapters live in acts. add an act first.', { error: true });
      return;
    }
  } else if (kind === 'scene') {
    parent = altSelectedId === null ? null : altNearestAncestor(altSelectedId, 'chapter');
    if (parent === null) {
      setMessage('scenes live in chapters.', { error: true });
      return;
    }
  }
  const { dlg, input, err, goBtn, cancelBtn } = altTitleDialog(`new ${kind}`, '', 'add');
  goBtn.addEventListener('click', async () => {
    const title = input.value.trim();
    if (!title) {
      err.textContent = 'title cannot be empty';
      err.hidden = false;
      return;
    }
    try {
      const res = await core.altAddNode(altOutlineId, parent, kind, title);
      altTree = res.outline;
      altSelectedId = res.new_id;
      dlg.close();
      setMessage(`added ${kind} ${title}`);
      loadOutline();
    } catch (e) {
      err.textContent = `cannot add ${kind} ${title}: ${errText(e)}`;
      err.hidden = false;
    }
  });
  cancelBtn.addEventListener('click', () => dlg.close(), { once: true });
  openModal(dlg, input);
}

function altRenameFlow(id) {
  const hit = altFindWithParent(id, altTree);
  if (!hit) return;
  const { dlg, input, err, goBtn, cancelBtn } = altTitleDialog('rename', hit.node.title, 'rename');
  goBtn.addEventListener('click', async () => {
    const title = input.value.trim();
    if (!title) {
      err.textContent = 'title cannot be empty';
      err.hidden = false;
      return;
    }
    try {
      altTree = await core.altRenameNode(altOutlineId, id, title);
      dlg.close();
      setMessage(`renamed to ${title}`);
      loadOutline();
    } catch (e) {
      err.textContent = `cannot rename ${hit.node.title}: ${errText(e)}`;
      err.hidden = false;
    }
  });
  cancelBtn.addEventListener('click', () => dlg.close(), { once: true });
  openModal(dlg, input);
}

function altRemoveFlow(id) {
  const hit = altFindWithParent(id, altTree);
  if (!hit) return;
  const kids = (hit.node.children || []).length;
  const run = async () => {
    try {
      altTree = await core.altRemoveNode(altOutlineId, id);
      if (altSelectedId === id) altSelectedId = null;
      loadOutline();
    } catch (err) {
      setMessage(`could not delete ${hit.node.title}: ${errText(err)}`, { error: true });
    }
  };
  if (kids === 0 && !(hit.node.kind === 'scene' && hit.node.words > 0)) {
    run();
    return;
  }
  const dlg = ensureDialog('alt-outline-confirm', 'delete');
  dlg.innerHTML = '';
  const h = document.createElement('h2');
  h.textContent = 'delete';
  dlg.appendChild(h);
  const p = document.createElement('p');
  p.textContent = kids > 0
    ? `"${hit.node.title}" holds ${kids} item${kids === 1 ? '' : 's'}. drafts stay on disk.`
    : `"${hit.node.title}" has ${hit.node.words} words. the draft stays on disk.`;
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
    run();
  }, { once: true });
  cancel.addEventListener('click', () => dlg.close(), { once: true });
  openModal(dlg, cancel);
}

function createAltOutlineFlow() {
  const { dlg, input, err, goBtn, cancelBtn } = altTitleDialog('new outline', '', 'create');
  goBtn.addEventListener('click', async () => {
    const name = input.value.trim();
    if (!name) {
      err.textContent = 'name cannot be empty';
      err.hidden = false;
      return;
    }
    try {
      const id = await core.createAltOutline(name);
      dlg.close();
      setMessage(`created outline ${name}`);
      altOutlineId = id;
      altSelectedId = null;
      loadOutline();
    } catch (e) {
      err.textContent = `cannot create outline ${name}: ${errText(e)}`;
      err.hidden = false;
    }
  });
  cancelBtn.addEventListener('click', () => dlg.close(), { once: true });
  openModal(dlg, input);
}

function renameAltOutlineFlow(id) {
  const o = altOutlines.find((entry) => entry.id === id);
  if (!o) return;
  const { dlg, input, err, goBtn, cancelBtn } = altTitleDialog('rename outline', o.title, 'rename');
  goBtn.addEventListener('click', async () => {
    const name = input.value.trim();
    if (!name) {
      err.textContent = 'name cannot be empty';
      err.hidden = false;
      return;
    }
    try {
      await core.renameAltOutline(id, name);
      dlg.close();
      loadOutline();
    } catch (e) {
      err.textContent = `cannot rename outline ${o.title}: ${errText(e)}`;
      err.hidden = false;
    }
  });
  cancelBtn.addEventListener('click', () => dlg.close(), { once: true });
  openModal(dlg, input);
}

function deleteAltOutlineFlow(id) {
  const o = altOutlines.find((entry) => entry.id === id);
  if (!o) return;
  const dlg = ensureDialog('alt-outline-confirm', 'delete outline');
  dlg.innerHTML = '';
  const h = document.createElement('h2');
  h.textContent = 'delete outline';
  dlg.appendChild(h);
  const p = document.createElement('p');
  p.textContent = `remove "${o.title}" and its whole tree? drafts stay on disk.`;
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
    try {
      await core.deleteAltOutline(id);
      setMessage(`deleted outline ${o.title}`);
    } catch (err) {
      setMessage(`could not delete outline ${o.title}: ${errText(err)}`, { error: true });
      return;
    }
    if (altOutlineId === id) {
      altOutlineId = null;
      altSelectedId = null;
      altTree = null;
    }
    loadOutline();
  }, { once: true });
  cancel.addEventListener('click', () => dlg.close(), { once: true });
  openModal(dlg, cancel);
}

function renderOutlineTable(element, outline, stats) {
  element.innerHTML = '';
  if (!(outline.children || []).length) {
    const p = document.createElement('p');
    p.className = 'muted';
    p.textContent = 'no manuscript yet. build it in the binder: press a, then c, then s.';
    element.appendChild(p);
    return;
  }
  const wrap = document.createElement('div');
  wrap.className = 'table-wrap';
  const table = document.createElement('table');
  table.className = 'outline-table';
  table.setAttribute('aria-label', 'manuscript outline');
  const head = document.createElement('thead');
  const header = document.createElement('tr');
  for (const text of ['title', 'pov', 'setting', 'date', 'words', 'status']) {
    const th = document.createElement('th');
    th.textContent = text;
    th.scope = 'col';
    header.appendChild(th);
  }
  head.appendChild(header);
  table.appendChild(head);
  const body = document.createElement('tbody');
  for (const entry of outlineFlatRows(outline)) body.appendChild(outlineRow(entry));
  table.appendChild(body);
  const foot = document.createElement('tfoot');
  const total = document.createElement('tr');
  const name = document.createElement('td');
  name.textContent = `total · ${stats.scenes} scene${stats.scenes === 1 ? '' : 's'}`;
  total.appendChild(name);
  for (let i = 0; i < 3; i++) total.appendChild(document.createElement('td'));
  const words = document.createElement('td');
  words.textContent = stats.target > 0 ? `${stats.words}/${stats.target}` : `${stats.words}`;
  total.appendChild(words);
  const dotCell = document.createElement('td');
  const pct = stats.target > 0 ? Math.min(100, Math.round((stats.words / stats.target) * 100)) : 0;
  const dot = document.createElement('span');
  dot.className = `status-dot ${stats.target > 0 && stats.words >= stats.target ? 'st-done' : stats.words > 0 ? 'st-draft' : 'st-empty'}`;
  dot.textContent = '●';
  dot.title = `${pct}% of target`;
  dot.setAttribute('role', 'img');
  dot.setAttribute('aria-label', `${pct}% of target`);
  dotCell.appendChild(dot);
  total.appendChild(dotCell);
  foot.appendChild(total);
  table.appendChild(foot);
  wrap.appendChild(table);
  element.appendChild(wrap);
}

function outlineMarkSelected(id) {
  const element = document.getElementById('outline');
  if (!element) return;
  for (const tr of element.querySelectorAll('tr[aria-selected="true"]')) {
    tr.removeAttribute('aria-selected');
  }
  const tr = element.querySelector(`tr[data-id="${id}"]`);
  if (tr) tr.setAttribute('aria-selected', 'true');
}

/* Binder tree (left column). Same OutlineNodeDto data as the Outline view.
 * DOM-built (textContent, no innerHTML). Roving tabindex; keyboard:
 * up/down move, left/right collapse/expand, Enter opens scenes.
 * F2/Del/a/c/s arrive in P3.2b; collapse + menu + moves in P3.2c.
 */

let binderTree = null;
let binderFocusId = null;
let binderSelectedId = null;
let binderCollapsed = loadBinderCollapsed();

function loadBinderCollapsed() {
  try {
    const raw = localStorage.getItem('yonro.collapsed');
    const arr = raw ? JSON.parse(raw) : [];
    if (!Array.isArray(arr)) return new Set();
    return new Set(arr.filter((n) => Number.isInteger(n)));
  } catch (err) {
    return new Set();
  }
}

function saveBinderCollapsed() {
  try {
    localStorage.setItem('yonro.collapsed', JSON.stringify(Array.from(binderCollapsed)));
  } catch (err) {
    /* prefs only; ignore */
  }
}

function binderToggle(id) {
  if (binderCollapsed.has(id)) binderCollapsed.delete(id);
  else binderCollapsed.add(id);
  saveBinderCollapsed();
  renderBinderTree();
  binderFocus(id, true);
}

async function loadBinder() {
  const box = document.getElementById('binder-outline');
  try {
    binderTree = await core.outline();
    const title = document.getElementById('project-title');
    if (title) title.textContent = binderTree.title || 'untitled';
    renderBinderTree();
    await refreshFiles();
  } catch (err) {
    binderTree = null;
    if (box) {
      box.innerHTML = '';
      const p = document.createElement('p');
      p.className = 'muted';
      p.textContent = `could not load binder: ${errText(err)}`;
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
    if (binderCollapsed.has(node.id)) return;
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
  row.setAttribute('aria-selected', node.id === binderSelectedId ? 'true' : 'false');
  const hasKids = (node.children || []).length > 0;
  const collapsed = binderCollapsed.has(node.id);
  if (hasKids) row.setAttribute('aria-expanded', collapsed ? 'false' : 'true');
  const twisty = document.createElement('span');
  twisty.className = 'twisty';
  twisty.setAttribute('aria-hidden', 'true');
  twisty.textContent = !hasKids ? '·' : collapsed ? '▸' : '▾';
  twisty.addEventListener('click', (e) => {
    e.stopPropagation();
    if (hasKids) binderToggle(node.id);
    else binderFocus(node.id, true);
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
  if (node.kind === 'scene' && node.file && typeof isDocDirty === 'function' && isDocDirty(node.file)) {
    const dot = document.createElement('span');
    dot.className = 'dirty';
    dot.textContent = ' ●';
    dot.title = 'unsaved changes';
    dot.setAttribute('aria-hidden', 'true');
    row.appendChild(dot);
  }
  row.addEventListener('click', () => {
    binderFocus(node.id, true);
    binderSelect(node.id);
    if (node.kind === 'scene') openSceneDoc(node.id);
  });
  wrap.appendChild(row);
  if (hasKids && !collapsed) {
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
    row.setAttribute('aria-selected', 'false');
  }
  const row = binderRowFor(id);
  if (row) row.setAttribute('aria-selected', 'true');
  if (typeof outlineMarkSelected === 'function') outlineMarkSelected(id);
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
  } else if (e.key === 'ArrowRight') {
    e.preventDefault();
    if (id === null || id === undefined) return;
    const node = binderFind(id);
    if (!node) return;
    if ((node.children || []).length > 0 && binderCollapsed.has(id)) {
      binderToggle(id);
    } else if ((node.children || []).length > 0) {
      const first = node.children[0].id;
      binderFocus(first, true);
      binderSelect(first);
    }
  } else if (e.key === 'ArrowLeft') {
    e.preventDefault();
    if (id === null || id === undefined) return;
    const node = binderFind(id);
    if (!node) return;
    if ((node.children || []).length > 0 && !binderCollapsed.has(id)) {
      binderToggle(id);
    } else {
      const parent = binderParentOf(id);
      if (parent && binderTree && parent.id !== binderTree.id) {
        binderFocus(parent.id, true);
        binderSelect(parent.id);
      }
    }
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
  } else if (e.key === 'ContextMenu' || (e.key === 'F10' && e.shiftKey)) {
    if (id === null || id === undefined) return;
    e.preventDefault();
    binderOpenMenu(id);
  }
});

document.getElementById('binder-outline').addEventListener('contextmenu', (e) => {
  const row = e.target && e.target.closest ? e.target.closest('.row') : null;
  if (!row || row.dataset.id === undefined) return;
  e.preventDefault();
  const x = Number.isFinite(e.clientX) ? e.clientX : undefined;
  const y = Number.isFinite(e.clientY) ? e.clientY : undefined;
  binderOpenMenu(Number(row.dataset.id), x, y);
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
      setMessage(`could not rename ${node.title}: ${errText(err)}`, { error: true });
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
  binderBeginAddAt(parent, kind);
}

function binderBeginAddAt(parent, kind) {
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
      setMessage(`could not add ${kind} ${title}: ${errText(err)}`, { error: true });
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
  openModal(dlg, cancel);
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
    const node = binderFind(id);
    setMessage(`could not delete ${node ? node.title : `item ${id}`}: ${errText(err)}`, { error: true });
    binderFocus(id, true);
  }
}

/* Move up/down (the keyboard-accessible reorder; no drag and drop).
 * Index math lives here; hierarchy truth stays in core.
 */

async function binderMove(id, dir) {
  const sib = binderSiblingsOf(id);
  if (!sib.parent || sib.index === -1) return;
  const next = sib.index + dir;
  if (next < 0 || next >= sib.kids.length) {
    setMessage(dir < 0 ? 'already first' : 'already last');
    return;
  }
  try {
    binderTree = await core.moveNode(id, sib.parent.id, next);
    renderBinderTree();
    binderFocus(id, true);
    binderSelect(id);
  } catch (err) {
    const node = binderFind(id);
    setMessage(`could not move ${node ? node.title : `item ${id}`}: ${errText(err)}`, { error: true });
  }
}

/* Context menu: rename, add child, move up/down, delete. */

let binderMenuEl = null;

function binderCloseMenu() {
  if (binderMenuEl) {
    binderMenuEl.remove();
    binderMenuEl = null;
  }
}

function binderChildKind(node) {
  if (node.kind === 'chapter') return 'scene';
  if (node.kind === 'act') return 'chapter';
  if (node.kind === 'scene') return 'scene';
  return 'act';
}

function binderBeginChildAdd(id) {
  const node = binderFind(id);
  if (!node) return;
  if (node.kind === 'scene') {
    const parent = binderParentOf(id);
    if (!parent || (binderTree && parent.id === binderTree.id)) {
      setMessage('scenes live in chapters.', { error: true });
      return;
    }
    binderBeginAddAt(parent.id, 'scene');
  } else if (node.kind === 'chapter') {
    binderBeginAddAt(id, 'scene');
  } else if (node.kind === 'act') {
    binderBeginAddAt(id, 'chapter');
  } else {
    binderBeginAddAt(null, 'act');
  }
}

function binderOpenMenu(id, x, y) {
  binderCloseMenu();
  const node = binderFind(id);
  if (!node) return;
  binderFocus(id, false);
  binderSelect(id);
  const menu = document.createElement('div');
  menu.id = 'binder-menu';
  menu.setAttribute('role', 'menu');
  menu.setAttribute('aria-label', `actions for ${node.title}`);
  const childKind = binderChildKind(node);
  const items = [
    { label: 'rename', fn: () => binderBeginRename(id) },
    { label: `add ${childKind}`, fn: () => binderBeginChildAdd(id) },
    { label: 'move up', fn: () => binderMove(id, -1) },
    { label: 'move down', fn: () => binderMove(id, 1) },
    { label: 'delete', fn: () => binderRemove(id) },
  ];
  for (const item of items) {
    const btn = document.createElement('button');
    btn.setAttribute('role', 'menuitem');
    btn.textContent = item.label;
    btn.addEventListener('click', () => {
      binderCloseMenu();
      item.fn();
    });
    menu.appendChild(btn);
  }
  menu.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      binderCloseMenu();
      const back = binderRowFor(id);
      if (back) back.focus();
    }
  });
  document.body.appendChild(menu);
  const w = 190;
  const h = items.length * 34 + 12;
  const vw = Number.isFinite(window.innerWidth) ? window.innerWidth : 1024;
  const vh = Number.isFinite(window.innerHeight) ? window.innerHeight : 768;
  let left = Number.isFinite(x) ? x : 8;
  let top = Number.isFinite(y) ? y : 8;
  if (left + w > vw) left = Math.max(8, vw - w - 8);
  if (top + h > vh) top = Math.max(8, vh - h - 8);
  menu.style.left = `${left}px`;
  menu.style.top = `${top}px`;
  binderMenuEl = menu;
  const close = () => binderCloseMenu();
  document.addEventListener('click', close, { once: true });
  const first = menu.querySelector('button');
  if (first) first.focus();
}

/* Files section: scene-less workspace .md files. Click opens, mod+E focuses. */

function shortNameOf(path) {
  if (typeof shortName === 'function') return shortName(path);
  const parts = String(path || '').split('/');
  return parts[parts.length - 1] || 'untitled';
}

async function refreshFiles() {
  const box = document.getElementById('binder-files');
  if (!box) return;
  try {
    const files = await core.listFiles();
    box.innerHTML = '';
    if (!files.length) {
      const p = document.createElement('p');
      p.className = 'muted';
      p.textContent = 'no loose files. scenes live in the tree above.';
      box.appendChild(p);
      return;
    }
    for (const file of files) {
      const row = document.createElement('button');
      row.className = 'file-row';
      row.textContent = shortNameOf(file);
      row.title = file;
      row.setAttribute('aria-label', shortNameOf(file));
      row.addEventListener('click', () => {
        openDoc(file);
        closeDrawersOnNarrow();
      });
      box.appendChild(row);
    }
  } catch (err) {
    box.innerHTML = '';
    const p = document.createElement('p');
    p.className = 'muted';
    p.textContent = `could not list files: ${errText(err)}`;
    box.appendChild(p);
  }
}
