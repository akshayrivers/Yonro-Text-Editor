/* Yonro timeline view. Classic script. */

async function loadTimeline() {
  try {
    const timeline = await core.timeline();
    const notes = timeline.notes || [];
    document.getElementById('continuity').innerHTML = notes.length
      ? notes.map((n) => `<div class="note">⚠ ${esc(n.message)}</div>`).join('')
      : '<p class="muted">No continuity notes — every POV/setting transition reads clean.</p>';
    document.getElementById('timeline').innerHTML = (timeline.entries || [])
      .map(
        (e) =>
          `<li><strong>${esc(e.title)}</strong> <span class="when">#${e.index + 1} · ${esc(e.chapter)} · ${esc(e.pov) || 'no POV'} · ${esc(e.setting) || 'no setting'}${e.story_date ? ` · ${esc(e.story_date)}` : ''} · ${e.words}w</span></li>`,
      )
      .join('');
  } catch (err) {
    document.getElementById('timeline').innerHTML = `<p class="muted">Timeline unavailable: ${esc(err)}</p>`;
  }
}
