/* Connection profile editor, Docker discovery picker and table creation dialog. */

const DRIVER_LABEL = { sqlite: 'SQLite', mysql: 'MySQL', postgres: 'PostgreSQL' };
const ID_TYPE = { sqlite: 'INTEGER', mysql: 'bigint', postgres: 'bigint' };
const TYPE_SUGGEST = {
  sqlite: ['INTEGER', 'TEXT', 'REAL', 'NUMERIC', 'BLOB', 'DATE', 'DATETIME'],
  mysql: ['bigint', 'int', 'varchar(64)', 'text', 'datetime', 'decimal(18,2)', 'json', 'blob'],
  postgres: ['bigint', 'integer', 'text', 'varchar(255)', 'timestamp', 'numeric(18,2)', 'jsonb', 'bytea', 'boolean'],
};

function field(labelText, input) {
  return el('div', { class: 'field' }, [el('label', { text: labelText }), input]);
}

function connDialog(p) {
  const src = p || { driver: 'sqlite', name: '', host: '127.0.0.1', port: 0, user: '', password: '', database: '', schema: '', file: '', ssl: '', charset: '' };
  const f = {
    name: el('input', { value: src.name || '', placeholder: '我的本地库' }),
    driver: el('select', {}, ['sqlite', 'mysql', 'postgres'].map((v) => el('option', { value: v, text: DRIVER_LABEL[v] }))),
    file: el('input', { value: src.file || '', placeholder: 'D:/data/app.db — 不存在则新建' }),
    host: el('input', { value: src.host || '127.0.0.1', placeholder: '127.0.0.1 或 192.168.x.x' }),
    port: el('input', { value: src.port || 3306, type: 'number', min: '1', max: '65535', style: 'width:100px' }),
    user: el('input', { value: src.user || '', placeholder: 'root / postgres' }),
    password: el('input', { value: src.password || '', type: 'password' }),
    database: el('input', { value: src.database || '', placeholder: '留空则列出全部' }),
    schema: el('input', { value: src.schema || '', placeholder: '默认 public' }),
    charset: el('input', { value: src.charset || '', placeholder: 'utf8mb4' }),
    ssl: el('select', {}, [['', '关闭'], ['disabled', 'disabled'], ['preferred', 'preferred'], ['require', 'require / required'], ['verify-ca', 'verify-ca']].map(([v, t]) => el('option', { value: v, text: t }))),
  };
  f.driver.value = src.driver || 'sqlite';
  f.ssl.value = src.ssl || '';

  const pick = el('button', {
    class: 'btn mini', text: '浏览…', onclick: async () => {
      const path = await open({ multiple: false, defaultPath: f.file.value || undefined, filters: [{ name: 'SQLite', extensions: ['db', 'sqlite', 'sqlite3', 'db3'] }] });
      if (path) f.file.value = path;
    },
  });
  const gFile = el('div', {}, [field('文件', el('div', { style: 'display:flex;gap:6px' }, [f.file, pick]))]);
  const gNet = el('div', {}, [
    field('主机', f.host),
    field('端口', f.port),
    field('用户', f.user),
    field('密码', f.password),
  ]);
  const gDb = el('div', {}, [
    field('数据库', f.database),
    field('模式', f.schema),
    field('字符集', f.charset),
    field('SSL', f.ssl),
  ]);

  const dockerBtn = el('button', { class: 'btn ghost', text: 'Docker 容器', title: '从运行中的容器回填主机、端口与账号', onclick: () => dockerDialog() });

  function applyDriver() {
    const drv = f.driver.value;
    gFile.style.display = drv === 'sqlite' ? '' : 'none';
    gNet.style.display = drv === 'sqlite' ? 'none' : '';
    dockerBtn.style.display = drv === 'sqlite' ? 'none' : '';
    f.schema.parentNode.style.display = drv === 'postgres' ? '' : 'none';
    f.charset.parentNode.style.display = drv === 'mysql' ? '' : 'none';
    if (!p || !p.port) f.port.value = state.drivers[drv]?.port || 0;
    if (drv === 'postgres' && !f.user.value) f.user.value = 'postgres';
  }
  f.driver.addEventListener('change', applyDriver);
  applyDriver();

  function buildCfg() {
    const drv = f.driver.value;
    const defPort = state.drivers[drv]?.port || 0;
    return {
      id: src.id || '',
      name: f.name.value.trim() || (drv === 'sqlite' ? baseName(f.file.value) : `${DRIVER_LABEL[drv]} ${f.host.value}`),
      driver: drv,
      host: drv === 'sqlite' ? '' : f.host.value.trim(),
      port: drv === 'sqlite' ? 0 : Number(f.port.value) || defPort,
      user: drv === 'sqlite' ? '' : f.user.value.trim(),
      password: drv === 'sqlite' ? '' : f.password.value,
      database: drv === 'sqlite' ? '' : f.database.value.trim(),
      schema: drv === 'postgres' ? f.schema.value.trim() : '',
      file: drv === 'sqlite' ? f.file.value.trim() : '',
      ssl: f.ssl.value,
      charset: drv === 'mysql' ? f.charset.value.trim() : '',
    };
  }

  const card = el('div', {}, [
    el('h3', { text: src.id ? `编辑连接 · ${src.name || src.driver}` : '新建连接' }),
    field('名称', f.name),
    field('类型', f.driver),
    gFile, gNet, gDb,
    el('p', { class: 'hint', text: '本地文件选 SQLite；Docker 容器填 127.0.0.1 + 映射端口；远程填对方主机与端口。' }),
    el('div', { class: 'modal-foot' }, [
      dockerBtn,
      el('span', { style: 'flex:1' }),
      el('button', { class: 'btn ghost', text: '取消', onclick: closeModal }),
      el('button', {
        class: 'btn', text: '测试连接', onclick: async () => {
          const a = await call('conn_test', { cfg: buildCfg() });
          toast(`连接成功：${a.driver} ${a.version || ''} · ${a.database || a.host}`);
        },
      }),
      el('button', { class: 'btn primary', text: '保存并连接', onclick: () => persistAndConnect(buildCfg()) }),
    ]),
  ]);
  showModal(card);
  (src.driver === 'sqlite' ? f.file : f.host).focus();
}

function baseName(path) {
  const parts = String(path || '').split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] || 'SQLite';
}

async function persistAndConnect(cfg) {
  const saved = await call('profile_save', { cfg });
  if (state.open.has(saved.id)) {
    await call('conn_close', { id: saved.id });
    state.open.delete(saved.id);
  }
  closeModal();
  await loadProfiles();
  const p = state.profiles.find((x) => x.id === saved.id);
  if (!p) return;
  try {
    state.about[p.id] = await call('conn_connect', { cfg: p });
    state.open.add(p.id);
    p._autoExpand = true;
    status(`已连接 ${p.name}`, 'ok');
  } catch (_) { /* conn_connect 已经提示过原因 */ }
  renderTree();
}

/* ---------------------------------- docker ---------------------------------- */

async function dockerDialog() {
  const card = el('div', {}, [el('h3', { text: 'Docker 容器' }), el('p', { class: 'hint', text: '正在扫描 docker ps…' })]);
  showModal(card, 560);
  let st;
  try { st = await call('docker_inspect'); } catch (e) { card.innerHTML = ''; card.appendChild(el('p', { class: 'err-line', text: String(e) })); return; }
  card.innerHTML = '';
  card.appendChild(el('h3', { text: 'Docker 容器' }));
  if (!st.installed || !st.reachable) {
    card.appendChild(el('p', { class: 'err-line', text: st.error || (st.installed ? 'Docker 未运行' : '未检测到 docker 命令') }));
    card.appendChild(el('p', { class: 'hint', text: '安装并启动 Docker Desktop 后重试；也可以直接手动填写 127.0.0.1 与映射端口。' }));
    card.appendChild(el('div', { class: 'modal-foot' }, [el('button', { class: 'btn ghost', text: '关闭', onclick: closeModal })]));
    return;
  }
  card.appendChild(el('p', { class: 'hint', text: `${st.version || 'Docker'} · ${st.containers.length} 个运行中容器` }));
  if (!st.containers.length) {
    card.appendChild(el('p', { class: 'hint', text: '没有运行中的容器。' }));
    return;
  }
  for (const c of st.containers) {
    card.appendChild(el('div', { style: 'border:1px solid var(--line);border-radius:8px;padding:8px 10px;margin-bottom:8px' }, [
      el('div', { style: 'display:flex;gap:8px;align-items:center' }, [
        el('b', { text: c.name }),
        el('span', { class: 'hint', text: c.image }),
        el('span', { class: 'badge', text: c.status }),
      ]),
      el('div', { style: 'margin-top:6px;display:flex;flex-direction:column;gap:4px' }, c.picks.length ? c.picks.map((pk) => el('div', { style: 'display:flex;gap:8px;align-items:center' }, [
        el('span', { class: 'hint', style: 'width:96px', text: pk.driver || '未知端口' }),
        el('code', { style: 'font-family:var(--mono);flex:1', text: `${pk.host}:${pk.hostPort} → ${pk.containerPort}` }),
        el('button', {
          class: 'btn mini', text: '用此连接', onclick: () => {
            closeModal();
            connDialog({
              driver: pk.driver || 'mysql', name: c.name, host: pk.host || '127.0.0.1', port: pk.hostPort,
              user: pk.user || (pk.driver === 'postgres' ? 'postgres' : 'root'), password: pk.password || '',
              database: pk.database || '', schema: '', file: '', ssl: '', charset: '',
            });
          },
        }),
      ])) : [el('span', { class: 'hint', text: '无对外映射端口' })]),
    ]));
  }
  card.appendChild(el('div', { class: 'modal-foot' }, [el('button', { class: 'btn ghost', text: '关闭', onclick: closeModal })]));
}

/* -------------------------------- new table -------------------------------- */

function newTableDialog(p, d, s) {
  const cols = [{ name: 'id', col_type: ID_TYPE[p.driver] || 'bigint', nullable: false, primary_key: true, auto_increment: true, default_value: '' }];
  const nameIn = el('input', { value: '', placeholder: 'table_name', style: 'width:220px' });

  function suggestion(i) {
    return (TYPE_SUGGEST[p.driver] || []).map((t) => el('option', { value: t }));
  }
  function rowNode(c, i) {
    const nm = el('input', { value: c.name, placeholder: '字段名', oninput: (e) => { c.name = e.target.value; } });
    const ty = el('input', { value: c.col_type, placeholder: '类型', list: 'oc-types', oninput: (e) => { c.col_type = e.target.value; } });
    const nn = el('input', { type: 'checkbox', checked: c.nullable ? '' : null, onchange: (e) => { c.nullable = e.target.checked; } });
    const pk = el('input', { type: 'checkbox', checked: c.primary_key ? '' : null, onchange: (e) => { c.primary_key = e.target.checked; if (e.target.checked) c.auto_increment = false; } });
    const ai = el('input', { type: 'checkbox', checked: c.auto_increment ? '' : null, onchange: (e) => { c.auto_increment = e.target.checked; if (e.target.checked) { c.primary_key = true; c.nullable = false; } } });
    const dv = el('input', { value: c.default_value, placeholder: '默认值', oninput: (e) => { c.default_value = e.target.value; } });
    const del = el('button', { class: 'mini', text: '✕', title: '删除字段', onclick: () => { if (cols.length > 1) { cols.splice(i, 1); redraw(); } } });
    return el('tr', {}, [nm, ty, el('td', { style: 'text-align:center' }, [nn]), el('td', { style: 'text-align:center' }, [pk]), el('td', { style: 'text-align:center' }, [ai]), dv, del].map((x) => x.tagName === 'TD' ? x : el('td', {}, [x])));
  }
  const head = el('tr', {}, ['字段', '类型', '可空', '主键', '自增', '默认值', ''].map((t) => el('th', { text: t })));
  const body = el('tbody');
  const table = el('table', { class: 'list', style: 'width:100%' }, [el('thead', {}, [head]), body]);
  function redraw() {
    body.innerHTML = '';
    cols.forEach((c, i) => body.appendChild(rowNode(c, i)));
  }
  redraw();

  const card = el('div', {}, [
    el('h3', { text: `新建表 · ${d.name || p.file || ''}${s.name ? '.' + s.name : ''}` }),
    el('div', { class: 'field' }, [el('label', { text: '表名' }), nameIn]),
    el('datalist', { id: 'oc-types' }, suggestion()),
    table,
    el('div', { style: 'display:flex;gap:8px;margin-top:10px' }, [
      el('button', { class: 'btn mini', text: '＋ 添加字段', onclick: () => { cols.push({ name: '', col_type: TYPE_SUGGEST[p.driver]?.[1] || 'text', nullable: true, primary_key: false, auto_increment: false, default_value: '' }); redraw(); } }),
      el('span', { class: 'spacer', style: 'flex:1' }),
      el('button', { class: 'btn ghost', text: '取消', onclick: closeModal }),
      el('button', {
        class: 'btn primary', text: '创建', onclick: async () => {
          const clean = cols.filter((c) => c.name.trim());
          if (!nameIn.value.trim()) { toast('请填写表名', true); return; }
          if (!clean.length) { toast('至少一个字段', true); return; }
          if (!clean.some((c) => c.primary_key)) { toast('建议至少设置一个主键，否则表格不可编辑', true); }
          const req = {
            connectionId: p.id, database: d.name, schema: s.name, name: nameIn.value.trim(),
            columns: clean.map((c) => ({ name: c.name.trim(), type: c.col_type.trim(), nullable: c.nullable, primaryKey: c.primary_key, autoIncrement: c.auto_increment, defaultValue: c.default_value })),
          };
          const r = await call('ddl_create_table', { req });
          toast(r.message);
          closeModal();
          refreshTables(p, d, s);
          openTableTab(p, d, s, { name: req.name, kind: 'table' });
        },
      }),
    ]),
  ]);
  showModal(card, 700);
  nameIn.focus();
}

loadProfiles();
