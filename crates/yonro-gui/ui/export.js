/* Yonro export (P5.5). Classic script; loaded before app.js.
 * Palette ">export manuscript as markdown/html" lands here.
 * Success message: "exported 38,204 words, 36 scenes -> export/sample-novel.md".
 * Backend errors already name the path plus the reason.
 */

async function exportFlow(format) {
  if (typeof flushSync === 'function') {
    try {
      await flushSync();
    } catch (err) {
      void err;
    }
  }
  let receipt = null;
  try {
    receipt = await core.exportManuscript(format, null);
  } catch (err) {
    setMessage(`could not export manuscript as ${format}: ${errText(err)}`, { error: true });
    return;
  }
  const words = Number(receipt.words || 0).toLocaleString('en-US');
  const dir = yonroWorkspace && yonroWorkspace.dir ? yonroWorkspace.dir : '';
  const rel = dir && receipt.path.indexOf(dir) === 0
    ? receipt.path.slice(dir.length + 1)
    : receipt.path;
  setMessage(`exported ${words} words, ${receipt.scenes} scenes -> ${rel}`);
}
