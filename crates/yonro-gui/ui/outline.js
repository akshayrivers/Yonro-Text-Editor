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
