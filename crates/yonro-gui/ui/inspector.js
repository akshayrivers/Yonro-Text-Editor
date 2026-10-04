/* Yonro inspector (right column). Classic script; loaded before app.js.
 * Shows the selection of the current view. Scene forms save on blur/Enter
 * via core.setSceneMeta — never per keystroke. A bad target keeps its value
 * with an inline message. After a save the binder reloads now and the
 * outline/timeline/lore views reload lazily when next shown.
 */

let inspectorId = null;
let inspectorSaving = false;
let inspectorQueued = false;

function inspectorPlaceholder(text) {
  const box = document.getElementById('inspector');
  if (!box) return;
  box.innerHTML = '';
  const p = document.createElement('p');
  p.className = 'muted';
  p.textContent = text;
  box.appendChild(p);
}

async function showInspectorFor(id) {
  inspectorId = id;
  const box = document.getElementById('inspector');
  if (!box) return;
  if (id === null || id === undefined) {
    inspectorPlaceholder('select a scene to see its meta.');
    return;
  }
  const node = typeof binderFind === 'function' ? binderFind(id) : null;
  if (!node) {
    inspectorPlaceholder('select a scene to see its meta.');
    return;
  }
  if (node.kind !== 'scene') {
    renderStructuralInspector(box, node);
    return;
  }
  let detail = null;
  let names = [];
  try {
    const res = await Promise.all([core.getScene(id), core.lore()]);
    detail = res[0];
    names = (res[1] || []).map((e) => e.name);
  } catch (err) {
    inspectorPlaceholder(`inspector unavailable: ${err}`);
    return;
  }
  if (inspectorId !== id) return;
  renderSceneInspector(box, detail, names);
}

function inspectorCrumb(node) {
  const parts = [node.title];
  if (typeof binderParentOf === 'function') {
    let cursor = binderParentOf(node.id);
    while (cursor) {
      parts.unshift(cursor.title);
      cursor = binderParentOf(cursor.id);
    }
  }
  return parts;
}

function renderStructuralInspector(box, node) {
  box.innerHTML = '';
  const crumb = document.createElement('p');
  crumb.className = 'muted crumb';
  crumb.textContent = inspectorCrumb(node).join(' > ');
  box.appendChild(crumb);
  const head = document.createElement('h3');
  head.textContent = node.title;
  box.appendChild(head);
  const meta = document.createElement('p');
  meta.className = 'meta counts';
  meta.textContent = node.target > 0 ? `${node.words}/${node.target} words` : `${node.words} words`;
  box.appendChild(meta);
  if (node.target > 0) box.appendChild(inspectorBar(node.words, node.target));
  const hint = document.createElement('p');
  hint.className = 'muted';
  hint.textContent = 'scenes carry the meta. select one to edit.';
  box.appendChild(hint);
}

function inspectorBar(words, target) {
  const bar = document.createElement('div');
  bar.className = 'pbar';
  const fill = document.createElement('div');
  fill.style.width = `${Math.min(100, Math.round((words / target) * 100))}%`;
  bar.appendChild(fill);
  return bar;
}

function inspectorField(form, kind, name, labelText, value, opts) {
  const label = document.createElement('label');
  label.textContent = labelText;
  let input = null;
  if (kind === 'textarea') {
    input = document.createElement('textarea');
    input.rows = 3;
  } else {
    input = document.createElement('input');
    input.type = (opts && opts.type) || 'text';
    if (opts && opts.list) input.setAttribute('list', opts.list);
    if (opts && opts.min !== undefined) input.min = String(opts.min);
  }
  input.className = 'field';
  input.setAttribute('data-field', name);
  input.value = value === null || value === undefined ? '' : String(value);
  input.dataset.clean = input.value;
  label.appendChild(input);
  form.appendChild(label);
  return input;
}

function renderSceneInspector(box, detail, names) {
  box.innerHTML = '';
  const crumb = document.createElement('p');
  crumb.className = 'muted crumb';
  crumb.textContent = (detail.breadcrumb || []).join(' > ');
  box.appendChild(crumb);
  const head = document.createElement('h3');
  head.textContent = detail.title;
  box.appendChild(head);
  const meta = document.createElement('p');
  meta.className = 'meta counts';
  meta.textContent = detail.target > 0 ? `${detail.words}/${detail.target} words` : `${detail.words} words`;
  box.appendChild(meta);
  if (detail.target > 0) box.appendChild(inspectorBar(detail.words, detail.target));
  const form = document.createElement('form');
  form.className = 'inspect-form';
  form.setAttribute('aria-label', `meta for ${detail.title}`);
  form.addEventListener('submit', (e) => {
    e.preventDefault();
    saveInspectorForm(detail.id);
  });
  const fields = [
    inspectorField(form, 'input', 'pov', 'pov', detail.meta.pov, { list: 'lore-names' }),
    inspectorField(form, 'input', 'setting', 'setting', detail.meta.setting),
    inspectorField(form, 'input', 'story_date', 'story date', detail.meta.story_date),
    inspectorField(form, 'input', 'story_time', 'story time', detail.meta.story_time),
    inspectorField(form, 'textarea', 'synopsis', 'synopsis', detail.meta.synopsis),
    inspectorField(form, 'input', 'target_words', 'target words', detail.meta.target_words, { type: 'number', min: 0 }),
  ];
  const list = document.createElement('datalist');
  list.id = 'lore-names';
  for (const name of names) {
    const opt = document.createElement('option');
    opt.value = name;
    list.appendChild(opt);
  }
  form.appendChild(list);
  const err = document.createElement('p');
  err.className = 'field-error';
  err.setAttribute('aria-live', 'polite');
  err.hidden = true;
  form.appendChild(err);
  box.appendChild(form);
  const histHead = document.createElement('div');
  histHead.className = 'inspector-subhead';
  histHead.textContent = 'history';
  box.appendChild(histHead);
  const histList = document.createElement('div');
  histList.className = 'history-list';
  const loading = document.createElement('p');
  loading.className = 'muted';
  loading.textContent = 'loading…';
  histList.appendChild(loading);
  box.appendChild(histList);
  loadSceneHistory(detail.id, detail.file, histList);
  for (const input of fields) {
    input.addEventListener('blur', () => {
      if (input.dataset.skipBlur) {
        delete input.dataset.skipBlur;
        return;
      }
      saveInspectorForm(detail.id);
    });
    if (input.tagName !== 'TEXTAREA') {
      input.addEventListener('keydown', (e) => {
        if (e.key === 'Enter') {
          e.preventDefault();
          input.dataset.clean = input.value;
          saveInspectorForm(detail.id);
        } else if (e.key === 'Escape') {
          e.preventDefault();
          input.dataset.skipBlur = '1';
          input.value = input.dataset.clean || '';
          input.blur();
        }
      });
    }
  }
}

function inspectorFieldValue(name) {
  const box = document.getElementById('inspector');
  if (!box) return '';
  const el = box.querySelector(`.field[data-field="${name}"]`);
  return el ? el.value : '';
}

function inspectorShowError(text) {
  const box = document.getElementById('inspector');
  if (!box) return;
  const err = box.querySelector('.field-error');
  if (!err) return;
  if (!text) {
    err.hidden = true;
    err.textContent = '';
  } else {
    err.hidden = false;
    err.textContent = text;
  }
}

async function saveInspectorForm(id) {
  if (inspectorSaving) {
    inspectorQueued = true;
    return;
  }
  inspectorSaving = true;
  try {
    await saveInspectorFormNow(id);
    while (inspectorQueued) {
      inspectorQueued = false;
      await saveInspectorFormNow(id);
    }
  } finally {
    inspectorSaving = false;
  }
}

async function saveInspectorFormNow(id) {
  if (inspectorId !== id) return;
  const rawTarget = inspectorFieldValue('target_words').trim();
  let target = 0;
  if (rawTarget !== '') {
    target = Number(rawTarget);
    if (!Number.isInteger(target) || target < 0) {
      inspectorShowError('target must be a whole number, 0 clears it.');
      return;
    }
  }
  const meta = {
    pov: inspectorFieldValue('pov'),
    setting: inspectorFieldValue('setting'),
    story_date: inspectorFieldValue('story_date'),
    story_time: inspectorFieldValue('story_time'),
    synopsis: inspectorFieldValue('synopsis'),
    target_words: target,
  };
  try {
    await core.setSceneMeta(id, meta);
  } catch (err) {
    inspectorShowError(`save failed: ${err}`);
    return;
  }
  if (inspectorId !== id) return;
  inspectorShowError(null);
  inspectorMarkClean();
  refreshCountsLine(id, target);
  setMessage('scene meta saved');
  refreshViewsAfterMeta();
}

function inspectorMarkClean() {
  const box = document.getElementById('inspector');
  if (!box) return;
  for (const el of box.querySelectorAll('.field')) el.dataset.clean = el.value;
}

function refreshCountsLine(id, target) {
  const box = document.getElementById('inspector');
  if (!box) return;
  const node = typeof binderFind === 'function' ? binderFind(id) : null;
  const words = node ? node.words : 0;
  const line = box.querySelector('.counts');
  if (line) line.textContent = target > 0 ? `${words}/${target} words` : `${words} words`;
}

function refreshViewsAfterMeta() {
  if (typeof loadBinder === 'function') loadBinder();
  const visible = (view) => {
    const sec = document.getElementById(view);
    return Boolean(sec) && !sec.classList.contains('hidden');
  };
  if (visible('view-outline') && typeof loadOutline === 'function') loadOutline();
  if (visible('view-timeline') && typeof loadTimeline === 'function') loadTimeline();
  if (visible('view-lore') && typeof loadLore === 'function') loadLore();
}

async function showEntityInspector(id, graphNode) {
  inspectorId = id;
  const box = document.getElementById('inspector');
  if (!box) return;
  try {
    const detail = await core.getEntity(id);
    if (inspectorId !== id) return;
    renderEntityInspector(box, detail, graphNode && graphNode.neighbors ? graphNode.neighbors : null);
  } catch (err) {
    inspectorPlaceholder(`entity unavailable: ${err}`);
  }
}

function renderEntityInspector(box, detail, neighbors) {
  box.innerHTML = '';
  const head = document.createElement('div');
  head.className = 'inspector-entity-head';

  const title = document.createElement('h3');
  title.textContent = detail.name;
  head.appendChild(title);

  const badge = document.createElement('span');
  badge.className = `badge k-${detail.kind}`;
  badge.textContent = detail.kind;
  head.appendChild(badge);

  box.appendChild(head);

  if (detail.aliases && detail.aliases.length > 0) {
    const aliases = document.createElement('p');
    aliases.className = 'muted';
    aliases.textContent = `also: ${detail.aliases.join(', ')}`;
    box.appendChild(aliases);
  }

  const sheetHeading = document.createElement('div');
  sheetHeading.className = 'inspector-subhead';
  sheetHeading.textContent = 'lore sheet';
  box.appendChild(sheetHeading);

  const sheetText = document.createElement('p');
  sheetText.className = 'inspector-sheet-prose';
  sheetText.textContent = detail.sheet || 'no sheet yet.';
  box.appendChild(sheetText);

  if (neighbors && neighbors.length > 0) {
    const nbHeading = document.createElement('div');
    nbHeading.className = 'inspector-subhead';
    nbHeading.textContent = `ranked neighbors (${neighbors.length})`;
    box.appendChild(nbHeading);

    const nbList = document.createElement('div');
    nbList.className = 'scene-links-list';
    for (const nb of neighbors) {
      const nbBtn = document.createElement('button');
      nbBtn.className = 'scene-link-btn';
      nbBtn.textContent = `${nb.name} (${nb.weight})`;
      nbBtn.addEventListener('click', () => {
        showEntityInspector(nb.id);
      });
      nbList.appendChild(nbBtn);
    }
    box.appendChild(nbList);
  } else {
    const mentionCount = (detail.mention_scenes || []).length;
    const povCount = (detail.pov_scene_links || []).length;
    const stats = document.createElement('p');
    stats.className = 'muted';
    stats.textContent = `POV in ${povCount} scenes · mentioned in ${mentionCount} scenes`;
    box.appendChild(stats);
  }

  const openLoreBtn = document.createElement('button');
  openLoreBtn.className = 'btn';
  openLoreBtn.textContent = 'open in Lore';
  openLoreBtn.style.marginTop = '12px';
  openLoreBtn.addEventListener('click', () => {
    if (typeof show === 'function') show('lore');
    if (typeof selectLoreEntity === 'function') selectLoreEntity(detail.id);
  });
  box.appendChild(openLoreBtn);
}

/* Scene history (P5.6): snapshot list (timestamp + words) with restore.
 * Restores adopt into the open buffer when present (dirty until saved),
 * otherwise open the scene from the restored disk text.
 */

function historyDisplayName(name) {
  const stem = String(name || '').replace(/\.md$/, '');
  const secs = Number(stem.split('-')[0]);
  if (Number.isFinite(secs) && secs > 0) {
    try {
      return new Date(secs * 1000).toLocaleString();
    } catch (err) {
      void err;
    }
  }
  return name;
}

async function loadSceneHistory(sceneId, file, list) {
  list.innerHTML = '';
  let entries = [];
  try {
    entries = await core.history(sceneId);
  } catch (err) {
    const p = document.createElement('p');
    p.className = 'muted';
    p.textContent = `history unavailable: ${err}`;
    list.appendChild(p);
    return;
  }
  if (!entries.length) {
    const p = document.createElement('p');
    p.className = 'muted';
    p.textContent = 'no snapshots yet. snapshots land on save, ten minutes apart.';
    list.appendChild(p);
    return;
  }
  for (const entry of entries) {
    const row = document.createElement('div');
    row.className = 'history-row';
    const label = document.createElement('span');
    label.textContent = `${historyDisplayName(entry.name)} · ${entry.words} words`;
    row.appendChild(label);
    const btn = document.createElement('button');
    btn.textContent = 'restore';
    btn.setAttribute('aria-label', `restore snapshot ${entry.name}`);
    btn.addEventListener('click', () => restoreSceneSnapshot(sceneId, file, entry.name));
    row.appendChild(btn);
    list.appendChild(row);
  }
}

async function restoreSceneSnapshot(sceneId, file, name) {
  let text = null;
  try {
    text = await core.restoreSnapshot(sceneId, name);
  } catch (err) {
    setMessage(`restore failed: ${err}`, { error: true });
    return;
  }
  let target = null;
  for (const [id, doc] of docs) {
    if (doc.path === file) {
      target = id;
      break;
    }
  }
  if (target === null) {
    if (typeof openSceneDoc === 'function') openSceneDoc(sceneId);
  } else {
    if (target !== activeDoc) await activateDoc(target);
    applyingRemote = true;
    editor.value = text;
    applyingRemote = false;
    docCache.set(target, text);
    syncPending = false;
    markDirtyLocal(target);
    scheduleSync();
    refreshMentionsNow();
    renderTabs();
    updateCaret();
  }
  setMessage(`restored ${historyDisplayName(name)} — save to keep it.`);
  if (typeof refreshBinder === 'function') refreshBinder();
  showInspectorFor(sceneId);
}
