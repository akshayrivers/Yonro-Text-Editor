/* Yonro shell bootstrap. Classic script, loaded last.
 * Owns nav, binder/inspector panels, zen, keymap, prose prefs.
 * Calls show('write') on boot. No backend access except via core.
 */

const views = ['write', 'outline', 'graph', 'timeline', 'lore'];

function show(name) {
  for (const v of views) {
    const sec = document.getElementById(`view-${v}`);
    if (sec) sec.classList.toggle('hidden', v !== name);
  }
  for (const btn of document.querySelectorAll('#topnav [data-view]')) {
    const on = btn.dataset.view === name;
    btn.classList.toggle('active', on);
    btn.setAttribute('aria-selected', on ? 'true' : 'false');
    btn.tabIndex = on ? 0 : -1;
  }
  if (name === 'outline' && typeof loadOutline === 'function') loadOutline();
  if (name === 'graph' && typeof loadGraph === 'function') loadGraph();
  if (name === 'timeline' && typeof loadTimeline === 'function') loadTimeline();
  if (name === 'lore' && typeof loadLore === 'function') loadLore();
  syncDrawers();
}

for (const btn of document.querySelectorAll('#topnav [data-view]')) {
  btn.addEventListener('click', () => show(btn.dataset.view));
}

/* ---------- binder (read-only outline tree + files; editing lands later) --- */
async function refreshBinder() {
  const box = document.getElementById('binder-outline');
  try {
    const outline = await core.outline();
    const title = document.getElementById('project-title');
    if (title) title.textContent = outline.title || 'untitled';
    if (!box) return;
    const kids = (outline.children || []).map((k) => renderNode(k, true)).join('');
    if (!kids) {
      box.innerHTML = '<p class="muted">no scenes yet. add acts in the terminal outline.</p>';
      return;
    }
    box.innerHTML = kids;
    box.querySelectorAll('[data-file]').forEach((row) => {
      row.tabIndex = 0;
      row.setAttribute('role', 'button');
      row.addEventListener('click', () => {
        openDoc(row.dataset.file);
        closeDrawersOnNarrow();
      });
      row.addEventListener('keydown', (e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          openDoc(row.dataset.file);
          closeDrawersOnNarrow();
        }
      });
    });
  } catch (err) {
    if (box) box.innerHTML = `<p class="muted">binder unavailable: ${esc(err)}</p>`;
  }
}

function laterMsg(what) {
  setMessage(`${what} lands after P1 — outline editing stays in the terminal for now.`);
}

/* ---------- panels: collapse + draggable widths + drawers ------------------ */
function applyPanelPrefs() {
  try {
    const bw = localStorage.getItem('yonro.binderW');
    const iw = localStorage.getItem('yonro.inspectorW');
    if (bw) document.documentElement.style.setProperty('--binder-w', `${clampW(Number(bw))}px`);
    if (iw) document.documentElement.style.setProperty('--inspector-w', `${clampW(Number(iw))}px`);
    if (localStorage.getItem('yonro.binderHidden') === '1') document.body.classList.add('hide-binder');
    if (localStorage.getItem('yonro.inspectorHidden') === '1') document.body.classList.add('hide-inspector');
  } catch (err) {
    void err;
  }
}

function clampW(n) {
  if (!Number.isFinite(n)) return 260;
  return Math.min(420, Math.max(200, Math.round(n)));
}

function toggleBinder() {
  document.body.classList.toggle('hide-binder');
  try {
    localStorage.setItem('yonro.binderHidden', document.body.classList.contains('hide-binder') ? '1' : '0');
  } catch (err) {
    void err;
  }
  syncDrawers();
}

function toggleInspector() {
  document.body.classList.toggle('hide-inspector');
  try {
    localStorage.setItem('yonro.inspectorHidden', document.body.classList.contains('hide-inspector') ? '1' : '0');
  } catch (err) {
    void err;
  }
  syncDrawers();
}

function isNarrowInspector() {
  return window.matchMedia && window.matchMedia('(max-width: 999px)').matches;
}

function isNarrowBinder() {
  return window.matchMedia && window.matchMedia('(max-width: 799px)').matches;
}

function syncDrawers() {
  const scrim = document.getElementById('scrim');
  if (!scrim) return;
  const binderOpen = !document.body.classList.contains('hide-binder');
  const inspOpen = !document.body.classList.contains('hide-inspector');
  const need = (isNarrowInspector() && inspOpen) || (isNarrowBinder() && binderOpen);
  scrim.hidden = !need;
}

function closeDrawersOnNarrow() {
  if (isNarrowBinder() && !document.body.classList.contains('hide-binder')) toggleBinder();
  else if (isNarrowInspector() && !document.body.classList.contains('hide-inspector')) toggleInspector();
  syncDrawers();
}

function bindSplitter(elmId, which) {
  const split = document.getElementById(elmId);
  if (!split) return;
  let startX = 0;
  let startW = 0;
  const prop = which === 'binder' ? '--binder-w' : '--inspector-w';
  const key = which === 'binder' ? 'yonro.binderW' : 'yonro.inspectorW';
  const cur = () => {
    const v = getComputedStyle(document.documentElement).getPropertyValue(prop);
    return clampW(parseFloat(v) || 260);
  };
  split.addEventListener('pointerdown', (e) => {
    startX = e.clientX;
    startW = cur();
    split.setPointerCapture(e.pointerId);
    const move = (ev) => {
      const dx = ev.clientX - startX;
      const w = clampW(startW + (which === 'binder' ? dx : -dx));
      document.documentElement.style.setProperty(prop, `${w}px`);
    };
    const up = () => {
      split.removeEventListener('pointermove', move);
      split.removeEventListener('pointerup', up);
      try {
        localStorage.setItem(key, String(cur()));
      } catch (err) {
        void err;
      }
    };
    split.addEventListener('pointermove', move);
    split.addEventListener('pointerup', up);
  });
  split.addEventListener('keydown', (e) => {
    if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
    e.preventDefault();
    const d = e.key === 'ArrowRight' ? 10 : -10;
    const w = clampW(cur() + (which === 'binder' ? d : -d));
    document.documentElement.style.setProperty(prop, `${w}px`);
    try {
      localStorage.setItem(key, String(w));
    } catch (err) {
      void err;
    }
  });
}

/* ---------- zen ------------------------------------------------------------ */
function toggleZen(force) {
  const on = force !== undefined ? force : !document.body.classList.contains('zen');
  document.body.classList.toggle('zen', on);
  if (typeof refreshTypewriterToggle === 'function') refreshTypewriterToggle();
  if (typeof centerCaretSoon === 'function') centerCaretSoon();
}

document.getElementById('zen-toggle').addEventListener('click', () => toggleZen());

/* ---------- theme + prose prefs -------------------------------------------- */
function bindThemeToggle() {
  const btn = document.getElementById('theme-toggle');
  if (!btn) return;
  btn.addEventListener('click', () => {
    cycleTheme();
    refreshThemeButton(getTheme());
  });
  refreshThemeButton(getTheme());
}

function applyProsePrefs() {
  let size = 18;
  let measure = 70;
  let leading = 1.7;
  try {
    const s = Number(localStorage.getItem('yonro.proseSize'));
    const m = Number(localStorage.getItem('yonro.proseMeasure'));
    const l = Number(localStorage.getItem('yonro.proseLeading'));
    if (Number.isFinite(s)) size = Math.min(24, Math.max(16, Math.round(s)));
    if (Number.isFinite(m)) measure = Math.min(84, Math.max(56, Math.round(m)));
    if (l === 1.5 || l === 1.7 || l === 1.9) leading = l;
  } catch (err) {
    void err;
  }
  const root = document.documentElement.style;
  root.setProperty('--prose-size', `${size}px`);
  root.setProperty('--measure', `${measure}ch`);
  root.setProperty('--leading', String(leading));
  const sv = document.getElementById('prose-size-val');
  const mv = document.getElementById('prose-measure-val');
  const sr = document.getElementById('prose-size');
  const mr = document.getElementById('prose-measure');
  if (sv) sv.textContent = `${size}px`;
  if (mv) mv.textContent = `${measure}ch`;
  if (sr) sr.value = String(size);
  if (mr) mr.value = String(measure);
  for (const b of document.querySelectorAll('#prose-pop [data-leading]')) {
    b.classList.toggle('active', Number(b.dataset.leading) === leading);
  }
}

function bindProsePop() {
  const toggle = document.getElementById('prose-toggle');
  const pop = document.getElementById('prose-pop');
  if (!toggle || !pop) return;
  toggle.addEventListener('click', () => {
    pop.hidden = !pop.hidden;
  });
  const size = document.getElementById('prose-size');
  const measure = document.getElementById('prose-measure');
  if (size) {
    size.addEventListener('input', () => {
      const v = Math.min(24, Math.max(16, Math.round(Number(size.value) || 18)));
      document.documentElement.style.setProperty('--prose-size', `${v}px`);
      const lab = document.getElementById('prose-size-val');
      if (lab) lab.textContent = `${v}px`;
      try {
        localStorage.setItem('yonro.proseSize', String(v));
      } catch (err) {
        void err;
      }
    });
  }
  if (measure) {
    measure.addEventListener('input', () => {
      const v = Math.min(84, Math.max(56, Math.round(Number(measure.value) || 70)));
      document.documentElement.style.setProperty('--measure', `${v}ch`);
      const lab = document.getElementById('prose-measure-val');
      if (lab) lab.textContent = `${v}ch`;
      try {
        localStorage.setItem('yonro.proseMeasure', String(v));
      } catch (err) {
        void err;
      }
    });
  }
  for (const b of document.querySelectorAll('#prose-pop [data-leading]')) {
    b.addEventListener('click', () => {
      const v = b.dataset.leading;
      document.documentElement.style.setProperty('--leading', v);
      try {
        localStorage.setItem('yonro.proseLeading', v);
      } catch (err) {
        void err;
      }
      for (const x of document.querySelectorAll('#prose-pop [data-leading]')) {
        x.classList.toggle('active', x === b);
      }
    });
  }
  if (!document.getElementById('autosave-toggle')) {
    const label = document.createElement('label');
    const box = document.createElement('input');
    box.type = 'checkbox';
    box.id = 'autosave-toggle';
    try {
      box.checked = localStorage.getItem('yonro.autosave') !== '0';
    } catch (err) {
      void err;
      box.checked = true;
    }
    box.addEventListener('change', () => {
      try {
        localStorage.setItem('yonro.autosave', box.checked ? '1' : '0');
      } catch (err) {
        void err;
      }
      setMessage(box.checked ? 'autosave on (3s idle)' : 'autosave off');
    });
    label.appendChild(box);
    label.appendChild(document.createTextNode(' autosave (3s idle, tracked files only)'));
    pop.appendChild(label);
  }
}

/* ---------- overlays -------------------------------------------------------- */
function toggleShortcuts(force) {
  const sheet = document.getElementById('shortcuts');
  if (!sheet) return;
  sheet.hidden = force !== undefined ? !force : !sheet.hidden;
}

function typingTarget(e) {
  const t = e.target;
  return Boolean(t && (t.tagName === 'TEXTAREA' || t.tagName === 'INPUT' || t.isContentEditable));
}

/* ---------- global keymap --------------------------------------------------- */
document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') {
    const openDlg = document.querySelector('dialog[open]');
    if (openDlg) return;
    const sheet = document.getElementById('shortcuts');
    const pop = document.getElementById('prose-pop');
    if (sheet && !sheet.hidden) {
      toggleShortcuts(false);
      return;
    }
    if (pop && !pop.hidden) {
      pop.hidden = true;
      return;
    }
    if (document.body.classList.contains('zen')) {
      toggleZen(false);
      return;
    }
    const scrim = document.getElementById('scrim');
    if (scrim && !scrim.hidden) {
      closeDrawersOnNarrow();
      return;
    }
    return;
  }
  if (e.key === 'F11') {
    e.preventDefault();
    toggleZen();
    return;
  }
  if (isMod(e) && (e.key === '.' || e.key.toLowerCase() === 'p' || e.key.toLowerCase() === 'j' || e.key.toLowerCase() === 'o' || e.key.toLowerCase() === 'e' || e.key.toLowerCase() === 'n' || e.key.toLowerCase() === 's' || e.key.toLowerCase() === 'f')) {
    const k = e.key.toLowerCase();
    if (k === '.') {
      e.preventDefault();
      toggleZen();
    } else if (k === 'o') {
      e.preventDefault();
      toggleBinder();
    } else if (k === 'j') {
      e.preventDefault();
      toggleInspector();
    } else if (k === 'e') {
      e.preventDefault();
      const b = document.getElementById('binder');
      if (b) b.focus();
    } else if (k === 'n') {
      e.preventDefault();
      openDoc(null);
    } else if (k === 'p' && !typingTarget(e)) {
      e.preventDefault();
      setMessage(e.shiftKey ? 'command mode lands later — outline opens scenes for now.' : 'palette lands later — pick a scene in the binder.');
    } else if (k === 'f' && !typingTarget(e)) {
      e.preventDefault();
      setMessage(e.shiftKey ? 'project search lands later.' : 'find in doc lands later — the editor keeps native find for now.');
    } else if (k === 's' && !typingTarget(e)) {
      e.preventDefault();
      if (e.shiftKey && typeof saveAsFlow === 'function') saveAsFlow();
      else saveActive();
    }
    return;
  }
  if (isMod(e) && e.key >= '1' && e.key <= '5') {
    e.preventDefault();
    show(views[Number(e.key) - 1]);
    return;
  }
  if (e.ctrlKey && e.key === 'Tab') {
    e.preventDefault();
    if (typeof cycleDoc === 'function') cycleDoc(e.shiftKey ? -1 : 1);
    return;
  }
  if (!isMod(e) && !typingTarget(e) && (e.key === '?' || e.key === 'h')) {
    if (e.key === '?') {
      e.preventDefault();
      toggleShortcuts();
    }
  }
});

/* Binder key hints (TUI parity): navigation works, creation waits for backend. */
document.addEventListener('keydown', (e) => {
  const binder = document.getElementById('binder');
  if (!binder || !binder.contains(document.activeElement)) return;
  if (e.key === 'a' || e.key === 'c' || e.key === 's' || e.key === 'Delete' || e.key === 'F2') {
    e.preventDefault();
    laterMsg('outline editing');
  }
});

/* ---------- boot ------------------------------------------------------------ */
applyPanelPrefs();
bindSplitter('split-binder', 'binder');
bindSplitter('split-inspector', 'inspector');
bindThemeToggle();
bindProsePop();
applyProsePrefs();
refreshBinder();
show('write');
syncDrawers();
window.addEventListener('resize', debounce(syncDrawers, 120));
