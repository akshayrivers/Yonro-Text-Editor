/* Yonro lore view. Classic script. */

async function loadLore() {
  try {
    const entities = await core.lore();
    if (!entities.length) return; // keep the helpful placeholder
    document.getElementById('lore').innerHTML = entities
      .map(
        (e) => `<div class="entity"><h3>${esc(e.name)}<span class="kind">${esc(e.kind)}</span></h3>` +
          (e.aliases && e.aliases.length
            ? `<p class="aliases">Also known as: ${esc(e.aliases.join(', '))}</p>`
            : '') +
          (e.sheet ? `<p class="sheet">${esc(e.sheet)}</p>` : '') +
          (e.pov_scenes && e.pov_scenes.length
            ? `<p class="povs">POV in: ${esc(e.pov_scenes.join(', '))}</p>`
            : '') +
          `</div>`,
      )
      .join('');
  } catch (err) {
    document.getElementById('lore').innerHTML = `<p class="muted">Lore unavailable: ${esc(err)}</p>`;
  }
}
