/* Yonro lore view (center). Classic script.
 * List (left, filter input, grouped by kind) + detail (right).
 * Detail: name (inline edit), kind badge, aliases as removable chips + add input,
 * sheet textarea (save on blur), "POV in" + "mentioned in" scene lists (click => open scene).
 * [+ new entity] with kind select. Delete = confirm <dialog> naming the entity
 * and how many scenes mention it. Empty: "no entities yet. set a scene POV or type @Name."
 */

let loreEntities = [];
let loreSelectedId = null;
let loreFilterText = '';

const LORE_KINDS = ['character', 'place', 'faction', 'item', 'lore'];

async function loadLore() {
  const container = document.getElementById('lore');
  if (!container) return;
  try {
    loreEntities = await core.lore();
  } catch (err) {
    container.innerHTML = `<p class="muted">could not load lore: ${esc(errText(err))}</p>`;
    return;
  }
  if (!loreEntities || loreEntities.length === 0) {
    loreSelectedId = null;
    renderLoreEmpty(container);
    return;
  }
  if (loreSelectedId === null || !loreEntities.some((e) => e.id === loreSelectedId)) {
    loreSelectedId = loreEntities[0].id;
  }
  renderLoreView(container);
}

function selectLoreEntity(id) {
  loreSelectedId = id;
  const container = document.getElementById('lore');
  if (container) {
    if (!loreEntities.some((e) => e.id === id)) {
      loadLore();
    } else {
      renderLoreView(container);
    }
  }
}

function renderLoreEmpty(container) {
  container.innerHTML = '';
  const wrap = document.createElement('div');
  wrap.className = 'lore-empty';
  const p = document.createElement('p');
  p.className = 'muted';
  p.textContent = 'no entities yet. set a scene POV or type @Name.';
  wrap.appendChild(p);
  const btn = document.createElement('button');
  btn.className = 'btn';
  btn.textContent = '+ new entity';
  btn.addEventListener('click', () => showAddEntityDialog());
  wrap.appendChild(btn);
  container.appendChild(wrap);
}

function renderLoreView(container) {
  container.innerHTML = '';
  const wrap = document.createElement('div');
  wrap.className = 'lore-wrap';

  // Left sidebar: filter + add + grouped list
  const sidebar = document.createElement('div');
  sidebar.className = 'lore-sidebar';

  const header = document.createElement('div');
  header.className = 'lore-header-row';

  const filterInput = document.createElement('input');
  filterInput.type = 'search';
  filterInput.className = 'lore-search';
  filterInput.placeholder = 'filter entities…';
  filterInput.value = loreFilterText;
  filterInput.setAttribute('aria-label', 'Filter entities');
  filterInput.addEventListener('input', () => {
    loreFilterText = filterInput.value.trim().toLowerCase();
    updateLoreListOnly(sidebar);
  });
  header.appendChild(filterInput);

  const addBtn = document.createElement('button');
  addBtn.className = 'btn';
  addBtn.textContent = '+ new';
  addBtn.title = 'Add new entity';
  addBtn.addEventListener('click', () => showAddEntityDialog());
  header.appendChild(addBtn);

  sidebar.appendChild(header);

  const listContainer = document.createElement('div');
  listContainer.className = 'lore-list-container';
  listContainer.id = 'lore-list-container';
  sidebar.appendChild(listContainer);
  populateLoreList(listContainer);

  wrap.appendChild(sidebar);

  // Right detail pane
  const detailPane = document.createElement('div');
  detailPane.className = 'lore-detail';
  detailPane.id = 'lore-detail-pane';
  wrap.appendChild(detailPane);

  container.appendChild(wrap);

  if (loreSelectedId !== null) {
    loadAndRenderLoreDetail(loreSelectedId, detailPane);
  }
}

function updateLoreListOnly(sidebar) {
  const listContainer = sidebar.querySelector('#lore-list-container');
  if (listContainer) populateLoreList(listContainer);
}

function populateLoreList(listContainer) {
  listContainer.innerHTML = '';
  const needle = loreFilterText.toLowerCase();
  const filtered = loreEntities.filter((e) => {
    if (!needle) return true;
    if (e.name.toLowerCase().includes(needle)) return true;
    return (e.aliases || []).some((a) => a.toLowerCase().includes(needle));
  });

  if (filtered.length === 0) {
    const emptyP = document.createElement('p');
    emptyP.className = 'muted empty-search';
    emptyP.textContent = 'no matching entities.';
    listContainer.appendChild(emptyP);
    return;
  }

  for (const kind of LORE_KINDS) {
    const group = filtered.filter((e) => e.kind.toLowerCase() === kind);
    if (group.length === 0) continue;

    const heading = document.createElement('div');
    heading.className = 'lore-group-heading';
    heading.textContent = `${kind} (${group.length})`;
    listContainer.appendChild(heading);

    for (const entity of group) {
      const row = document.createElement('div');
      row.className = `lore-row ${entity.id === loreSelectedId ? 'active' : ''}`;
      row.setAttribute('role', 'button');
      row.tabIndex = 0;

      const dot = document.createElement('span');
      dot.className = `lore-dot k-${entity.kind}`;
      dot.textContent = '●';
      row.appendChild(dot);

      const nameSpan = document.createElement('span');
      nameSpan.className = 'lore-row-name';
      nameSpan.textContent = entity.name;
      row.appendChild(nameSpan);

      row.addEventListener('click', () => {
        selectLoreEntity(entity.id);
      });
      row.addEventListener('keydown', (e) => {
        if (e.key === 'Enter') {
          e.preventDefault();
          selectLoreEntity(entity.id);
        }
      });

      listContainer.appendChild(row);
    }
  }
}

async function loadAndRenderLoreDetail(id, detailPane) {
  detailPane.innerHTML = '<p class="muted">loading…</p>';
  try {
    const detail = await core.getEntity(id);
    if (loreSelectedId !== id) return;
    renderLoreDetail(detail, detailPane);
    if (window.innerWidth < 1000 && typeof showEntityInspector === 'function') {
      showEntityInspector(id);
    }
  } catch (err) {
    detailPane.innerHTML = `<p class="muted">could not load entity ${id} detail: ${esc(errText(err))}</p>`;
  }
}

function renderLoreDetail(detail, pane) {
  pane.innerHTML = '';

  const header = document.createElement('div');
  header.className = 'lore-detail-header';

  // Inline name edit
  const nameInput = document.createElement('input');
  nameInput.type = 'text';
  nameInput.className = 'lore-name-edit';
  nameInput.value = detail.name;
  nameInput.dataset.clean = detail.name;
  nameInput.setAttribute('aria-label', 'Entity name');

  const saveName = async () => {
    const trimmed = nameInput.value.trim();
    if (!trimmed || trimmed === nameInput.dataset.clean) {
      nameInput.value = nameInput.dataset.clean;
      return;
    }
    try {
      await core.updateEntity(detail.id, { name: trimmed });
      nameInput.dataset.clean = trimmed;
      toast('name updated');
      await refreshLoreListPreservingSelection();
    } catch (err) {
      toastError(`could not rename ${detail.name}: ${errText(err)}`);
      nameInput.value = nameInput.dataset.clean;
    }
  };

  nameInput.addEventListener('blur', saveName);
  nameInput.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      nameInput.blur();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      nameInput.value = nameInput.dataset.clean;
      nameInput.blur();
    }
  });
  header.appendChild(nameInput);

  // Kind badge
  const badge = document.createElement('span');
  badge.className = `badge k-${detail.kind}`;
  badge.textContent = detail.kind;
  header.appendChild(badge);

  // Delete button
  const delBtn = document.createElement('button');
  delBtn.className = 'btn-del';
  delBtn.textContent = 'delete';
  delBtn.title = 'Delete entity';
  delBtn.addEventListener('click', () => showDeleteEntityDialog(detail));
  header.appendChild(delBtn);

  pane.appendChild(header);

  // Aliases section
  const aliasSec = document.createElement('div');
  aliasSec.className = 'lore-section';
  const aliasLabel = document.createElement('label');
  aliasLabel.textContent = 'aliases';
  aliasSec.appendChild(aliasLabel);

  const chipsWrap = document.createElement('div');
  chipsWrap.className = 'lore-chips';

  const currentAliases = [...(detail.aliases || [])];

  const renderChips = () => {
    chipsWrap.innerHTML = '';
    for (const alias of currentAliases) {
      const chip = document.createElement('span');
      chip.className = 'chip';
      chip.textContent = alias;

      const removeBtn = document.createElement('button');
      removeBtn.className = 'chip-remove';
      removeBtn.textContent = '×';
      removeBtn.setAttribute('aria-label', `remove alias ${alias}`);
      removeBtn.addEventListener('click', async (e) => {
        e.stopPropagation();
        const next = currentAliases.filter((a) => a !== alias);
        try {
          await core.updateEntity(detail.id, { aliases: next });
          detail.aliases = next;
          currentAliases.length = 0;
          currentAliases.push(...next);
          renderChips();
          toast('alias removed');
          await refreshLoreListPreservingSelection();
        } catch (err) {
          toastError(`could not remove alias ${alias} from ${detail.name}: ${errText(err)}`);
        }
      });
      chip.appendChild(removeBtn);
      chipsWrap.appendChild(chip);
    }

    const addInput = document.createElement('input');
    addInput.type = 'text';
    addInput.className = 'alias-add-input';
    addInput.placeholder = '+ add alias';
    addInput.setAttribute('aria-label', 'Add alias');

    const handleAdd = async () => {
      const text = addInput.value.trim();
      if (!text) return;
      if (currentAliases.some((a) => a.toLowerCase() === text.toLowerCase())) {
        toastError('alias already exists');
        addInput.value = '';
        return;
      }
      const next = [...currentAliases, text];
      try {
        await core.updateEntity(detail.id, { aliases: next });
        detail.aliases = next;
        currentAliases.push(text);
        renderChips();
        toast('alias added');
        await refreshLoreListPreservingSelection();
      } catch (err) {
        toastError(`could not add alias to ${detail.name}: ${errText(err)}`);
      }
    };

    addInput.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') {
        e.preventDefault();
        handleAdd();
      }
    });
    chipsWrap.appendChild(addInput);
  };

  renderChips();
  aliasSec.appendChild(chipsWrap);
  pane.appendChild(aliasSec);

  // Sheet textarea
  const sheetSec = document.createElement('div');
  sheetSec.className = 'lore-section';
  const sheetLabel = document.createElement('label');
  sheetLabel.textContent = 'lore sheet';
  sheetSec.appendChild(sheetLabel);

  const sheetArea = document.createElement('textarea');
  sheetArea.className = 'lore-sheet';
  sheetArea.rows = 8;
  sheetArea.placeholder = 'appearance, history, secrets…';
  sheetArea.value = detail.sheet || '';
  sheetArea.dataset.clean = detail.sheet || '';

  sheetArea.addEventListener('blur', async () => {
    if (sheetArea.value !== sheetArea.dataset.clean) {
      try {
        await core.updateEntity(detail.id, { sheet: sheetArea.value });
        sheetArea.dataset.clean = sheetArea.value;
        toast('sheet saved');
      } catch (err) {
        toastError(`could not save sheet for ${detail.name}: ${errText(err)}`);
      }
    }
  });
  sheetSec.appendChild(sheetArea);
  pane.appendChild(sheetSec);

  // POV in scene list
  const povSec = document.createElement('div');
  povSec.className = 'lore-section';
  const povHeading = document.createElement('div');
  povHeading.className = 'lore-subhead';
  const povCount = (detail.pov_scene_links || []).length;
  povHeading.textContent = `POV in (${povCount})`;
  povSec.appendChild(povHeading);

  if (povCount === 0) {
    const noneP = document.createElement('p');
    noneP.className = 'muted';
    noneP.textContent = 'none';
    povSec.appendChild(noneP);
  } else {
    const povList = document.createElement('div');
    povList.className = 'scene-links-list';
    for (const ref of detail.pov_scene_links) {
      const link = document.createElement('button');
      link.className = 'scene-link-btn';
      link.textContent = ref.title;
      link.addEventListener('click', async () => {
        if (typeof openSceneDoc === 'function') {
          await openSceneDoc(ref.scene_id);
          show('write');
        }
      });
      povList.appendChild(link);
    }
    povSec.appendChild(povList);
  }
  pane.appendChild(povSec);

  // Mentioned in scene list
  const mentionSec = document.createElement('div');
  mentionSec.className = 'lore-section';
  const mentionHeading = document.createElement('div');
  mentionHeading.className = 'lore-subhead';
  const mentionCount = (detail.mention_scenes || []).length;
  mentionHeading.textContent = `mentioned in (${mentionCount})`;
  mentionSec.appendChild(mentionHeading);

  if (mentionCount === 0) {
    const noneP = document.createElement('p');
    noneP.className = 'muted';
    noneP.textContent = 'none';
    mentionSec.appendChild(noneP);
  } else {
    const mentionList = document.createElement('div');
    mentionList.className = 'scene-links-list';
    for (const item of detail.mention_scenes) {
      const link = document.createElement('button');
      link.className = 'scene-link-btn';
      link.textContent = `${item.title} (${item.count})`;
      link.addEventListener('click', async () => {
        if (typeof openSceneDoc === 'function') {
          await openSceneDoc(item.scene_id);
          show('write');
        }
      });
      mentionList.appendChild(link);
    }
    mentionSec.appendChild(mentionList);
  }
  pane.appendChild(mentionSec);
}

async function refreshLoreListPreservingSelection() {
  try {
    loreEntities = await core.lore();
    const sidebar = document.querySelector('.lore-sidebar');
    if (sidebar) updateLoreListOnly(sidebar);
  } catch (err) {
    void err;
  }
}

function showAddEntityDialog() {
  let dlg = document.getElementById('lore-add-dialog');
  if (!dlg) {
    dlg = document.createElement('dialog');
    dlg.id = 'lore-add-dialog';
    document.body.appendChild(dlg);
  }
  dlg.innerHTML = '';

  const h = document.createElement('h2');
  h.textContent = 'new entity';
  dlg.appendChild(h);

  const form = document.createElement('form');
  form.className = 'dialog-form';

  const kindLabel = document.createElement('label');
  kindLabel.textContent = 'kind';
  const select = document.createElement('select');
  for (const k of LORE_KINDS) {
    const opt = document.createElement('option');
    opt.value = k;
    opt.textContent = k;
    select.appendChild(opt);
  }
  kindLabel.appendChild(select);
  form.appendChild(kindLabel);

  const nameLabel = document.createElement('label');
  nameLabel.textContent = 'name';
  const input = document.createElement('input');
  input.type = 'text';
  input.required = true;
  input.placeholder = 'e.g. Mara Stone';
  nameLabel.appendChild(input);
  form.appendChild(nameLabel);

  const errP = document.createElement('p');
  errP.className = 'field-error';
  errP.hidden = true;
  form.appendChild(errP);

  const btnRow = document.createElement('div');
  btnRow.className = 'dialog-actions';

  const cancelBtn = document.createElement('button');
  cancelBtn.type = 'button';
  cancelBtn.textContent = 'cancel';
  cancelBtn.addEventListener('click', () => dlg.close());
  btnRow.appendChild(cancelBtn);

  const submitBtn = document.createElement('button');
  submitBtn.type = 'submit';
  submitBtn.className = 'btn-primary';
  submitBtn.textContent = 'add entity';
  btnRow.appendChild(submitBtn);

  form.appendChild(btnRow);

  form.addEventListener('submit', async (e) => {
    e.preventDefault();
    const name = input.value.trim();
    if (!name) return;
    try {
      const added = await core.addEntity(select.value, name);
      dlg.close();
      loreSelectedId = added.id;
      toast(`added ${added.name}`);
      await loadLore();
    } catch (err) {
      errP.textContent = `could not add entity ${name}: ${errText(err)}`;
      errP.hidden = false;
    }
  });

  dlg.appendChild(form);
  openModal(dlg, input);
}

function showDeleteEntityDialog(detail) {
  let dlg = document.getElementById('lore-delete-dialog');
  if (!dlg) {
    dlg = document.createElement('dialog');
    dlg.id = 'lore-delete-dialog';
    document.body.appendChild(dlg);
  }
  dlg.innerHTML = '';

  const h = document.createElement('h2');
  h.textContent = 'delete entity';
  dlg.appendChild(h);

  const mentionTotal = (detail.mention_scenes || []).reduce((acc, s) => acc + (s.count || 1), 0);
  const scenesCount = (detail.mention_scenes || []).length;
  const p = document.createElement('p');
  const mentionText = scenesCount > 0
    ? `mentioned in ${scenesCount} scene${scenesCount === 1 ? '' : 's'} (${mentionTotal} total mentions).`
    : 'not mentioned in any scenes.';
  p.textContent = `remove "${detail.name}"? ${mentionText}`;
  dlg.appendChild(p);

  const btnRow = document.createElement('div');
  btnRow.className = 'dialog-actions';

  const cancelBtn = document.createElement('button');
  cancelBtn.type = 'button';
  cancelBtn.textContent = 'cancel';
  cancelBtn.addEventListener('click', () => dlg.close());
  btnRow.appendChild(cancelBtn);

  const confirmBtn = document.createElement('button');
  confirmBtn.type = 'button';
  confirmBtn.className = 'danger';
  confirmBtn.textContent = 'delete';
  confirmBtn.addEventListener('click', async () => {
    dlg.close();
    try {
      await core.removeEntity(detail.id);
      toast(`removed ${detail.name}`);
      loreSelectedId = null;
      await loadLore();
    } catch (err) {
      toastError(`could not delete ${detail.name}: ${errText(err)}`);
    }
  });
  btnRow.appendChild(confirmBtn);

  dlg.appendChild(btnRow);
  openModal(dlg, cancelBtn);
}
