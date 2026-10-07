/* Structure panel, overview dashboard and ER map — all read-only insights. */

function fmtBytes(n) {
  if (n == null) return '—';
  const u = ['B', 'KB', 'MB', 'GB', 'TB'];
  let v = Number(n), i = 0;
  while (v >= 1024 && i < u.length - 1) { v /= 1024; i++; }
  return `${i ? v.toFixed(1) : v} ${u[i]}`;
}

/* --------------------------------- structure --------------------------------- */

function openStructure(p, d, s, t) {
  const key = `meta:${p.id}:${d.name}:${s.name}:${t.name}`;
  const view = state.tabs.find((x) => x.key === key)?.view || { meta: null, loading: false, err: '' };
  openTab({
    key,
    title: `结构 · ${t.name}`,
    crumbs: `${p.name || p.driver} / ${pathCrumb(p, d, s)} / ${t.name} · 结构`,
    view,
    run: () => loadMeta(p, d, s, t, view),
    render: (pane) => renderMeta(pane, p, d, s, t, view),
  });
  if (!view.meta && !view.loading) loadMeta(p, d, s, t, view);
}

async function loadMeta(p, d, s, t, view) {
  view.loading = true;
  view.err = '';
  try {
    view.meta = await call('meta_table', { r: refOf(p, d, s, t) });
  } catch (e) {
    view.err = String(e);
    view.meta = null;
  } finally {
    view.loading = false;
    if (state.tabs.find((x) => x.view === view)) renderPane();
  }
}

function renderMeta(pane, p, d, s, t, view) {
  if (view.loading) { pane.appendChild(el('div', { class: 'panel', text: '读取结构…' })); return; }
  if (view.err) { pane.appendChild(el('div', { class: 'err-line', text: view.err })); return; }
  const m = view.meta;
  if (!m) { pane.appendChild(el('div', { class: 'panel', text: '准备加载…' })); return; }

  const colRow = (c) => el('tr', {}, [
    el('td', { style: 'font-family:var(--mono)', text: c.name }),
    el('td', { style: 'font-family:var(--mono);color:var(--accent2)', text: c.type }),
    el('td', { text: c.nullable ? '是' : '否' }),
    el('td', { style: 'font-family:var(--mono);color:var(--fg2)', text: c.defaultValue ?? '—' }),
    el('td', { text: c.isPk ? 'PK' : '' }),
    el('td', { style: 'color:var(--fg2)', text: c.references ?? '' }),
    el('td', { style: 'color:var(--fg2)', text: c.extra ?? '' }),
    el('td', { style: 'color:var(--fg2)', text: c.comment ?? '' }),
  ]);

  pane.appendChild(el('div', { class: 'toolbar' }, [
    el('button', { class: 'btn', text: '刷新', onclick: () => loadMeta(p, d, s, t, view) }),
    el('button', { class: 'btn', text: '打开数据', onclick: () => openTableTab(p, d, s, t) }),
    el('button', { class: 'btn ghost', text: '生成 SELECT', onclick: () => openSqlWith(selectStar(p, d, s, t)) }),
    el('button', { class: 'btn ghost', text: '复制 DDL', onclick: async () => { try { await navigator.clipboard.writeText(m.ddl); toast('DDL 已复制'); } catch (_) { toast('无法访问剪贴板', true); } } }),
    el('span', { class: 'spacer' }),
    el('span', { class: 'hint', text: `${m.columns.length} 字段 · ${m.indexes.length} 索引 · ${m.foreignKeys.length} 外键` }),
  ]));

  pane.appendChild(el('div', { class: 'panel' }, [
    el('h3', { text: m.table }),
    el('div', { class: 'kv' }, [
      el('span', { class: 'k', text: '引擎' }), el('span', { text: `${DRIVER_LABEL[p.driver] || p.driver}${state.about[p.id]?.version ? ' · ' + state.about[p.id].version : ''}` }),
      el('span', { class: 'k', text: '位置' }), el('span', { text: `${d.name || p.file || '—'}${s.name ? ' / ' + s.name : ''}` }),
      el('span', { class: 'k', text: '行数' }), el('span', { text: m.rowCount == null ? '未知' : String(m.rowCount) }),
      el('span', { class: 'k', text: '可编辑' }), el('span', { text: m.editable ? '是（有主键）' : '否（无主键，只能用 SQL 改）' }),
      el('span', { class: 'k', text: '主键' }), el('span', { text: m.primaryKeys.join(', ') || '—' }),
    ]),
  ]));

  pane.appendChild(el('div', { class: 'panel', style: 'padding-top:0' }, [
    el('h3', { text: '字段' }),
    el('table', { class: 'list' }, [
      el('thead', {}, [el('tr', {}, ['字段', '类型', '可空', '默认值', '键', '引用', '额外', '注释'].map((x) => el('th', { text: x })))]),
      el('tbody', {}, m.columns.map(colRow)),
    ]),
  ]));

  if (m.indexes.length) {
    pane.appendChild(el('div', { class: 'panel', style: 'padding-top:0' }, [
      el('h3', { text: '索引' }),
      el('table', { class: 'list' }, [
        el('thead', {}, [el('tr', {}, ['名称', '唯一', '主键', '方法', '列'].map((x) => el('th', { text: x })))]),
        el('tbody', {}, m.indexes.map((i) => el('tr', {}, [
          el('td', { style: 'font-family:var(--mono)', text: i.name }),
          el('td', { text: i.unique ? '✓' : '' }), el('td', { text: i.primary ? '✓' : '' }),
          el('td', { style: 'color:var(--fg2)', text: i.kind ?? '' }),
          el('td', { style: 'font-family:var(--mono)', text: i.columns.join(', ') }),
        ]))),
      ]),
    ]));
  }
  if (m.foreignKeys.length) {
    pane.appendChild(el('div', { class: 'panel', style: 'padding-top:0' }, [
      el('h3', { text: '外键' }),
      el('table', { class: 'list' }, [
        el('thead', {}, [el('tr', {}, ['名称', '列', '引用', 'ON DELETE', 'ON UPDATE'].map((x) => el('th', { text: x })))]),
        el('tbody', {}, m.foreignKeys.map((f) => el('tr', {}, [
          el('td', { style: 'font-family:var(--mono)', text: f.name }),
          el('td', { style: 'font-family:var(--mono)', text: f.columns.join(', ') }),
          el('td', { style: 'font-family:var(--mono);color:var(--accent2)', text: `${f.refTable}(${f.refColumns.join(', ')})` }),
          el('td', { style: 'color:var(--fg2)', text: f.onDelete ?? '' }),
          el('td', { style: 'color:var(--fg2)', text: f.onUpdate ?? '' }),
        ]))),
      ]),
    ]));
  }
  pane.appendChild(el('div', { class: 'panel', style: 'padding-top:0' }, [el('h3', { text: 'DDL' }), el('pre', { class: 'sql', text: m.ddl || '（该引擎不返回 DDL）' })]));
}

/* ---------------------------------- overview ---------------------------------- */

function openDashboard(p, d, s) {
  const key = `ov:${p.id}:${d.name}:${s.name}`;
  const view = state.tabs.find((x) => x.key === key)?.view || { data: null, loading: false, err: '' };
  openTab({
    key, title: `总览 · ${d.name || s.name || p.name || ''}`,
    crumbs: `${p.name || p.driver} / ${pathCrumb(p, d, s)} · 数据总览`,
    view,
    run: () => loadOverview(p, d, s, view),
    render: (pane) => renderOverview(pane, p, d, s, view),
  });
  if (!view.data && !view.loading) loadOverview(p, d, s, view);
}

async function loadOverview(p, d, s, view) {
  view.loading = true;
  view.err = '';
  try { view.data = await call('overview', { scope: scopeOf(p.id, d.name, s.name) }); } catch (e) { view.err = String(e); view.data = null; }
  finally { view.loading = false; if (state.tabs.find((x) => x.view === view)) renderPane(); }
}

function renderOverview(pane, p, d, s, view) {
  if (view.loading) { pane.appendChild(el('div', { class: 'panel', text: '统计中，正在逐表计数…' })); return; }
  if (view.err) { pane.appendChild(el('div', { class: 'err-line', text: view.err })); return; }
  const o = view.data;
  if (!o) { pane.appendChild(el('div', { class: 'panel', text: '准备加载…' })); return; }
  const card = (n, t) => el('div', { class: 'card' }, [el('div', { class: 'n', text: String(n) }), el('div', { class: 't', text: t })]);

  pane.appendChild(el('div', { class: 'toolbar' }, [
    el('button', { class: 'btn', text: '重新统计', onclick: () => loadOverview(p, d, s, view) }),
    el('span', { class: 'spacer' }),
    el('span', { class: 'hint', text: `${o.database || p.file || ''}${o.schema ? '.' + o.schema : ''} · ${o.driver}` }),
  ]));
  pane.appendChild(el('div', { class: 'panel' }, [
    el('div', { class: 'cards' }, [
      card(o.totals.tables, '表'), card(o.totals.views, '视图'), card(o.totals.columns, '字段'),
      card(o.totals.rows.toLocaleString(), '总行数'), card(fmtBytes(o.totals.sizeBytes), '占用'),
    ]),
    barChart('行数排行', o.top.filter((t) => t.rowCount != null).slice(0, 12), (t) => t.rowCount, (v) => v.toLocaleString()),
    o.biggest.some((t) => t.sizeBytes) ? barChart('体积排行', o.biggest.filter((t) => t.sizeBytes).slice(0, 12), (t) => t.sizeBytes, fmtBytes) : null,
    !o.top.length ? el('p', { class: 'hint', text: '该范围内没有表。' }) : null,
  ]));
}

function barChart(title, items, value, format) {
  if (!items.length) return null;
  const max = Math.max(1, ...items.map(value));
  const rows = items.map((t) => el('tr', {}, [
    el('td', { style: 'width:200px;font-family:var(--mono)', text: t.name, title: t.comment || '' }),
    el('td', { style: 'width:52%' }, [el('span', { class: 'bar', style: `width:${Math.max(2, (value(t) / max) * 100).toFixed(1)}%` })]),
    el('td', { style: 'width:110px;text-align:right;color:var(--accent2)', text: format(value(t)) }),
    el('td', { style: 'width:70px;color:var(--fg2)', text: t.kind === 'view' ? '视图' : '' }),
  ]));
  return el('div', { style: 'margin-bottom:16px' }, [
    el('h3', { text: title }),
    el('table', { class: 'list' }, [el('tbody', {}, rows)]),
  ]);
}

/* ------------------------------------- ER ------------------------------------- */

function openMap(p, d, s) {
  const key = `map:${p.id}:${d.name}:${s.name}`;
  const view = state.tabs.find((x) => x.key === key)?.view || { data: null, loading: false, err: '', pos: {}, pan: { x: 0, y: 0 }, zoom: { k: 1 } };
  openTab({
    key, title: `关系图 · ${d.name || s.name || ''}`,
    crumbs: `${p.name || p.driver} / ${pathCrumb(p, d, s)} · ER 关系图`,
    view,
    run: () => loadMap(p, d, s, view),
    render: (pane) => renderMap(pane, p, d, s, view),
  });
  if (!view.data && !view.loading) loadMap(p, d, s, view);
}

async function loadMap(p, d, s, view) {
  view.loading = true;
  view.err = '';
  view.pos = {};
  view.pan = { x: 0, y: 0 };
  view.zoom = { k: 1 };
  try { view.data = await call('map_schema', { scope: scopeOf(p.id, d.name, s.name) }); } catch (e) { view.err = String(e); view.data = null; }
  finally { view.loading = false; if (state.tabs.find((x) => x.view === view)) renderPane(); }
}

const SVG_NS = 'http://www.w3.org/2000/svg';
const svg = (tag, attrs = {}, kids = []) => {
  const n = document.createElementNS(SVG_NS, tag);
  for (const [k, v] of Object.entries(attrs)) if (v !== null && v !== undefined) n.setAttribute(k, v);
  for (const c of kids.flat()) if (c) n.appendChild(typeof c === 'string' ? document.createTextNode(c) : c);
  return n;
};

function renderMap(pane, p, d, s, view) {
  if (view.loading) { pane.appendChild(el('div', { class: 'panel', text: '读取外键关系…' })); return; }
  if (view.err) { pane.appendChild(el('div', { class: 'err-line', text: view.err })); return; }
  const m = view.data;
  if (!m) { pane.appendChild(el('div', { class: 'panel', text: '准备加载…' })); return; }

  pane.style.display = 'flex';
  pane.style.flexDirection = 'column';
  pane.style.overflow = 'hidden';
  if (!m.nodes.length) { pane.appendChild(el('div', { class: 'panel hint', text: '该范围内没有表。' })); return; }
  const canvas = mapCanvas(p, d, s, m, view);
  pane.appendChild(el('div', { class: 'toolbar' }, [
    el('button', { class: 'btn', text: '重新读取', onclick: () => loadMap(p, d, s, view) }),
    el('span', { class: 'hint', text: `${m.nodes.length} 表 · ${m.edges.length} 关系${m.truncated ? ` · 仅前 80 张（共 ${m.totalTables}）` : ''}` }),
    el('span', { class: 'spacer' }),
    el('button', { class: 'btn', text: '−', title: '缩小（滚轮可缩放）', onclick: () => canvas.zoomBy(1 / 1.25) }),
    canvas.readout,
    el('button', { class: 'btn', text: '+', title: '放大（滚轮可缩放）', onclick: () => canvas.zoomBy(1.25) }),
    el('button', { class: 'btn', text: '适应窗口', onclick: () => canvas.fit() }),
  ]));
  pane.appendChild(canvas.box);
  if (!m.edges.length) pane.appendChild(el('div', { class: 'panel hint', text: '没有检测到外键关系 — 图只列出表结构。' }));
}

/* Zoom bounds keep the diagram readable; the wheel step is a fixed factor because
   e.deltaY varies wildly between a mouse notch and a trackpad. */
const MIN_K = 0.35, MAX_K = 3, WHEEL_STEP = 1.15;

function mapCanvas(p, d, s, m, view) {
  const W = 210, RH = 16, GAPX = 70, GAPY = 34;
  const layout = new Map();
  const cols = Math.max(1, Math.ceil(Math.sqrt(m.nodes.length)));
  const sizes = m.nodes.map((n) => ({ h: 26 + Math.min(n.columns.length, 8) * RH + (n.columns.length > 8 ? RH : 0) }));
  const colH = new Array(cols).fill(0);
  m.nodes.forEach((n, i) => {
    let best = 0;
    for (let k = 0; k < cols; k++) if (colH[k] < colH[best]) best = k;
    // The stored entry is the live one, so a drag writes straight through to it.
    const b = view.pos[n.name] || (view.pos[n.name] = { x: best * (W + GAPX) + 20, y: colH[best] + 20 });
    b.w = W;
    b.h = sizes[i].h;
    layout.set(n.name, b);
    colH[best] += sizes[i].h + GAPY;
  });

  const root = svg('svg', { width: '100%', height: '100%', style: 'display:block' });
  root.appendChild(svg('defs', {}, [svg('marker', { id: 'arr', viewBox: '0 0 10 10', refX: '9', refY: '5', markerWidth: '7', markerHeight: '7', orient: 'auto-start-reverse' }, [svg('path', { d: 'M 0 0 L 10 5 L 0 10 z', style: 'fill:var(--fg2)' })])]));
  const viewport = svg('g', {});
  const edgeLayer = svg('g', {});
  const nodeLayer = svg('g', {});
  viewport.appendChild(edgeLayer);
  viewport.appendChild(nodeLayer);
  root.appendChild(viewport);

  const edgeEls = m.edges.map((e) => {
    const path = svg('path', { 'marker-end': 'url(#arr)', style: 'fill:none;stroke:var(--fg2);stroke-width:1.2;opacity:.75' }, [
      svg('title', {}, [`${e.name}: ${e.columns.join(', ')} → ${e.to}(${e.refColumns.join(', ')})`]),
    ]);
    edgeLayer.appendChild(path);
    return { e, path };
  });
  const redrawEdges = () => {
    for (const { e, path } of edgeEls) {
      const a = layout.get(e.from), b = layout.get(e.to);
      if (!a || !b) continue;
      const leftFirst = a.x <= b.x;
      const x1 = leftFirst ? a.x + a.w : a.x, x2 = leftFirst ? b.x : b.x + b.w;
      const y1 = a.y + a.h / 2, y2 = b.y + b.h / 2;
      const mx = (x1 + x2) / 2;
      path.setAttribute('d', `M ${x1} ${y1} C ${mx} ${y1}, ${mx} ${y2}, ${x2} ${y2}`);
    }
  };
  redrawEdges();

  m.nodes.forEach((n) => {
    const b = layout.get(n.name);
    const g = svg('g', { transform: `translate(${b.x},${b.y})`, style: 'cursor:grab' });
    g.appendChild(svg('rect', { x: 0, y: 0, width: b.w, height: b.h, rx: 6, style: `fill:var(--bg3);stroke:${n.kind === 'view' ? 'var(--ok)' : 'var(--line)'}` }));
    g.appendChild(svg('rect', { x: 0, y: 0, width: b.w, height: 22, rx: 6, style: 'fill:var(--sel)' }));
    g.appendChild(svg('text', { x: 8, y: 15, 'font-size': '12', style: 'fill:var(--accent2);font-family:var(--mono),Consolas,monospace' }, [n.name]));
    if (n.rowCount != null) g.appendChild(svg('text', { x: b.w - 8, y: 15, 'font-size': '10', 'text-anchor': 'end', style: 'fill:var(--fg2)' }, [String(n.rowCount)]));
    n.columns.slice(0, 8).forEach((c, k) => {
      const y = 22 + (k + 1) * RH;
      g.appendChild(svg('text', { x: 8, y, 'font-size': '11', style: `fill:${c.isPk ? 'var(--accent)' : 'var(--fg)'};font-family:Consolas,monospace` }, [`${c.isPk ? '▸ ' : '   '}${c.name}`]));
      g.appendChild(svg('text', { x: b.w - 8, y, 'font-size': '10', 'text-anchor': 'end', style: 'fill:var(--fg2)' }, [c.type.slice(0, 14)]));
    });
    if (n.columns.length > 8) g.appendChild(svg('text', { x: 8, y: b.h - 5, 'font-size': '10', style: 'fill:var(--fg2)' }, [`… 还有 ${n.columns.length - 8} 列`]));
    g.addEventListener('click', () => { if (!g.__moved) openTableTab(p, d, s, { name: n.name, kind: n.kind }); });
    dragNode(g, b, redrawEdges, view);
    nodeLayer.appendChild(g);
  });

  const box = el('div', { class: 'grid-wrap', style: 'flex:1;min-height:0;padding:10px;overflow:hidden;cursor:grab;user-select:none;touch-action:none' }, [root]);
  const readout = el('span', { class: 'hint', style: 'min-width:46px;text-align:center' });
  const applyView = () => {
    viewport.setAttribute('transform', `translate(${view.pan.x},${view.pan.y}) scale(${view.zoom.k})`);
    readout.textContent = `${Math.round(view.zoom.k * 100)}%`;
  };
  applyView();

  /* `s` is the anchor in SVG user units; keeping it fixed while k changes is what
     makes the zoom feel attached to the pointer instead of the corner. */
  const zoomTo = (k2, s) => {
    const k = view.zoom.k;
    k2 = Math.min(MAX_K, Math.max(MIN_K, k2));
    if (k2 === k) return;
    view.pan.x = s.x - (s.x - view.pan.x) * (k2 / k);
    view.pan.y = s.y - (s.y - view.pan.y) * (k2 / k);
    view.zoom.k = k2;
    applyView();
  };
  const anchor = (e) => {
    const r = root.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  };
  root.addEventListener('wheel', (e) => {
    e.preventDefault();
    zoomTo(view.zoom.k * (e.deltaY < 0 ? WHEEL_STEP : 1 / WHEEL_STEP), anchor(e));
  }, { passive: false });

  const fit = () => {
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    for (const b of layout.values()) {
      minX = Math.min(minX, b.x); minY = Math.min(minY, b.y);
      maxX = Math.max(maxX, b.x + b.w); maxY = Math.max(maxY, b.y + b.h);
    }
    const r = root.getBoundingClientRect();
    const cw = maxX - minX, ch = maxY - minY;
    if (!isFinite(cw) || !isFinite(ch) || cw <= 0 || ch <= 0) return;
    const k = Math.max(MIN_K, Math.min(r.width / cw, r.height / ch, 1));
    view.zoom.k = k;
    view.pan.x = (r.width - k * cw) / 2 - k * minX;
    view.pan.y = (r.height - k * ch) / 2 - k * minY;
    applyView();
  };
  panOnDrag(box, view, applyView);
  return { box, readout, fit, zoomBy: (f) => { const r = root.getBoundingClientRect(); zoomTo(view.zoom.k * f, { x: r.width / 2, y: r.height / 2 }); } };
}

/* Drag a table card; the foreign-key lines follow it. A press that barely moves
   still counts as a click and opens the table. */
function dragNode(g, b, redrawEdges, view) {
  let start = null;
  g.addEventListener('pointerdown', (e) => {
    if (e.button !== 0) return;
    start = { x: e.clientX, y: e.clientY, bx: b.x, by: b.y };
    g.__moved = false;
    g.setPointerCapture(e.pointerId);
    g.style.cursor = 'grabbing';
    e.preventDefault();
  });
  g.addEventListener('pointermove', (e) => {
    if (!start) return;
    const k = view.zoom.k || 1;
    const dx = (e.clientX - start.x) / k, dy = (e.clientY - start.y) / k;
    if (!g.__moved && Math.abs(dx) + Math.abs(dy) < 3 / k) return;
    g.__moved = true;
    b.x = start.bx + dx;
    b.y = start.by + dy;
    g.setAttribute('transform', `translate(${b.x},${b.y})`);
    redrawEdges();
  });
  const stop = (e) => {
    if (!start) return;
    start = null;
    g.releasePointerCapture?.(e.pointerId);
    g.style.cursor = 'grab';
  };
  g.addEventListener('pointerup', stop);
  g.addEventListener('pointercancel', stop);
}

/* Drag empty canvas to pan the whole diagram — works even when it all fits on screen.
   Pan lives outside the scale, so its delta stays in raw screen pixels. */
function panOnDrag(surface, view, applyView) {
  let start = null;
  surface.addEventListener('pointerdown', (e) => {
    if (e.button !== 0 || e.target.closest('g')) return;
    start = { x: e.clientX, y: e.clientY, px: view.pan.x, py: view.pan.y };
    surface.setPointerCapture(e.pointerId);
    surface.style.cursor = 'grabbing';
  });
  surface.addEventListener('pointermove', (e) => {
    if (!start) return;
    view.pan.x = start.px + (e.clientX - start.x);
    view.pan.y = start.py + (e.clientY - start.y);
    applyView();
  });
  const stop = () => { if (start) { start = null; surface.style.cursor = 'grab'; } };
  surface.addEventListener('pointerup', stop);
  surface.addEventListener('pointercancel', stop);
}
