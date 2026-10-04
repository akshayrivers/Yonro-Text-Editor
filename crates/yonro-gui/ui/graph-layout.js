/* Yonro force layout. Classic script; `forceLayout` is global for graph.js.
 * Pure function: no DOM, no backend. Hand-rolled Fruchterman-Reingold,
 * deterministic (golden-angle seed). O(n^2) per iteration: fine to ~600
 * nodes (256 nodes x 200 iters ~ 13M ops, well under 100ms).
 */
function forceLayout(nodes, edges, opts = {}) {
  const n = nodes.length;
  const W = Math.max(900, Math.sqrt(n) * 110);      // world grows with n
  const H = W * 0.55;
  const L = Math.sqrt((W * H) / Math.max(1, n)) * 0.6; // ideal edge length
  const idx = new Map(nodes.map((nd, i) => [nd.id, i]));
  const x = new Float64Array(n), y = new Float64Array(n);
  for (let i = 0; i < n; i++) {
    const r = L * Math.sqrt(i + 0.5), a = i * 2.399963;
    x[i] = Math.cos(a) * r; y[i] = Math.sin(a) * r;
  }
  const links = [];
  for (const e of edges) {
    const a = idx.get(e.a), b = idx.get(e.b);
    if (a !== undefined && b !== undefined) links.push([a, b, e.weight]);
  }
  const iters = opts.iters ?? (n > 600 ? 80 : 200);
  const dx = new Float64Array(n), dy = new Float64Array(n);
  let t = L; // max step, cools each iteration
  for (let it = 0; it < iters; it++) {
    dx.fill(0); dy.fill(0);
    for (let i = 0; i < n; i++) {
      for (let j = i + 1; j < n; j++) {
        let ex = x[i] - x[j], ey = y[i] - y[j];
        let d2 = ex * ex + ey * ey;
        if (d2 < 0.01) { ex = (i - j) * 0.1; ey = 0.1; d2 = ex * ex + ey * ey; }
        const f = (L * L) / d2;                      // repulsion k^2/d, as ex * k^2/d^2
        dx[i] += ex * f; dy[i] += ey * f; dx[j] -= ex * f; dy[j] -= ey * f;
      }
    }
    for (const [a, b, w] of links) {
      const ex = x[a] - x[b], ey = y[a] - y[b];
      const f = (Math.hypot(ex, ey) / L) * (0.5 + Math.min(w, 4) * 0.25); // attraction d^2/k
      dx[a] -= ex * f; dy[a] -= ey * f; dx[b] += ex * f; dy[b] += ey * f;
    }
    for (let i = 0; i < n; i++) {
      dx[i] -= x[i] * 0.01; dy[i] -= y[i] * 0.01;    // weak gravity keeps islands in frame
      const d = Math.hypot(dx[i], dy[i]) || 1;
      const s = Math.min(d, t) / d;
      x[i] += dx[i] * s; y[i] += dy[i] * s;
    }
    t = Math.max(0.5, t * 0.97);
  }
  const pos = {};
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  nodes.forEach((nd, i) => {
    pos[nd.id] = { x: x[i], y: y[i] };
    x0 = Math.min(x0, x[i]); x1 = Math.max(x1, x[i]);
    y0 = Math.min(y0, y[i]); y1 = Math.max(y1, y[i]);
  });
  return { pos, bbox: { x0, y0, x1, y1 }, spacing: L };
}
