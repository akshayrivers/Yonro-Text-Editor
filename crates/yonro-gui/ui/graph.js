/* Yonro graph view. Classic script.
 * Edges use CSS classes (.edge.mention solid, .edge.shared dashed) so
 * meaning never depends on color alone. No hardcoded colors here.
 */
let graphSpotlight = null;
let graphFocusedNodeId = null;
let graphHoveredNodeId = null;
let graphWalkNeighborIdx = 0;
let graphData = null;
let graphPos = null;
let graphLayout = null; // { pos, bbox, spacing } from forceLayout()
let graphFitBox = null; // fitted viewBox { x, y, w, h }; zoom clamps to it
let graphLabelTopN = 25;
let graphViewBox = { x: 0, y: 0, w: 900, h: 480 };
let graphPanning = false;
let graphPanStart = { x: 0, y: 0, vbx: 0, vby: 0 };
let graphControlsBound = false;
/* Custom canvases: exactly one inferred tab plus any number of hand-made
 * ones. Null = inferred (text-derived); otherwise a CustomGraphId.
 */
let graphCustomId = null;
let customGraphs = [];
let pendingEdgeFrom = null;
let graphDragged = false;

function updateGraphViewBox() {
  const svg = document.getElementById('graph');
  if (svg) {
    svg.setAttribute('viewBox', `${graphViewBox.x} ${graphViewBox.y} ${graphViewBox.w} ${graphViewBox.h}`);
  }
}

/* Fit the layout bbox (+6% padding) into the svg's real aspect. The fitted
 * box anchors zoom clamps (0.3x..8x) and label density.
 */
function fitGraph() {
  const svg = document.getElementById('graph');
  if (!svg || !graphLayout || !graphData) return;
  const bb = graphLayout.bbox;
  if (!Number.isFinite(bb.x0 + bb.y0 + bb.x1 + bb.y1)) return;
  let x0 = bb.x0;
  let y0 = bb.y0;
  let x1 = bb.x1;
  let y1 = bb.y1;
  if (x1 <= x0) {
    x0 -= 100;
    x1 += 100;
  }
  if (y1 <= y0) {
    y0 -= 100;
    y1 += 100;
  }
  const pad = 0.06;
  const bw = (x1 - x0) * (1 + pad * 2) || 1;
  const bh = (y1 - y0) * (1 + pad * 2) || 1;
  const cw = svg.clientWidth || 900;
  const ch = svg.clientHeight || 480;
  const scale = Math.max(bw / cw, bh / ch);
  const w = cw * scale;
  const h = ch * scale;
  const cx = (x0 + x1) / 2;
  const cy = (y0 + y1) / 2;
  graphFitBox = { x: cx - w / 2, y: cy - h / 2, w, h };
  graphViewBox = { ...graphFitBox };
  updateGraphViewBox();
  updateGraphLabelLod();
  renderGraph(graphData);
}

function graphZoom() {
  if (!graphFitBox || !graphFitBox.w) return 1;
  return graphFitBox.w / (graphViewBox.w || graphFitBox.w);
}

/* Label budget grows with zoom^2 so zooming in reveals names. Runs on zoom
 * end (debounced), never per wheel tick.
 */
function updateGraphLabelLod() {
  const zoom = graphZoom();
  graphLabelTopN = Math.max(5, Math.round(25 * zoom * zoom));
}

const refreshGraphLabelsSoon = debounce(() => {
  updateGraphLabelLod();
  if (graphData) renderGraph(graphData);
}, 150);

function zoomGraph(factor, rx, ry) {
  const rX = typeof rx === 'number' ? rx : 0.5;
  const rY = typeof ry === 'number' ? ry : 0.5;
  const svg = document.getElementById('graph');
  const cw = (svg && svg.clientWidth) || 900;
  const ch = (svg && svg.clientHeight) || 480;
  const aspect = ch / (cw || 1);
  let lo = 120;
  let hi = 4800;
  if (graphFitBox && graphFitBox.w > 0) {
    lo = graphFitBox.w / 8;
    hi = graphFitBox.w / 0.3;
  }
  const newW = Math.max(lo, Math.min(hi, graphViewBox.w * factor));
  const newH = newW * aspect;
  graphViewBox.x += (graphViewBox.w - newW) * rX;
  graphViewBox.y += (graphViewBox.h - newH) * rY;
  graphViewBox.w = newW;
  graphViewBox.h = newH;
  updateGraphViewBox();
}

/* Incremental simulation --------------------------------------------------
 * The loop renders each step and stops below alpha 0.02 (no idle CPU).
 * Reduced motion forces the synchronous path. simDirty marks user
 * interaction so converge-end never yanks a hand-placed camera.
 */
let graphSim = null;
let simRAF = 0;
let simDirty = false;
let dragNode = null;

function graphAnimates() {
  const on = graphDisplayState ? graphDisplayState.animate !== false : true;
  if (!on) return false;
  try {
    return !(window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)').matches);
  } catch (err) {
    void err;
    return true;
  }
}

function simParams() {
  const d = graphDisplayState || defaultGraphDisplay();
  return { gravity: d.gravity ?? 0.01, repel: d.repel ?? 1, linkDist: d.linkDist ?? 1 };
}

function runSimSync() {
  if (!graphSim) return;
  let guard = 0;
  while (graphSim.alpha() >= 0.02 && guard++ < 2000) graphSim.step();
}

function stopSimLoop() {
  if (simRAF) {
    cancelAnimationFrame(simRAF);
    simRAF = 0;
  }
}

function ensureSimLoop() {
  if (simRAF || !graphSim) return;
  const tick = () => {
    simRAF = 0;
    if (!graphSim) return;
    const a = graphSim.step();
    if (graphData) renderGraph(graphData);
    if (a >= 0.02) {
      simRAF = requestAnimationFrame(tick);
    } else if (!simDirty && graphLayout) {
      graphLayout.bbox = graphSim.bbox();
      fitGraph();
    }
  };
  simRAF = requestAnimationFrame(tick);
}

function buildSimAndRun(nodes, edges, initPos) {
  stopSimLoop();
  simDirty = false;
  if (!nodes.length) {
    graphSim = null;
    graphPos = {};
    graphLayout = null;
    renderGraph(graphData);
    return;
  }
  graphSim = createSim(nodes, edges, simParams(), initPos || null);
  graphPos = graphSim.positions();
  graphLayout = { pos: graphPos, bbox: graphSim.bbox(), spacing: graphSim.spacing };
  if (graphAnimates()) {
    fitGraph();
    ensureSimLoop();
  } else {
    runSimSync();
    fitGraph();
  }
}

function onAnimateToggle() {
  if (!graphSim) return;
  if (graphAnimates()) {
    graphSim.reheat(0.3);
    ensureSimLoop();
  } else {
    stopSimLoop();
    runSimSync();
    if (graphData) renderGraph(graphData);
  }
}

function bindGraphControlsOnce() {
  if (graphControlsBound) return;
  graphControlsBound = true;

  const btnIn = document.getElementById('graph-zoom-in');
  if (btnIn) btnIn.addEventListener('click', () => {
    simDirty = true;
    zoomGraph(0.8);
    refreshGraphLabelsSoon();
  });

  const btnOut = document.getElementById('graph-zoom-out');
  if (btnOut) btnOut.addEventListener('click', () => {
    simDirty = true;
    zoomGraph(1.25);
    refreshGraphLabelsSoon();
  });

  const btnFit = document.getElementById('graph-zoom-fit');
  if (btnFit) {
    btnFit.addEventListener('click', () => {
      simDirty = true;
      fitGraph();
    });
  }

  const btnRename = document.getElementById('graph-rename');
  if (btnRename) {
    btnRename.addEventListener('click', () => {
      if (graphCustomId !== null) renameCustomGraphFlow(graphCustomId);
    });
  }

  const btnDelete = document.getElementById('graph-delete');
  if (btnDelete) {
    btnDelete.addEventListener('click', () => {
      if (graphCustomId !== null) deleteCustomGraphFlow(graphCustomId);
    });
  }

  window.addEventListener('resize', debounce(() => {
    const view = document.getElementById('view-graph');
    if (view && !view.classList.contains('hidden') && graphData) fitGraph();
  }, 150));

  const svg = document.getElementById('graph');
  if (svg) {
    svg.addEventListener('mousedown', (e) => {
      if (e.button !== 0) return;
      // Node drags spotlight on click; only empty canvas starts a pan.
      if (e.target && e.target.closest && e.target.closest('g.gnode')) return;
      simDirty = true;
      graphPanning = true;
      graphPanStart = { x: e.clientX, y: e.clientY, vbx: graphViewBox.x, vby: graphViewBox.y };
      svg.classList.add('panning');
    });

    window.addEventListener('mousemove', (e) => {
      if (!graphPanning) return;
      const rect = svg.getBoundingClientRect();
      const dx = (e.clientX - graphPanStart.x) * (graphViewBox.w / (rect.width || 1));
      const dy = (e.clientY - graphPanStart.y) * (graphViewBox.h / (rect.height || 1));
      graphViewBox.x = graphPanStart.vbx - dx;
      graphViewBox.y = graphPanStart.vby - dy;
      updateGraphViewBox();
    });

  window.addEventListener('mouseup', () => {
    if (graphPanning) {
      graphPanning = false;
      svg.classList.remove('panning');
    }
    if (!dragNode) return;
    const done = dragNode;
    dragNode = null;
    if (!graphSim) return;
    if (done.custom) {
      // Custom canvases stay hand-placed: persist on release, no reload.
      if (done.moved) {
        const g = customById(graphCustomId);
        const found = g ? g.nodes.find((nn) => nn.id === done.id) : null;
        const p = graphPos[done.id];
        if (found && p) {
          found.x = p.x;
          found.y = p.y;
          core.moveGraphNode(graphCustomId, done.id, p.x, p.y).catch((err) => {
            setMessage(`could not move node: ${errText(err)}`, { error: true });
          });
        }
      }
    } else {
      graphSim.unpin(done.i);
    }
  });

  window.addEventListener('mousemove', (ev) => {
    if (!dragNode || !graphSim) return;
    const pt = svgPoint(ev);
    graphSim.move(dragNode.i, pt.x, pt.y);
    if (Math.hypot(ev.clientX - dragNode.sx, ev.clientY - dragNode.sy) > 4) {
      dragNode.moved = true;
      graphDragged = true;
    }
    if (!simRAF && graphData) renderGraph(graphData);
  });

    svg.addEventListener('wheel', (e) => {
      e.preventDefault();
      simDirty = true;
      const rect = svg.getBoundingClientRect();
      const rx = rect.width ? (e.clientX - rect.left) / rect.width : 0.5;
      const ry = rect.height ? (e.clientY - rect.top) / rect.height : 0.5;
      const factor = e.deltaY < 0 ? 0.9 : 1.1;
      zoomGraph(factor, rx, ry);
      refreshGraphLabelsSoon();
    }, { passive: false });

    svg.addEventListener('dblclick', (e) => {
      if (graphCustomId === null) return;
      const row = e.target && e.target.closest ? e.target.closest('g.gnode') : null;
      if (row && row.dataset.id !== undefined) {
        const nodeId = Number(row.dataset.id);
        const g = customById(graphCustomId);
        const found = g ? g.nodes.find((n) => n.id === nodeId) : null;
        renameCustomNodeFlow(graphCustomId, nodeId, found ? found.label : '');
        return;
      }
      const pt = svgPoint(e);
      core.addGraphNode(graphCustomId, 'node', pt.x, pt.y).then(() => {
        reloadCustomGraphs(graphCustomId);
      }).catch((err) => {
        setMessage(`could not add node: ${errText(err)}`, { error: true });
      });
    });
  }
}

async function loadGraph() {
  bindGraphControlsOnce();
  await loadGraphTabs();
  await loadLensList();
  await initGraphFilters();
  if (graphCustomId !== null && customGraphs.some((g) => g.id === graphCustomId)) {
    renderCustomGraph();
    return;
  }
  graphCustomId = null;
  graphSpotlight = null;
  pendingEdgeFrom = null;
  renderGraphTabs();
  if (isLensTable()) {
    await loadPresence();
    return;
  }
  showGraphCanvas();
  await reloadInferred();
}

/* Filtered reload of the inferred tab (custom tabs render separately and
 * ignore the panel). Local-graph needs a selected node first.
 */
async function reloadInferred() {
  const localBox = document.getElementById('graph-f-local');
  if (localBox && localBox.checked && graphSpotlight === null && graphFocusedNodeId === null) {
    localBox.checked = false;
    saveGraphFilters(readGraphFilters());
    setMessage('select a node first (click it), then local graph.');
  }
  try {
    const query = graphCustomId === null ? buildGraphQuery() : null;
    graphData = await core.graph(query, graphCustomId === null ? graphLens : null);
    if (query === null) graphFull = graphData;
    const nodes = graphData.nodes || [];
    const edges = graphData.edges || [];
    buildSimAndRun(nodes, edges, null);
  } catch (err) {
    document.getElementById('graph-legend').textContent = `could not load graph: ${errText(err)}`;
  }
}

const requeryGraphSoon = debounce(() => {
  if (graphCustomId !== null || isLensTable()) return;
  reloadInferred();
}, 150);

function resetGraphFilters() {
  const keep = readGraphFilters();
  const fresh = defaultGraphFilters();
  fresh.open = keep.open;
  syncFilterControls(fresh);
  saveGraphFilters(fresh);
  requeryGraphSoon();
}

/* Option lists need the outline (scope/pov) and the unfiltered graph
 * (kind counts). Runs once per view load, before the first render.
 */
async function initGraphFilters() {
  bindGraphFiltersOnce();
  let outline = null;
  try {
    outline = await core.outline();
  } catch (err) {
    void err;
  }
  graphOutlineCache = outline;
  sceneActMap = {};
  if (outline) {
    let actIdx = 0;
    const walk = (node, act) => {
      const current = node.kind === 'act' ? actIdx++ : act;
      if (node.kind === 'scene' && current !== null && current !== undefined) {
        sceneActMap[node.id] = current;
      }
      for (const child of node.children || []) walk(child, current);
    };
    walk(outline, null);
  }
  if (!graphFull) {
    try {
      graphFull = await core.graph(null);
    } catch (err) {
      void err;
      graphFull = null;
    }
  }
  const counts = {};
  for (const n of (graphFull && graphFull.nodes) || []) {
    counts[n.kind] = (counts[n.kind] || 0) + 1;
  }
  buildKindChecks(counts);
  buildScopeOptions(outline);
  buildPovOptions(outline);
  syncFilterControls(loadGraphFilters());
}
function bindGraphFiltersOnce() {
  if (graphFiltersBound) return;
  graphFiltersBound = true;
  buildDisplayControls();
  const saved = loadGraphFilters();
  const toggle = document.getElementById('graph-panel-toggle');
  const body = document.getElementById('graph-panel-body');
  if (toggle && body) {
    toggle.setAttribute('aria-expanded', saved.open ? 'true' : 'false');
    body.hidden = !saved.open;
    toggle.addEventListener('click', () => {
      const shut = !body.hidden;
      body.hidden = shut;
      toggle.setAttribute('aria-expanded', shut ? 'false' : 'true');
      saveGraphFilters(readGraphFilters());
    });
  }
  const text = document.getElementById('graph-f-text');
  if (text) text.addEventListener('input', () => requeryGraphSoon());
  for (const id of ['graph-f-scope', 'graph-f-pov', 'graph-f-depth']) {
    const el = document.getElementById(id);
    if (el) el.addEventListener('change', () => requeryGraphSoon());
  }
  for (const id of ['graph-f-weight', 'graph-f-degree']) {
    const el = document.getElementById(id);
    if (el) el.addEventListener('input', () => requeryGraphSoon());
  }
  for (const id of ['graph-f-orphans', 'graph-f-local']) {
    const el = document.getElementById(id);
    if (el) el.addEventListener('change', () => requeryGraphSoon());
  }
  const reset = document.getElementById('graph-f-reset');
  if (reset) reset.addEventListener('click', () => resetGraphFilters());
  const animBox = document.getElementById('graph-d-animate');
  if (animBox) animBox.addEventListener('change', () => onAnimateToggle());
}

/* Custom canvases -------------------------------------------------------- */

async function loadGraphTabs() {
  try {
    customGraphs = await core.customGraphs();
  } catch (err) {
    void err;
    customGraphs = [];
  }
  if (!Array.isArray(customGraphs)) customGraphs = [];
  renderGraphTabs();
}

function renderGraphTabs() {
  const bar = document.getElementById('graph-tabs');
  if (!bar) return;
  bar.innerHTML = '';
  const mk = (key, label, selected, hint) => {
    const b = document.createElement('button');
    b.className = 'tab';
    b.textContent = label;
    b.title = hint || label;
    b.setAttribute('role', 'tab');
    b.setAttribute('aria-selected', selected ? 'true' : 'false');
    b.addEventListener('click', () => switchGraphView(key));
    if (key !== 'inferred') {
      b.addEventListener('dblclick', (e) => {
        e.stopPropagation();
        renameCustomGraphFlow(key);
      });
    }
    bar.appendChild(b);
  };
  mk('inferred', 'inferred', graphCustomId === null, 'the single text-derived graph');
  for (const g of customGraphs) {
    mk(g.id, g.title, g.id === graphCustomId, 'double-click renames');
  }
  const add = document.createElement('button');
  add.className = 'tab';
  add.textContent = '+ new';
  add.title = 'new hand-drawn graph';
  add.addEventListener('click', () => createCustomGraphFlow());
  bar.appendChild(add);
  for (const id of ['graph-rename', 'graph-delete']) {
    const btn = document.getElementById(id);
    if (btn) btn.hidden = graphCustomId === null;
  }
  const strip = document.getElementById('lens-strip');
  if (strip) strip.hidden = graphCustomId !== null;
  const panel = document.getElementById('graph-panel');
  if (panel) panel.hidden = graphCustomId !== null;
}

function switchGraphView(key) {
  graphSpotlight = null;
  pendingEdgeFrom = null;
  graphDragged = false;
  graphCustomId = key === 'inferred' ? null : key;
  if (graphCustomId === null) loadGraph();
  else renderCustomGraph();
}

function customById(id) {
  return customGraphs.find((g) => g.id === id);
}

function svgPoint(e) {
  const svg = document.getElementById('graph');
  const rect = svg.getBoundingClientRect();
  return {
    x: graphViewBox.x + ((e.clientX - rect.left) / (rect.width || 1)) * graphViewBox.w,
    y: graphViewBox.y + ((e.clientY - rect.top) / (rect.height || 1)) * graphViewBox.h,
  };
}

async function reloadCustomGraphs(keepId) {
  try {
    customGraphs = await core.customGraphs();
  } catch (err) {
    void err;
  }
  if (!Array.isArray(customGraphs)) customGraphs = [];
  if (keepId !== null && keepId !== undefined && !customGraphs.some((g) => g.id === keepId)) {
    keepId = null;
  }
  graphCustomId = keepId;
  renderGraphTabs();
  if (graphCustomId === null) loadGraph();
  else renderCustomGraph();
}

/* Adapt a hand-made graph to the inferred render shape (degrees and
 * neighbor lists derived from its own edges; positions already placed).
 */
function renderCustomGraph() {
  const g = customById(graphCustomId);
  const svg = document.getElementById('graph');
  if (!g) {
    graphCustomId = null;
    loadGraph();
    return;
  }
  if (!g.nodes.length) {
    stopSimLoop();
    graphSim = null;
    graphLayout = null;
    graphPos = {};
    graphData = { nodes: [], edges: [] };
    graphViewBox = { x: 0, y: 0, w: 900, h: 480 };
    updateGraphViewBox();
    svg.innerHTML = '';
    document.getElementById('graph-legend').textContent =
      'empty canvas. double-click to place the first node.';
    return;
  }
  const nameOf = {};
  for (const n of g.nodes) nameOf[n.id] = n.label;
  const adj = {};
  for (const e of g.edges) {
    (adj[e.a] = adj[e.a] || []).push({ id: e.b, name: nameOf[e.b] || '', weight: 1 });
    (adj[e.b] = adj[e.b] || []).push({ id: e.a, name: nameOf[e.a] || '', weight: 1 });
  }
  const nodes = g.nodes.map((n) => ({
    id: n.id,
    label: n.label,
    kind: 'lore',
    degree: (adj[n.id] || []).length,
    neighbors: adj[n.id] || [],
  }));
  const edges = g.edges.map((e) => ({
    a: e.a,
    b: e.b,
    weight: 2,
    kinds: ['custom'],
    label: e.label || '',
  }));
  const initPos = {};
  for (const n of g.nodes) initPos[n.id] = { x: n.x, y: n.y };
  graphData = { nodes, edges };
  buildSimAndRun(nodes, edges, initPos);
  document.getElementById('graph-legend').textContent =
    `${nodes.length} nodes · ${edges.length} edges. double-click adds a node, shift-click two nodes to link, drag moves.`;
}

async function linkCustomFlow(id) {
  if (pendingEdgeFrom === null || pendingEdgeFrom === id) {
    pendingEdgeFrom = pendingEdgeFrom === id ? null : id;
    graphSpotlight = pendingEdgeFrom;
    renderGraph(graphData);
    return;
  }
  try {
    await core.addGraphEdge(graphCustomId, pendingEdgeFrom, id, '');
  } catch (err) {
    setMessage(`could not link nodes: ${errText(err)}`, { error: true });
    pendingEdgeFrom = null;
    return;
  }
  pendingEdgeFrom = null;
  reloadCustomGraphs(graphCustomId);
}

function createCustomGraphFlow() {
  const dlg = ensureDialog('custom-graph-dialog', 'new graph');
  dlg.innerHTML = '';
  const h = document.createElement('h2');
  h.textContent = 'new graph';
  dlg.appendChild(h);
  const label = document.createElement('label');
  label.textContent = 'name';
  const input = document.createElement('input');
  input.type = 'text';
  input.placeholder = 'alliances';
  input.setAttribute('aria-label', 'graph name');
  label.appendChild(input);
  dlg.appendChild(label);
  const err = document.createElement('p');
  err.className = 'field-error';
  err.setAttribute('aria-live', 'polite');
  err.hidden = true;
  dlg.appendChild(err);
  const row = document.createElement('div');
  const goBtn = document.createElement('button');
  goBtn.textContent = 'create';
  const cancelBtn = document.createElement('button');
  cancelBtn.textContent = 'cancel';
  row.appendChild(goBtn);
  row.appendChild(cancelBtn);
  dlg.appendChild(row);
  goBtn.addEventListener('click', async () => {
    const name = input.value.trim();
    if (!name) {
      err.textContent = 'name cannot be empty';
      err.hidden = false;
      return;
    }
    try {
      const id = await core.createCustomGraph(name);
      dlg.close();
      setMessage(`created graph ${name}`);
      reloadCustomGraphs(id);
    } catch (e) {
      err.textContent = `cannot create graph ${name}: ${errText(e)}`;
      err.hidden = false;
    }
  });
  cancelBtn.addEventListener('click', () => dlg.close(), { once: true });
  openModal(dlg, input);
}

function renameCustomGraphFlow(id) {
  const g = customById(id);
  if (!g) return;
  const dlg = ensureDialog('custom-graph-dialog', 'rename graph');
  dlg.innerHTML = '';
  const h = document.createElement('h2');
  h.textContent = 'rename graph';
  dlg.appendChild(h);
  const label = document.createElement('label');
  label.textContent = 'name';
  const input = document.createElement('input');
  input.type = 'text';
  input.value = g.title;
  input.setAttribute('aria-label', 'graph name');
  label.appendChild(input);
  dlg.appendChild(label);
  const err = document.createElement('p');
  err.className = 'field-error';
  err.setAttribute('aria-live', 'polite');
  err.hidden = true;
  dlg.appendChild(err);
  const row = document.createElement('div');
  const goBtn = document.createElement('button');
  goBtn.textContent = 'rename';
  const cancelBtn = document.createElement('button');
  cancelBtn.textContent = 'cancel';
  row.appendChild(goBtn);
  row.appendChild(cancelBtn);
  dlg.appendChild(row);
  goBtn.addEventListener('click', async () => {
    const name = input.value.trim();
    if (!name) {
      err.textContent = 'name cannot be empty';
      err.hidden = false;
      return;
    }
    try {
      await core.renameCustomGraph(id, name);
      dlg.close();
      reloadCustomGraphs(id);
    } catch (e) {
      err.textContent = `cannot rename graph ${g.title}: ${errText(e)}`;
      err.hidden = false;
    }
  });
  cancelBtn.addEventListener('click', () => dlg.close(), { once: true });
  openModal(dlg, input);
}

function deleteCustomGraphFlow(id) {
  const g = customById(id);
  if (!g) return;
  const dlg = ensureDialog('custom-graph-confirm', 'delete graph');
  dlg.innerHTML = '';
  const h = document.createElement('h2');
  h.textContent = 'delete graph';
  dlg.appendChild(h);
  const p = document.createElement('p');
  p.textContent = `remove "${g.title}" with ${g.nodes.length} nodes and ${g.edges.length} edges?`;
  dlg.appendChild(p);
  const row = document.createElement('div');
  const del = document.createElement('button');
  del.textContent = 'delete';
  const cancel = document.createElement('button');
  cancel.textContent = 'cancel';
  row.appendChild(del);
  row.appendChild(cancel);
  dlg.appendChild(row);
  del.addEventListener('click', async () => {
    dlg.close();
    try {
      await core.deleteCustomGraph(id);
      setMessage(`deleted graph ${g.title}`);
    } catch (err) {
      setMessage(`could not delete graph ${g.title}: ${errText(err)}`, { error: true });
      return;
    }
    reloadCustomGraphs(null);
  }, { once: true });
  cancel.addEventListener('click', () => dlg.close(), { once: true });
  openModal(dlg, cancel);
}

function renameCustomNodeFlow(id, nodeId, current) {
  const dlg = ensureDialog('custom-graph-dialog', 'rename node');
  dlg.innerHTML = '';
  const h = document.createElement('h2');
  h.textContent = 'rename node';
  dlg.appendChild(h);
  const label = document.createElement('label');
  label.textContent = 'label';
  const input = document.createElement('input');
  input.type = 'text';
  input.value = current || '';
  input.setAttribute('aria-label', 'node label');
  label.appendChild(input);
  dlg.appendChild(label);
  const err = document.createElement('p');
  err.className = 'field-error';
  err.setAttribute('aria-live', 'polite');
  err.hidden = true;
  dlg.appendChild(err);
  const row = document.createElement('div');
  const goBtn = document.createElement('button');
  goBtn.textContent = 'rename';
  const cancelBtn = document.createElement('button');
  cancelBtn.textContent = 'cancel';
  row.appendChild(goBtn);
  row.appendChild(cancelBtn);
  dlg.appendChild(row);
  goBtn.addEventListener('click', async () => {
    const name = input.value.trim();
    if (!name) {
      err.textContent = 'label cannot be empty';
      err.hidden = false;
      return;
    }
    try {
      await core.renameGraphNode(id, nodeId, name);
      dlg.close();
      reloadCustomGraphs(id);
    } catch (e) {
      err.textContent = `cannot rename node: ${errText(e)}`;
      err.hidden = false;
    }
  });
  cancelBtn.addEventListener('click', () => dlg.close(), { once: true });
  openModal(dlg, input);
}

function removeCustomNodeFlow(id, nodeId, node) {  const g = customById(id);
  const degrees = node && node.neighbors ? node.neighbors.length : 0;
  const label = (node && node.label) || `node ${nodeId}`;
  const run = async () => {
    try {
      await core.removeGraphNode(id, nodeId);
    } catch (err) {
      setMessage(`could not remove ${label}: ${errText(err)}`, { error: true });
      return;
    }
    reloadCustomGraphs(id);
  };
  if (!g || degrees === 0) {
    run();
    return;
  }
  const dlg = ensureDialog('custom-graph-confirm', 'remove node');
  dlg.innerHTML = '';
  const h = document.createElement('h2');
  h.textContent = 'remove node';
  dlg.appendChild(h);
  const p = document.createElement('p');
  p.textContent = `remove "${label}" and its ${degrees} link${degrees === 1 ? '' : 's'}?`;
  dlg.appendChild(p);
  const row = document.createElement('div');
  const del = document.createElement('button');
  del.textContent = 'remove';
  const cancel = document.createElement('button');
  cancel.textContent = 'cancel';
  row.appendChild(del);
  row.appendChild(cancel);
  dlg.appendChild(row);
  del.addEventListener('click', async () => {
    dlg.close();
    run();
  }, { once: true });
  cancel.addEventListener('click', () => dlg.close(), { once: true });
  openModal(dlg, cancel);
}

/* Filter panel (inferred graph only) ---------------------------------------
 * Controls rebuild a core GraphQuery; the panel is collapsible and its
 * state persists as prefs. Re-querying is wired separately (debounced).
 */
const GRAPH_KINDS = ['character', 'place', 'faction', 'item', 'lore'];
let graphFull = null; // unfiltered dto: kind counts + filter-off baseline
let graphOutlineCache = null;
let graphFiltersBound = false;

function defaultGraphFilters() {
  return {
    open: true,
    text: '',
    kinds: GRAPH_KINDS.slice(),
    scope: '',
    pov: '',
    weight: 1,
    degree: 0,
    orphans: false,
    local: false,
    depth: 2,
  };
}

function loadGraphFilters() {
  const fallback = defaultGraphFilters();
  try {
    const raw = localStorage.getItem('yonro.graphFilters');
    if (!raw) return fallback;
    const saved = JSON.parse(raw);
    if (!saved || typeof saved !== 'object') return fallback;
    const kinds = Array.isArray(saved.kinds)
      ? saved.kinds.filter((k) => GRAPH_KINDS.includes(k))
      : fallback.kinds;
    return {
      open: saved.open !== false,
      text: typeof saved.text === 'string' ? saved.text : '',
      kinds,
      scope: typeof saved.scope === 'string' ? saved.scope : '',
      pov: typeof saved.pov === 'string' ? saved.pov : '',
      weight: Number.isFinite(Number(saved.weight)) ? Math.max(0, Math.min(99, Math.round(Number(saved.weight)))) : 1,
      degree: Number.isFinite(Number(saved.degree)) ? Math.max(0, Math.min(99, Math.round(Number(saved.degree)))) : 0,
      orphans: saved.orphans === true,
      local: saved.local === true,
      depth: saved.depth === 1 || saved.depth === 3 ? saved.depth : 2,
    };
  } catch (err) {
    void err;
    return fallback;
  }
}

function saveGraphFilters(state) {
  try {
    localStorage.setItem('yonro.graphFilters', JSON.stringify(state));
  } catch (err) {
    void err;
  }
}

function readGraphFilters() {
  const text = document.getElementById('graph-f-text');
  const scope = document.getElementById('graph-f-scope');
  const pov = document.getElementById('graph-f-pov');
  const weight = document.getElementById('graph-f-weight');
  const degree = document.getElementById('graph-f-degree');
  const orphans = document.getElementById('graph-f-orphans');
  const local = document.getElementById('graph-f-local');
  const depth = document.getElementById('graph-f-depth');
  const kinds = GRAPH_KINDS.filter((kind) => {
    const box = document.querySelector(`#graph-f-kinds input[data-kind="${kind}"]`);
    return box && box.checked;
  });
  return {
    open: document.getElementById('graph-panel-body')
      && !document.getElementById('graph-panel-body').hidden,
    text: text ? text.value : '',
    kinds,
    scope: scope ? scope.value : '',
    pov: pov ? pov.value : '',
    weight: weight ? Math.max(0, Math.min(99, Math.round(Number(weight.value) || 0))) : 1,
    degree: degree ? Math.max(0, Math.min(99, Math.round(Number(degree.value) || 0))) : 0,
    orphans: Boolean(orphans && orphans.checked),
    local: Boolean(local && local.checked),
    depth: depth && (depth.value === '1' || depth.value === '3') ? Number(depth.value) : 2,
  };
}

/* Query object for core (snake_case). Null when every control sits at its
 * default, so core returns today's full output untouched.
 */
function buildGraphQuery() {
  const f = readGraphFilters();
  saveGraphFilters(f);
  const allKinds = f.kinds.length === GRAPH_KINDS.length;
  const isDefault = f.text.trim() === '' && allKinds && f.scope === '' && f.pov === ''
    && f.weight === 1 && f.degree === 0 && !f.orphans && !f.local;
  if (isDefault) return null;
  let focus = null;
  if (f.local) {
    focus = graphSpotlight !== null ? graphSpotlight : graphFocusedNodeId;
  }
  return {
    kinds: allKinds ? [] : (f.kinds.length ? f.kinds : ['__none__']),
    text: f.text.trim(),
    scope: f.scope === '' ? null : Number(f.scope),
    pov: f.pov === '' ? null : f.pov,
    min_weight: f.weight,
    min_degree: f.degree,
    hide_orphans: f.orphans,
    focus,
    depth: f.depth,
    max_nodes: 0,
  };
}

function syncFilterControls(state) {
  const set = (id, value) => {
    const el = document.getElementById(id);
    if (el) el.value = value;
  };
  set('graph-f-text', state.text);
  set('graph-f-scope', state.scope);
  set('graph-f-pov', state.pov);
  set('graph-f-weight', String(state.weight));
  set('graph-f-degree', String(state.degree));
  const orphans = document.getElementById('graph-f-orphans');
  if (orphans) orphans.checked = state.orphans;
  const local = document.getElementById('graph-f-local');
  if (local) local.checked = state.local;
  set('graph-f-depth', String(state.depth));
  for (const kind of GRAPH_KINDS) {
    const box = document.querySelector(`#graph-f-kinds input[data-kind="${kind}"]`);
    if (box) box.checked = state.kinds.includes(kind);
  }
  const toggle = document.getElementById('graph-panel-toggle');
  const body = document.getElementById('graph-panel-body');
  if (toggle && body) {
    toggle.setAttribute('aria-expanded', state.open ? 'true' : 'false');
    body.hidden = !state.open;
  }
}

function buildKindChecks(counts) {
  const box = document.getElementById('graph-f-kinds');
  if (!box) return;
  box.querySelectorAll('.kind-check').forEach((row) => row.remove());
  const saved = loadGraphFilters();
  for (const kind of GRAPH_KINDS) {
    const row = document.createElement('label');
    row.className = 'kind-check';
    const input = document.createElement('input');
    input.type = 'checkbox';
    input.dataset.kind = kind;
    input.checked = saved.kinds.includes(kind);
    input.setAttribute('aria-label', `show ${kind}s`);
    input.addEventListener('change', () => requeryGraphSoon());
    row.appendChild(input);
    const name = document.createElement('span');
    name.textContent = kind;
    row.appendChild(name);
    const count = document.createElement('span');
    count.className = 'count';
    count.textContent = String(counts[kind] || 0);
    row.appendChild(count);
    box.appendChild(row);
  }
}

function buildScopeOptions(outline) {
  const sel = document.getElementById('graph-f-scope');
  if (!sel) return;
  sel.innerHTML = '';
  const all = document.createElement('option');
  all.value = '';
  all.textContent = 'whole book';
  sel.appendChild(all);
  const walk = (node, trail) => {
    if (node.kind === 'act' || node.kind === 'chapter') {
      const opt = document.createElement('option');
      opt.value = String(node.id);
      opt.textContent = trail ? `${trail} / ${node.title}` : node.title;
      sel.appendChild(opt);
    }
    const next = node.kind === 'project' ? '' : (trail ? `${trail} / ${node.title}` : node.title);
    for (const child of node.children || []) walk(child, next);
  };
  if (outline) walk(outline, '');
  const saved = loadGraphFilters();
  sel.value = saved.scope;
  if (sel.value !== saved.scope) sel.value = '';
}

function buildPovOptions(outline) {
  const sel = document.getElementById('graph-f-pov');
  if (!sel) return;
  sel.innerHTML = '';
  const all = document.createElement('option');
  all.value = '';
  all.textContent = 'any pov';
  sel.appendChild(all);
  const povs = [];
  const walk = (node) => {
    if (node.kind === 'scene' && node.pov && !povs.includes(node.pov)) povs.push(node.pov);
    for (const child of node.children || []) walk(child);
  };
  if (outline) walk(outline);
  povs.sort((a, b) => a.toLowerCase().localeCompare(b.toLowerCase()));
  for (const pov of povs) {
    const opt = document.createElement('option');
    opt.value = pov;
    opt.textContent = pov;
    sel.appendChild(opt);
  }
  const saved = loadGraphFilters();
  sel.value = saved.pov;
  if (sel.value !== saved.pov) sel.value = '';
}

/* Display settings (UI-only, prefs) ---------------------------------------
 * Thickness/opacity/size/labels/arrows change rendering only — never the
 * query. Opacity defaults to the --edge-opacity token; the slider
 * overrides it at runtime on :root.
 */
let graphDisplayState = null;

function defaultGraphDisplay() {
  return {
    thick: 1, opacity: 0.35, nodeSize: 1, labelZoom: 1, arrows: false, animate: true,
    gravity: 0.01, repel: 1, linkDist: 1,
  };
}

function clampNum(value, fallback, lo, hi) {
  const n = Number(value);
  if (!Number.isFinite(n)) return fallback;
  return Math.min(hi, Math.max(lo, n));
}

function loadGraphDisplay() {
  const fallback = defaultGraphDisplay();
  try {
    const raw = localStorage.getItem('yonro.graphDisplay');
    if (!raw) return fallback;
    const saved = JSON.parse(raw);
    if (!saved || typeof saved !== 'object') return fallback;
    return {
      thick: clampNum(saved.thick, 1, 0.5, 3),
      opacity: clampNum(saved.opacity, 0.35, 0.1, 0.8),
      nodeSize: clampNum(saved.nodeSize, 1, 0.5, 2),
      labelZoom: clampNum(saved.labelZoom, 1, 0.5, 4),
      arrows: saved.arrows === true,
      animate: saved.animate !== false,
      gravity: clampNum(saved.gravity, 0.01, 0, 0.05),
      repel: clampNum(saved.repel, 1, 0, 3),
      linkDist: clampNum(saved.linkDist, 1, 0.5, 2),
    };
  } catch (err) {
    void err;
    return fallback;
  }
}

function saveGraphDisplay(state) {
  try {
    localStorage.setItem('yonro.graphDisplay', JSON.stringify(state));
  } catch (err) {
    void err;
  }
}

function applyEdgeOpacityToken(value) {
  try {
    document.documentElement.style.setProperty('--edge-opacity', String(value));
  } catch (err) {
    void err;
  }
}

function buildDisplayControls() {
  const body = document.getElementById('graph-panel-body');
  if (!body || document.getElementById('graph-f-display')) return;
  if (!graphDisplayState) graphDisplayState = loadGraphDisplay();
  const state = graphDisplayState;
  const box = document.createElement('fieldset');
  box.id = 'graph-f-display';
  const legend = document.createElement('legend');
  legend.textContent = 'display';
  box.appendChild(legend);
  const mkRange = (id, key, label, min, max, step, value, fmt, after) => {
    const row = document.createElement('label');
    row.className = 'gf-row';
    const head = document.createElement('span');
    head.textContent = label + ' ';
    const val = document.createElement('span');
    val.className = 'gf-val';
    val.textContent = fmt(value);
    head.appendChild(val);
    row.appendChild(head);
    const input = document.createElement('input');
    input.id = id;
    input.type = 'range';
    input.min = String(min);
    input.max = String(max);
    input.step = String(step);
    input.value = String(value);
    input.setAttribute('aria-label', label);
    input.addEventListener('input', () => {
      const next = clampNum(input.value, value, min, max);
      val.textContent = fmt(next);
      graphDisplayState = { ...graphDisplayState, [key]: next };
      saveGraphDisplay(graphDisplayState);
      if (key === 'opacity') applyEdgeOpacityToken(next);
      if (typeof after === 'function') after(next);
      else if (graphData) renderGraph(graphData);
    });
    row.appendChild(input);
    box.appendChild(row);
  };
  const oneDecimal = (n) => String(Math.round(n * 10) / 10);
  mkRange('graph-d-thick', 'thick', 'link thickness', 0.5, 3, 0.5, state.thick, oneDecimal);
  mkRange('graph-d-opacity', 'opacity', 'link opacity', 0.1, 0.8, 0.05, state.opacity, oneDecimal);
  mkRange('graph-d-size', 'nodeSize', 'node size', 0.5, 2, 0.25, state.nodeSize, oneDecimal);
  mkRange('graph-d-zoom', 'labelZoom', 'label zoom', 0.5, 4, 0.25, state.labelZoom, oneDecimal);
  const mkCheck = (id, label, value, key) => {
    const row = document.createElement('label');
    row.className = 'gf-check';
    const input = document.createElement('input');
    input.id = id;
    input.type = 'checkbox';
    input.checked = value;
    input.addEventListener('change', () => {
      graphDisplayState = { ...graphDisplayState, [key]: input.checked };
      saveGraphDisplay(graphDisplayState);
      if (graphData) renderGraph(graphData);
    });
    row.appendChild(input);
    row.appendChild(document.createTextNode(` ${label}`));
    box.appendChild(row);
  };
  mkCheck('graph-d-arrows', 'arrows (custom graphs)', state.arrows, 'arrows');
  mkCheck('graph-d-animate', 'animate layout', state.animate, 'animate');
  const simAfter = (key) => (next) => {
    if (graphSim) {
      const patch = {};
      patch[key] = next;
      graphSim.setParams(patch);
      graphLayout.spacing = graphSim.spacing;
      graphSim.reheat(0.3);
      if (graphAnimates()) ensureSimLoop();
      else {
        runSimSync();
        if (graphData) renderGraph(graphData);
      }
    }
  };
  const threeDecimal = (n) => String(Math.round(n * 1000) / 1000);
  mkRange('graph-d-grav', 'gravity', 'center gravity', 0, 0.05, 0.005, state.gravity, threeDecimal, simAfter('gravity'));
  mkRange('graph-d-repel', 'repel', 'repel', 0, 3, 0.25, state.repel, oneDecimal, simAfter('repel'));
  mkRange('graph-d-link', 'linkDist', 'link distance', 0.5, 2, 0.25, state.linkDist, oneDecimal, simAfter('linkDist'));
  body.appendChild(box);
  applyEdgeOpacityToken(state.opacity);
}

/* Lenses: exactly one inferred view at a time ------------------------------
 * Null = the full book graph (today's output). Builtins + customs come
 * from core; the strip also drives the story-map and presence views.
 */
let lensList = [];
let graphLens = null;
let presenceCache = null;
let sceneActMap = {};

function lensEngine() {
  return graphLens ? graphLens.engine : 'entity';
}

function isLensTable() {
  return lensEngine() === 'presence';
}

function loadLensPref() {
  try {
    const name = localStorage.getItem('yonro.graphLens');
    return name || null;
  } catch (err) {
    void err;
    return null;
  }
}

function saveLensPref() {
  try {
    if (graphLens) localStorage.setItem('yonro.graphLens', graphLens.name);
    else localStorage.removeItem('yonro.graphLens');
  } catch (err) {
    void err;
  }
}

async function loadLensList() {
  try {
    lensList = await core.lenses();
  } catch (err) {
    void err;
    lensList = [];
  }
  if (!Array.isArray(lensList)) lensList = [];
  const saved = loadLensPref();
  graphLens = saved ? lensList.find((lens) => lens.name === saved) || null : null;
  renderLensStrip();
}

function renderLensStrip() {
  const bar = document.getElementById('lens-strip');
  if (!bar) return;
  bar.hidden = graphCustomId !== null;
  bar.innerHTML = '';
  const label = document.createElement('span');
  label.className = 'lens-label';
  label.textContent = 'lenses';
  bar.appendChild(label);
  const mk = (name, lens, hint) => {
    const selected = (lens === null && graphLens === null)
      || (lens !== null && graphLens !== null && lens.name === graphLens.name);
    const b = document.createElement('button');
    b.className = 'tab';
    b.textContent = name;
    b.title = hint || name;
    b.setAttribute('role', 'tab');
    b.setAttribute('aria-selected', selected ? 'true' : 'false');
    b.addEventListener('click', () => selectLens(lens ? lens.name : null));
    if (lens !== null) {
      b.addEventListener('dblclick', (e) => {
        e.stopPropagation();
        deleteLensFlow(lens.name);
      });
    }
    bar.appendChild(b);
  };
  mk('all', null, 'the full book graph');
  for (const lens of lensList) {
    mk(lens.name, lens, lens.engine === 'entity' ? 'entity view' : lens.engine === 'scene' ? 'story map' : 'presence table');
  }
  const add = document.createElement('button');
  add.className = 'tab';
  add.textContent = '+ lens';
  add.title = 'save a new lens';
  add.addEventListener('click', () => createLensFlow());
  bar.appendChild(add);
}

async function selectLens(name) {
  graphLens = name === null ? null : lensList.find((lens) => lens.name === name) || null;
  saveLensPref();
  renderLensStrip();
  graphSpotlight = null;
  pendingEdgeFrom = null;
  if (isLensTable()) {
    await loadPresence();
    return;
  }
  showGraphCanvas();
  if (graphCustomId !== null) renderCustomGraph();
  else await reloadInferred();
}

function showGraphCanvas() {
  const svg = document.getElementById('graph');
  const legend = document.getElementById('graph-legend');
  const wrap = document.getElementById('presence-wrap');
  if (svg) svg.hidden = false;
  if (legend) legend.hidden = false;
  if (wrap) wrap.hidden = true;
}

function showPresenceTable() {
  const svg = document.getElementById('graph');
  const legend = document.getElementById('graph-legend');
  const wrap = document.getElementById('presence-wrap');
  if (svg) svg.hidden = true;
  if (legend) legend.hidden = true;
  if (wrap) wrap.hidden = false;
}

async function loadPresence() {
  showPresenceTable();
  try {
    presenceCache = await core.presence(graphLens);
    renderPresenceTable(presenceCache);
  } catch (err) {
    presenceCache = null;
    const table = document.getElementById('presence-table');
    table.innerHTML = '';
    const caption = document.createElement('caption');
    caption.textContent = `could not load presence: ${errText(err)}`;
    table.appendChild(caption);
  }
}

function createLensFlow() {
  const dlg = ensureDialog('lens-dialog', 'new lens');
  dlg.innerHTML = '';
  const h = document.createElement('h2');
  h.textContent = 'new lens';
  dlg.appendChild(h);
  const nameLabel = document.createElement('label');
  nameLabel.textContent = 'name';
  const nameInput = document.createElement('input');
  nameInput.type = 'text';
  nameInput.placeholder = 'Rivals';
  nameInput.setAttribute('aria-label', 'lens name');
  nameLabel.appendChild(nameInput);
  dlg.appendChild(nameLabel);
  const engineLabel = document.createElement('label');
  engineLabel.textContent = 'view';
  const engineSel = document.createElement('select');
  for (const [value, text] of [['entity', 'entities'], ['scene', 'story map'], ['presence', 'presence table']]) {
    const opt = document.createElement('option');
    opt.value = value;
    opt.textContent = text;
    engineSel.appendChild(opt);
  }
  engineSel.setAttribute('aria-label', 'lens view');
  engineLabel.appendChild(engineSel);
  dlg.appendChild(engineLabel);
  const aLabel = document.createElement('label');
  aLabel.textContent = 'kinds a (comma separated)';
  const aInput = document.createElement('input');
  aInput.type = 'text';
  aInput.placeholder = 'character';
  aInput.setAttribute('aria-label', 'side a kinds');
  aLabel.appendChild(aInput);
  dlg.appendChild(aLabel);
  const bLabel = document.createElement('label');
  bLabel.textContent = 'kinds b (blank mirrors a)';
  const bInput = document.createElement('input');
  bInput.type = 'text';
  bInput.placeholder = 'place';
  bInput.setAttribute('aria-label', 'side b kinds');
  bLabel.appendChild(bInput);
  dlg.appendChild(bLabel);
  const err = document.createElement('p');
  err.className = 'field-error';
  err.setAttribute('aria-live', 'polite');
  err.hidden = true;
  dlg.appendChild(err);
  const row = document.createElement('div');
  const goBtn = document.createElement('button');
  goBtn.textContent = 'save';
  const cancelBtn = document.createElement('button');
  cancelBtn.textContent = 'cancel';
  row.appendChild(goBtn);
  row.appendChild(cancelBtn);
  dlg.appendChild(row);
  const kindsOf = (input) => input.value.split(',').map((s) => s.trim().toLowerCase()).filter((s) => s !== '');
  goBtn.addEventListener('click', async () => {
    const name = nameInput.value.trim();
    if (!name) {
      err.textContent = 'name cannot be empty';
      err.hidden = false;
      return;
    }
    const lens = {
      name,
      engine: engineSel.value,
      kinds_a: kindsOf(aInput),
      kinds_b: kindsOf(bInput),
    };
    try {
      await core.saveLens(name, lens);
      dlg.close();
      setMessage(`saved lens ${name}`);
      await loadLensList();
      selectLens(name);
    } catch (e) {
      err.textContent = `cannot save lens ${name}: ${errText(e)}`;
      err.hidden = false;
    }
  });
  cancelBtn.addEventListener('click', () => dlg.close(), { once: true });
  openModal(dlg, nameInput);
}

function deleteLensFlow(name) {
  const builtin = ['Cast', 'Cast x Places', 'Cast x Items', 'Factions', 'Story map', 'Act presence'];
  if (builtin.some((b) => b.toLowerCase() === name.toLowerCase())) {
    setMessage('builtin lenses cannot be deleted.');
    return;
  }
  const dlg = ensureDialog('lens-confirm', 'delete lens');
  dlg.innerHTML = '';
  const h = document.createElement('h2');
  h.textContent = 'delete lens';
  dlg.appendChild(h);
  const p = document.createElement('p');
  p.textContent = `remove the "${name}" lens?`;
  dlg.appendChild(p);
  const row = document.createElement('div');
  const del = document.createElement('button');
  del.textContent = 'delete';
  const cancel = document.createElement('button');
  cancel.textContent = 'cancel';
  row.appendChild(del);
  row.appendChild(cancel);
  dlg.appendChild(row);
  del.addEventListener('click', async () => {
    dlg.close();
    try {
      await core.deleteLens(name);
      setMessage(`deleted lens ${name}`);
    } catch (err) {
      setMessage(`could not delete lens ${name}: ${errText(err)}`, { error: true });
      return;
    }
    if (graphLens && graphLens.name === name) graphLens = null;
    await loadLensList();
    selectLens(graphLens ? graphLens.name : null);
  }, { once: true });
  cancel.addEventListener('click', () => dlg.close(), { once: true });
  openModal(dlg, cancel);
}

function updateGraphRovingTabindex(activeId) {  const nodes = document.querySelectorAll('#graph g.gnode');
  nodes.forEach((el) => {
    const isTarget = Number(el.dataset.id) === activeId;
    el.setAttribute('tabindex', isTarget ? '0' : '-1');
    if (isTarget) {
      el.classList.add('focused');
    } else {
      el.classList.remove('focused');
    }
  });
}

function focusGraphNode(id, resetWalk) {
  graphFocusedNodeId = id;
  if (resetWalk !== false) graphWalkNeighborIdx = 0;
  updateGraphRovingTabindex(id);
  const target = document.querySelector(`#graph g[data-id="${id}"]`);
  if (target) target.focus();
  const node = (graphData && graphData.nodes ? graphData.nodes : []).find((x) => x.id === id);
  if (graphCustomId === null && node && typeof showEntityInspector === 'function') {
    showEntityInspector(id, node);
  }
}

/* Re-render wipes the focused <g>; put focus back so keyboard flow continues. */
function refocusGraphNode(id) {
  graphFocusedNodeId = id;
  updateGraphRovingTabindex(id);
  const target = document.querySelector(`#graph g[data-id="${id}"]`);
  if (target) target.focus({ preventScroll: true });
}

function renderGraph(graph) {
  const svg = document.getElementById('graph');
  if (!svg) return;
  updateGraphViewBox();

  const nodes = graph.nodes || [];
  const edges = graph.edges || [];
  if (!nodes.length) {
    svg.innerHTML = '';
    document.getElementById('graph-legend').textContent = 'no linked entities yet — set scene POVs and write @mentions.';
    return;
  }

  // Inferred min weight lives in the filter panel now; custom canvases
  // draw everything (their edges are hand-placed, weight is fixed).
  const weightInput = document.getElementById('graph-f-weight');
  const panelWeight = weightInput ? Math.max(0, Number(weightInput.value) || 0) : 1;
  const minWeight = graphCustomId === null ? panelWeight : 0;
  const aboveSlider = edges.filter((e) => (e.weight || 0) >= minWeight);

  const neighborIds = new Set();
  if (graphSpotlight !== null) {
    neighborIds.add(graphSpotlight);
    for (const e of edges) {
      if (e.a === graphSpotlight) neighborIds.add(e.b);
      if (e.b === graphSpotlight) neighborIds.add(e.a);
    }
  }

  // Edge LOD: strongest first, capped at 2n; spotlight edges always drawn.
  // Small graphs (<60 nodes) draw everything above the slider.
  const ranked = aboveSlider.slice().sort((a, b) => (b.weight || 0) - (a.weight || 0));
  let drawnEdges = ranked;
  if (nodes.length >= 60) {
    const cap = nodes.length * 2;
    const must = new Set();
    const rest = [];
    for (const e of ranked) {
      if (graphSpotlight !== null && (e.a === graphSpotlight || e.b === graphSpotlight)) must.add(e);
      else rest.push(e);
    }
    drawnEdges = [...must, ...rest.slice(0, Math.max(0, cap - must.size))];
  }

  const byDegree = nodes.slice().sort((a, b) => (b.degree || 0) - (a.degree || 0));
  const labelIds = new Set(byDegree.slice(0, graphLabelTopN).map((n) => n.id));

  const spacing = (graphLayout && graphLayout.spacing) || 60;
  const display = graphDisplayState || defaultGraphDisplay();
  const baseR = Math.min(8, Math.max(3, spacing * 0.12));
  const nodeR = (n) => (baseR + Math.min(4, Math.sqrt(n.degree || 0))) * display.nodeSize;
  const byId = {};
  for (const n of nodes) byId[n.id] = n;
  const showTopLabels = graphZoom() >= display.labelZoom;
  const wantArrows = display.arrows && graphCustomId !== null;
  let defs = '';
  if (wantArrows) {
    defs = '<defs><marker id="graph-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0,0 L10,5 L0,10 z" fill="context-stroke"/></marker></defs>';
  }

  // Determine active roving tabindex node
  let rovingId = graphFocusedNodeId;
  if (rovingId === null || !nodes.some((n) => n.id === rovingId)) {
    if (graphSpotlight !== null && nodes.some((n) => n.id === graphSpotlight)) {
      rovingId = graphSpotlight;
    } else {
      const strongest = nodes.slice().sort((a, b) => (b.degree || 0) - (a.degree || 0))[0];
      rovingId = strongest ? strongest.id : nodes[0].id;
    }
    graphFocusedNodeId = rovingId;
  }

  const dimNode = (id) => (graphSpotlight === null || neighborIds.has(id) ? '' : ' dim');

  let html = defs;
  for (const e of drawnEdges) {
    const a = graphPos[e.a];
    const b = graphPos[e.b];
    if (!a || !b) continue;
    const kinds = e.kinds || [];
    const cls = kinds.includes('mention')
      ? 'edge mention'
      : kinds.includes('custom') ? 'edge custom' : 'edge shared';
    const spot = graphSpotlight !== null && (e.a === graphSpotlight || e.b === graphSpotlight);
    let w = (1 + Math.min(6, e.weight || 1)) * display.thick;
    if (spot) w *= 1.5;
    const dimmed = graphSpotlight !== null && !spot;
    const op = spot ? ' stroke-opacity="0.9"' : dimmed ? ' stroke-opacity="0.08"' : '';
    const edgeTitle = e.label ? `<title>${esc(e.label)}</title>` : '';
    let x1 = a.x;
    let y1 = a.y;
    let x2 = b.x;
    let y2 = b.y;
    let marker = '';
    if (wantArrows && cls === 'edge custom') {
      const dx = x2 - x1;
      const dy = y2 - y1;
      const d = Math.hypot(dx, dy) || 1;
      const r1 = nodeR(byId[e.a] || { degree: 0 }) + 3;
      const r2 = nodeR(byId[e.b] || { degree: 0 }) + 3;
      x1 += (dx / d) * r1;
      y1 += (dy / d) * r1;
      x2 -= (dx / d) * r2;
      y2 -= (dy / d) * r2;
      marker = ' marker-end="url(#graph-arrow)"';
    }
    html += `<line x1="${x1.toFixed(1)}" y1="${y1.toFixed(1)}" x2="${x2.toFixed(1)}" y2="${y2.toFixed(1)}" class="${cls}${dimmed ? ' dim' : ''}" stroke-width="${w.toFixed(2)}"${op}${marker}>${edgeTitle}</line>`;
  }

  for (const n of nodes) {
    const p = graphPos[n.id];
    if (!p) continue;
    const isSpotlight = n.id === graphSpotlight ? ' spotlight' : '';
    const isFocused = n.id === graphFocusedNodeId ? ' focused' : '';
    const tabIndex = n.id === graphFocusedNodeId ? '0' : '-1';
    const kindCls = ` k-${esc(n.kind || 'lore')}`;
    const ariaLabel = `${esc(n.label)}, ${esc(n.kind)}, ${n.degree || 0} links`;

    const showLabel =
      (showTopLabels && labelIds.has(n.id)) ||
      n.id === graphSpotlight ||
      (graphSpotlight !== null && neighborIds.has(n.id)) ||
      n.id === graphFocusedNodeId ||
      n.id === graphHoveredNodeId;

    const fullLabel = String(n.label || '');
    const displayLabel = fullLabel.length > 14 ? `${fullLabel.slice(0, 13)}…` : fullLabel;
    const textTag = showLabel
      ? `<text x="${p.x}" y="${p.y + 4}" text-anchor="middle">${esc(displayLabel)}</text>`
      : '';
    const titleTag = `<title>${esc(n.label)} (${esc(n.kind)})</title>`;

    html += `<g role="button" data-id="${n.id}" tabindex="${tabIndex}" aria-label="${ariaLabel}" class="gnode${kindCls}${dimNode(n.id)}${isSpotlight}${isFocused}"><circle cx="${p.x}" cy="${p.y}" r="${nodeR(n).toFixed(1)}"/>${textTag}${titleTag}</g>`;
  }

  svg.innerHTML = html;

  svg.querySelectorAll('g[data-id]').forEach((g) => {
    const id = Number(g.dataset.id);
    const node = nodes.find((x) => x.id === id);

    g.addEventListener('click', (ev) => {
      ev.stopPropagation();
      if (graphDragged) {
        graphDragged = false;
        return;
      }
      if (graphCustomId !== null) {
        if (ev.shiftKey) {
          linkCustomFlow(id);
          return;
        }
        graphFocusedNodeId = id;
        graphWalkNeighborIdx = 0;
        graphSpotlight = graphSpotlight === id ? null : id;
        updateGraphRovingTabindex(id);
        renderGraph(graph);
        return;
      }
      graphFocusedNodeId = id;
      graphWalkNeighborIdx = 0;
      graphSpotlight = graphSpotlight === id ? null : id;
      updateGraphRovingTabindex(id);
      if (node && typeof showEntityInspector === 'function') {
        showEntityInspector(id, node);
      }
      renderGraph(graph);
      const localBox = document.getElementById('graph-f-local');
      if (localBox && localBox.checked) requeryGraphSoon();
    });

    g.addEventListener('focus', () => {
      if (graphFocusedNodeId !== id) {
        graphFocusedNodeId = id;
        updateGraphRovingTabindex(id);
        if (graphCustomId === null && node && typeof showEntityInspector === 'function') {
          showEntityInspector(id, node);
        }
      }
    });

    g.addEventListener('mouseenter', () => {
      graphHoveredNodeId = id;
      if (!g.querySelector('text')) {
        const p = graphPos[id];
        if (!p) return;
        const full = String(node ? node.label : '');
        const label = full.length > 14 ? `${full.slice(0, 13)}…` : full;
        const text = document.createElementNS('http://www.w3.org/2000/svg', 'text');
        text.setAttribute('x', String(p.x));
        text.setAttribute('y', String(p.y + 4));
        text.setAttribute('text-anchor', 'middle');
        text.textContent = label;
        text.dataset.hover = '1';
        g.appendChild(text);
      }
    });

    g.addEventListener('mouseleave', () => {
      graphHoveredNodeId = null;
      const text = g.querySelector('text[data-hover="1"]');
      if (text) text.remove();
    });

    g.addEventListener('pointerdown', (ev) => {
      if (ev.button !== 0 || ev.shiftKey || !graphSim) return;
      const i = graphSim.nodeIndex(id);
      if (i < 0) return;
      simDirty = true;
      const p = graphPos[id];
      graphSim.pin(i, p ? p.x : 0, p ? p.y : 0);
      graphSim.reheat(0.3);
      dragNode = {
        id, i, custom: graphCustomId !== null, moved: false, sx: ev.clientX, sy: ev.clientY,
      };
      ensureSimLoop();
    });

    g.addEventListener('keydown', (e) => {
      if (!node) return;
      if ((e.key === 'Delete' || e.key === 'Backspace') && graphCustomId !== null) {
        e.preventDefault();
        removeCustomNodeFlow(graphCustomId, id, node);
        return;
      }
      const neighbors = node.neighbors || [];

      if (e.key === 'ArrowRight' || e.key === 'ArrowLeft') {
        e.preventDefault();
        if (neighbors.length === 0) return;
        if (e.key === 'ArrowRight') {
          graphWalkNeighborIdx = (graphWalkNeighborIdx + 1) % neighbors.length;
        } else {
          graphWalkNeighborIdx = (graphWalkNeighborIdx - 1 + neighbors.length) % neighbors.length;
        }
        const target = neighbors[graphWalkNeighborIdx];
        if (target) focusGraphNode(target.id, false);
      } else if (e.key === 'Home') {
        e.preventDefault();
        if (neighbors.length > 0) {
          graphWalkNeighborIdx = 0;
          focusGraphNode(neighbors[0].id, false);
        } else {
          const strongest = nodes.slice().sort((a, b) => (b.degree || 0) - (a.degree || 0))[0];
          if (strongest) focusGraphNode(strongest.id);
        }
      } else if (e.key === 'End') {
        e.preventDefault();
        if (neighbors.length > 0) {
          graphWalkNeighborIdx = neighbors.length - 1;
          focusGraphNode(neighbors[graphWalkNeighborIdx].id, false);
        } else {
          const weakest = nodes.slice().sort((a, b) => (a.degree || 0) - (b.degree || 0))[0];
          if (weakest) focusGraphNode(weakest.id);
        }
      } else if (e.key === 'Enter') {
        e.preventDefault();
        graphSpotlight = graphSpotlight === id ? null : id;
        renderGraph(graph);
        refocusGraphNode(id);
      } else if (e.key === 'Escape') {
        e.preventDefault();
        graphSpotlight = null;
        renderGraph(graph);
        refocusGraphNode(id);
      }
    });
  });

  const legend = document.getElementById('graph-legend');
  legend.innerHTML = '';
  if (graphCustomId !== null) return;
  if (!nodes.length) {
    const span = document.createElement('span');
    span.textContent = 'nothing matches. loosen a filter or ';
    legend.appendChild(span);
    const reset = document.createElement('button');
    reset.className = 'btn';
    reset.textContent = 'reset';
    reset.addEventListener('click', () => resetGraphFilters());
    legend.appendChild(reset);
    return;
  }
  const shownEdges = drawnEdges.filter((e) =>
    graphSpotlight === null || e.a === graphSpotlight || e.b === graphSpotlight
  ).length;
  const totalNodes = graph.total_nodes ?? graphData.total_nodes ?? nodes.length;
  const totalEdges = graph.total_edges ?? graphData.total_edges ?? edges.length;
  let line = `showing ${nodes.length} of ${totalNodes} entities · ${shownEdges} of ${totalEdges} links.`;
  if (edges.length > 0 && shownEdges === 0) {
    line += ' no links at this weight. lower min weight.';
  }
  legend.textContent = line;
}
