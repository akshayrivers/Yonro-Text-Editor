/* Yonro timeline view. Classic script.
 * Grouped by chapter; entries show title · POV chip (kind-colored) ·
 * setting · date · words. Continuity notes render as cards above the
 * list with scene-title buttons that select the scene in the binder
 * (which also opens the inspector on it). POV filter is UI-only.
 */

let timelinePovFilter = '';

async function loadTimeline() {
  try {
    const timeline = await core.timeline();
    renderTimeline(timeline);
  } catch (err) {
    document.getElementById('timeline').innerHTML = '';
    const p = document.createElement('p');
    p.className = 'muted';
    p.textContent = `timeline unavailable: ${err}`;
    document.getElementById('timeline').appendChild(p);
  }
}

function renderTimeline(timeline) {
  const entries = timeline.entries || [];
  const notes = timeline.notes || [];
  renderTimelineFilter(entries);
  renderContinuityNotes(notes);
  renderTimelineEntries(entries);
}

function renderTimelineFilter(entries) {
  const sel = document.getElementById('timeline-pov');
  if (!sel) return;
  const povs = [];
  for (const e of entries) {
    if (e.pov && !povs.includes(e.pov)) povs.push(e.pov);
  }
  povs.sort((a, b) => a.toLowerCase().localeCompare(b.toLowerCase()));
  sel.innerHTML = '';
  const all = document.createElement('option');
  all.value = '';
  all.textContent = 'all';
  sel.appendChild(all);
  for (const pov of povs) {
    const opt = document.createElement('option');
    opt.value = pov;
    opt.textContent = pov;
    sel.appendChild(opt);
  }
  sel.value = povs.includes(timelinePovFilter) ? timelinePovFilter : '';
  timelinePovFilter = sel.value;
}

function renderContinuityNotes(notes) {
  const box = document.getElementById('continuity');
  box.innerHTML = '';
  if (!notes.length) {
    const p = document.createElement('p');
    p.className = 'muted';
    p.textContent = 'no continuity notes — every POV/setting transition reads clean.';
    box.appendChild(p);
    return;
  }
  for (const note of notes) {
    const card = document.createElement('div');
    card.className = 'note';
    const msg = document.createElement('div');
    msg.className = 'note-message';
    msg.textContent = note.message;
    card.appendChild(msg);
    const scenes = note.scenes || [];
    const titles = note.scene_titles || [];
    if (scenes.length) {
      const row = document.createElement('div');
      row.className = 'note-scenes';
      scenes.forEach((sceneId, i) => {
        const btn = document.createElement('button');
        btn.className = 'scene-link-btn';
        btn.textContent = titles[i] || `scene ${sceneId}`;
        btn.setAttribute('aria-label', `select ${(titles[i] || `scene ${sceneId}`)} in binder`);
        btn.addEventListener('click', () => {
          if (typeof binderSelect === 'function') binderSelect(sceneId);
        });
        row.appendChild(btn);
      });
      card.appendChild(row);
    }
    box.appendChild(card);
  }
}

function renderTimelineEntries(entries) {
  const list = document.getElementById('timeline');
  list.innerHTML = '';
  if (!entries.length) {
    const p = document.createElement('p');
    p.className = 'muted';
    p.textContent = 'no scenes yet. build the outline first (mod+1... Outline).';
    list.appendChild(p);
    return;
  }
  const visible = timelinePovFilter
    ? entries.filter((e) => e.pov === timelinePovFilter)
    : entries;
  if (!visible.length) {
    const p = document.createElement('p');
    p.className = 'muted';
    p.textContent = `no scenes with POV ${timelinePovFilter}.`;
    list.appendChild(p);
    return;
  }
  let lastChapter = null;
  for (const e of visible) {
    if (e.chapter !== lastChapter) {
      lastChapter = e.chapter;
      const heading = document.createElement('h3');
      heading.className = 'timeline-chapter';
      heading.textContent = e.chapter || 'unchaptered';
      list.appendChild(heading);
    }
    const li = document.createElement('li');
    const title = document.createElement('strong');
    title.textContent = e.title;
    li.appendChild(title);
    li.appendChild(document.createTextNode(' '));
    if (e.pov) {
      const chip = document.createElement('span');
      chip.className = 'badge timeline-pov' + (e.pov_kind ? ` k-${e.pov_kind}` : '');
      chip.textContent = e.pov;
      li.appendChild(chip);
    } else {
      const none = document.createElement('span');
      none.className = 'muted';
      none.textContent = 'no POV';
      li.appendChild(none);
    }
    const meta = document.createElement('span');
    meta.className = 'when';
    const bits = [];
    if (e.setting) bits.push(e.setting);
    if (e.story_date) bits.push(e.story_date);
    bits.push(`${e.words}w`);
    meta.textContent = ` · ${bits.join(' · ')}`;
    li.appendChild(document.createTextNode(' '));
    li.appendChild(meta);
    list.appendChild(li);
  }
}

(function bindTimelineFilterOnce() {
  document.addEventListener('DOMContentLoaded', () => {
    const sel = document.getElementById('timeline-pov');
    if (sel) {
      sel.addEventListener('change', () => {
        timelinePovFilter = sel.value;
        if (typeof loadTimeline === 'function') loadTimeline();
      });
    }
  });
})();
