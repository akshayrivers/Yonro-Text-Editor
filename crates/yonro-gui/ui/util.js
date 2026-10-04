/* Yonro shared helpers. Classic script; every top-level fn is global.
 * No backend access here. All user feedback goes through setMessage().
 */

const esc = (s) =>
  String(s ?? '').replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));

function isMod(e) {
  return Boolean(e && (e.ctrlKey || e.metaKey));
}

function $(sel, root) {
  return (root || document).querySelector(sel);
}

function $$(sel, root) {
  return Array.from((root || document).querySelectorAll(sel));
}

function el(tag, attrs, ...kids) {
  const node = document.createElement(tag);
  const a = attrs || {};
  for (const key of Object.keys(a)) {
    const v = a[key];
    if (v === null || v === undefined || v === false) continue;
    if (key === 'text') node.textContent = String(v);
    else if (key === 'class') node.className = String(v);
    else if (key.startsWith('on') && typeof v === 'function') {
      node.addEventListener(key.slice(2).toLowerCase(), v);
    } else {
      node.setAttribute(key, String(v));
    }
  }
  for (const k of kids) {
    if (k === null || k === undefined || k === false) continue;
    if (typeof k === 'string') node.appendChild(document.createTextNode(k));
    else node.appendChild(k);
  }
  return node;
}

function debounce(fn, ms) {
  let t = null;
  return (...args) => {
    if (t !== null) clearTimeout(t);
    t = setTimeout(() => {
      t = null;
      fn(...args);
    }, ms);
  };
}

let msgTimer = null;

/* Single status channel. Transient notes expire after 5s; errors persist
 * until the next message. Styled via --danger when error is true.
 */
function setMessage(text, opts) {
  const error = Boolean(opts && opts.error);
  const slot = document.getElementById('st-msg');
  if (msgTimer !== null) {
    clearTimeout(msgTimer);
    msgTimer = null;
  }
  if (slot) {
    slot.textContent = text || '';
    slot.classList.toggle('is-error', error);
  }
  const legacy = document.getElementById('doc-status');
  if (legacy && !error && text) legacy.textContent = String(text);
  if (!error && text) {
    const stamp = String(text);
    msgTimer = setTimeout(() => {
      const s = document.getElementById('st-msg');
      if (s && s.textContent === stamp) s.textContent = '';
      msgTimer = null;
    }, 5000);
  }
}

function toast(text) {
  setMessage(text, { error: false });
}

function toastError(text) {
  setMessage(text, { error: true });
}

/* Clean backend error text. Tauri rejects Err(String) with a string;
 * JS throws give Error. Never surface "undefined" or "[object Object]".
 */
function errText(err) {
  if (err === null || err === undefined) return 'unknown error';
  if (typeof err === 'string') return err || 'unknown error';
  if (err instanceof Error && err.message) return err.message;
  try {
    const s = String(err);
    if (s === '[object Object]') {
      try {
        return JSON.stringify(err);
      } catch (inner) {
        void inner;
        return 'unknown error';
      }
    }
    return s || 'unknown error';
  } catch (outer) {
    void outer;
    return 'unknown error';
  }
}

/* Native dialog opener: showModal, focus first control, Esc closes
 * natively, focus returns to the opener on close.
 */
let modalOpener = null;

function openModal(dlg, focusEl) {
  if (!dlg) return;
  if (!dlg.open) {
    modalOpener = document.activeElement;
    if (typeof dlg.showModal === 'function') dlg.showModal();
  }
  const target = focusEl || dlg.querySelector('input, select, textarea, button');
  if (target && typeof target.focus === 'function') {
    try {
      target.focus();
      if (target.tagName === 'INPUT' && typeof target.select === 'function') target.select();
    } catch (err) {
      void err;
    }
  }
  if (!dlg.dataset.returnBound) {
    dlg.dataset.returnBound = '1';
    dlg.addEventListener('close', () => {
      const back = modalOpener;
      modalOpener = null;
      if (back && typeof back.focus === 'function') {
        try {
          back.focus();
        } catch (err) {
          void err;
        }
      }
    });
  }
}
