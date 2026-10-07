use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

pub type Res<T> = Result<T, String>;

pub fn err<E: std::fmt::Display>(e: E) -> String {
    format!("{e}")
}

/// A saved connection profile. Also used as the ad-hoc config passed to
/// `test_connection` / `open_connection`, so the UI has one shape for both.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ConnCfg {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub database: String,
    pub schema: String,
    pub file: String,
    pub ssl: String,
    pub charset: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Scope {
    pub connection_id: String,
    pub database: String,
    pub schema: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TableRef {
    pub connection_id: String,
    pub database: String,
    pub schema: String,
    pub table: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SortSpec {
    pub column: String,
    pub dir: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FilterSpec {
    pub column: String,
    pub op: String,
    pub value: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PageReq {
    pub connection_id: String,
    pub database: String,
    pub schema: String,
    pub table: String,
    pub page: u32,
    pub size: u32,
    pub sort: Vec<SortSpec>,
    pub filters: Vec<FilterSpec>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScriptReq {
    pub connection_id: String,
    pub database: String,
    pub schema: String,
    pub sql: String,
    pub limit: u32,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RowPatch {
    pub pk: HashMap<String, Value>,
    pub set: HashMap<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RowValues {
    pub values: HashMap<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EditReq {
    pub connection_id: String,
    pub database: String,
    pub schema: String,
    pub table: String,
    pub updates: Vec<RowPatch>,
    pub inserts: Vec<RowValues>,
    pub deletes: Vec<RowValues>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ColDef {
    pub name: String,
    #[serde(rename = "type")]
    pub col_type: String,
    pub nullable: bool,
    pub primary_key: bool,
    pub auto_increment: bool,
    pub default_value: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CreateTableReq {
    pub connection_id: String,
    pub database: String,
    pub schema: String,
    pub name: String,
    pub columns: Vec<ColDef>,
}

/* ---------------------------------- responses ---------------------------------- */

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct About {
    pub driver: String,
    pub version: Option<String>,
    pub database: String,
    pub schema: String,
    pub host: String,
    pub connected: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub name: String,
    pub comment: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableInfo {
    pub name: String,
    pub kind: String,
    pub row_count: Option<i64>,
    pub size_bytes: Option<i64>,
    pub comment: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Column {
    pub name: String,
    #[serde(rename = "type")]
    pub col_type: String,
    pub nullable: bool,
    pub default_value: Option<String>,
    pub comment: Option<String>,
    pub extra: Option<String>,
    pub is_pk: bool,
    pub references: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnLite {
    pub name: String,
    #[serde(rename = "type")]
    pub col_type: String,
    pub nullable: bool,
    pub is_pk: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeignKey {
    pub name: String,
    pub columns: Vec<String>,
    pub ref_table: String,
    pub ref_columns: Vec<String>,
    pub on_delete: Option<String>,
    pub on_update: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexInfo {
    pub name: String,
    pub unique: bool,
    pub primary: bool,
    pub kind: Option<String>,
    pub columns: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableMeta {
    pub table: String,
    pub columns: Vec<Column>,
    pub primary_keys: Vec<String>,
    pub foreign_keys: Vec<ForeignKey>,
    pub indexes: Vec<IndexInfo>,
    pub ddl: String,
    pub editable: bool,
    pub row_count: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Grid {
    pub table: String,
    pub columns: Vec<String>,
    pub column_meta: Vec<ColumnLite>,
    pub primary_keys: Vec<String>,
    pub editable: bool,
    pub rows: Vec<Vec<Value>>,
    pub page: u32,
    pub size: u32,
    pub total: Option<i64>,
    pub ms: u128,
    pub sql: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StmtResult {
    pub index: usize,
    pub sql: String,
    pub is_query: bool,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    pub affected: Option<i64>,
    pub ms: u128,
    pub error: Option<String>,
    pub truncated: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptResult {
    pub statements: Vec<StmtResult>,
    pub ok: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditItem {
    pub kind: String,
    pub sql: String,
    pub ok: bool,
    pub affected: Option<i64>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditResult {
    pub table: String,
    pub applied: usize,
    pub items: Vec<EditItem>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub tables: usize,
    pub views: usize,
    pub rows: i64,
    pub size_bytes: i64,
    pub columns: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub driver: String,
    pub database: String,
    pub schema: String,
    pub totals: Totals,
    pub top: Vec<TableInfo>,
    pub biggest: Vec<TableInfo>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapNode {
    pub name: String,
    pub kind: String,
    pub row_count: Option<i64>,
    pub columns: Vec<ColumnLite>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapEdge {
    pub from: String,
    pub to: String,
    pub name: String,
    pub columns: Vec<String>,
    pub ref_columns: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaMap {
    pub nodes: Vec<MapNode>,
    pub edges: Vec<MapEdge>,
    pub truncated: bool,
    pub total_tables: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpResult {
    pub ok: bool,
    pub sql: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub path: String,
    pub rows: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerPick {
    pub container_port: u16,
    pub host: String,
    pub host_port: u16,
    pub driver: Option<String>,
    pub user: Option<String>,
    pub password: Option<String>,
    pub database: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerContainer {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub picks: Vec<DockerPick>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerStatus {
    pub installed: bool,
    pub reachable: bool,
    pub version: Option<String>,
    pub error: Option<String>,
    pub containers: Vec<DockerContainer>,
}

