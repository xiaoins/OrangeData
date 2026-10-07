//! Engine-agnostic work: every query here is assembled through a `Dialect`,
//! so the tree, grid, editor and dashboards exist once for all three drivers.

use super::dialect::{check_ident, is_query, qualify, safe_cast_type, split_statements, Ctx, Dialect, Sql};
use super::engine::RawGrid;
use super::engine::Db;
use crate::model::{
    err, About, Column, ColumnLite, ConnCfg, CreateTableReq, EditItem, EditResult, EditReq, ExportResult,
    FilterSpec, ForeignKey, Grid, IndexInfo, MapEdge, MapNode, Node, OpResult, Overview, PageReq,
    Res, RowPatch, RowValues, SchemaMap, Scope, ScriptReq, ScriptResult, StmtResult, TableInfo,
    TableMeta, TableRef, Totals,
};
use serde_json::Value;
use std::collections::HashMap;

/// Guard rails: the UI can never ask for more than this in one round trip.
pub const MAX_PAGE_SIZE: u32 = 500;
pub const MAX_SCRIPT_ROWS: usize = 2000;
/// SQLite has no catalog statistics, so exact counts cost one query per table.
const MAX_COUNTED_TABLES: usize = 150;

fn pick(v: &str, fallback: &str) -> String {
    let t = v.trim();
    if t.is_empty() { fallback.to_string() } else { t.to_string() }
}

/// Build the metadata context for one call, letting the session fill in whatever
/// the UI left blank, and validate every identifier that will be interpolated.
pub fn ctx_for(db: &dyn Db, database: &str, schema: &str, table: &str) -> Res<Ctx> {
    let base = db.ctx();
    let mut c = Ctx {
        database: pick(database, &base.database),
        schema: pick(schema, &base.schema),
        table: String::new(),
    };
    if !c.database.is_empty() {
        c.database = check_ident(&c.database, "数据库名")?;
    }
    if !c.schema.is_empty() {
        c.schema = check_ident(&c.schema, "模式名")?;
    }
    if !table.trim().is_empty() {
        c.table = check_ident(table, "表名")?;
    }
    Ok(c)
}

fn q(d: &dyn Dialect, name: &str) -> Res<String> {
    let n = check_ident(name, "列名")?;
    Ok(qualify(d.quote(), &[&n]))
}

fn qual(d: &dyn Dialect, database: &str, schema: &str, table: &str) -> Res<String> {
    let t = check_ident(table, "表名")?;
    let mut parts: Vec<String> = Vec::new();
    if !database.trim().is_empty() && !d.has_schemas() {
        parts.push(check_ident(database, "数据库名")?);
    }
    if !schema.trim().is_empty() {
        parts.push(check_ident(schema, "模式名")?);
    }
    parts.push(t);
    let refs: Vec<&str> = parts.iter().map(|s| s.as_str()).collect();
    Ok(qualify(d.quote(), &refs))
}

/* --------------------------------- rows --------------------------------- */

/// Catalog result sets are addressed by alias, never by position, because the
/// three engines order their metadata columns differently.
struct Rows {
    idx: HashMap<String, usize>,
    rows: Vec<Vec<Value>>,
}

impl From<RawGrid> for Rows {
    fn from(g: RawGrid) -> Self {
        let idx = g.columns.iter().enumerate().map(|(i, c)| (c.to_lowercase(), i)).collect();
        Rows { idx, rows: g.rows }
    }
}

static NULL: Value = Value::Null;

impl Rows {
    fn cell<'r>(&self, row: &'r Vec<Value>, key: &str) -> &'r Value {
        self.idx.get(key).and_then(|i| row.get(*i)).unwrap_or(&NULL)
    }

    fn text(&self, row: &Vec<Value>, key: &str) -> String {
        match self.cell(row, key) {
            Value::Null => String::new(),
            Value::String(s) => s.clone(),
            other => other.to_string(),
        }
    }
    fn opt(&self, row: &Vec<Value>, key: &str) -> Option<String> {
        let s = self.text(row, key);
        if s.is_empty() { None } else { Some(s) }
    }
    fn num(&self, row: &Vec<Value>, key: &str) -> Option<i64> {
        match self.cell(row, key) {
            Value::Number(n) => n.as_i64().or_else(|| n.to_string().parse().ok()),
            Value::String(s) => s.trim().parse().ok(),
            _ => None,
        }
    }
    fn flag(&self, row: &Vec<Value>, key: &str) -> bool {
        match self.cell(row, key) {
            Value::Bool(b) => *b,
            Value::Number(n) => n.as_i64().unwrap_or(0) != 0,
            Value::String(s) => matches!(s.trim(), "1" | "t" | "true" | "YES"),
            _ => false,
        }
    }
    fn first(&self) -> Option<&Vec<Value>> {
        self.rows.first()
    }
}

async fn rows(db: &dyn Db, sql: &Sql) -> Res<Rows> {
    db.query(&sql.text, &sql.args).await.map(Rows::from)
}

fn split_list(s: &str) -> Vec<String> {
    s.split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect()
}

/* ------------------------------- metadata ------------------------------- */

pub async fn about(db: &dyn Db, scope: &Scope) -> Res<About> {
    let d = db.dialect();
    let version = match rows(db, &d.version_sql()).await {
        Ok(r) => r.first().map(|row| r.text(row, "version")).unwrap_or_default(),
        Err(_) => String::new(),
    };
    let mut ctx = ctx_for(db, &scope.database, &scope.schema, "")?;
    if ctx.database.is_empty() {
        if let Some(sql) = d.current_db_sql() {
            if let Ok(r) = rows(db, &sql).await {
                if let Some(row) = r.first() {
                    ctx.database = r.text(row, "name");
                }
            }
        }
    }
    Ok(About {
        driver: d.name().to_string(),
        version: if version.is_empty() { None } else { Some(version) },
        database: ctx.database,
        schema: ctx.schema,
        host: db.host_label(),
        connected: true,
    })
}

pub async fn list_databases(db: &dyn Db) -> Res<Vec<Node>> {
    let ctx = db.ctx();
    let r = rows(db, &db.dialect().databases_sql(&ctx)).await?;
    Ok(r.rows.iter().map(|row| Node { name: r.text(row, "name"), comment: r.opt(row, "comment") }).collect())
}

pub async fn list_schemas(db: &dyn Db, scope: &Scope) -> Res<Vec<Node>> {
    let ctx = ctx_for(db, &scope.database, "", "")?;
    let r = rows(db, &db.dialect().schemas_sql(&ctx)).await?;
    Ok(r.rows.iter().map(|row| Node { name: r.text(row, "name"), comment: r.opt(row, "comment") }).collect())
}

pub async fn list_tables(db: &dyn Db, scope: &Scope) -> Res<Vec<TableInfo>> {
    let d = db.dialect();
    let ctx = ctx_for(db, &scope.database, &scope.schema, "")?;
    let r = rows(db, &d.tables_sql(&ctx)).await?;
    let mut out: Vec<TableInfo> = Vec::new();
    for row in &r.rows {
        out.push(TableInfo {
            name: r.text(row, "name"),
            kind: r.text(row, "kind"),
            row_count: r.num(row, "row_estimate"),
            size_bytes: r.num(row, "size_bytes"),
            comment: r.opt(row, "comment"),
        });
    }
    if d.exact_counts() && out.len() <= MAX_COUNTED_TABLES {
        for t in out.iter_mut() {
            if t.kind != "table" {
                continue;
            }
            let c = ctx_for(db, &scope.database, &scope.schema, &t.name)?;
            let cs = d.count_sql(&c);
            if let Ok(g) = db.query(&cs.text, &cs.args).await {
                t.row_count = g.rows.first().and_then(|r0| r0.first()).and_then(|v| match v {
                    Value::Number(n) => n.as_i64(),
                    Value::String(s) => s.trim().parse().ok(),
                    _ => None,
                });
            }
        }
    }
    Ok(out)
}

pub async fn columns(db: &dyn Db, ctx: &Ctx) -> Res<Vec<Column>> {
    let r = rows(db, &db.dialect().columns_sql(ctx)).await?;
    let pks = primary_keys(db, ctx).await.unwrap_or_default();
    let fks = foreign_keys(db, ctx).await.unwrap_or_default();
    let mut refs: HashMap<String, String> = HashMap::new();
    for f in &fks {
        for c in &f.columns {
            refs.insert(c.to_lowercase(), format!("{}({})", f.ref_table, f.ref_columns.join(", ")));
        }
    }
    Ok(r.rows
        .iter()
        .map(|row| {
            let name = r.text(row, "name");
            Column {
                is_pk: pks.iter().any(|p| p.eq_ignore_ascii_case(&name)),
                references: refs.get(&name.to_lowercase()).cloned(),
                col_type: r.text(row, "type"),
                nullable: r.flag(row, "nullable"),
                comment: r.opt(row, "comment"),
                default_value: r.opt(row, "default_value"),
                extra: r.opt(row, "extra"),
                name,
            }
        })
        .collect())
}

pub async fn primary_keys(db: &dyn Db, ctx: &Ctx) -> Res<Vec<String>> {
    let r = rows(db, &db.dialect().pk_sql(ctx)).await?;
    match r.first() {
        Some(row) => Ok(split_list(&r.text(row, "pk_columns"))),
        None => Ok(Vec::new()),
    }
}

pub async fn foreign_keys(db: &dyn Db, ctx: &Ctx) -> Res<Vec<ForeignKey>> {
    let r = rows(db, &db.dialect().fk_sql(ctx)).await?;
    let mut order: Vec<String> = Vec::new();
    let mut map: HashMap<String, ForeignKey> = HashMap::new();
    for row in &r.rows {
        let name = {
            let n = r.text(row, "fk_name");
            if n.is_empty() { "fk".to_string() } else { n }
        };
        let entry = map.entry(name.clone()).or_insert_with(|| {
            order.push(name.clone());
            ForeignKey {
                name,
                columns: Vec::new(),
                ref_table: String::new(),
                ref_columns: Vec::new(),
                on_delete: None,
                on_update: None,
            }
        });
        let col = r.text(row, "column_name");
        if !col.is_empty() && !entry.columns.contains(&col) {
            entry.columns.push(col);
        }
        let rc = r.text(row, "ref_column");
        if !rc.is_empty() && !entry.ref_columns.contains(&rc) {
            entry.ref_columns.push(rc);
        }
        if entry.ref_table.is_empty() {
            entry.ref_table = r.text(row, "ref_table");
        }
        if entry.on_delete.is_none() {
            entry.on_delete = r.opt(row, "on_delete");
        }
        if entry.on_update.is_none() {
            entry.on_update = r.opt(row, "on_update");
        }
    }
    Ok(order.into_iter().filter_map(|n| map.remove(&n)).collect())
}

pub async fn indexes(db: &dyn Db, ctx: &Ctx) -> Res<Vec<IndexInfo>> {
    let r = rows(db, &db.dialect().index_sql(ctx)).await?;
    Ok(r.rows
        .iter()
        .map(|row| IndexInfo {
            name: r.text(row, "index_name"),
            unique: r.flag(row, "is_unique"),
            primary: r.flag(row, "is_primary"),
            kind: r.opt(row, "index_kind"),
            columns: split_list(&r.text(row, "column_list")),
        })
        .collect())
}

/* ------------------------------- table detail ------------------------------- */

pub async fn table_meta(db: &dyn Db, r: &TableRef) -> Res<TableMeta> {
    let ctx = ctx_for(db, &r.database, &r.schema, &r.table)?;
    let cols = columns(db, &ctx).await?;
    if cols.is_empty() {
        return Err(format!("表 {} 不存在或没有可见列", ctx.table));
    }
    let pks = primary_keys(db, &ctx).await.unwrap_or_default();
    let fks = foreign_keys(db, &ctx).await.unwrap_or_default();
    let idx = indexes(db, &ctx).await.unwrap_or_default();
    let ddl = ddl_text(db, &ctx, &cols, &pks, &fks).await;
    Ok(TableMeta {
        table: ctx.table,
        columns: cols,
        primary_keys: pks.clone(),
        foreign_keys: fks,
        indexes: idx,
        ddl,
        editable: !pks.is_empty(),
        row_count: None,
    })
}

/// Prefer the engine's own DDL text; fall back to rebuilding it from catalog rows.
async fn ddl_text(db: &dyn Db, ctx: &Ctx, cols: &[Column], pks: &[String], fks: &[ForeignKey]) -> String {
    let d = db.dialect();
    if let Some(sql) = d.ddl_sql(ctx) {
        if let Ok(g) = db.query(&sql.text, &sql.args).await {
            if let Some(row) = g.rows.first() {
                let want = d.ddl_key().unwrap_or("");
                let at = g
                    .columns
                    .iter()
                    .position(|c| c.eq_ignore_ascii_case(want))
                    .or_else(|| g.columns.iter().position(|c| c.eq_ignore_ascii_case("ddl")))
                    .unwrap_or(0);
                if let Some(Value::String(s)) = row.get(at) {
                    return s.clone();
                }
                if let Some(v) = row.get(at) {
                    return v.to_string();
                }
            }
        }
    }
    synthesize_ddl(d, ctx, cols, pks, fks)
}

fn synthesize_ddl(d: &dyn Dialect, ctx: &Ctx, cols: &[Column], pks: &[String], fks: &[ForeignKey]) -> String {
    let tbl = match q(d, &ctx.table) {
        Ok(s) => s,
        Err(_) => return String::new(),
    };
    let mut lines: Vec<String> = Vec::new();
    for c in cols {
        let cname = match q(d, &c.name) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let extra = c.extra.clone().unwrap_or_default().to_lowercase();
        let auto = extra.contains("auto_increment") || extra == "identity" || extra == "serial";
        let mut s = format!("  {cname} {}", if c.col_type.is_empty() { "TEXT" } else { &c.col_type });
        if auto && d.name() == "mysql" {
            s.push_str(" AUTO_INCREMENT");
        } else if auto && d.name() == "postgres" {
            s.push_str(" GENERATED BY DEFAULT AS IDENTITY");
        }
        if !c.nullable && !pks.iter().any(|p| p.eq_ignore_ascii_case(&c.name)) {
            s.push_str(" NOT NULL");
        }
        if let Some(def) = &c.default_value {
            if !auto && !def.is_empty() {
                s.push_str(&format!(" DEFAULT {}", default_literal(def)));
            }
        }
        lines.push(s);
    }
    if !pks.is_empty() {
        let list = pks
            .iter()
            .filter_map(|p| q(d, p).ok())
            .collect::<Vec<_>>()
            .join(", ");
        if !(pks.len() == 1 && d.name() != "postgres" && cols.iter().any(|c| {
            c.name.eq_ignore_ascii_case(&pks[0]) && c.extra.clone().unwrap_or_default().to_lowercase().contains("auto")
        })) {
            lines.push(format!("  PRIMARY KEY ({list})"));
        }
    }
    for f in fks {
        if f.columns.is_empty() || f.ref_columns.is_empty() || f.ref_table.is_empty() {
            continue;
        }
        let lc = f.columns.iter().filter_map(|c| q(d, c).ok()).collect::<Vec<_>>().join(", ");
        let rc = f.ref_columns.iter().filter_map(|c| q(d, c).ok()).collect::<Vec<_>>().join(", ");
        let rtab = match q(d, &f.ref_table) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let mut line = format!("  FOREIGN KEY ({lc}) REFERENCES {rtab} ({rc})");
        if let Some(x) = &f.on_delete {
            if !x.eq_ignore_ascii_case("NO ACTION") {
                line.push_str(&format!(" ON DELETE {x}"));
            }
        }
        if let Some(x) = &f.on_update {
            if !x.eq_ignore_ascii_case("NO ACTION") {
                line.push_str(&format!(" ON UPDATE {x}"));
            }
        }
        lines.push(line);
    }
    format!("CREATE TABLE {tbl} (\n{}\n);", lines.join(",\n"))
}

/// Catalog defaults arrive as raw expressions; quote anything that clearly is a
/// plain literal so the rebuilt DDL still executes.
fn default_literal(def: &str) -> String {
    let s = def.trim();
    let bare = s.trim_matches('\'');
    if s.starts_with('\'')
        || s.starts_with("CURRENT_TIMESTAMP")
        || s.starts_with("current_timestamp")
        || s.starts_with("now(")
        || s.parse::<f64>().is_ok()
        || bare.eq_ignore_ascii_case("null")
        || s.contains('(')
        || s.contains("::")
    {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "''"))
    }
}

/* ------------------------------ filter / sort ------------------------------ */

const CMP_OPS: [&str; 6] = ["=", "<>", ">", ">=", "<", "<="];

/// Turn the visual filter row into a WHERE clause with bound values.
fn build_where(d: &dyn Dialect, filters: &[FilterSpec], types: &HashMap<String, String>) -> Res<(String, Vec<String>)> {
    let mut parts: Vec<String> = Vec::new();
    let mut args: Vec<String> = Vec::new();
    for f in filters {
        if f.column.trim().is_empty() {
            continue;
        }
        let col = q(d, &f.column)?;
        let ty = types.get(&f.column.to_lowercase()).cloned().unwrap_or_else(|| "text".to_string());
        match f.op.trim() {
            "null" => parts.push(format!("{col} IS NULL")),
            "notnull" => parts.push(format!("{col} IS NOT NULL")),
            "contains" => {
                parts.push(format!("{} LIKE ?", d.cast_text(&col)));
                args.push(format!("%{}%", f.value));
            }
            "starts" => {
                parts.push(format!("{} LIKE ?", d.cast_text(&col)));
                args.push(format!("{}%", f.value));
            }
            "ends" => {
                parts.push(format!("{} LIKE ?", d.cast_text(&col)));
                args.push(format!("%{}", f.value));
            }
            op => {
                let real = if op.is_empty() { "=" } else { op };
                if !CMP_OPS.contains(&real) {
                    return Err(format!("不支持的筛选操作符：{real}"));
                }
                parts.push(d.cmp(&col, &ty, real));
                args.push(f.value.clone());
            }
        }
    }
    let where_sql = if parts.is_empty() { String::new() } else { format!(" WHERE {}", parts.join(" AND ")) };
    Ok((where_sql, args))
}

fn build_order(d: &dyn Dialect, sort: &[crate::model::SortSpec]) -> Res<String> {
    let mut parts: Vec<String> = Vec::new();
    for s in sort {
        if s.column.trim().is_empty() {
            continue;
        }
        let col = q(d, &s.column)?;
        let dir = if s.dir.trim().eq_ignore_ascii_case("desc") { "DESC" } else { "ASC" };
        parts.push(format!("{col} {dir}"));
    }
    Ok(if parts.is_empty() { String::new() } else { format!(" ORDER BY {}", parts.join(", ")) })
}

fn type_map(cols: &[Column]) -> HashMap<String, String> {
    cols.iter().map(|c| (c.name.to_lowercase(), c.col_type.clone())).collect()
}

/* ---------------------------------- grid ---------------------------------- */

pub async fn fetch_page(db: &dyn Db, req: &PageReq) -> Res<Grid> {
    let d = db.dialect();
    let ctx = ctx_for(db, &req.database, &req.schema, &req.table)?;
    let cols = columns(db, &ctx).await?;
    if cols.is_empty() {
        return Err(format!("表 {} 不存在或没有可见列", ctx.table));
    }
    let names: Vec<String> = cols.iter().map(|c| c.name.clone()).collect();
    let select_list = cols.iter().map(|c| q(d, &c.name)).collect::<Res<Vec<_>>>()?.join(", ");
    let tbl = d.table_name(&ctx);
    let types = type_map(&cols);
    let (where_sql, args) = build_where(d, &req.filters, &types)?;
    let order = build_order(d, &req.sort)?;
    let size = req.size.clamp(1, MAX_PAGE_SIZE) as u64;
    let page = if req.page == 0 { 1 } else { req.page };
    let offset = (page as u64 - 1) * size;
    let text = format!("SELECT {select_list} FROM {tbl}{where_sql}{order} {}", d.paging(size, offset));

    let started = std::time::Instant::now();
    let g = db.query(&text, &args).await?;
    let ms = started.elapsed().as_millis();
    let total = count_of(db, &tbl, &where_sql, &args).await.ok();

    let pks = primary_keys(db, &ctx).await.unwrap_or_default();
    let column_meta: Vec<ColumnLite> = cols
        .iter()
        .map(|c| ColumnLite {
            name: c.name.clone(),
            col_type: c.col_type.clone(),
            nullable: c.nullable,
            is_pk: pks.iter().any(|p| p.eq_ignore_ascii_case(&c.name)),
        })
        .collect();
    Ok(Grid {
        table: ctx.table,
        columns: if g.columns.is_empty() { names } else { g.columns },
        column_meta,
        primary_keys: pks.clone(),
        editable: !pks.is_empty(),
        rows: g.rows,
        page,
        size: size as u32,
        total,
        ms,
        sql: text,
    })
}

async fn count_of(db: &dyn Db, tbl: &str, where_sql: &str, args: &[String]) -> Res<i64> {
    let text = format!("SELECT COUNT(*) AS row_count FROM {tbl}{where_sql}");
    let g = db.query(&text, args).await?;
    g.rows
        .first()
        .and_then(|r| r.first())
        .map(|v| match v {
            Value::Number(n) => n.as_i64().unwrap_or(0),
            Value::String(s) => s.trim().parse().unwrap_or(0),
            _ => 0,
        })
        .ok_or_else(|| "计数失败".to_string())
}

/* --------------------------------- runner --------------------------------- */

pub async fn run_script(db: &dyn Db, req: &ScriptReq) -> Res<ScriptResult> {
    let parts = split_statements(&req.sql);
    if parts.is_empty() {
        return Err("没有可执行的 SQL".to_string());
    }
    let limit = req.limit.clamp(1, MAX_SCRIPT_ROWS as u32) as usize;
    let mut out: Vec<StmtResult> = Vec::new();
    let mut ok = true;
    for (i, text) in parts.iter().enumerate() {
        let started = std::time::Instant::now();
        let query = is_query(text);
        let item = if query {
            match db.query(text, &[]).await {
                Ok(g) => {
                    let truncated = g.rows.len() > limit;
                    let mut rows = g.rows;
                    if truncated {
                        rows.truncate(limit);
                    }
                    StmtResult {
                        index: i,
                        sql: text.clone(),
                        is_query: true,
                        columns: g.columns,
                        rows,
                        affected: None,
                        ms: started.elapsed().as_millis(),
                        error: None,
                        truncated,
                    }
                }
                Err(e) => {
                    ok = false;
                    StmtResult {
                        index: i,
                        sql: text.clone(),
                        is_query: true,
                        columns: Vec::new(),
                        rows: Vec::new(),
                        affected: None,
                        ms: started.elapsed().as_millis(),
                        error: Some(e),
                        truncated: false,
                    }
                }
            }
        } else {
            match db.execute(text, &[]).await {
                Ok(n) => StmtResult {
                    index: i,
                    sql: text.clone(),
                    is_query: false,
                    columns: Vec::new(),
                    rows: Vec::new(),
                    affected: Some(n as i64),
                    ms: started.elapsed().as_millis(),
                    error: None,
                    truncated: false,
                },
                Err(e) => {
                    ok = false;
                    StmtResult {
                        index: i,
                        sql: text.clone(),
                        is_query: false,
                        columns: Vec::new(),
                        rows: Vec::new(),
                        affected: None,
                        ms: started.elapsed().as_millis(),
                        error: Some(e),
                        truncated: false,
                    }
                }
            }
        };
        let stop = item.error.is_some();
        out.push(item);
        if stop {
            break;
        }
    }
    Ok(ScriptResult { statements: out, ok })
}

/* --------------------------------- editing --------------------------------- */

fn cell_arg(v: &Value) -> Res<Option<String>> {
    match v {
        Value::Null => Ok(None),
        Value::Bool(b) => Ok(Some(if *b { "1".into() } else { "0".into() })),
        Value::Number(n) => Ok(Some(n.to_string())),
        Value::String(s) => Ok(Some(s.clone())),
        Value::Object(o) if o.get("__bin").is_some() => Err("二进制字段暂不支持在表格中写回，请使用 SQL 执行".to_string()),
        Value::Object(o) => serde_json::to_string(o).map(Some).map_err(err),
        Value::Array(a) => serde_json::to_string(a).map(Some).map_err(err),
    }
}

/// Build one statement plus the values to bind; NULL never becomes a bound
/// parameter because engines need the literal.
fn update_stmt(d: &dyn Dialect, tbl: &str, cols: &[Column], p: &RowPatch) -> Res<Sql> {
    let types = type_map(cols);
    let mut sets: Vec<String> = Vec::new();
    let mut args: Vec<String> = Vec::new();
    let mut order: Vec<String> = cols.iter().map(|c| c.name.clone()).collect();
    order.retain(|k| p.set.contains_key(k));
    for k in &order {
        let col = q(d, k)?;
        let ty = types.get(&k.to_lowercase()).cloned().unwrap_or_else(|| "text".into());
        match cell_arg(&p.set[k])? {
            Some(v) => {
                sets.push(d.assign(&col, &ty));
                args.push(v);
            }
            None => sets.push(format!("{col} = NULL")),
        }
    }
    if sets.is_empty() {
        return Err("没有需要保存的字段".to_string());
    }
    let (sqls, bounds) = predicate(d, cols, &p.pk)?;
    args.extend(bounds.into_iter().flatten());
    let text = format!("UPDATE {tbl} SET {} WHERE {}", sets.join(", "), sqls.join(" AND "));
    Ok(Sql::with(text, args))
}

/// WHERE fragments for one row key, plus the value to bind (`None` for NULL).
fn predicate(d: &dyn Dialect, cols: &[Column], keys: &HashMap<String, Value>) -> Res<(Vec<String>, Vec<Option<String>>)> {
    if keys.is_empty() {
        return Err("该行缺少主键，无法定位".to_string());
    }
    let types = type_map(cols);
    let mut sqls: Vec<String> = Vec::new();
    let mut bounds: Vec<Option<String>> = Vec::new();
    let mut names: Vec<String> = keys.keys().cloned().collect();
    names.sort();
    for k in &names {
        let col = q(d, k)?;
        let ty = types.get(&k.to_lowercase()).cloned().unwrap_or_else(|| "text".into());
        match cell_arg(&keys[k])? {
            Some(v) => {
                sqls.push(d.cmp(&col, &ty, "="));
                bounds.push(Some(v));
            }
            None => {
                sqls.push(format!("{col} IS NULL"));
                bounds.push(None);
            }
        }
    }
    Ok((sqls, bounds))
}

fn insert_stmt(d: &dyn Dialect, tbl: &str, cols: &[Column], row: &RowValues) -> Res<Sql> {
    let types = type_map(cols);
    let mut sql_names: Vec<String> = Vec::new();
    let mut values: Vec<String> = Vec::new();
    let mut args: Vec<String> = Vec::new();
    for c in cols {
        if !row.values.contains_key(&c.name) {
            continue;
        }
        sql_names.push(q(d, &c.name)?);
        match cell_arg(&row.values[&c.name])? {
            Some(v) => {
                let ty = types.get(&c.name.to_lowercase()).cloned().unwrap_or_else(|| "text".into());
                values.push(d.value_placeholder(&ty));
                args.push(v);
            }
            None => values.push("NULL".to_string()),
        }
    }
    if sql_names.is_empty() {
        return Err("新增行没有任何字段".to_string());
    }
    Ok(Sql::with(format!("INSERT INTO {tbl} ({}) VALUES ({})", sql_names.join(", "), values.join(", ")), args))
}

fn delete_stmt(d: &dyn Dialect, tbl: &str, cols: &[Column], row: &RowValues) -> Res<Sql> {
    let (sqls, bounds) = predicate(d, cols, &row.values)?;
    let args: Vec<String> = bounds.into_iter().flatten().collect();
    Ok(Sql::with(format!("DELETE FROM {tbl} WHERE {}", sqls.join(" AND ")), args))
}

pub async fn apply_edits(db: &dyn Db, req: &EditReq) -> Res<EditResult> {
    let ctx = ctx_for(db, &req.database, &req.schema, &req.table)?;
    let d = db.dialect();
    let cols = columns(db, &ctx).await?;
    let pks = primary_keys(db, &ctx).await?;
    if pks.is_empty() {
        return Err("该表没有主键，不能通过表格修改".to_string());
    }
    let tbl = d.table_name(&ctx);
    let mut stmts: Vec<Sql> = Vec::new();
    let mut kinds: Vec<String> = Vec::new();
    for u in &req.updates {
        stmts.push(update_stmt(d, &tbl, &cols, u)?);
        kinds.push("update".into());
    }
    for i in &req.inserts {
        stmts.push(insert_stmt(d, &tbl, &cols, i)?);
        kinds.push("insert".into());
    }
    for del in &req.deletes {
        stmts.push(delete_stmt(d, &tbl, &cols, del)?);
        kinds.push("delete".into());
    }
    if stmts.is_empty() {
        return Ok(EditResult { table: ctx.table, applied: 0, items: Vec::new() });
    }
    let results = db.tx(stmts.clone()).await?;
    let mut items: Vec<EditItem> = Vec::new();
    let mut applied = 0usize;
    for (idx, r) in results.iter().enumerate() {
        let kind = kinds.get(idx).cloned().unwrap_or_default();
        match r {
            Ok(n) => {
                applied += 1;
                items.push(EditItem { kind, sql: stmts[idx].text.clone(), ok: true, affected: Some(*n as i64), error: None });
            }
            Err(e) => items.push(EditItem {
                kind,
                sql: stmts[idx].text.clone(),
                ok: false,
                affected: None,
                error: Some(e.clone()),
            }),
        }
    }
    Ok(EditResult { table: ctx.table, applied, items })
}

/* ---------------------------------- ddl ---------------------------------- */

pub async fn create_table(db: &dyn Db, req: &CreateTableReq) -> Res<OpResult> {
    let ctx = ctx_for(db, &req.database, &req.schema, "")?;
    let d = db.dialect();
    let table = check_ident(&req.name, "表名")?;
    if req.columns.is_empty() {
        return Err("至少需要一个字段".to_string());
    }
    let mut lines: Vec<String> = Vec::new();
    let mut pks: Vec<String> = Vec::new();
    for c in &req.columns {
        let col = q(d, &c.name)?;
        if c.primary_key {
            pks.push(col.clone());
        }
        let declared = c.col_type.trim().to_lowercase();
        if !declared.is_empty() && declared != safe_cast_type(&declared) {
            return Err(format!("字段类型不合法：{}", c.col_type));
        }
        let mut ty = if declared.is_empty() { d.id_type().to_string() } else { declared };
        if c.auto_increment {
            ty = d.id_type().to_string();
        }
        let mut line = format!("{col} {ty}");
        if c.auto_increment {
            line.push_str(" NOT NULL ");
            line.push_str(d.auto_increment());
        } else {
            if !c.nullable {
                line.push_str(" NOT NULL");
            }
            let def = c.default_value.trim();
            if !def.is_empty() {
                line.push_str(&format!(" DEFAULT {}", default_literal(def)));
            }
        }
        lines.push(line);
    }
    let inline_pk = lines.iter().any(|l| l.contains("PRIMARY KEY"));
    if !pks.is_empty() && !inline_pk {
        lines.push(format!("PRIMARY KEY ({})", pks.join(", ")));
    }
    let tbl = qual(d, &ctx.database, &ctx.schema, &table)?;
    let text = format!("CREATE TABLE {tbl} ({})", lines.join(", "));
    db.execute(&text, &[]).await?;
    Ok(OpResult { ok: true, sql: text, message: format!("已创建表 {table}") })
}

pub async fn drop_object(db: &dyn Db, r: &TableRef, kind: &str) -> Res<OpResult> {
    let d = db.dialect();
    let ctx = ctx_for(db, &r.database, &r.schema, &r.table)?;
    let verb = match kind {
        "view" => "DROP VIEW",
        _ => "DROP TABLE",
    };
    let text = format!("{verb} IF EXISTS {}", d.table_name(&ctx));
    db.execute(&text, &[]).await?;
    Ok(OpResult { ok: true, sql: text, message: format!("已删除 {}", ctx.table) })
}

pub async fn truncate_table(db: &dyn Db, r: &TableRef) -> Res<OpResult> {
    let d = db.dialect();
    let ctx = ctx_for(db, &r.database, &r.schema, &r.table)?;
    let text = if d.name() == "sqlite" {
        format!("DELETE FROM {}", d.table_name(&ctx))
    } else {
        format!("TRUNCATE TABLE {}", d.table_name(&ctx))
    };
    let n = db.execute(&text, &[]).await?;
    Ok(OpResult { ok: true, sql: text, message: format!("已清空，影响 {n} 行") })
}

/* --------------------------------- insights --------------------------------- */

/// Foreign-key graph for the schema ER map. Column and key lookups cost one
/// query per table, so the map is capped instead of scanning huge schemas.
const MAP_TABLE_CAP: usize = 80;

pub async fn schema_map(db: &dyn Db, scope: &Scope) -> Res<SchemaMap> {
    let all = list_tables(db, scope).await?;
    let truncated = all.len() > MAP_TABLE_CAP;
    let picked: Vec<&TableInfo> = all.iter().take(MAP_TABLE_CAP).collect();
    let mut nodes: Vec<MapNode> = Vec::new();
    let mut edges: Vec<MapEdge> = Vec::new();
    for t in &picked {
        let ctx = ctx_for(db, &scope.database, &scope.schema, &t.name)?;
        let cols = columns(db, &ctx).await.unwrap_or_default();
        let pks = primary_keys(db, &ctx).await.unwrap_or_default();
        nodes.push(MapNode {
            name: t.name.clone(),
            kind: t.kind.clone(),
            row_count: t.row_count,
            columns: cols
                .iter()
                .map(|c| ColumnLite {
                    name: c.name.clone(),
                    col_type: c.col_type.clone(),
                    nullable: c.nullable,
                    is_pk: pks.iter().any(|p| p.eq_ignore_ascii_case(&c.name)),
                })
                .collect(),
        });
        for f in foreign_keys(db, &ctx).await.unwrap_or_default() {
            edges.push(MapEdge {
                from: t.name.clone(),
                to: f.ref_table,
                name: f.name,
                columns: f.columns,
                ref_columns: f.ref_columns,
            });
        }
    }
    Ok(SchemaMap { nodes, edges, truncated, total_tables: all.len() })
}

/// Dashboard numbers. Column totals sample at most this many tables because no
/// engine exposes a cheap per-schema column count through our catalog queries.
const OVERVIEW_SCAN_CAP: usize = 60;

pub async fn overview(db: &dyn Db, scope: &Scope) -> Res<Overview> {
    let tables = list_tables(db, scope).await?;
    let mut totals = Totals::default();
    let mut scanned = 0usize;
    for t in &tables {
        if t.kind == "view" {
            totals.views += 1;
        } else {
            totals.tables += 1;
        }
        totals.rows += t.row_count.unwrap_or(0);
        totals.size_bytes += t.size_bytes.unwrap_or(0);
        if scanned < OVERVIEW_SCAN_CAP && t.kind != "view" {
            let ctx = ctx_for(db, &scope.database, &scope.schema, &t.name)?;
            if let Ok(c) = columns(db, &ctx).await {
                totals.columns += c.len();
            }
            scanned += 1;
        }
    }
    let mut by_rows = tables.clone();
    by_rows.sort_by(|a, b| b.row_count.unwrap_or(-1).cmp(&a.row_count.unwrap_or(-1)));
    let mut by_size = tables.clone();
    by_size.sort_by(|a, b| b.size_bytes.unwrap_or(0).cmp(&a.size_bytes.unwrap_or(0)));
    Ok(Overview {
        driver: db.dialect().name().to_string(),
        database: scope.database.clone(),
        schema: scope.schema.clone(),
        totals,
        top: by_rows.into_iter().take(10).collect(),
        biggest: by_size.into_iter().take(10).collect(),
    })
}

const MAX_EXPORT_ROWS: u64 = 100_000;

pub async fn export_csv(db: &dyn Db, req: &PageReq, path: &str) -> Res<ExportResult> {
    let d = db.dialect();
    let ctx = ctx_for(db, &req.database, &req.schema, &req.table)?;
    let cols = columns(db, &ctx).await?;
    let select_list = cols.iter().map(|c| q(d, &c.name)).collect::<Res<Vec<_>>>()?.join(", ");
    let tbl = d.table_name(&ctx);
    let types = type_map(&cols);
    let (where_sql, args) = build_where(d, &req.filters, &types)?;
    let order = build_order(d, &req.sort)?;
    let text = format!("SELECT {select_list} FROM {tbl}{where_sql}{order} {}", d.paging(MAX_EXPORT_ROWS, 0));
    let g = db.query(&text, &args).await?;
    let mut buf = String::new();
    buf.push_str(&cols.iter().map(|c| csv_field(&c.name)).collect::<Vec<_>>().join(","));
    buf.push('\n');
    for row in &g.rows {
        buf.push_str(&row.iter().map(csv_value).collect::<Vec<_>>().join(","));
        buf.push('\n');
    }
    tokio::fs::write(path, buf).await.map_err(err)?;
    Ok(ExportResult { path: path.to_string(), rows: g.rows.len() })
}

fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn csv_value(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => csv_field(s),
        Value::Object(o) if o.get("__bin").is_some() => csv_field(&o.get("hex").and_then(|h| h.as_str()).unwrap_or("")),
        other => csv_field(&other.to_string()),
    }
}

/* --------------------------------- targeting --------------------------------- */

/// Point a live session at the catalog the tree currently shows.
pub async fn ensure_target(db: &dyn Db, base: &ConnCfg, database: &str, schema: &str) -> Res<()> {
    if database.trim().is_empty() && schema.trim().is_empty() {
        return Ok(());
    }
    let mut cfg = base.clone();
    if !database.trim().is_empty() {
        cfg.database = database.trim().to_string();
    }
    if !schema.trim().is_empty() {
        cfg.schema = schema.trim().to_string();
    }
    db.set_target(&cfg).await
}
