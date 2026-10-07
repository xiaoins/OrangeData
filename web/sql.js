/* SQL worksheet: multi-statement runner with per-statement result tabs and history. */

function openSqlWith(sql) {
  const sel = state.selection || {};
  const connId = sel.connectionId || (state.open.size ? [...state.open][0] : '');
  if (!connId) { toast('请先在左侧选择一个已打开的连接', true); return; }
  const key = `sql:${connId}`;
  const prev = state.tabs.find((x) => x.key === key);
  const view = prev?.view || {
    connId, database: sel.database || '', schema: sel.schema || '',
    sql: '', limit: 200, res: null, shown: 0, loading: false,
    dbs: [], schemas: [], showHistory: false, history: [], err: '',
  };
  view.connId = connId;
  if (sel.database) view.database = view.database || sel.database;
  if (sel.schema) view.schema = view.schema || sel.schema;
  if (sql) view.sql = sql;
  const p = profileOf(connId);
  openTab({
    key,
    title: `SQL · ${p.name || connId}`,
    crumbs: `${DRIVER_LABEL[p.driver] || p.driver} / ${view.database || '默认库'}${view.schema ? ' / ' + view.schema : ''}`,
    view,
    run: () => runScript(p, view),
    render: (pane) => renderSql(pane, p, view),
  });
  if (!view.dbs.length) loadDatabases(p, view);
  if (!view.history.length) call('history_list').then((h) => { view.history = h; if (state.active === key) renderPane(); }).catch(() => {});
}

async function loadDatabases(p, view) {
  try {
    view.dbs = await call('meta_databases', { id: p.id });
    if (p.driver === 'postgres') view.schemas = await call('meta_schemas', { scope: scopeOf(p.id, view.database, '') });
  } catch (_) { view.dbs = []; }
  if (state.tabs.find((x) => x.view === view)) renderPane();
}

async function runScript(p, view) {
  const sql = (view.sql || '').trim();
  if (!sql) { toast('请输入 SQL', true); return; }
  view.loading = true;
  view.err = '';
  try {
    const req = { connectionId: p.id, database: view.database, schema: view.schema, sql, limit: view.limit };
    view.res = await call('script_run', { req });
    view.shown = Math.max(0, view.res.statements.length - 1);
    view.history = await call('history_list');
  } catch (e) {
    view.err = String(e);
    view.res = null;
  } finally {
    view.loading = false;
    if (state.tabs.find((x) => x.view === view)) renderPane();
  }
}

/* ---------------------------------- render ---------------------------------- */

function renderSql(pane, p, view) {
  const ta = el('textarea', {
    spellcheck: 'false', placeholder: 'SELECT * FROM xxx WHERE id = 1;\nUPDATE xxx SET a = 1 WHERE id = 2;',
    oninput: (e) => { view.sql = e.target.value; },
    onkeydown: (e) => { if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); runScript(p, view); } },
  });
  ta.value = view.sql || '';

  const connSel = el('select', { onchange: (e) => { state.selection = { kind: 'profile', connectionId: e.target.value }; openSqlWith(''); } },
    [...state.open].map((id) => el('option', { value: id, text: profileOf(id).name || id, selected: id === p.id ? '' : null })));
  const dbSel = el('select', {
    onchange: (e) => { view.database = e.target.value; view.schemas = []; if (p.driver === 'postgres') call('meta_schemas', { scope: scopeOf(p.id, view.database, '') }).then((s) => { view.schemas = s; renderPane(); }); renderPane(); },
  }, [el('option', { value: '', text: '（默认）' })].concat(view.dbs.map((d) => el('option', { value: d.name, text: d.name, selected: d.name === view.database ? '' : null }))));
  const schSel = el('select', { onchange: (e) => { view.schema = e.target.value; renderPane(); } },
    [el('option', { value: '', text: '（默认）' })].concat((view.schemas || []).map((s) => el('option', { value: s.name, text: s.name, selected: s.name === view.schema ? '' : null }))));
  const limSel = el('select', { onchange: (e) => { view.limit = Number(e.target.value); } },
    [50, 200, 500, 1000, 2000].map((n) => el('option', { value: n, text: `最多 ${n} 行`, selected: n === view.limit ? '' : null })));

  pane.appendChild(el('div', { class: 'runner' }, [
    el('div', { class: 'toolbar' }, [
      el('button', { class: 'btn primary', text: view.loading ? '执行中…' : '执行 (Ctrl+Enter)', disabled: view.loading, onclick: () => runScript(p, view) }),
      el('button', { class: 'btn ghost', text: '格式化', onclick: () => { view.sql = tidySql(view.sql); renderPane(); } }),
      el('button', { class: 'btn ghost', text: '清空', onclick: () => { view.sql = ''; view.res = null; renderPane(); } }),
      el('span', { class: 'hint', text: '连接' }), connSel,
      el('span', { class: 'hint', text: '库' }), dbSel,
      p.driver === 'postgres' ? el('span', { class: 'hint', text: '模式' }) : null,
      p.driver === 'postgres' ? schSel : null,
      el('span', { class: 'hint', text: '行数' }), limSel,
      el('span', { class: 'spacer' }),
      el('button', { class: 'btn ghost', text: view.showHistory ? '隐藏历史' : '历史', onclick: () => { view.showHistory = !view.showHistory; renderPane(); } }),
    ]),
    ta,
    view.err ? el('div', { class: 'err-line', text: view.err }) : null,
    el('div', { class: 'out' }, view.showHistory && !view.res ? historyBox(p, view) : resultBox(p, view)),
  ]));
}

function historyBox(p, view) {
  if (!view.history.length) return el('div', { class: 'panel hint', text: '还没有执行记录。' });
  const rows = view.history.slice(0, 40).map((h) => el('tr', {
    style: 'cursor:pointer', title: '点击回填',
    onclick: () => { view.sql = h.sql; view.showHistory = false; renderPane(); },
  }, [
    el('td', { style: 'width:70px', text: h.driver }),
    el('td', { style: 'width:120px;color:var(--fg2)', text: h.database || '—' }),
    el('td', { style: 'font-family:var(--mono);overflow:hidden;text-overflow:ellipsis;max-width:400px', text: h.sql.replace(/\s+/g, ' ').slice(0, 160) }),
    el('td', { style: 'width:52px;color:' + (h.ok ? 'var(--ok)' : 'var(--err)'), text: h.ok ? '成功' : '失败' }),
    el('td', { style: 'width:130px;color:var(--fg2)', text: (h.at || '').slice(0, 19).replace('T', ' ') }),
  ]));
  return el('div', {}, [
    el('div', { class: 'stmt-tabs' }, [
      el('span', { class: 'hint', style: 'align-self:center', text: `最近 ${view.history.length} 条` }),
      el('button', { class: 'mini', text: '清空历史', onclick: async () => { await call('history_clear'); view.history = []; renderPane(); } }),
    ]),
    el('table', { class: 'list' }, [el('tbody', {}, rows)]),
  ]);
}

function resultBox(p, view) {
  if (view.loading) return el('div', { class: 'panel hint', text: '执行中…' });
  if (!view.res) return el('div', { class: 'panel hint', text: '尚未执行。Ctrl+Enter 运行，多语句用分号分隔，遇错即停。' });
  const sts = view.res.statements;
  const cur = sts[view.shown];
  return el('div', {}, [
    el('div', { class: 'stmt-tabs' }, sts.map((st, i) => el('button', {
      class: 'mini' + (i === view.shown ? ' on' : ''),
      style: (i === view.shown ? 'border-color:var(--accent);color:var(--accent2);' : '') + (st.error ? 'color:var(--err);' : ''),
      text: `#${st.index + 1} ${st.isQuery ? `${st.rows.length} 行` : `${st.affected ?? 0} 影响`} · ${st.ms} ms${st.error ? ' ✕' : ''}`,
      title: st.sql,
      onclick: () => { view.shown = i; renderPane(); },
    }))),
    cur.error ? el('div', { class: 'err-line', html: `<b>${escapeHtml(cur.error)}</b><pre class="sql">${escapeHtml(cur.sql)}</pre>` }) : null,
    cur.isQuery ? resultGrid(cur) : el('div', { class: 'panel' }, [
      el('p', { style: 'margin:0 0 8px', html: `<b style="color:var(--ok)">成功</b> · 影响 ${cur.affected ?? 0} 行 · ${cur.ms} ms` }),
      el('pre', { class: 'sql', text: cur.sql }),
    ]),
  ]);
}

function resultGrid(st) {
  if (!st.columns.length) return el('div', { class: 'panel hint', text: '无结果集列。' });
  const head = el('tr', {}, [el('th', { text: '#', style: 'width:36px' })].concat(st.columns.map((c) => el('th', { text: c }))));
  const cells = (r) => r.map((v) => el('td', {
    class: v === null ? 'null' : (typeof v === 'object' && v?.__bin ? 'bin' : ''),
    text: v === null ? 'NULL' : cellText(v) ?? '',
    title: v === null ? '' : String(cellText(v) ?? ''),
  }));
  const trs = st.rows.map((r, i) => el('tr', {}, [el('td', { style: 'color:var(--fg2)', text: String(i + 1) })].concat(cells(r))));
  return el('div', { class: 'grid-wrap' }, [
    el('table', { class: 'grid' }, [el('thead', {}, [head]), el('tbody', {}, trs)]),
    st.truncated ? el('div', { class: 'panel hint', text: `结果已截断，仅显示前 ${st.rows.length} 行 — 提高“行数”或加 LIMIT。` }) : null,
  ]);
}

function tidySql(s) {
  const KW = ['select', 'from', 'where', 'group by', 'order by', 'limit', 'having', 'join', 'left join', 'right join', 'inner join', 'on', 'set', 'values', 'insert into', 'update', 'delete from', 'union all', 'union'];
  let out = String(s || '');
  for (const k of KW) out = out.replace(new RegExp(`\\b${k.replace(/ /g, '\\s+')}\\b`, 'gi'), `\n${k.toUpperCase()}`);
  return out.replace(/^\n/, '').replace(/\n{3,}/g, '\n\n').replace(/,\n/g, ',\n  ').trim();
}
