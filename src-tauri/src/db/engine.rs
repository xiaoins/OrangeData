use super::dialect::{to_dollar, Ctx, Dialect, Sql};
use super::dialect_sqlite::SqliteDialect;
use super::mysql_dialect::MySqlDialect;
use super::pg_dialect::PgDialect;
use super::values::{mysql_row_json, pg_row_json, sqlite_row_json};
use crate::model::{err, ConnCfg, Res};
use async_trait::async_trait;
use serde_json::Value;
use sqlx::mysql::{MySqlConnectOptions, MySqlConnection, MySqlSslMode};
use sqlx::postgres::{PgConnectOptions, PgConnection, PgSslMode};
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection};
use sqlx::{Column, Connection, Encode, Executor, Row, Statement, Type};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

/// A result set ready for the UI. Values are already JSON-safe (see values.rs).
#[derive(Debug, Default)]
pub struct RawGrid {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

/// One live connection per saved profile. A single connection (rather than a
/// pool) keeps session state such as PostgreSQL `search_path` predictable, and
/// the async mutex serialises concurrent UI calls.
#[async_trait]
pub trait Db: Send + Sync {
    fn dialect(&self) -> &dyn Dialect;
    fn ctx(&self) -> Ctx;
    fn host_label(&self) -> String;
    async fn query(&self, sql: &str, args: &[String]) -> Res<RawGrid>;
    async fn execute(&self, sql: &str, args: &[String]) -> Res<u64>;
    /// Run statements atomically; rolls back on the first failure.
    async fn tx(&self, stmts: Vec<Sql>) -> Res<Vec<Res<u64>>>;
    /// Re-point the session at another catalog (MySQL database, PG schema,
    /// SQLite file) — reconnecting when the engine requires it.
    async fn set_target(&self, cfg: &ConnCfg) -> Res<()>;
    async fn close(&self);
}

fn bind_all<'q, DB>(
    mut q: sqlx::query::Query<'q, DB, <DB as sqlx::Database>::Arguments<'q>>,
    args: &'q [String],
) -> sqlx::query::Query<'q, DB, <DB as sqlx::Database>::Arguments<'q>>
where
    DB: sqlx::Database,
    for<'a> &'a str: Encode<'a, DB> + Type<DB>,
{
    for a in args {
        q = q.bind(a.as_str());
    }
    q
}

/// MySQL refuses `USE` in the prepared-statement protocol (error 1295), so a
/// worksheet `USE <catalog>;` is recognised here and applied by re-pointing the
/// session at that catalog instead of sending the statement to the server.
fn use_target(sql: &str) -> Option<String> {
    let mut words = sql.trim().split_whitespace();
    let verb = words.next()?.to_ascii_lowercase();
    let arg = words.next()?;
    if verb != "use" || words.next().is_some() {
        return None;
    }
    let name = arg.trim_end_matches(';').trim_matches('`').trim();
    (!name.is_empty()).then(|| name.to_string())
}

pub struct SqliteDb {
    conn: tokio::sync::Mutex<SqliteConnection>,
    /// Attached schema currently browsed; "main" for the opened file.
    schema: std::sync::Mutex<String>,
    file: std::sync::Mutex<String>,
}

impl SqliteDb {
    pub async fn connect(cfg: &ConnCfg) -> Res<Self> {
        let file = cfg.file.trim().to_string();
        if file.is_empty() {
            return Err("请选择 SQLite 数据库文件".to_string());
        }
        let conn = Self::open(&Self::options(&file)?).await?;
        Ok(Self {
            conn: tokio::sync::Mutex::new(conn),
            schema: std::sync::Mutex::new("main".into()),
            file: std::sync::Mutex::new(file),
        })
    }

    fn current_file(&self) -> String {
        self.file.lock().map(|f| f.clone()).unwrap_or_default()
    }

    /// An absent file becomes a new empty database, but only when the name looks
    /// like a SQLite file and its folder exists — that still catches mistyped paths.
    fn options(file: &str) -> Res<SqliteConnectOptions> {
        let path = Path::new(file);
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if !matches!(ext.as_str(), "db" | "sqlite" | "sqlite3" | "db3") {
            return Err(format!("建议使用 .db / .sqlite / .sqlite3 后缀：{file}"));
        }
        let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
        if !parent.is_dir() {
            return Err(format!("目录不存在：{}", parent.display()));
        }
        let fresh = !path.exists();
        Ok(SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(fresh)
            .busy_timeout(Duration::from_secs(15)))
    }

    async fn open(opts: &SqliteConnectOptions) -> Res<SqliteConnection> {
        tokio::time::timeout(Duration::from_secs(20), SqliteConnection::connect_with(opts))
            .await
            .map_err(|_| "连接超时：SQLite 文件无法打开".to_string())?
            .map_err(err)
    }

    async fn read(conn: &mut SqliteConnection, text: &str, args: &[String]) -> Res<RawGrid> {
        let grid = bind_all(sqlx::query(text), args).fetch_all(&mut *conn).await.map_err(err)?;
        let mut columns: Vec<String> = Vec::new();
        if let Some(r) = grid.first() {
            columns = r.columns().iter().map(|c| c.name().to_string()).collect();
        } else if let Ok(st) = (&mut *conn).prepare(text).await {
            columns = st.columns().iter().map(|c| c.name().to_string()).collect();
        }
        Ok(RawGrid { columns, rows: grid.iter().map(sqlite_row_json).collect() })
    }
}

#[async_trait]
impl Db for SqliteDb {
    fn dialect(&self) -> &dyn Dialect {
        &SqliteDialect
    }
    fn ctx(&self) -> Ctx {
        let db = self
            .schema
            .lock()
            .map(|s| s.clone())
            .unwrap_or_else(|_| "main".into());
        Ctx { database: db, schema: String::new(), table: String::new() }
    }
    fn host_label(&self) -> String {
        self.current_file()
    }
    async fn query(&self, sql: &str, args: &[String]) -> Res<RawGrid> {
        let mut guard = self.conn.lock().await;
        SqliteDb::read(&mut guard, sql, args).await
    }
    async fn execute(&self, sql: &str, args: &[String]) -> Res<u64> {
        let mut guard = self.conn.lock().await;
        let r = bind_all(sqlx::query(sql), args).execute(&mut *guard).await.map_err(err)?;
        Ok(r.rows_affected())
    }
    async fn tx(&self, stmts: Vec<Sql>) -> Res<Vec<Res<u64>>> {
        let mut guard = self.conn.lock().await;
        let mut tx = (&mut *guard).begin().await.map_err(err)?;
        let mut out: Vec<Res<u64>> = Vec::new();
        for s in &stmts {
            let bound = bind_all(sqlx::query(&s.text), &s.args);
            match bound.execute(&mut *tx).await {
                Ok(r) => out.push(Ok(r.rows_affected())),
                Err(e) => {
                    out.push(Err(err(e)));
                    let _ = tx.rollback().await;
                    return Ok(out);
                }
            }
        }
        tx.commit().await.map_err(err)?;
        Ok(out)
    }
    async fn set_target(&self, cfg: &ConnCfg) -> Res<()> {
        let file = cfg.file.trim().to_string();
        if !file.is_empty() && file != self.current_file() {
            let conn = SqliteDb::open(&SqliteDb::options(&file)?).await?;
            let mut guard = self.conn.lock().await;
            *guard = conn;
            if let Ok(mut s) = self.schema.lock() {
                *s = "main".into();
            }
            if let Ok(mut f) = self.file.lock() {
                *f = file;
            }
        }
        if !cfg.database.is_empty() {
            if let Ok(mut s) = self.schema.lock() {
                *s = cfg.database.clone();
            }
        }
        Ok(())
    }
    async fn close(&self) {
        // Connection::close consumes self, which cannot move out of the guard. The
        // socket or file is released by Drop once the registry drops the last Arc;
        // acquiring the lock here is what keeps that safe against in-flight work.
        let _guard = self.conn.lock().await;
    }
}

pub struct MySqlDb {
    conn: tokio::sync::Mutex<MySqlConnection>,
    cfg: StdMutex<ConnCfg>,
}

impl MySqlDb {
    pub async fn connect(cfg: &ConnCfg) -> Res<Self> {
        let conn = Self::open(&Self::options(cfg)).await?;
        Ok(Self { conn: tokio::sync::Mutex::new(conn), cfg: StdMutex::new(cfg.clone()) })
    }

    fn options(cfg: &ConnCfg) -> MySqlConnectOptions {
        let host = if cfg.host.trim().is_empty() { "127.0.0.1" } else { cfg.host.trim() };
        let user = if cfg.user.trim().is_empty() { "root" } else { cfg.user.trim() };
        let mut o = MySqlConnectOptions::new()
            .host(host)
            .port(if cfg.port == 0 { 3306 } else { cfg.port })
            .username(user)
            .password(&cfg.password)
            .ssl_mode(match cfg.ssl.trim().to_lowercase().as_str() {
                "required" | "require" | "true" | "1" => MySqlSslMode::Required,
                "preferred" | "prefer" => MySqlSslMode::Preferred,
                "verify-ca" => MySqlSslMode::VerifyCa,
                "verify_identity" | "verify_identity_required" | "verify-full" => MySqlSslMode::VerifyIdentity,
                _ => MySqlSslMode::Disabled,
            });
        if !cfg.database.trim().is_empty() {
            o = o.database(cfg.database.trim());
        }
        if !cfg.charset.trim().is_empty() {
            o = o.charset(&cfg.charset);
        }
        o
    }

    async fn open(o: &MySqlConnectOptions) -> Res<MySqlConnection> {
        tokio::time::timeout(Duration::from_secs(20), MySqlConnection::connect_with(o))
            .await
            .map_err(|_| "连接超时：MySQL 服务不可达".to_string())?
            .map_err(err)
    }

    async fn read(conn: &mut MySqlConnection, text: &str, args: &[String]) -> Res<RawGrid> {
        let rows = bind_all(sqlx::query(text), args).fetch_all(&mut *conn).await.map_err(err)?;
        let columns = match rows.first() {
            Some(r) => r.columns().iter().map(|c| c.name().to_string()).collect(),
            None => Vec::new(),
        };
        Ok(RawGrid { columns, rows: rows.iter().map(mysql_row_json).collect() })
    }

    fn current(&self) -> ConnCfg {
        self.cfg.lock().map(|c| c.clone()).unwrap_or_default()
    }
}

#[async_trait]
impl Db for MySqlDb {
    fn dialect(&self) -> &dyn Dialect {
        &MySqlDialect
    }
    fn ctx(&self) -> Ctx {
        let c = self.current();
        Ctx { database: c.database, schema: String::new(), table: String::new() }
    }
    fn host_label(&self) -> String {
        let c = self.current();
        format!("{}:{}", if c.host.is_empty() { "127.0.0.1" } else { &c.host }, if c.port == 0 { 3306 } else { c.port })
    }
    async fn query(&self, sql: &str, args: &[String]) -> Res<RawGrid> {
        let mut guard = self.conn.lock().await;
        MySqlDb::read(&mut guard, sql, args).await
    }
    async fn execute(&self, sql: &str, args: &[String]) -> Res<u64> {
        if args.is_empty() {
            if let Some(name) = use_target(sql) {
                let mut want = self.current();
                want.database = name;
                // Resolved before the guard exists: set_target locks the connection itself.
                return self.set_target(&want).await.map(|_| 0);
            }
        }
        let mut guard = self.conn.lock().await;
        let r = bind_all(sqlx::query(sql), args).execute(&mut *guard).await.map_err(err)?;
        Ok(r.rows_affected())
    }
    async fn tx(&self, stmts: Vec<Sql>) -> Res<Vec<Res<u64>>> {
        let mut guard = self.conn.lock().await;
        let mut tx = (&mut *guard).begin().await.map_err(err)?;
        let mut out: Vec<Res<u64>> = Vec::new();
        for s in &stmts {
            let bound = bind_all(sqlx::query(&s.text), &s.args);
            match bound.execute(&mut *tx).await {
                Ok(r) => out.push(Ok(r.rows_affected())),
                Err(e) => {
                    out.push(Err(err(e)));
                    let _ = tx.rollback().await;
                    return Ok(out);
                }
            }
        }
        tx.commit().await.map_err(err)?;
        Ok(out)
    }
    async fn set_target(&self, cfg: &ConnCfg) -> Res<()> {
        let want = if cfg.database.trim().is_empty() { self.current() } else { cfg.clone() };
        if want.database == self.current().database {
            return Ok(());
        }
        let conn = MySqlDb::open(&MySqlDb::options(&want)).await?;
        let mut guard = self.conn.lock().await;
        *guard = conn;
        if let Ok(mut c) = self.cfg.lock() {
            *c = want;
        }
        Ok(())
    }
    async fn close(&self) {
        // Connection::close consumes self, which cannot move out of the guard. The
        // socket or file is released by Drop once the registry drops the last Arc;
        // acquiring the lock here is what keeps that safe against in-flight work.
        let _guard = self.conn.lock().await;
    }
}

pub struct PgDb {
    conn: tokio::sync::Mutex<PgConnection>,
    cfg: StdMutex<ConnCfg>,
}

impl PgDb {
    pub async fn connect(cfg: &ConnCfg) -> Res<Self> {
        let conn = Self::open(&Self::options(cfg)).await?;
        let me = Self { conn: tokio::sync::Mutex::new(conn), cfg: StdMutex::new(cfg.clone()) };
        if !cfg.schema.trim().is_empty() {
            let sql = PgDialect
                .set_schema_sql(cfg.schema.trim())
                .ok_or_else(|| "模式名不合法".to_string())?;
            let mut guard = me.conn.lock().await;
            sqlx::query(&sql).execute(&mut *guard).await.map_err(err)?;
        }
        Ok(me)
    }

    fn options(cfg: &ConnCfg) -> PgConnectOptions {
        let host = if cfg.host.trim().is_empty() { "127.0.0.1" } else { cfg.host.trim() };
        let mut o = PgConnectOptions::new()
            .host(host)
            .port(if cfg.port == 0 { 5432 } else { cfg.port })
            .username(if cfg.user.trim().is_empty() { "postgres" } else { cfg.user.trim() })
            .password(&cfg.password)
            .application_name("OrangeData")
            .ssl_mode(match cfg.ssl.trim().to_lowercase().as_str() {
                "require" | "required" | "true" | "1" => PgSslMode::Require,
                "preferred" | "prefer" => PgSslMode::Prefer,
                "verify-ca" => PgSslMode::VerifyCa,
                "verify-full" => PgSslMode::VerifyFull,
                _ => PgSslMode::Disable,
            });
        o = if cfg.database.trim().is_empty() { o.database("postgres") } else { o.database(cfg.database.trim()) };
        o
    }

    async fn open(o: &PgConnectOptions) -> Res<PgConnection> {
        tokio::time::timeout(Duration::from_secs(20), PgConnection::connect_with(o))
            .await
            .map_err(|_| "连接超时：PostgreSQL 服务不可达".to_string())?
            .map_err(err)
    }

    /// PostgreSQL speaks `$n`; only parameterised statements are rewritten so a
    /// literal `?` in user SQL (the JSON operators) survives untouched.
    fn text(&self, sql: &str, args: &[String]) -> String {
        if args.is_empty() { sql.to_string() } else { to_dollar(sql) }
    }

    async fn read(conn: &mut PgConnection, text: &str, args: &[String]) -> Res<RawGrid> {
        let rows = bind_all(sqlx::query(text), args).fetch_all(&mut *conn).await.map_err(err)?;
        let columns = match rows.first() {
            Some(r) => r.columns().iter().map(|c| c.name().to_string()).collect(),
            None => Vec::new(),
        };
        Ok(RawGrid { columns, rows: rows.iter().map(pg_row_json).collect() })
    }

    fn current(&self) -> ConnCfg {
        self.cfg.lock().map(|c| c.clone()).unwrap_or_default()
    }
}

#[async_trait]
impl Db for PgDb {
    fn dialect(&self) -> &dyn Dialect {
        &PgDialect
    }
    fn ctx(&self) -> Ctx {
        let c = self.current();
        let schema = if c.schema.is_empty() { "public".to_string() } else { c.schema.clone() };
        Ctx { database: c.database, schema, table: String::new() }
    }
    fn host_label(&self) -> String {
        let c = self.current();
        format!("{}:{}", if c.host.is_empty() { "127.0.0.1" } else { &c.host }, if c.port == 0 { 5432 } else { c.port })
    }
    async fn query(&self, sql: &str, args: &[String]) -> Res<RawGrid> {
        let text = self.text(sql, args);
        let mut guard = self.conn.lock().await;
        PgDb::read(&mut guard, &text, args).await
    }
    async fn execute(&self, sql: &str, args: &[String]) -> Res<u64> {
        let text = self.text(sql, args);
        let mut guard = self.conn.lock().await;
        let r = bind_all(sqlx::query(&text), args).execute(&mut *guard).await.map_err(err)?;
        Ok(r.rows_affected())
    }
    async fn tx(&self, stmts: Vec<Sql>) -> Res<Vec<Res<u64>>> {
        let owned: Vec<Sql> = stmts
            .into_iter()
            .map(|s| Sql { text: if s.args.is_empty() { s.text } else { to_dollar(&s.text) }, ..s })
            .collect();
        let mut guard = self.conn.lock().await;
        let mut tx = (&mut *guard).begin().await.map_err(err)?;
        let mut out: Vec<Res<u64>> = Vec::new();
        for s in &owned {
            let bound = bind_all(sqlx::query(&s.text), &s.args);
            match bound.execute(&mut *tx).await {
                Ok(r) => out.push(Ok(r.rows_affected())),
                Err(e) => {
                    out.push(Err(err(e)));
                    let _ = tx.rollback().await;
                    return Ok(out);
                }
            }
        }
        tx.commit().await.map_err(err)?;
        Ok(out)
    }
    async fn set_target(&self, cfg: &ConnCfg) -> Res<()> {
        let cur = self.current();
        if !cfg.database.trim().is_empty() && cfg.database.trim() != cur.database {
            let mut want = cfg.clone();
            if want.schema.is_empty() {
                want.schema = cur.schema.clone();
            }
            let conn = PgDb::open(&PgDb::options(&want)).await?;
            let mut guard = self.conn.lock().await;
            *guard = conn;
            if let Ok(mut c) = self.cfg.lock() {
                *c = want;
            }
            return Ok(());
        }
        let schema = if cfg.schema.trim().is_empty() { cur.schema.clone() } else { cfg.schema.trim().to_string() };
        if schema == cur.schema {
            return Ok(());
        }
        let sql = PgDialect
            .set_schema_sql(&schema)
            .ok_or_else(|| "模式名不合法".to_string())?;
        let mut guard = self.conn.lock().await;
        sqlx::query(&sql).execute(&mut *guard).await.map_err(err)?;
        if let Ok(mut c) = self.cfg.lock() {
            c.schema = schema;
        }
        Ok(())
    }
    async fn close(&self) {
        // Connection::close consumes self, which cannot move out of the guard. The
        // socket or file is released by Drop once the registry drops the last Arc;
        // acquiring the lock here is what keeps that safe against in-flight work.
        let _guard = self.conn.lock().await;
    }
}

/* --------------------------------- factory --------------------------------- */

pub async fn open(cfg: &ConnCfg) -> Res<Arc<dyn Db>> {
    let driver = cfg.driver.trim().to_lowercase();
    let db: Arc<dyn Db> = match driver.as_str() {
        "sqlite" | "sqlite3" => Arc::new(SqliteDb::connect(cfg).await?),
        "mysql" | "mariadb" => Arc::new(MySqlDb::connect(cfg).await?),
        "postgres" | "postgresql" | "pg" => Arc::new(PgDb::connect(cfg).await?),
        other => return Err(format!("不支持的驱动：{other}")),
    };
    Ok(db)
}

/// Live connections keyed by profile id, so the UI can address a session with a
/// short id instead of resend credentials on every call.
#[derive(Default)]
pub struct Registry {
    map: StdMutex<HashMap<String, (ConnCfg, Arc<dyn Db>)>>,
}

impl Registry {
    pub fn get(&self, id: &str) -> Res<Arc<dyn Db>> {
        self.entry(id).map(|(_, db)| db)
    }

    pub fn entry(&self, id: &str) -> Res<(ConnCfg, Arc<dyn Db>)> {
        let held = {
            match self.map.lock() {
                Ok(m) => m.get(id).cloned(),
                Err(_) => return Err("内部错误：连接表被污染".to_string()),
            }
        };
        held.ok_or_else(|| "连接尚未打开，请先在左侧连接数据库".to_string())
    }

    pub fn is_open(&self, id: &str) -> bool {
        self.map.lock().map(|m| m.contains_key(id)).unwrap_or(false)
    }

    pub fn put(&self, cfg: ConnCfg, db: Arc<dyn Db>) {
        if let Ok(mut m) = self.map.lock() {
            m.insert(cfg.id.clone(), (cfg, db));
        }
    }

    pub fn open_ids(&self) -> Vec<String> {
        self.map.lock().map(|m| m.keys().cloned().collect()).unwrap_or_default()
    }

    pub async fn close(&self, id: &str) -> bool {
        let removed = self.map.lock().map(|mut m| m.remove(id)).ok().flatten();
        match removed {
            Some((_, db)) => {
                db.close().await;
                true
            }
            None => false,
        }
    }
}
