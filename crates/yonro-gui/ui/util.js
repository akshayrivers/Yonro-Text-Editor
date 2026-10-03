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
