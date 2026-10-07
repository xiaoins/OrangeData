/* OrangeData — vanilla JS front end for the Tauri shell. */
const { invoke } = window.__TAURI__.core;
const { open, save } = window.__TAURI__.dialog;

const $ = (sel, root = document) => root.querySelector(sel);
const el = (tag, attrs = {}, kids = []) => {
  const n = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === 'class') n.className = v;
    else if (k === 'html') n.innerHTML = v;
    else if (k === 'text') n.textContent = v;
    else if (k.startsWith('on') && typeof v === 'function') n.addEventListener(k.slice(2), v);
    else if (typeof v === 'boolean') { if (v) n.setAttribute(k, ''); }
    else if (v !== null && v !== undefined) n.setAttribute(k, v);
  }
  for (const kid of (Array.isArray(kids) ? kids : [kids]).flat()) if (kid) n.appendChild(typeof kid === 'string' ? document.createTextNode(kid) : kid);
  return n;
};

const state = {
  profiles: [],
  open: new Set(),
  about: {},
  tree: {},           // connectionId -> expanded state / cached children
  tabs: [],
  active: null,
  selection: null,    // { kind, connectionId, database, schema, table }
  drivers: { sqlite: { label: 'SQLite', port: 0 }, mysql: { label: 'MySQL', port: 3306 }, postgres: { label: 'PostgreSQL', port: 5432 } },
};

/* ---------------------------------- shell ---------------------------------- */

function toast(msg, isError) {
  const t = el('div', { class: 'toast' + (isError ? ' err' : ''), text: msg });
  document.body.appendChild(t);
  setTimeout(() => t.remove(), isError ? 5200 : 2600);
}
function status(msg, cls) {
  const s = $('#status-text');
  s.textContent = msg;
  s.className = cls || '';
  if (cls === 'err') toast(msg, true);
}
async function call(cmd, args) {
  try {
    return await invoke(cmd, args || {});
  } catch (e) {
    status(String(e), 'err');
    throw e;
  }
}

/* SQLite reports its database and its schema both as "main" — show the name once. */
function pathCrumb(p, d, s) {
  const db = d.name || p.file || '';
  return [(d.name && d.name === s.name ? '' : db), s.name || ''].filter(Boolean).join(' / ');
}

function setCrumbs(text) {
  $('#crumbs').textContent = text;
  invoke('window_title', { text });
}

/* ---------------------------------- theme ---------------------------------- */

const colorRef = (hex) => {
  const h = String(hex).trim().replace('#', '');
  const n = h.length === 3 ? h.split('').map((c) => c + c).join('') : h;
  const v = parseInt(n.slice(0, 6), 16);
  return ((v & 0xff) << 16) | (v & 0xff00) | ((v >> 16) & 0xff);
};

async function applyTheme(next) {
  document.documentElement.dataset.theme = next;
  localStorage.setItem('od.theme', next);
  $('#btn-theme').textContent = next === 'light' ? '☾' : '☀';
  const cs = getComputedStyle(document.documentElement);
  await invoke('chrome_apply', {
    dark: next === 'dark',
    caption: colorRef(cs.getPropertyValue('--bg2')),
    text: colorRef(cs.getPropertyValue('--fg')),
    border: colorRef(cs.getPropertyValue('--line')),
  });
}

$('#btn-theme').addEventListener('click', () => {
  applyTheme(document.documentElement.dataset.theme === 'light' ? 'dark' : 'light');
});
applyTheme(localStorage.getItem('od.theme') === 'light' ? 'light' : 'dark');

window.addEventListener('error', (e) => {
  const file = String(e.filename || '').split('/').pop();
  status(`界面错误：${e.message}（${file}:${e.lineno}）`, 'err');
});
window.addEventListener('unhandledrejection', (e) => status(`界面错误：${String(e.reason?.message || e.reason || e)}`, 'err'));

/* --------------------------------- profiles --------------------------------- */

async function loadProfiles() {
  state.profiles = await call('profile_list');
  try { state.open = new Set(await call('conn_open')); } catch (_) { state.open = new Set(); }
  for (const p of state.profiles) {
    if (state.open.has(p.id)) {
      try { state.about[p.id] = await call('conn_connect', { cfg: p }); } catch (_) { state.open.delete(p.id); }
    }
  }
  renderTree();
}

function scopeOf(connId, database, schema) {
  return { connectionId: connId, database: database || '', schema: schema || '' };
}

/* ----------------------------------- tree ----------------------------------- */

async function renderTree() {
  const root = $('#tree');
  root.innerHTML = '';
  if (!state.profiles.length) {
    root.appendChild(el('div', { class: 'hint', style: 'padding:10px', text: '还没有连接，点击“新建连接”。' }));
    return;
  }
  for (const p of state.profiles) root.appendChild(await profileNode(p));
  $('#side-hint').textContent = `${state.profiles.length} 个连接 · ${state.open.size} 已打开`;
}

/* Dead space in the sidebar still must not fall through to the webview's own menu. */
$('#tree').addEventListener('contextmenu', (e) => {
  if (e.target.closest('.row')) return;
  e.preventDefault();
  menu(e, [
    { label: '新建连接', run: () => connDialog() },
    { label: '刷新连接列表', run: () => loadProfiles() },
  ]);
});

async function profileNode(p) {
  const node = el('div', { class: 'node' });
  const isOpen = state.open.has(p.id);
  const kids = el('div', { class: 'kids' });
  const row = el('div', { class: 'row' }, [
    el('span', { class: 'twist', text: isOpen ? '▾' : '▸', onclick: (e) => { e.stopPropagation(); toggleProfile(p, node, kids); } }),
    el('span', { class: 'ico', text: isOpen ? '●' : '○', style: `color:${isOpen ? 'var(--ok)' : 'var(--fg2)'}` }),
    el('span', { class: 'lbl', text: p.name || p.driver, title: p.file || `${p.host}:${p.port}` }),
    el('span', { class: 'badge', text: state.drivers[p.driver]?.label || p.driver }),
  ]);
  const select = () => {
    state.selection = { kind: 'profile', connectionId: p.id };
    highlight(row);
    setCrumbs(`${p.name || p.driver} · ${state.open.has(p.id) ? '已连接' : '未连接'}`);
  };
  row.addEventListener('dblclick', () => toggleProfile(p, node, kids));
  row.addEventListener('click', select);
  row.addEventListener('contextmenu', (e) => { e.preventDefault(); select(); profileMenu(p, e, node, kids); });
  node.appendChild(row);
  node.appendChild(kids);
  if (isOpen && p._autoExpand) toggleProfile(p, node, kids);
  return node;
}

function highlight(row) {
  document.querySelectorAll('.row.sel').forEach((n) => n.classList.remove('sel'));
  row.classList.add('sel');
}

function toggleProfile(p, node, kids) {
  const opened = node.dataset.open === '1';
  if (opened) { node.dataset.open = '0'; node.classList.add('closed'); node.querySelector('.twist').textContent = '▸'; return; }
  node.dataset.open = '1';
  node.classList.remove('closed');
  node.querySelector('.twist').textContent = '▾';
  fillProfileKids(p, node, kids);
}

function markConnected(node, p) {
  const ico = node.querySelector('.ico');
  if (ico) { ico.textContent = '●'; ico.style.color = 'var(--ok)'; }
  if (state.selection && state.selection.connectionId === p.id) setCrumbs(`${p.name || p.driver} · 已连接`);
}

async function fillProfileKids(p, node, box) {
  box.innerHTML = '';
  box.appendChild(el('div', { class: 'hint', style: 'padding:2px 10px', text: '加载中…' }));
  if (!state.open.has(p.id)) {
    try { await call('conn_connect', { cfg: p }); state.open.add(p.id); markConnected(node, p); } catch (_) { box.innerHTML = ''; return; }
  }
  box.innerHTML = '';
  const dbs = await call('meta_databases', { id: p.id });
  if (!dbs.length) {
    box.appendChild(el('div', { class: 'hint', style: 'padding:2px 10px', text: '无可访问的数据库' }));
    return;
  }
  for (const d of dbs) box.appendChild(dbNode(p, d));
}

/* SQLite and MySQL hang tables straight off the database, so the schema-scoped
   views belong on its row; PostgreSQL lists one row per schema and offers them there. */
function implicitSchema(p) {
  return p.driver === 'sqlite' ? { name: 'main' } : p.driver === 'mysql' ? { name: '' } : null;
}

function dbNode(p, d) {
  const node = el('div', { class: 'node closed' });
  const kids = el('div', { class: 'kids' });
  const open = () => toggleNode(node, row, () => fillDbKids(p, d, kids));
  const row = el('div', { class: 'row' }, [
    el('span', { class: 'twist', text: '▸' }),
    el('span', { class: 'ico', text: '▤' }),
    el('span', { class: 'lbl', text: d.name }),
    d.comment ? el('span', { class: 'badge', text: d.comment, title: d.comment }) : null,
  ]);
  row.addEventListener('click', open);
  row.addEventListener('contextmenu', (e) => {
    e.preventDefault();
    state.selection = { kind: 'database', connectionId: p.id, database: d.name };
    highlight(row);
    setCrumbs(`${p.name || p.driver} / ${d.name}`);
    const s = implicitSchema(p);
    const items = [
      { label: '刷新', run: () => { if (node.dataset.open === '1') fillDbKids(p, d, kids); else open(); } },
      { label: '新建 SQL 查询', run: () => openSqlWith('') },
    ];
    if (s) items.push({ label: '数据总览', run: () => openDashboard(p, d, s) }, { label: 'ER 关系图', run: () => openMap(p, d, s) }, { label: '新建表', run: () => newTableDialog(p, d, s) });
    menu(e, items);
  });
  node.appendChild(row);
  node.appendChild(kids);
  return node;
}

async function fillDbKids(p, d, box) {
  box.innerHTML = '';
  const drv = p.driver;
  if (drv === 'postgres') {
    const schemas = await call('meta_schemas', { scope: scopeOf(p.id, d.name, '') });
    for (const s of schemas) box.appendChild(schemaNode(p, d, s));
    return;
  }
  if (drv === 'sqlite') {
    const list = await call('meta_schemas', { scope: scopeOf(p.id, '', '') });
    const targets = list.length ? list : [{ name: 'main' }];
    for (const s of targets) box.appendChild(tableListNode(p, { name: d.name }, s));
    return;
  }
  box.appendChild(tableListNode(p, d, { name: '' }));
}

function schemaNode(p, d, s) {
  const node = el('div', { class: 'node closed' });
  const kids = el('div', { class: 'kids' });
  const fill = () => { kids.innerHTML = ''; kids.appendChild(tableListNode(p, d, s)); };
  const open = () => toggleNode(node, row, fill);
  const row = el('div', { class: 'row' }, [
    el('span', { class: 'twist', text: '▸' }),
    el('span', { class: 'ico', text: '◈' }),
    el('span', { class: 'lbl', text: s.name }),
  ]);
  row.addEventListener('click', open);
  row.addEventListener('contextmenu', (e) => {
    e.preventDefault();
    state.selection = { kind: 'schema', connectionId: p.id, database: d.name, schema: s.name };
    highlight(row);
    setCrumbs(`${p.name || p.driver} / ${pathCrumb(p, d, s)}`);
    menu(e, [
      { label: '刷新', run: () => { if (node.dataset.open === '1') fill(); else open(); } },
      { label: '新建 SQL 查询', run: () => openSqlWith('') },
      { label: '数据总览', run: () => openDashboard(p, d, s) },
      { label: 'ER 关系图', run: () => openMap(p, d, s) },
      { label: '新建表', run: () => newTableDialog(p, d, s) },
    ]);
  });
  node.appendChild(row);
  node.appendChild(kids);
  return node;
}

function toggleNode(node, row, firstOpen) {
  const opened = node.dataset.open === '1';
  if (opened) { node.dataset.open = '0'; node.classList.add('closed'); row.querySelector('.twist').textContent = '▸'; return; }
  node.dataset.open = '1';
  node.classList.remove('closed');
  row.querySelector('.twist').textContent = '▾';
  if (firstOpen && node.dataset.loaded !== '1') { node.dataset.loaded = '1'; firstOpen(); }
}

function tableListNode(p, d, s) {
  const node = el('div', { class: 'node' });
  const kids = el('div', { class: 'kids' });
  const select = () => {
    state.selection = { kind: 'schema', connectionId: p.id, database: d.name, schema: s.name };
    highlight(tools);
    setCrumbs(`${p.name || p.driver} / ${pathCrumb(p, d, s)} · 表`);
  };
  const tools = el('div', { class: 'row', style: 'color:var(--fg2)' }, [
    el('span', { class: 'twist' }),
    el('span', { class: 'ico', text: '☰' }),
    el('span', { class: 'lbl', text: '表' }),
    el('button', { class: 'mini', title: '数据总览', onclick: (e) => { e.stopPropagation(); openDashboard(p, d, s); }, text: '总览' }),
    el('button', { class: 'mini', title: 'ER 关系图', onclick: (e) => { e.stopPropagation(); openMap(p, d, s); }, text: '关系图' }),
    el('button', { class: 'mini', title: '新建表', onclick: (e) => { e.stopPropagation(); newTableDialog(p, d, s); }, text: '建表' }),
  ]);
  tools.addEventListener('contextmenu', (e) => {
    e.preventDefault();
    select();
    menu(e, [
      { label: '刷新', run: () => loadTables(p, d, s, kids) },
      { label: '新建 SQL 查询', run: () => openSqlWith('') },
      { label: '数据总览', run: () => openDashboard(p, d, s) },
      { label: 'ER 关系图', run: () => openMap(p, d, s) },
      { label: '新建表', run: () => newTableDialog(p, d, s) },
    ]);
  });
  node.appendChild(tools);
  node.appendChild(kids);
  loadTables(p, d, s, kids);
  return node;
}

async function loadTables(p, d, s, box) {
  box.innerHTML = '';
  box.appendChild(el('div', { class: 'hint', style: 'padding:2px 10px', text: '读取表…' }));
  let tables;
  try { tables = await call('meta_tables', { scope: scopeOf(p.id, d.name, s.name) }); } catch (_) { box.innerHTML = ''; return; }
  box.innerHTML = '';
  if (!tables.length) { box.appendChild(el('div', { class: 'hint', style: 'padding:2px 10px', text: '（空）' })); return; }
  for (const t of tables) {
    const row = el('div', { class: 'row' }, [
      el('span', { class: 'twist' }),
      el('span', { class: 'ico', text: t.kind === 'view' ? '▣' : '▦' }),
      el('span', { class: 'lbl', text: t.name, title: t.comment || '' }),
      t.rowCount != null ? el('span', { class: 'badge', text: String(t.rowCount) }) : null,
    ]);
    const select = () => {
      highlight(row);
      state.selection = { kind: 'table', connectionId: p.id, database: d.name, schema: s.name, table: t.name, kind2: t.kind };
    };
    row.addEventListener('click', () => { select(); openTableTab(p, d, s, t); });
    row.addEventListener('contextmenu', (e) => { e.preventDefault(); select(); tableMenu(p, d, s, t, e); });
    box.appendChild(row);
  }
}

function tableMenu(p, d, s, t, e) {
  menu(e, [
    { label: '打开数据', run: () => openTableTab(p, d, s, t) },
    { label: '结构 / DDL', run: () => openStructure(p, d, s, t) },
    { label: '生成 SELECT', run: () => openSqlWith(selectStar(p, d, s, t)) },
    { label: '导出 CSV', run: () => exportTable(p, d, s, t) },
    { label: '清空表', danger: true, run: () => destructive(`清空 ${t.name}`, async () => { const r = await call('ddl_truncate', { r: refOf(p, d, s, t) }); toast(r.message); refreshTables(p, d, s); }) },
    { label: '删除表', danger: true, run: () => destructive(`删除 ${t.name}`, async () => { const r = await call('ddl_drop', { r: refOf(p, d, s, t), kind: t.kind }); toast(r.message); refreshTables(p, d, s); }) },
  ]);
}

function refOf(p, d, s, t) {
  return { connectionId: p.id, database: d.name, schema: s.name, table: t.name };
}
function selectStar(p, d, s, t) {
  const pre = d.name ? `"${d.name}"` : '';
  const sch = s.name ? `"${s.name}".` : '';
  return `SELECT * FROM ${pre ? pre + '.' : ''}${sch}"${t.name}" LIMIT 100;`;
}
function refreshTables(p, d, s) {
  const node = [...document.querySelectorAll('.node')].find((n) => n.querySelector('.lbl')?.textContent === (s.name || d.name));
  if (node) node.dataset.loaded = '0';
  renderTree();
}

/* ------------------------------- context menu ------------------------------- */

function menu(e, items) {
  document.querySelectorAll('.ctxmenu').forEach((n) => n.remove());
  const box = el('div', { class: 'ctxmenu' }, items.map((it) => el('div', {
    class: 'ctxitem' + (it.danger ? ' danger' : ''),
    text: it.label,
    onclick: () => { box.remove(); it.run(); },
  })));
  box.style.cssText = `position:fixed;left:${e.clientX}px;top:${e.clientY}px;z-index:70;background:var(--bg3);border:1px solid var(--line);border-radius:6px;padding:4px;min-width:140px`;
  [...box.children].forEach((c) => { c.style.cssText = 'padding:4px 10px;cursor:pointer;border-radius:4px'; c.onmouseenter = () => c.style.background = 'var(--bg2)'; c.onmouseleave = () => c.style.background = ''; });
  box.querySelector('.danger') && [...box.children].filter((c) => c.classList.contains('danger')).forEach((c) => c.style.color = 'var(--err)');
  document.body.appendChild(box);
  const off = () => { box.remove(); document.removeEventListener('click', off, true); };
  setTimeout(() => document.addEventListener('click', off, true), 0);
}

function profileMenu(p, e, node, kids) {
  const isOpen = state.open.has(p.id);
  menu(e, [
    { label: isOpen ? '断开连接' : '连接', run: async () => { if (isOpen) { await call('conn_close', { id: p.id }); state.open.delete(p.id); } else { await call('conn_connect', { cfg: p }); state.open.add(p.id); } renderTree(); } },
    { label: '刷新', run: () => { if (node.dataset.open === '1') fillProfileKids(p, node, kids); else toggleProfile(p, node, kids); } },
    { label: '新建 SQL 查询', run: () => openSqlWith('') },
    { label: '编辑', run: () => connDialog(p) },
    { label: '删除', danger: true, run: () => destructive(`删除连接 ${p.name}`, async () => { await call('profile_delete', { id: p.id }); if (isOpen) await call('conn_close', { id: p.id }); await loadProfiles(); }) },
  ]);
}

function destructive(label, run) {
  confirmDialog(`确定要${label}吗？此操作不可撤销。`, run, true);
}

/* ----------------------------------- tabs ----------------------------------- */

function openTab(tab) {
  const existing = state.tabs.find((t) => t.key === tab.key);
  if (existing) { Object.assign(existing, tab); }
  else state.tabs.push(tab);
  renderTabs();
  activateTab(tab.key);
}

function closeTab(key) {
  const i = state.tabs.findIndex((t) => t.key === key);
  if (i < 0) return;
  state.tabs.splice(i, 1);
  if (state.active === key) state.active = state.tabs[Math.max(0, i - 1)]?.key || null;
  renderTabs();
  renderPane();
}

function activateTab(key) {
  state.active = key;
  renderTabs();
  renderPane();
}

function renderTabs() {
  const box = $('#tabs');
  box.innerHTML = '';
  for (const t of state.tabs) {
    const node = el('div', { class: 'tab' + (state.active === t.key ? ' active' : ''), onclick: () => activateTab(t.key) }, [
      el('span', { text: t.title }),
      el('span', { class: 'x', text: '×', onclick: (e) => { e.stopPropagation(); closeTab(t.key); } }),
    ]);
    box.appendChild(node);
  }
}

function renderPane() {
  const pane = $('#pane');
  pane.style.cssText = '';
  pane.innerHTML = '';
  const t = state.tabs.find((x) => x.key === state.active);
  if (!t) {
    pane.appendChild(el('div', { class: 'welcome', html: '<h2>OrangeData</h2><p>轻量数据库客户端 · SQLite / MySQL / PostgreSQL</p><ol><li>点击左上角<b>新建连接</b>，本地文件选 SQLite，容器或远程选 MySQL / PostgreSQL</li><li>左侧树展开数据库 → 表，点击表即可浏览与编辑数据</li><li><b>SQL 查询</b>标签里可执行多语句脚本，结果分页展示</li></ol>' }));
    setCrumbs(state.selection ? '已选择节点' : '未连接');
    return;
  }
  setCrumbs(t.crumbs || t.title);
  t.render(pane);
}

function currentScope() {
  const s = state.selection || {};
  if (!s.connectionId) return null;
  return { connectionId: s.connectionId, database: s.database || '', schema: s.schema || '' };
}

function profileOf(id) {
  return state.profiles.find((p) => p.id === id) || { id, driver: '', database: '', host: '', port: 0, user: '', password: '', schema: '', file: '' };
}

/* --------------------------------- shortcuts --------------------------------- */

$('#btn-new').addEventListener('click', () => connDialog(null));
$('#btn-sql').addEventListener('click', () => openSqlWith(''));
$('#btn-refresh').addEventListener('click', () => { renderPane(); renderTree(); });
// The OS keeps the previous caption across a page reload, so re-sync it on boot.
renderPane();

document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') closeModal();
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') {
    const t = state.tabs.find((x) => x.key === state.active);
    if (t?.save) { e.preventDefault(); t.save(); }
  }
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'e') {
    const t = state.tabs.find((x) => x.key === state.active);
    if (t?.run) { e.preventDefault(); t.run(); }
  }
});

/* ---------------------------------- modals ---------------------------------- */

function showModal(card, width) {
  const m = $('#modal');
  const host = $('#modal-card');
  host.innerHTML = '';
  host.style.width = width ? `${width}px` : '';
  host.appendChild(card);
  m.classList.remove('hidden');
  host.querySelectorAll('input,textarea,select').forEach((n) => n.addEventListener('keydown', (e) => { if (e.key === 'Escape') closeModal(); }));
}
function closeModal() { $('#modal').classList.add('hidden'); $('#modal-card').innerHTML = ''; $('#modal-card').style.width = ''; }
$('#modal').addEventListener('click', (e) => { if (e.target.id === 'modal') closeModal(); });

function confirmDialog(text, onOk, danger) {
  showModal(el('div', {}, [
    el('h3', { text: danger ? '危险操作确认' : '确认' }),
    el('p', { text, style: 'color:var(--fg2)' }),
    el('div', { class: 'modal-foot' }, [
      el('button', { class: 'btn ghost', text: '取消', onclick: closeModal }),
      el('button', { class: 'btn ' + (danger ? 'danger' : 'primary'), text: '确定', onclick: async () => { closeModal(); await onOk(); } }),
    ]),
  ]));
}
