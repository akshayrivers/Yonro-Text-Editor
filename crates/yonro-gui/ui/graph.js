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
  }
}

async function loadGraph() {
  bindGraphControlsOnce();
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
    const cls = (e.kinds || []).includes('mention') ? 'edge mention' : 'edge shared';
    html += `<line x1="${a.x}" y1="${a.y}" x2="${b.x}" y2="${b.y}" class="${cls}${dimEdge(e)}" stroke-width="${w}"/>`;
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
