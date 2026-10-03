/* Yonro theme bootstrap. Loaded in <head> WITHOUT defer so the saved
 * theme lands on <html data-theme> before first paint (no flash).
 * Classic script: exposes getTheme / setTheme / cycleTheme globally.
 * Prefs only in localStorage ("yonro.theme"); never user data.
 */

var THEME_KEY = "yonro.theme";

function getTheme() {
  try {
    var v = localStorage.getItem(THEME_KEY);
    if (v === "light" || v === "system" || v === "dark") return v;
  } catch (err) {
    void err;
  }
  return "dark";
}

function resolvedTheme(choice) {
  var c = choice || getTheme();
  if (c !== "system") return c;
  try {
    if (window.matchMedia && window.matchMedia("(prefers-color-scheme: light)").matches) {
      return "light";
    }
  } catch (err) {
    void err;
  }
  return "dark";
}

function applyTheme(choice) {
  var c = choice || getTheme();
  document.documentElement.setAttribute("data-theme", resolvedTheme(c));
  refreshThemeButton(c);
}

function setTheme(choice) {
  if (choice !== "dark" && choice !== "light" && choice !== "system") return;
  try {
    localStorage.setItem(THEME_KEY, choice);
  } catch (err) {
    void err;
  }
  applyTheme(choice);
}

function cycleTheme() {
  var cur = getTheme();
  var next = cur === "dark" ? "light" : cur === "light" ? "system" : "dark";
  setTheme(next);
  return next;
}

function refreshThemeButton(choice) {
  var btn = document.getElementById("theme-toggle");
  if (!btn) return;
  var c = choice || getTheme();
  var eff = resolvedTheme(c);
  btn.setAttribute("aria-label", "theme: " + c + " (showing " + eff + ")");
  btn.setAttribute("data-choice", c);
  btn.textContent = c === "system" ? "auto" : eff === "light" ? "light" : "dark";
}

/* Apply before paint. */
try {
  applyTheme(getTheme());
} catch (err) {
  void err;
}

/* Follow the OS while the user chose "system". */
try {
  var mq = window.matchMedia("(prefers-color-scheme: light)");
  var onChange = function () {
    if (getTheme() === "system") applyTheme("system");
  };
  if (mq && mq.addEventListener) mq.addEventListener("change", onChange);
  else if (mq && mq.addListener) mq.addListener(onChange);
} catch (err) {
  void err;
}

document.addEventListener("DOMContentLoaded", function () {
  refreshThemeButton(getTheme());
});
