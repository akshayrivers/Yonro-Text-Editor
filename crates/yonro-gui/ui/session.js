/* Yonro session + goals (P5.4). Classic script; loaded before app.js.
 * Statusline shows `session +N` plus a bar built from the core 0..1
 * progress (never from word counts in JS). Goal edits live behind the
 * palette ">set daily goal" command. Goal-reached messages once per day.
 */

function sessionBar(progress) {
  const clamped = Math.min(1, Math.max(0, Number(progress) || 0));
  const filled = Math.round(clamped * 6);
  return '▓'.repeat(filled) + '░'.repeat(6 - filled);
}

function streakText(days) {
  if (days > 1) return `${days}-day streak`;
  if (days === 1) return '1-day streak — keep it going.';
  return 'no streak yet — write today.';
}

async function refreshSession() {
  let summary = null;
  try {
    summary = await core.session();
  } catch (err) {
    void err;
    return;
  }
  if (!summary) return;
  const stSession = document.getElementById('st-session');
  const stGoal = document.getElementById('st-goal');
  if (stSession) stSession.textContent = `session +${summary.words_today}`;
  if (stGoal) {
    stGoal.textContent = summary.goal > 0
      ? `${sessionBar(summary.progress)} ${Math.round(summary.progress * 100)}%`
      : 'no goal';
  }
  const start = document.getElementById('start-streak');
  if (start) start.textContent = streakText(summary.streak_days);
  syncInspectorStreak(summary);
  if (summary.goal > 0 && summary.words_today >= summary.goal) {
    celebrateGoal(summary.goal);
  }
}

function maybeRefreshSessionSoon() {
  if (!maybeRefreshSessionSoon.fn) maybeRefreshSessionSoon.fn = debounce(refreshSession, 400);
  maybeRefreshSessionSoon.fn();
}

function celebrateGoal(goal) {
  const today = new Date().toISOString().slice(0, 10);
  let last = null;
  try {
    last = localStorage.getItem('yonro.goalDay');
  } catch (err) {
    void err;
  }
  if (last === today) return;
  try {
    localStorage.setItem('yonro.goalDay', today);
  } catch (err) {
    void err;
  }
  setMessage(`daily goal reached: ${goal} words.`);
}

function syncInspectorStreak(summary) {
  const box = document.getElementById('inspector');
  if (!box) return;
  const kids = Array.from(box.children);
  const empty = kids.length === 1 && kids[0].tagName === 'P' && kids[0].textContent.indexOf('select a scene') !== -1;
  let line = document.getElementById('inspector-streak');
  if (!empty) {
    if (line) line.remove();
    return;
  }
  if (!line) {
    line = document.createElement('p');
    line.id = 'inspector-streak';
    line.className = 'muted';
    box.appendChild(line);
  }
  line.textContent = streakText(summary.streak_days);
}

function promptGoalDialog() {
  const dlg = ensureDialog('goal-dialog', 'daily goal');
  dlg.innerHTML = '';
  const h = document.createElement('h2');
  h.textContent = 'daily goal';
  dlg.appendChild(h);
  const label = document.createElement('label');
  label.textContent = 'words per day (0 clears it)';
  const input = document.createElement('input');
  input.type = 'text';
  input.inputMode = 'numeric';
  input.placeholder = '500';
  input.setAttribute('aria-label', 'daily goal in words');
  label.appendChild(input);
  dlg.appendChild(label);
  const err = document.createElement('p');
  err.className = 'field-error';
  err.setAttribute('aria-live', 'polite');
  err.hidden = true;
  dlg.appendChild(err);
  const row = document.createElement('div');
  const setBtn = document.createElement('button');
  setBtn.textContent = 'set';
  const cancelBtn = document.createElement('button');
  cancelBtn.textContent = 'cancel';
  row.appendChild(setBtn);
  row.appendChild(cancelBtn);
  dlg.appendChild(row);
  const fail = (message) => {
    err.textContent = message;
    err.hidden = false;
  };
  setBtn.addEventListener('click', async () => {
    const raw = input.value.trim();
    if (!/^\d+$/.test(raw)) {
      fail('goal must be a whole number, 0 clears it.');
      return;
    }
    const words = Number(raw);
    if (!Number.isSafeInteger(words)) {
      fail('goal must be a whole number, 0 clears it.');
      return;
    }
    try {
      await core.setGoal(words);
    } catch (e) {
      fail(`could not set daily goal ${words}: ${errText(e)}`);
      return;
    }
    dlg.close();
    setMessage(words > 0 ? `daily goal: ${words} words.` : 'daily goal cleared.');
    refreshSession();
  });
  cancelBtn.addEventListener('click', () => dlg.close(), { once: true });
  openModal(dlg, input);
}

refreshSession();
