/* Yonro start screen + workspaces (P5.1). Classic script; loaded before app.js.
 * Full-center overlay when there is no project (or on "switch").
 * Create/open use an in-app dialog with a path text input; backend errors
 * render inline. Load warnings render as a dismissible banner.
 * Calls core.workspace/openWorkspace/createWorkspace only.
 */

let yonroWorkspace = null;

async function loadWorkspace() {
  try {
    yonroWorkspace = await core.workspace();
  } catch (err) {
    setMessage(`could not load workspace: ${errText(err)}`, { error: true });
    return;
  }
  const title = document.getElementById('project-title');
  if (title) title.textContent = (yonroWorkspace && yonroWorkspace.title) || 'untitled';
  renderWorkspaceWarnings();
  if (!yonroWorkspace.has_project) showStartScreen(true);
}

function renderWorkspaceWarnings() {
  const bar = document.getElementById('workspace-warnings');
  if (!bar) return;
  bar.innerHTML = '';
  const warnings = (yonroWorkspace && yonroWorkspace.warnings) || [];
  if (!warnings.length) {
    bar.hidden = true;
    return;
  }
  bar.hidden = false;
  for (const text of warnings) {
    const row = document.createElement('div');
    row.className = 'workspace-warning';
    const msg = document.createElement('span');
    msg.textContent = text;
    const dismiss = document.createElement('button');
    dismiss.textContent = 'dismiss';
    dismiss.setAttribute('aria-label', `dismiss warning: ${text}`);
    dismiss.addEventListener('click', () => {
      row.remove();
      if (!bar.querySelector('.workspace-warning')) bar.hidden = true;
    });
    row.appendChild(msg);
    row.appendChild(dismiss);
    bar.appendChild(row);
  }
}

function showStartScreen(on) {
  const start = document.getElementById('start-screen');
  if (!start) return;
  start.hidden = !on;
  if (!on) return;
  renderStartRecent();
  const back = document.getElementById('start-back');
  if (back) {
    const show = Boolean(yonroWorkspace && yonroWorkspace.has_project);
    back.hidden = !show;
    back.textContent = show ? `back to ${(yonroWorkspace && yonroWorkspace.title) || 'project'}` : 'back';
  }
  const first = document.getElementById('start-create');
  if (first) first.focus();
}

function renderStartRecent() {
  const box = document.getElementById('start-recent');
  if (!box) return;
  box.innerHTML = '';
  const recent = (yonroWorkspace && yonroWorkspace.recent) || [];
  if (!recent.length) {
    const p = document.createElement('p');
    p.className = 'muted';
    p.textContent = 'no recent projects yet. create one above, or open by path.';
    box.appendChild(p);
    return;
  }
  const head = document.createElement('p');
  head.className = 'muted';
  head.textContent = 'recent:';
  box.appendChild(head);
  for (const entry of recent) {
    const btn = document.createElement('button');
    btn.className = 'start-recent-row';
    btn.textContent = `${entry.title} — ${entry.path} — ${entry.last_opened}`;
    btn.title = entry.path;
    btn.addEventListener('click', () => switchWorkspace(entry.path, false));
    box.appendChild(btn);
  }
}

async function switchWorkspace(path, force) {
  let next = null;
  try {
    next = await core.openWorkspace(path, force);
  } catch (err) {
    if (!force && String(err).indexOf('unsaved changes') !== -1) {
      const choice = await confirmForceSwitch();
      if (choice === 'save') {
        const ok = typeof saveAllQuit === 'function' ? await saveAllQuit() : true;
        if (!ok) return;
        return switchWorkspace(path, true);
      }
      if (choice === 'force') return switchWorkspace(path, true);
      return;
    }
    setMessage(`could not open workspace ${path}: ${errText(err)}`, { error: true });
    return;
  }
  yonroWorkspace = next;
  afterWorkspaceSwitch(`opened ${(yonroWorkspace && yonroWorkspace.title) || path}`);
}

function afterWorkspaceSwitch(note) {
  const title = document.getElementById('project-title');
  if (title) title.textContent = (yonroWorkspace && yonroWorkspace.title) || 'untitled';
  renderWorkspaceWarnings();
  showStartScreen(false);
  setMessage(note);
  if (typeof refreshBinder === 'function') refreshBinder();
  if (typeof refreshSession === 'function') refreshSession();
  if (typeof show === 'function') show('write');
}

function confirmForceSwitch() {
  const dlg = ensureDialog('workspace-force', 'unsaved changes');
  dlg.innerHTML = '';
  const h = document.createElement('h2');
  h.textContent = 'unsaved changes';
  dlg.appendChild(h);
  const dirty = typeof dirtyList === 'function' ? dirtyList() : [];
  const p = document.createElement('p');
  p.textContent = `${dirty.length} file(s) have unsaved changes. switching drops open buffers.`;
  dlg.appendChild(p);
  const row = document.createElement('div');
  const saveBtn = document.createElement('button');
  saveBtn.textContent = 'save then switch';
  const forceBtn = document.createElement('button');
  forceBtn.textContent = 'switch anyway';
  const cancelBtn = document.createElement('button');
  cancelBtn.textContent = 'cancel';
  row.appendChild(saveBtn);
  row.appendChild(forceBtn);
  row.appendChild(cancelBtn);
  dlg.appendChild(row);
  return new Promise((resolve) => {
    saveBtn.addEventListener('click', () => { dlg.close(); resolve('save'); }, { once: true });
    forceBtn.addEventListener('click', () => { dlg.close(); resolve('force'); }, { once: true });
    cancelBtn.addEventListener('click', () => { dlg.close(); resolve('cancel'); }, { once: true });
    dlg.addEventListener('cancel', () => resolve('cancel'), { once: true });
    openModal(dlg, saveBtn);
  });
}

function workspaceDialog(mode) {
  const dlg = ensureDialog('workspace-dialog', mode === 'create' ? 'create project' : 'open project');
  dlg.innerHTML = '';
  const h = document.createElement('h2');
  h.textContent = mode === 'create' ? 'create project' : 'open project';
  dlg.appendChild(h);
  let titleInput = null;
  if (mode === 'create') {
    const titleLabel = document.createElement('label');
    titleLabel.textContent = 'title';
    titleInput = document.createElement('input');
    titleInput.type = 'text';
    titleInput.placeholder = 'Sample Novel';
    titleInput.setAttribute('aria-label', 'project title');
    titleLabel.appendChild(titleInput);
    dlg.appendChild(titleLabel);
  }
  const pathLabel = document.createElement('label');
  pathLabel.textContent = 'folder path';
  const pathInput = document.createElement('input');
  pathInput.type = 'text';
  pathInput.placeholder = '/home/you/novels/sample-novel';
  pathInput.setAttribute('aria-label', 'workspace folder path');
  pathLabel.appendChild(pathInput);
  dlg.appendChild(pathLabel);
  const err = document.createElement('p');
  err.className = 'field-error';
  err.setAttribute('aria-live', 'polite');
  err.hidden = true;
  dlg.appendChild(err);
  const row = document.createElement('div');
  const goBtn = document.createElement('button');
  goBtn.textContent = mode === 'create' ? 'create' : 'open';
  const cancelBtn = document.createElement('button');
  cancelBtn.textContent = 'cancel';
  row.appendChild(goBtn);
  row.appendChild(cancelBtn);
  dlg.appendChild(row);
  const fail = (message) => {
    err.textContent = message;
    err.hidden = false;
  };
  goBtn.addEventListener('click', async () => {
    const path = pathInput.value.trim();
    if (!path) {
      fail('path cannot be empty');
      return;
    }
    try {
      if (mode === 'create') {
        const title = titleInput ? titleInput.value.trim() : '';
        if (!title) {
          fail('title cannot be empty');
          return;
        }
        yonroWorkspace = await core.createWorkspace(path, title);
      } else {
        await switchWorkspace(path, false);
        dlg.close();
        return;
      }
    } catch (e) {
      fail(`cannot ${mode} workspace ${path}: ${errText(e)}`);
      return;
    }
    dlg.close();
    afterWorkspaceSwitch(`created ${(yonroWorkspace && yonroWorkspace.title) || path}`);
  });
  cancelBtn.addEventListener('click', () => dlg.close(), { once: true });
  openModal(dlg, titleInput || pathInput);
}

document.getElementById('start-create').addEventListener('click', () => workspaceDialog('create'));
document.getElementById('start-open').addEventListener('click', () => workspaceDialog('open'));
document.getElementById('start-back').addEventListener('click', () => showStartScreen(false));
document.getElementById('workspace-switch').addEventListener('click', () => showStartScreen(true));
loadWorkspace();
