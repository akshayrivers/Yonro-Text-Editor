/* Yonro force layout. Classic script; `forceLayout` and `createSim` are
 * global for graph.js. Hand-rolled Fruchterman-Reingold, deterministic
 * (golden-angle seed).
 *
 * forceLayout runs to convergence synchronously (the G0 path).
 * createSim exposes the same math incrementally: step() until alpha()
 * drops below 0.02, with pin/reheat for dragging. O(n^2) per step below
 * 600 nodes; above that a spatial grid (cell 2L, 3x3 cells) keeps steps
 * cheap.
 */
function forceLayout(nodes, edges, opts = {}) {
  const sim = createSim(nodes, edges, {}, null);
  const cap = opts.iters ?? 2000;
  let guard = 0;
  while (sim.alpha() >= 0.02 && guard++ < cap) sim.step();
  return { pos: sim.positions(), bbox: sim.bbox(), spacing: sim.spacing };
}

/* Two-column layout for pair lenses (positions only, no simulation).
 * Left = side A, right = side B, anything undivided sits middle.
 * Deterministic: id order within each column.
 */
function bipartiteLayout(nodes, leftIds, rightIds) {
  const leftSet = new Set(leftIds);
  const rightSet = new Set(rightIds);
  const left = [], right = [], mid = [];
  const sorted = nodes.slice().sort((a, b) => a.id - b.id);
  for (const n of sorted) {
    if (leftSet.has(n.id) && !rightSet.has(n.id)) left.push(n);
    else if (rightSet.has(n.id) && !leftSet.has(n.id)) right.push(n);
    else mid.push(n);
  }
  const rows = Math.max(left.length, right.length, mid.length, 1);
  const gap = 90;
  const width = 760;
  const top = 110;
  const place = (list, x) => {
    const off = ((rows - list.length) * gap) / 2;
    list.forEach((n, i) => {
      pos[n.id] = { x, y: top + off + i * gap };
    });
  };
  const pos = {};
  place(left, 120);
  place(mid, 120 + width / 2);
  place(right, 120 + width);
  return { pos, bbox: bboxOf(pos), spacing: 60 };
}

/* Row-per-group layout for the story map (positions only). `rows` is an
 * array of id arrays in outline order (one per act); ids absent from every
 * row continue after the last one.
 */
function orderedLayout(nodes, rows) {
  const at = new Map();
  rows.forEach((row, r) => row.forEach((id) => {
    if (!at.has(id)) at.set(id, r);
  }));
  const byRow = rows.map(() => []);
  const floating = [];
  for (const n of nodes) {
    if (at.has(n.id)) byRow[at.get(n.id)].push(n.id);
    else floating.push(n.id);
  }
  if (floating.length) byRow.push(floating);
  const pos = {};
  byRow.forEach((row, r) => {
    row.forEach((id, j) => {
      pos[id] = { x: 140 + j * 170, y: 140 + r * 180 };
    });
  });
  return { pos, bbox: bboxOf(pos), spacing: 80 };
}

function bboxOf(pos) {
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const id of Object.keys(pos)) {
    const p = pos[id];
    if (p.x < x0) x0 = p.x;
    if (p.x > x1) x1 = p.x;
    if (p.y < y0) y0 = p.y;
    if (p.y > y1) y1 = p.y;
  }
  return { x0, y0, x1, y1 };
}

function createSim(nodes, edges, params = {}, initPos = null) {
  const n = nodes.length;
  const W = Math.max(900, Math.sqrt(n) * 110);      // world grows with n
  const H = W * 0.55;
  const baseL = Math.sqrt((W * H) / Math.max(1, n)) * 0.6; // ideal edge length
  const P = {
    gravity: params.gravity ?? 0.01,
    repel: params.repel ?? 1,
    linkDist: params.linkDist ?? 1,
  };
  let L = baseL * (P.linkDist || 1);
  const idx = new Map(nodes.map((nd, i) => [nd.id, i]));
  const x = new Float64Array(n), y = new Float64Array(n);
  const seedAt = (i) => {
    const r = L * Math.sqrt(i + 0.5), a = i * 2.399963;
    x[i] = Math.cos(a) * r; y[i] = Math.sin(a) * r;
  };
  for (let i = 0; i < n; i++) {
    const p = initPos ? initPos[nodes[i].id] : null;
    if (p && Number.isFinite(p.x) && Number.isFinite(p.y)) {
      x[i] = p.x; y[i] = p.y;
    } else {
      seedAt(i);
    }
  }
  const links = [];
  for (const e of edges) {
    const a = idx.get(e.a), b = idx.get(e.b);
    if (a !== undefined && b !== undefined) links.push([a, b, e.weight]);
  }
  const dx = new Float64Array(n), dy = new Float64Array(n);
  const pinned = new Uint8Array(n);
  let t = L; // max step, cools each iteration
  const pos = {};
  nodes.forEach((nd, i) => {
    pos[nd.id] = { x: x[i], y: y[i] };
  });
  const syncPos = () => {
    for (let i = 0; i < n; i++) {
      pos[nodes[i].id].x = x[i];
      pos[nodes[i].id].y = y[i];
    }
  };
  const pushPair = (i, j) => {
    let ex = x[i] - x[j], ey = y[i] - y[j];
    let d2 = ex * ex + ey * ey;
    if (d2 < 0.01) { ex = (i - j) * 0.1; ey = 0.1; d2 = ex * ex + ey * ey; }
    const f = ((L * L) / d2) * P.repel;              // repulsion k^2/d
    dx[i] += ex * f; dy[i] += ey * f; dx[j] -= ex * f; dy[j] -= ey * f;
  };
  const repelExact = () => {
    for (let i = 0; i < n; i++) {
      for (let j = i + 1; j < n; j++) pushPair(i, j);
    }
  };
  const repelGrid = () => {
    const cell = Math.max(1, 2 * L);
    const keyOf = (gx, gz) => (gx + 4096) * 8192 + (gz + 4096);
    const cells = new Map();
    for (let i = 0; i < n; i++) {
      const k = keyOf(Math.floor(x[i] / cell), Math.floor(y[i] / cell));
      let arr = cells.get(k);
      if (!arr) {
        arr = [];
        cells.set(k, arr);
      }
      arr.push(i);
    }
    for (let i = 0; i < n; i++) {
      const cx = Math.floor(x[i] / cell), cy = Math.floor(y[i] / cell);
      for (let ox = -1; ox <= 1; ox++) {
        for (let oy = -1; oy <= 1; oy++) {
          const arr = cells.get(keyOf(cx + ox, cy + oy));
          if (!arr) continue;
          for (const j of arr) {
            if (j <= i) continue;
            pushPair(i, j);
          }
        }
      }
    }
  };
  const step = () => {
    dx.fill(0); dy.fill(0);
    if (n > 600) repelGrid();
    else repelExact();
    for (const [a, b, w] of links) {
      const ex = x[a] - x[b], ey = y[a] - y[b];
      const f = (Math.hypot(ex, ey) / L) * (0.5 + Math.min(w, 4) * 0.25); // attraction d^2/k
      dx[a] -= ex * f; dy[a] -= ey * f; dx[b] += ex * f; dy[b] += ey * f;
    }
    for (let i = 0; i < n; i++) {
      if (pinned[i]) continue;
      dx[i] -= x[i] * P.gravity; dy[i] -= y[i] * P.gravity;
      const d = Math.hypot(dx[i], dy[i]) || 1;
      const s = Math.min(d, t) / d;
      x[i] += dx[i] * s; y[i] += dy[i] * s;
      x[i] = Math.max(30, Math.min(W - 30, x[i]));
      y[i] = Math.max(30, Math.min(H - 30, y[i]));
    }
    t = Math.max(0.5, t * 0.97);
    syncPos();
    return t / L;
  };
  const bbox = () => {
    let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
    for (let i = 0; i < n; i++) {
      if (x[i] < x0) x0 = x[i];
      if (x[i] > x1) x1 = x[i];
      if (y[i] < y0) y0 = y[i];
      if (y[i] > y1) y1 = y[i];
    }
    return { x0, y0, x1, y1 };
  };
  return {
    step,
    alpha: () => t / L,
    pin(i, px, py) {
      if (i >= 0 && i < n) {
        pinned[i] = 1;
        x[i] = px; y[i] = py;
      }
    },
    unpin(i) {
      if (i >= 0 && i < n) pinned[i] = 0;
    },
    move(i, px, py) {
      if (i >= 0 && i < n && Number.isFinite(px) && Number.isFinite(py)) {
        x[i] = px; y[i] = py;
      }
    },
    reheat(a = 0.3) {
      t = Math.max(t, a * L);
    },
    setParams(patch) {
      Object.assign(P, patch);
      L = baseL * (P.linkDist || 1);
    },
    nodeIndex(id) {
      const i = idx.get(id);
      return i === undefined ? -1 : i;
    },
    positions: () => pos,
    bbox,
    get spacing() {
      return L;
    },
    count: n,
  };
}
