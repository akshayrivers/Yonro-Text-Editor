/* Yonro find in doc (P5.3). Classic script; loaded before app.js.
 * Inline bar above the editor: input, n/m count, prev/next, case toggle.
 * Matches come from core.searchBuffer (UTF-16 offsets); highlights reuse
 * the mention backdrop (mark.find, .find.current). Enter next,
 * Shift+Enter prev; wrapping; Esc closes.
 */

let findSeq = 0;

function isFindBarOpen() {
  const bar = document.getElementById('find-bar');
  return Boolean(bar && !bar.hidden);
}

function openFindBar() {
  if (activeDoc === null || !docs.has(activeDoc)) {
    setMessage('open a document first, then find.');
    return;
  }
  const bar = document.getElementById('find-bar');
  const input = document.getElementById('find-input');
  if (!bar || !input) return;
  bar.hidden = false;
  try {
    const sel = editor.value.slice(editor.selectionStart, editor.selectionEnd);
    if (sel && sel.indexOf('\n') === -1) input.value = sel;
  } catch (err) {
    void err;
  }
  input.focus();
  input.select();
  refreshFindNow();
}

function closeFindBar() {
  const bar = document.getElementById('find-bar');
  if (bar) bar.hidden = true;
  findSpans = [];
  findCurrent = -1;
  if (typeof paintBackdrop === 'function') paintBackdrop();
  if (editor) editor.focus();
}

function findCaseSensitive() {
  const btn = document.getElementById('find-case');
  return Boolean(btn && btn.getAttribute('aria-pressed') === 'true');
}

function maybeRefreshFind() {
  if (isFindBarOpen()) refreshFindNow();
}

function maybeRefreshFindSoon() {
  if (!isFindBarOpen()) return;
  if (!maybeRefreshFindSoon.fn) maybeRefreshFindSoon.fn = debounce(refreshFindNow, 150);
  maybeRefreshFindSoon.fn();
}

async function refreshFindNow() {
  const input = document.getElementById('find-input');
  if (!input || activeDoc === null) {
    setFindCount(0, -1);
    return;
  }
  const query = input.value;
  const id = activeDoc;
  const seq = ++findSeq;
  let spans = [];
  if (query) {
    try {
      spans = await core.searchBuffer(id, query, findCaseSensitive());
    } catch (err) {
      void err;
      spans = [];
    }
  }
  if (seq !== findSeq || id !== activeDoc) return;
  findSpans = Array.isArray(spans) ? spans : [];
  findCurrent = findSpans.length ? 0 : -1;
  paintBackdrop();
  setFindCount(findSpans.length, findCurrent);
  if (findCurrent >= 0) selectFindMatch(false);
}

function setFindCount(total, current) {
  const count = document.getElementById('find-count');
  if (count) count.textContent = total ? `${current + 1}/${total}` : '0/0';
}

function selectFindMatch(scroll) {
  const span = findSpans[findCurrent];
  if (!span || !editor) return;
  try {
    editor.setSelectionRange(span.start, span.end);
    updateCaret();
    if (scroll) scrollMatchIntoView(span.start);
  } catch (err) {
    void err;
  }
}

function scrollMatchIntoView(offset) {
  try {
    const cs = getComputedStyle(editor);
    const lh = parseFloat(cs.lineHeight) || 24;
    const before = editor.value.slice(0, offset).split('\n').length - 1;
    const top = before * lh;
    if (top < editor.scrollTop || top > editor.scrollTop + editor.clientHeight - lh * 2) {
      editor.scrollTop = Math.max(0, top - Math.round(editor.clientHeight / 2));
    }
    syncBackdropScroll();
  } catch (err) {
    void err;
  }
}

function stepFind(dir) {
  if (!findSpans.length) return;
  findCurrent = (findCurrent + dir + findSpans.length) % findSpans.length;
  paintBackdrop();
  setFindCount(findSpans.length, findCurrent);
  selectFindMatch(true);
}

document.getElementById('find-input').addEventListener('input', debounce(refreshFindNow, 120));
document.getElementById('find-input').addEventListener('keydown', (e) => {
  if (e.key === 'Enter') {
    e.preventDefault();
    stepFind(e.shiftKey ? -1 : 1);
  } else if (e.key === 'Escape') {
    e.preventDefault();
    e.stopPropagation();
    closeFindBar();
  }
});
document.getElementById('find-next').addEventListener('click', () => stepFind(1));
document.getElementById('find-prev').addEventListener('click', () => stepFind(-1));
document.getElementById('find-close').addEventListener('click', () => closeFindBar());
document.getElementById('find-case').addEventListener('click', () => {
  const btn = document.getElementById('find-case');
  const on = btn.getAttribute('aria-pressed') !== 'true';
  btn.setAttribute('aria-pressed', on ? 'true' : 'false');
  btn.classList.toggle('active', on);
  refreshFindNow();
});
