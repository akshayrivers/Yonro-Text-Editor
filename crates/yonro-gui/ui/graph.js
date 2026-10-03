/* Yonro graph view. Classic script.
 * Edges use CSS classes (.edge.mention solid, .edge.shared dashed) so
 * meaning never depends on color alone. No hardcoded colors here.
 */
let graphSpotlight = null;

async function loadGraph() {
  try {
    const graph = await core.graph();
    renderGraph(graph);
  } catch (err) {
    document.getElementById('graph-legend').textContent = `Graph unavailable: ${err}`;
  }
}

function renderGraph(graph) {
  const svg = document.getElementById('graph');
  const W = 900;
  const H = 480;
  svg.setAttribute('viewBox', `0 0 ${W} ${H}`);
  const nodes = graph.nodes || [];
  const edges = graph.edges || [];
  if (!nodes.length) {
    svg.innerHTML = '';
    document.getElementById('graph-legend').textContent = 'No linked entities yet — set scene POVs and write @mentions.';
    return;
  }
  const cx = W / 2;
  const cy = H / 2;
  const radius = Math.min(W, H) / 2 - 60;
  const pos = {};
  nodes.forEach((n, i) => {
    const angle = (2 * Math.PI * i) / nodes.length - Math.PI / 2;
    pos[n.id] = { x: cx + radius * Math.cos(angle), y: cy + radius * Math.sin(angle) };
  });
  const neighborIds = new Set();
  if (graphSpotlight !== null) {
    neighborIds.add(graphSpotlight);
    for (const e of edges) {
      if (e.a === graphSpotlight) neighborIds.add(e.b);
      if (e.b === graphSpotlight) neighborIds.add(e.a);
    }
  }
  const dim = (id) => (graphSpotlight === null || neighborIds.has(id) ? '' : ' dim');
  const dimEdge = (e) =>
    graphSpotlight === null || e.a === graphSpotlight || e.b === graphSpotlight ? '' : ' dim';
  let html = '';
  for (const e of edges) {
    const a = pos[e.a];
    const b = pos[e.b];
    if (!a || !b) continue;
    const w = 1 + Math.min(6, e.weight);
    const cls = (e.kinds || []).includes('mention') ? 'edge mention' : 'edge shared';
    html += `<line x1="${a.x}" y1="${a.y}" x2="${b.x}" y2="${b.y}" class="${cls}${dimEdge(e)}" stroke-width="${w}"><title>${e.shared} shared · ${e.mentions} mentions</title></line>`;
  }
  for (const n of nodes) {
    const p = pos[n.id];
    html += `<g data-id="${n.id}" class="gnode${dim(n.id)}"><circle cx="${p.x}" cy="${p.y}" r="16"/><text x="${p.x}" y="${p.y + 4}" text-anchor="middle">${esc(String(n.label).slice(0, 10))}</text><title>${esc(n.label)} (${esc(n.kind)})</title></g>`;
  }
  svg.innerHTML = html;
  svg.querySelectorAll('g[data-id]').forEach((g) => {
    g.addEventListener('click', () => {
      const id = Number(g.dataset.id);
      graphSpotlight = graphSpotlight === id ? null : id;
      renderGraph(graph);
    });
  });
  const shown = graphSpotlight === null ? edges.length : edges.filter((e) => e.a === graphSpotlight || e.b === graphSpotlight).length;
  document.getElementById('graph-legend').textContent =
    `${nodes.length} entities · ${edges.length} links (${shown} shown). Solid line = mention link, dashed = shared scene.`;
}
