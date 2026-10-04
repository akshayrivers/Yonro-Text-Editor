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
  if (!Number.isFinite(bb.x0) || bb.x1 <= bb.x0 || bb.y1 <= bb.y0) return;
  const pad = 0.06;
  const bw = (bb.x1 - bb.x0) * (1 + pad * 2) || 1;
  const bh = (bb.y1 - bb.y0) * (1 + pad * 2) || 1;
  const cw = svg.clientWidth || 900;
  const ch = svg.clientHeight || 480;
  const scale = Math.max(bw / cw, bh / ch);
  const w = cw * scale;
  const h = ch * scale;
  const cx = (bb.x0 + bb.x1) / 2;
  const cy = (bb.y0 + bb.y1) / 2;
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

function bindGraphControlsOnce() {
  if (graphControlsBound) return;
  graphControlsBound = true;

  const slider = document.getElementById('graph-min-weight');
  if (slider) {
    slider.addEventListener('input', () => {
      if (graphData) renderGraph(graphData);
    });
  }

  const btnIn = document.getElementById('graph-zoom-in');
  if (btnIn) btnIn.addEventListener('click', () => {
    zoomGraph(0.8);
    refreshGraphLabelsSoon();
  });

  const btnOut = document.getElementById('graph-zoom-out');
  if (btnOut) btnOut.addEventListener('click', () => {
    zoomGraph(1.25);
    refreshGraphLabelsSoon();
  });

  const btnFit = document.getElementById('graph-zoom-fit');
  if (btnFit) {
    btnFit.addEventListener('click', () => {
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
    });

    svg.addEventListener('wheel', (e) => {
      e.preventDefault();
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
  if (graphCustomId !== null && customGraphs.some((g) => g.id === graphCustomId)) {
    renderCustomGraph();
    return;
  }
  graphCustomId = null;
  graphSpotlight = null;
  pendingEdgeFrom = null;
  renderGraphTabs();
  try {
    graphData = await core.graph();
    const nodes = graphData.nodes || [];
    const edges = graphData.edges || [];
    // Slider spans the real data: max is never above every edge weight.
    const slider = document.getElementById('graph-min-weight');
    if (slider) {
      const maxW = edges.reduce((m, e) => Math.max(m, e.weight || 0), 1);
      slider.min = '1';
      slider.max = String(Math.max(1, maxW));
      slider.value = '1';
    }
    graphLayout = forceLayout(nodes, edges);
    graphPos = graphLayout.pos;
    fitGraph();
  } catch (err) {
    document.getElementById('graph-legend').textContent = `could not load graph: ${errText(err)}`;
  }
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
  const slider = document.querySelector('.graph-slider-label');
  if (slider) slider.style.display = graphCustomId === null ? '' : 'none';
  for (const id of ['graph-rename', 'graph-delete']) {
    const btn = document.getElementById(id);
    if (btn) btn.hidden = graphCustomId === null;
  }
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
  const pos = {};
  let x0 = Infinity;
  let y0 = Infinity;
  let x1 = -Infinity;
  let y1 = -Infinity;
  for (const n of g.nodes) {
    pos[n.id] = { x: n.x, y: n.y };
    x0 = Math.min(x0, n.x);
    x1 = Math.max(x1, n.x);
    y0 = Math.min(y0, n.y);
    y1 = Math.max(y1, n.y);
  }
  if (!Number.isFinite(x0) || x1 <= x0) {
    x0 -= 100;
    x1 += 100;
  }
  if (!Number.isFinite(y0) || y1 <= y0) {
    y0 -= 100;
    y1 += 100;
  }
  graphLayout = { pos, bbox: { x0, y0, x1, y1 }, spacing: 50 };
  graphPos = pos;
  graphData = { nodes, edges };
  fitGraph();
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

function updateGraphRovingTabindex(activeId) {
  const nodes = document.querySelectorAll('#graph g.gnode');
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

  const slider = document.getElementById('graph-min-weight');
  const minWeight = slider ? Number(slider.value) || 1 : 1;
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
  const baseR = Math.min(8, Math.max(3, spacing * 0.12));
  const nodeR = (n) => baseR + Math.min(4, Math.sqrt(n.degree || 0));

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
  const dimEdge = (e) =>
    graphSpotlight === null || e.a === graphSpotlight || e.b === graphSpotlight ? '' : ' dim';

  let html = '';
  for (const e of drawnEdges) {
    const a = graphPos[e.a];
    const b = graphPos[e.b];
    if (!a || !b) continue;
    const w = 1 + Math.min(6, e.weight || 1);
    const kinds = e.kinds || [];
    const cls = kinds.includes('mention')
      ? 'edge mention'
      : kinds.includes('custom') ? 'edge custom' : 'edge shared';
    const edgeTitle = e.label ? `<title>${esc(e.label)}</title>` : '';
    html += `<line x1="${a.x}" y1="${a.y}" x2="${b.x}" y2="${b.y}" class="${cls}${dimEdge(e)}" stroke-width="${w}">${edgeTitle}</line>`;
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
      labelIds.has(n.id) ||
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

    let downAt = null;
    g.addEventListener('pointerdown', (ev) => {
      if (graphCustomId === null || ev.shiftKey || ev.button !== 0) return;
      downAt = { x: ev.clientX, y: ev.clientY };
    });
    g.addEventListener('pointerup', (ev) => {
      if (!downAt || graphCustomId === null) return;
      const moved = Math.hypot(ev.clientX - downAt.x, ev.clientY - downAt.y);
      downAt = null;
      if (moved < 4) return;
      graphDragged = true;
      const pt = svgPoint(ev);
      core.moveGraphNode(graphCustomId, id, pt.x, pt.y).then(() => {
        reloadCustomGraphs(graphCustomId);
      }).catch((err) => {
        setMessage(`could not move node: ${errText(err)}`, { error: true });
      });
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
  if (!drawnEdges.length && aboveSlider.length === 0 && edges.length > 0) {
    legend.textContent = 'no links at this weight. lower the slider.';
    return;
  }
  const shownEdges = drawnEdges.filter((e) =>
    graphSpotlight === null || e.a === graphSpotlight || e.b === graphSpotlight
  ).length;
  const kindCounts = {};
  for (const n of nodes) kindCounts[n.kind] = (kindCounts[n.kind] || 0) + 1;
  const kindSummary = Object.keys(kindCounts)
    .sort()
    .map((k) => `${kindCounts[k]} ${k}${kindCounts[k] === 1 ? '' : 's'}`)
    .join(' · ');
  legend.textContent =
    `${nodes.length} entities (${kindSummary}) · ${shownEdges} of ${edges.length} links shown. solid: @mention · dashed: shared scene.`;
}
