//! The IPC surface. Every command follows the same shape: resolve the live
//! session by profile id, point it at the catalog the UI is showing, then hand
//! the work to the engine-agnostic `db::api` layer.

use crate::db::{api, engine::open, Registry};
use crate::model::{
    About, ConnCfg, CreateTableReq, EditReq, ExportResult, Grid, Node, OpResult, Overview, PageReq,
    Res, SchemaMap, Scope, ScriptReq, ScriptResult, TableInfo, TableMeta, TableRef,
};
use crate::store::Repo;
use crate::chrome;
use std::sync::Arc;
use tauri::{State, WebviewWindow};

pub struct AppState {
    pub registry: Registry,
    pub repo: Repo,
}

/// Resolve a session and make sure it is looking at the requested catalog.
async fn session(state: &AppState, id: &str, database: &str, schema: &str) -> Res<Arc<dyn crate::db::engine::Db>> {
    let (cfg, db) = state.registry.entry(id)?;
    api::ensure_target(&*db, &cfg, database, schema).await?;
    Ok(db)
}

/* ------------------------------- profiles ------------------------------- */

#[tauri::command]
pub fn profile_list(state: State<'_, AppState>) -> Vec<ConnCfg> {
    state.repo.profiles()
}

#[tauri::command]
pub fn profile_save(state: State<'_, AppState>, cfg: ConnCfg) -> Res<ConnCfg> {
    state.repo.upsert(cfg)
}

#[tauri::command]
pub fn profile_delete(state: State<'_, AppState>, id: String) -> Res<bool> {
    state.repo.remove(&id)
}

#[tauri::command]
pub fn history_list(state: State<'_, AppState>) -> Vec<crate::store::HistoryItem> {
    state.repo.history()
}

#[tauri::command]
pub fn history_clear(state: State<'_, AppState>) -> Res<()> {
    state.repo.clear_history()
}

/* ----------------------------- connections ----------------------------- */

/// Connect (or reuse an existing session) and return what the engine reported.
#[tauri::command]
pub async fn conn_connect(state: State<'_, AppState>, cfg: ConnCfg) -> Res<About> {
    let mut cfg = cfg;
    if cfg.id.trim().is_empty() {
        cfg.id = format!("tmp-{}", chrono::Local::now().timestamp_millis());
    }
    if state.registry.is_open(&cfg.id) {
        let db = state.registry.get(&cfg.id)?;
        let scope = Scope { connection_id: cfg.id.clone(), database: cfg.database.clone(), schema: cfg.schema.clone() };
        return api::about(&*db, &scope).await;
    }
    let db = open(&cfg).await?;
    let scope = Scope { connection_id: cfg.id.clone(), database: cfg.database.clone(), schema: cfg.schema.clone() };
    let about = api::about(&*db, &scope).await?;
    state.registry.put(cfg, db);
    Ok(about)
}

/// Validate credentials without keeping the session around.
#[tauri::command]
pub async fn conn_test(cfg: ConnCfg) -> Res<About> {
    let mut probe = cfg;
    if probe.id.trim().is_empty() {
        probe.id = "probe".to_string();
    }
    let db = open(&probe).await?;
    let scope = Scope { connection_id: probe.id.clone(), database: probe.database.clone(), schema: probe.schema.clone() };
    let about = api::about(&*db, &scope).await;
    db.close().await;
    about
}

#[tauri::command]
pub async fn conn_close(state: State<'_, AppState>, id: String) -> Res<bool> {
    Ok(state.registry.close(&id).await)
}

#[tauri::command]
pub fn conn_open(state: State<'_, AppState>) -> Vec<String> {
    state.registry.open_ids()
}

/* -------------------------------- metadata -------------------------------- */

#[tauri::command]
pub async fn meta_databases(state: State<'_, AppState>, id: String) -> Res<Vec<Node>> {
    let db = session(&state, &id, "", "").await?;
    api::list_databases(&*db).await
}

#[tauri::command]
pub async fn meta_schemas(state: State<'_, AppState>, scope: Scope) -> Res<Vec<Node>> {
    let db = session(&state, &scope.connection_id, &scope.database, "").await?;
    api::list_schemas(&*db, &scope).await
}

#[tauri::command]
pub async fn meta_tables(state: State<'_, AppState>, scope: Scope) -> Res<Vec<TableInfo>> {
    let db = session(&state, &scope.connection_id, &scope.database, &scope.schema).await?;
    api::list_tables(&*db, &scope).await
}

#[tauri::command]
pub async fn meta_table(state: State<'_, AppState>, r: TableRef) -> Res<TableMeta> {
    let db = session(&state, &r.connection_id, &r.database, &r.schema).await?;
    api::table_meta(&*db, &r).await
}

/* ---------------------------------- data ---------------------------------- */

#[tauri::command]
pub async fn grid_page(state: State<'_, AppState>, req: PageReq) -> Res<Grid> {
    let db = session(&state, &req.connection_id, &req.database, &req.schema).await?;
    api::fetch_page(&*db, &req).await
}

#[tauri::command]
pub async fn script_run(state: State<'_, AppState>, req: ScriptReq) -> Res<ScriptResult> {
    let (cfg, db) = state.registry.entry(&req.connection_id)?;
    api::ensure_target(&*db, &cfg, &req.database, &req.schema).await?;
    let out = api::run_script(&*db, &req).await?;
    let last = out.statements.last();
    let _ = state.repo.remember(&req.sql, cfg.driver.trim(), &req.database, last.map(|s| s.error.is_none()).unwrap_or(true));
    Ok(out)
}

#[tauri::command]
pub async fn edits_apply(state: State<'_, AppState>, req: EditReq) -> Res<crate::model::EditResult> {
    let db = session(&state, &req.connection_id, &req.database, &req.schema).await?;
    api::apply_edits(&*db, &req).await
}

#[tauri::command]
pub async fn export_csv(state: State<'_, AppState>, req: PageReq, path: String) -> Res<ExportResult> {
    let db = session(&state, &req.connection_id, &req.database, &req.schema).await?;
    api::export_csv(&*db, &req, &path).await
}

/* ----------------------------------- ddl ----------------------------------- */

#[tauri::command]
pub async fn ddl_create_table(state: State<'_, AppState>, req: CreateTableReq) -> Res<OpResult> {
    let db = session(&state, &req.connection_id, &req.database, &req.schema).await?;
    api::create_table(&*db, &req).await
}

#[tauri::command]
pub async fn ddl_drop(state: State<'_, AppState>, r: TableRef, kind: String) -> Res<OpResult> {
    let db = session(&state, &r.connection_id, &r.database, &r.schema).await?;
    api::drop_object(&*db, &r, &kind).await
}

#[tauri::command]
pub async fn ddl_truncate(state: State<'_, AppState>, r: TableRef) -> Res<OpResult> {
    let db = session(&state, &r.connection_id, &r.database, &r.schema).await?;
    api::truncate_table(&*db, &r).await
}

/* -------------------------------- insights -------------------------------- */

#[tauri::command]
pub async fn map_schema(state: State<'_, AppState>, scope: Scope) -> Res<SchemaMap> {
    let db = session(&state, &scope.connection_id, &scope.database, &scope.schema).await?;
    api::schema_map(&*db, &scope).await
}

#[tauri::command]
pub async fn overview(state: State<'_, AppState>, scope: Scope) -> Res<Overview> {
    let db = session(&state, &scope.connection_id, &scope.database, &scope.schema).await?;
    api::overview(&*db, &scope).await
}

/* ---------------------------------- docker ---------------------------------- */

#[tauri::command]
pub async fn docker_inspect() -> Res<crate::model::DockerStatus> {
    Ok(crate::docker::inspect().await)
}

/* ---------------------------------- chrome --------------------------------- */

#[tauri::command]
pub fn chrome_apply(window: WebviewWindow, dark: bool, caption: u32, text: u32, border: u32) {
    chrome::apply(&window, dark, caption, text, border);
}

#[tauri::command]
pub fn window_title(window: WebviewWindow, text: String) {
    chrome::title(&window, &text);
}
