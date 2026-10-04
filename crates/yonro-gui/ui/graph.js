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
let graphViewBox = { x: 0, y: 0, w: 900, h: 480 };
let graphPanning = false;
let graphPanStart = { x: 0, y: 0, vbx: 0, vby: 0 };
let graphControlsBound = false;

function computeGraphLayout(nodes, edges, W, H) {
  const cx = W / 2;
  const cy = H / 2;
  const pos = {};
  if (nodes.length <= 12) {
    const radius = Math.min(W, H) / 2 - 60;
    nodes.forEach((n, i) => {
      const angle = (2 * Math.PI * i) / (nodes.length || 1) - Math.PI / 2;
      pos[n.id] = { x: cx + radius * Math.cos(angle), y: cy + radius * Math.sin(angle) };
    });
    return pos;
  }

  // Deterministic seed by node id
  nodes.forEach((n, i) => {
    let h = Math.imul(Number(n.id) ^ 0x9e3779b9, 0x85ebca6b) ^ Math.imul(i, 0xc2b2ae35);
    h = Math.imul(h ^ (h >>> 16), 0x7feb352d);
    h = (h ^ (h >>> 15)) >>> 0;
    const angle = ((h % 10000) / 10000) * 2 * Math.PI;
    const dist = 30 + (((h >>> 14) % 10000) / 10000) * (Math.min(W, H) / 2 - 60);
    pos[n.id] = {
      x: cx + dist * Math.cos(angle),
      y: cy + dist * Math.sin(angle),
      vx: 0,
      vy: 0
    };
  });

  const iterations = 120;
  const k = Math.sqrt((W * H) / (nodes.length || 1));
  const k2 = k * k;

  for (let iter = 0; iter < iterations; iter++) {
    const temp = ((iterations - iter) / iterations) * 18;

    for (let i = 0; i < nodes.length; i++) {
      const p = pos[nodes[i].id];
      p.vx = (cx - p.x) * 0.01;
      p.vy = (cy - p.y) * 0.01;
    }

    for (let i = 0; i < nodes.length; i++) {
      const p1 = pos[nodes[i].id];
      for (let j = i + 1; j < nodes.length; j++) {
        const p2 = pos[nodes[j].id];
        let dx = p1.x - p2.x;
        let dy = p1.y - p2.y;
        let dist2 = dx * dx + dy * dy;
        if (dist2 < 1) {
          dx = 1;
          dist2 = 1;
        }
        const dist = Math.sqrt(dist2);
        const force = k2 / dist2;
        const fx = (dx / dist) * force;
        const fy = (dy / dist) * force;
        p1.vx += fx;
        p1.vy += fy;
        p2.vx -= fx;
        p2.vy -= fy;
      }
    }

    for (let i = 0; i < edges.length; i++) {
      const e = edges[i];
      const p1 = pos[e.a];
      const p2 = pos[e.b];
      if (!p1 || !p2) continue;
      const dx = p2.x - p1.x;
      const dy = p2.y - p1.y;
      const dist = Math.sqrt(dx * dx + dy * dy) || 1;
      const force = ((dist * dist) / k) * 0.08 * Math.min(e.weight || 1, 4);
      const fx = (dx / dist) * force;
      const fy = (dy / dist) * force;
      p1.vx += fx;
      p1.vy += fy;
      p2.vx -= fx;
      p2.vy -= fy;
    }

    for (let i = 0; i < nodes.length; i++) {
      const p = pos[nodes[i].id];
      const speed = Math.sqrt(p.vx * p.vx + p.vy * p.vy) || 1;
      const capped = Math.min(speed, temp);
      p.x += (p.vx / speed) * capped;
      p.y += (p.vy / speed) * capped;
      p.x = Math.max(30, Math.min(W - 30, p.x));
      p.y = Math.max(30, Math.min(H - 30, p.y));
    }
  }

  return pos;
}

function updateGraphViewBox() {
  const svg = document.getElementById('graph');
  if (svg) {
    svg.setAttribute('viewBox', `${graphViewBox.x} ${graphViewBox.y} ${graphViewBox.w} ${graphViewBox.h}`);
  }
}

function zoomGraph(factor, rx, ry) {
  const rX = typeof rx === 'number' ? rx : 0.5;
  const rY = typeof ry === 'number' ? ry : 0.5;
  const newW = Math.max(120, Math.min(4800, graphViewBox.w * factor));
  const newH = newW * (480 / 900);
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
  if (btnIn) btnIn.addEventListener('click', () => zoomGraph(0.8));

  const btnOut = document.getElementById('graph-zoom-out');
  if (btnOut) btnOut.addEventListener('click', () => zoomGraph(1.25));

  const btnFit = document.getElementById('graph-zoom-fit');
  if (btnFit) {
    btnFit.addEventListener('click', () => {
      graphViewBox = { x: 0, y: 0, w: 900, h: 480 };
      updateGraphViewBox();
    });
  }

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
    }, { passive: false });
  }
}

async function loadGraph() {
  bindGraphControlsOnce();
  try {
    graphData = await core.graph();
    graphPos = computeGraphLayout(graphData.nodes || [], graphData.edges || [], 900, 480);
    renderGraph(graphData);
  } catch (err) {
    document.getElementById('graph-legend').textContent = `Graph unavailable: ${err}`;
  }
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
  if (node && typeof showEntityInspector === 'function') {
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
  const filteredEdges = edges.filter((e) => (e.weight || 0) >= minWeight);

  const neighborIds = new Set();
  if (graphSpotlight !== null) {
    neighborIds.add(graphSpotlight);
    for (const e of edges) {
      if (e.a === graphSpotlight) neighborIds.add(e.b);
      if (e.b === graphSpotlight) neighborIds.add(e.a);
    }
  }

  const isDense = nodes.length > 200;
  const top25Ids = isDense
    ? new Set(nodes.slice().sort((a, b) => (b.degree || 0) - (a.degree || 0)).slice(0, 25).map((n) => n.id))
    : null;

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
  for (const e of filteredEdges) {
    const a = graphPos[e.a];
    const b = graphPos[e.b];
    if (!a || !b) continue;
    const w = 1 + Math.min(6, e.weight || 1);
    const cls = (e.kinds || []).includes('mention') ? 'edge mention' : 'edge shared';
    const titleTag = isDense ? '' : `<title>${e.shared || 0} shared · ${e.mentions || 0} mentions</title>`;
    html += `<line x1="${a.x}" y1="${a.y}" x2="${b.x}" y2="${b.y}" class="${cls}${dimEdge(e)}" stroke-width="${w}">${titleTag}</line>`;
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
      !isDense ||
      top25Ids.has(n.id) ||
      n.id === graphSpotlight ||
      (graphSpotlight !== null && neighborIds.has(n.id)) ||
      n.id === graphFocusedNodeId ||
      n.id === graphHoveredNodeId;

    const fullLabel = String(n.label || '');
    const displayLabel = fullLabel.length > 14 ? `${fullLabel.slice(0, 13)}…` : fullLabel;
    const textTag = showLabel
      ? `<text x="${p.x}" y="${p.y + 4}" text-anchor="middle">${esc(displayLabel)}</text>`
      : '';
    const titleTag = isDense ? '' : `<title>${esc(n.label)} (${esc(n.kind)})</title>`;

    html += `<g role="button" data-id="${n.id}" tabindex="${tabIndex}" aria-label="${ariaLabel}" class="gnode${kindCls}${dimNode(n.id)}${isSpotlight}${isFocused}"><circle cx="${p.x}" cy="${p.y}" r="16"/>${textTag}${titleTag}</g>`;
  }

  svg.innerHTML = html;

  svg.querySelectorAll('g[data-id]').forEach((g) => {
    const id = Number(g.dataset.id);
    const node = nodes.find((x) => x.id === id);

    g.addEventListener('click', (e) => {
      e.stopPropagation();
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
        if (node && typeof showEntityInspector === 'function') {
          showEntityInspector(id, node);
        }
      }
    });

    g.addEventListener('mouseenter', () => {
      if (isDense) {
        graphHoveredNodeId = id;
        if (!g.querySelector('text')) {
          const p = graphPos[id];
          const full = String(node ? node.label : '');
          const label = full.length > 14 ? `${full.slice(0, 13)}…` : full;
          const text = document.createElementNS('http://www.w3.org/2000/svg', 'text');
          text.setAttribute('x', String(p.x));
          text.setAttribute('y', String(p.y + 4));
          text.setAttribute('text-anchor', 'middle');
          text.textContent = label;
          g.appendChild(text);
        }
      }
    });

    g.addEventListener('mouseleave', () => {
      if (isDense) {
        graphHoveredNodeId = null;
        if (!top25Ids.has(id) && id !== graphSpotlight && id !== graphFocusedNodeId && (!graphSpotlight || !neighborIds.has(id))) {
          const text = g.querySelector('text');
          if (text) text.remove();
        }
      }
    });

    g.addEventListener('keydown', (e) => {
      if (!node) return;
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

  const shownEdges = filteredEdges.filter((e) =>
    graphSpotlight === null || e.a === graphSpotlight || e.b === graphSpotlight
  ).length;
  const kindCounts = {};
  for (const n of nodes) kindCounts[n.kind] = (kindCounts[n.kind] || 0) + 1;
  const kindSummary = Object.keys(kindCounts)
    .sort()
    .map((k) => `${kindCounts[k]} ${k}${kindCounts[k] === 1 ? '' : 's'}`)
    .join(' · ');
  document.getElementById('graph-legend').textContent =
    `${nodes.length} entities (${kindSummary}) · ${edges.length} links (${shownEdges} shown). solid: @mention · dashed: shared scene.`;
}
