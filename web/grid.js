/* Data grid: paging, sorting, visual filters and primary-key based inline editing. */

function openTableTab(p, d, s, t) {
  const key = `grid:${p.id}:${d.name}:${s.name}:${t.name}`;
  const view = state.tabs.find((x) => x.key === key)?.view || {
    page: 1, size: 50, sort: [], filters: [], showFilter: false,
    rows: null, columns: [], meta: [], pks: [], total: null, ms: 0, sql: '', editable: false,
    edits: new Map(), inserts: [], deletes: new Set(), loading: false,
  };
  openTab({
    key,
    title: t.name,
    crumbs: [p.name || p.driver, pathCrumb(p, d, s), t.name].filter(Boolean).join(' / '),
    view,
    save: () => saveEdits(p, d, s, t, view),
    run: () => loadGrid(p, d, s, t, view),
    render: (pane) => renderGrid(pane, p, d, s, t, view),
  });
  if (!view.rows && !view.loading) loadGrid(p, d, s, t, view);
}

async function loadGrid(p, d, s, t, view) {
  view.loading = true;
  try {
    const req = {
      connectionId: p.id, database: d.name, schema: s.name, table: t.name,
      page: view.page, size: view.size, sort: view.sort, filters: view.filters.filter((f) => f.column && (f.value !== '' || ['null', 'notnull'].includes(f.op))),
    };
    const g = await call('grid_page', { req });
    view.rows = g.rows;
    view.columns = g.columns;
    view.meta = g.columnMeta;
    view.pks = g.primaryKeys;
    view.editable = g.editable;
    view.total = g.total;
    view.ms = g.ms;
    view.sql = g.sql;
    view.edits = new Map();
    view.inserts = [];
    view.deletes = new Set();
    view.filterDefs = req.filters;
  } finally {
    view.loading = false;
    if (state.tabs.find((x) => x.view === view)) renderPane();
  }
}

/* ---------------------------------- render ---------------------------------- */

function renderGrid(pane, p, d, s, t, view) {
  if (view.loading) { pane.appendChild(el('div', { class: 'panel', text: '加载中…' })); return; }
  if (!view.rows) { pane.appendChild(el('div', { class: 'panel', text: '准备加载…' })); return; }

  const dirty = view.edits.size || view.inserts.length || view.deletes.size;
  pane.appendChild(el('div', { class: 'toolbar' }, [
    el('button', {
      class: 'btn', text: '刷新', onclick: () => {
        const dirty = view.edits.size || view.inserts.length || view.deletes.size;
        if (dirty) confirmDialog('有未保存的修改，刷新会丢弃它们。', () => discardAnd(view, () => loadGrid(p, d, s, t, view)), true);
        else loadGrid(p, d, s, t, view);
      },
    }),
    el('button', { class: 'btn', text: view.editable ? '新增行' : '只读', disabled: !view.editable, onclick: () => { view.inserts.push({}); rerender(view); } }),
    el('button', { class: 'btn', text: '保存', onclick: () => saveEdits(p, d, s, t, view) }),
    el('button', { class: 'btn ghost', text: '放弃', disabled: !dirty, onclick: () => discardAnd(view, () => rerender(view)) }),
    el('button', { class: 'btn ghost', text: view.showFilter ? '隐藏筛选' : '筛选', onclick: () => { view.showFilter = !view.showFilter; rerender(view); } }),
    el('button', { class: 'btn ghost', text: '导出 CSV', onclick: () => exportTable(p, d, s, t, view) }),
    el('button', { class: 'btn ghost', text: '结构', onclick: () => openStructure(p, d, s, t) }),
    el('span', { class: 'spacer' }),
    el('span', { class: 'hint', text: `共 ${view.total == null ? '?' : view.total} 行 · ${view.ms} ms` }),
    el('select', {
      onchange: (e) => { view.size = Number(e.target.value); view.page = 1; loadGrid(p, d, s, t, view); },
    }, [10, 25, 50, 100, 200].map((n) => el('option', { value: n, text: `${n} 行`, selected: n === view.size ? '' : null }))),
    el('button', { class: 'btn mini', text: '‹ 上一页', onclick: () => { if (view.page > 1) { view.page--; loadGrid(p, d, s, t, view); } } }),
    el('span', { class: 'hint', text: `第 ${view.page} 页` }),
    el('button', { class: 'btn mini', text: '下一页 ›', onclick: () => { view.page++; loadGrid(p, d, s, t, view); } }),
  ]));

  const wrap = el('div', { class: 'grid-wrap' });
  const table = el('table', { class: 'grid' });
  const thead = el('thead');
  const hrow = el('tr', {}, [el('th', { text: '#', style: 'width:36px' })]);
  view.columns.forEach((name, i) => {
    const m = view.meta[i];
    const isPk = m && m.isPk;
    const sortIdx = view.sort.findIndex((x) => x.column === name);
    const arrow = sortIdx < 0 ? '' : view.sort[sortIdx].dir === 'desc' ? ' ↓' : ' ↑';
    hrow.appendChild(el('th', { class: isPk ? 'pk' : '', text: name + arrow, title: `${m ? m.type : ''}${m && m.nullable ? ' · 可空' : ' · 非空'}${isPk ? ' · 主键' : ''}`, onclick: () => { toggleSort(view, name); loadGrid(p, d, s, t, view); } }));
  });
  thead.appendChild(hrow);
  if (view.showFilter) thead.appendChild(filterRow(view, p, d, s, t));
  table.appendChild(thead);

  const tbody = el('tbody');
  view.rows.forEach((row, ri) => tbody.appendChild(dataRow(view, p, d, s, t, row, ri)));
  view.inserts.forEach((ins, i) => tbody.appendChild(insertRow(view, i, ins)));
  if (!view.rows.length && !view.inserts.length) {
    tbody.appendChild(el('tr', {}, [el('td', { colspan: String(view.columns.length + 1), style: 'color:var(--fg2);padding:14px', text: '没有数据（0 行）— 可以点“新增行”或用 SQL 标签插入' })]));
  }
  table.appendChild(tbody);
  wrap.appendChild(table);
  pane.appendChild(wrap);
  pane.appendChild(el('div', { class: 'panel', style: 'padding:6px 10px;border-top:1px solid var(--line)', html: `<span class="hint">生成 SQL：</span><code style="font-family:var(--mono);color:var(--fg2)">${escapeHtml(view.sql)}</code>` }));
}

function rerender(view) {
  if (state.tabs.find((x) => x.view === view)) renderPane();
}

function filterRow(view, p, d, s, t) {
  const cur = view.filters || [];
  const at = (name) => cur.find((f) => f.column === name);
  const row = el('tr', {}, [el('th', { text: '⌕' })]);
  view.columns.forEach((name, i) => {
    const f = at(name) || { column: name, op: (view.meta[i] && /char|text|clob|json/i.test(view.meta[i].type)) ? 'contains' : '=', value: '' };
    const sel = el('select', { style: 'width:62px' }, ['=', '<>', '>', '>=', '<', '<=', 'contains', 'starts', 'ends', 'null', 'notnull'].map((op) => el('option', { value: op, text: op, selected: f.op === op ? '' : null })));
    const inp = el('input', { value: f.value, placeholder: '值', style: 'width:100%' });
    sel.onchange = () => { f.op = sel.value; inp.disabled = ['null', 'notnull'].includes(sel.value); };
    inp.onkeydown = (e) => { if (e.key === 'Enter') apply(); };
    function apply() {
      f.op = sel.value;
      f.value = inp.value.trim();
      const next = cur.filter((x) => x.column !== name);
      if (f.value !== '' || ['null', 'notnull'].includes(f.op)) next.push({ column: name, op: f.op, value: f.value });
      view.filters = next;
      view.page = 1;
      loadGrid(p, d, s, t, view);
    }
    row.appendChild(el('th', {}, [
      el('div', { style: 'display:flex;gap:3px' }, [sel, inp, el('button', { class: 'mini', text: 'ok', onclick: apply })]),
    ]));
  });
  return row;
}

function toggleSort(view, name) {
  const i = view.sort.findIndex((x) => x.column === name);
  if (i < 0) view.sort = [{ column: name, dir: 'asc' }];
  else if (view.sort[i].dir === 'asc') view.sort = [{ column: name, dir: 'desc' }];
  else view.sort = [];
}

function cellText(v) {
  if (v === null || v === undefined) return null;
  if (typeof v === 'object') {
    if (v.__bin) return `⬖ ${v.bytes} 字节 ${v.hex ? '(' + v.hex.slice(0, 24) + '…)' : ''}`;
    return JSON.stringify(v);
  }
  return String(v);
}

function dataRow(view, p, d, s, t, row, ri) {
  const edit = view.edits.get(ri) || {};
  const tr = el('tr', { class: view.deletes.has(ri) ? 'dirty' : Object.keys(edit).length ? 'dirty' : '' }, [
    el('td', { style: 'color:var(--fg2)' }, [
      view.editable ? el('span', {
        class: 'mini', style: 'cursor:pointer;color:' + (view.deletes.has(ri) ? 'var(--err)' : 'var(--fg2)'),
        text: view.deletes.has(ri) ? '↺' : '✕', title: view.deletes.has(ri) ? '取消删除' : '标记删除',
        onclick: () => { if (view.deletes.has(ri)) view.deletes.delete(ri); else view.deletes.add(ri); rerender(view); },
      }) : null,
    ]),
  ]);
  view.columns.forEach((name, ci) => {
    const raw = row[ci];
    const changed = Object.prototype.hasOwnProperty.call(edit, name);
    const shown = changed ? edit[name] : raw;
    const td = el('td', {
      class: (shown === null ? 'null ' : '') + (changed ? 'changed' : ''),
      text: shown === null ? 'NULL' : cellText(shown) ?? '∅',
      title: cellText(shown) || '',
    });
    if (view.editable && !view.deletes.has(ri)) td.addEventListener('dblclick', () => editCell(td, view, ri, name, shown, p, d, s, t));
    tr.appendChild(td);
  });
  return tr;
}

function editCell(td, view, ri, name, current, p, d, s, t) {
  if (td.querySelector('input')) return;
  const isNull = current === null;
  const input = el('input', { value: isNull ? '' : String(current), style: 'width:150px' });
  const commit = (val) => {
    const m = view.edits.get(ri) || {};
    m[name] = val;
    view.edits.set(ri, m);
    rerender(view);
  };
  const box = el('div', { style: 'display:flex;gap:2px' }, [
    input,
    el('button', { class: 'mini', text: '∅', title: '设为 NULL', onclick: () => commit(null) }),
  ]);
  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') commit(input.value);
    else if (e.key === 'Escape') rerender(view);
  });
  input.addEventListener('blur', () => { if (input.value !== (isNull ? '' : String(current))) commit(input.value); });
  td.classList.remove('null');
  td.textContent = '';
  td.classList.add('editing');
  td.appendChild(box);
  input.focus();
  input.select();
}

function insertRow(view, i, ins) {
  const tr = el('tr', { class: 'dirty' }, [el('td', { style: 'color:var(--fg2)' }, [el('span', { class: 'mini', style: 'cursor:pointer;color:var(--err)', text: '✕', title: '移除', onclick: () => { view.inserts.splice(i, 1); rerender(view); } })])]);
  view.meta.forEach((m) => {
    const has = Object.prototype.hasOwnProperty.call(ins, m.name);
    const td = el('td', { class: 'editing' });
    const inp = el('input', {
      value: has ? (ins[m.name] === null ? '' : String(ins[m.name])) : '',
      placeholder: has ? (ins[m.name] === null ? 'NULL' : '') : (m.isPk ? '自动生成/可空' : '点击填写'),
      style: 'width:100%',
    });
    inp.addEventListener('input', () => { ins[m.name] = inp.value; });
    // blur must not rebuild the pane: it fires while the pointer is already down on the
    // next cell, so a rerender would destroy that input and swallow the user's click.
    inp.addEventListener('blur', () => {
      if (inp.value === '') { delete ins[m.name]; inp.placeholder = m.isPk ? '自动生成/可空' : '点击填写'; }
    });
    td.appendChild(inp);
    tr.appendChild(td);
  });
  return tr;
}

/* ----------------------------------- save ----------------------------------- */

function discardAnd(view, then) {
  view.edits = new Map();
  view.inserts = [];
  view.deletes = new Set();
  then();
}

async function saveEdits(p, d, s, t, view) {
  if (!view.editable) { status('该表没有主键，请在 SQL 标签中修改', 'err'); return; }
  const pkIdx = view.pks.map((k) => view.columns.indexOf(k)).filter((i) => i >= 0);
  if (!pkIdx.length) { status('无法定位主键列', 'err'); return; }

  const updates = [];
  for (const [ri, set] of view.edits.entries()) {
    const row = view.rows[ri];
    const pk = {};
    pkIdx.forEach((i) => { pk[view.columns[i]] = row[i]; });
    const clean = {};
    for (const [k, v] of Object.entries(set)) clean[k] = v === undefined ? null : v;
    updates.push({ pk, set: clean });
  }
  const inserts = view.inserts
    .map((ins) => {
      const values = {};
      for (const [k, v] of Object.entries(ins)) if (v !== undefined && v !== '') values[k] = v === 'NULL' ? null : v;
      return { values };
    })
    .filter((x) => Object.keys(x.values).length);
  const deletes = [...view.deletes].map((ri) => {
    const row = view.rows[ri];
    const values = {};
    pkIdx.forEach((i) => { values[view.columns[i]] = row[i]; });
    return { values };
  });

  if (!updates.length && !inserts.length && !deletes.length) { toast('没有需要保存的修改'); return; }
  const req = { connectionId: p.id, database: d.name, schema: s.name, table: t.name, updates, inserts, deletes };
  const res = await call('edits_apply', { req });
  const failed = res.items.filter((x) => !x.ok);
  if (failed.length) status(`保存：成功 ${res.applied} 条，失败 ${failed.length} 条 — ${failed[0].error}`, 'err');
  else status(`已保存 ${res.applied} 条修改`, 'ok');
  await loadGrid(p, d, s, t, view);
}

async function exportTable(p, d, s, t, view) {
  const path = await save({ defaultPath: `${t.name}.csv`, filters: [{ name: 'CSV', extensions: ['csv'] }] });
  if (!path) return;
  const req = {
    connectionId: p.id, database: d.name, schema: s.name, table: t.name, page: 1, size: 500,
    sort: view?.sort || [], filters: view?.filters || [],
  };
  const r = await call('export_csv', { req, path });
  toast(`已导出 ${r.rows} 行 → ${r.path}`);
}

function escapeHtml(s) {
  return String(s || '').replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c]));
}
