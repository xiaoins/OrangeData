use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use serde_json::{json, Value};
use sqlx::types::{Decimal, Json, Uuid};
use sqlx::{Column, Row, TypeInfo, ValueRef};

/// Turns one driver row into JSON values. sqlx decodes only into a concrete Rust
/// type, and which type is valid depends on the column, so each cell is probed in
/// order. High-precision numbers become strings to survive JSON round-tripping.
///
/// `$uint` and `$dec` are per-driver helper calls because the probe must compile:
/// Postgres has no unsigned integer to decode into `u64`, SQLite has no decimal.
macro_rules! impl_row_json {
    ($cells:ident, $row_json:ident, $uint:ident, $dec:ident, $rowty:ty) => {
        pub fn $cells(row: &$rowty, i: usize, ty: &str) -> Value {
            if let Ok(raw) = row.try_get_raw(i) {
                if raw.is_null() {
                    return Value::Null;
                }
            }
            let lower = ty.to_ascii_lowercase();
            let binary_ty = lower.contains("blob") || lower.contains("binary") || lower.contains("bytea");
            if binary_ty {
                if let Ok(Some(v)) = row.try_get::<Option<Vec<u8>>, _>(i) {
                    return binary_cell(&v);
                }
            }
            if lower.contains("bool") {
                if let Ok(Some(v)) = row.try_get::<Option<bool>, _>(i) {
                    return json!(v);
                }
            }
            if let Ok(Some(v)) = row.try_get::<Option<i64>, _>(i) {
                return json!(v);
            }
            if let Ok(Some(v)) = row.try_get::<Option<i32>, _>(i) {
                return json!(v);
            }
            if let Ok(Some(v)) = row.try_get::<Option<i16>, _>(i) {
                return json!(v as i64);
            }
            if let Ok(Some(v)) = row.try_get::<Option<i8>, _>(i) {
                return json!(v as i64);
            }
            if let Some(v) = $uint(row, i) {
                return v;
            }
            if let Ok(Some(v)) = row.try_get::<Option<f64>, _>(i) {
                return json!(v);
            }
            if let Ok(Some(v)) = row.try_get::<Option<f32>, _>(i) {
                return json!(v as f64);
            }
            if let Some(v) = $dec(row, i) {
                return v;
            }
            if let Ok(Some(v)) = row.try_get::<Option<NaiveDateTime>, _>(i) {
                return json!(v.format("%Y-%m-%d %H:%M:%S%.f").to_string());
            }
            if let Ok(Some(v)) = row.try_get::<Option<DateTime<Utc>>, _>(i) {
                return json!(v.to_rfc3339());
            }
            if let Ok(Some(v)) = row.try_get::<Option<DateTime<Local>>, _>(i) {
                return json!(v.to_rfc3339());
            }
            if let Ok(Some(v)) = row.try_get::<Option<NaiveDate>, _>(i) {
                return json!(v.format("%Y-%m-%d").to_string());
            }
            if let Ok(Some(v)) = row.try_get::<Option<NaiveTime>, _>(i) {
                return json!(v.format("%H:%M:%S%.f").to_string());
            }
            if let Ok(Some(v)) = row.try_get::<Option<Json<Value>>, _>(i) {
                return v.0;
            }
            if let Ok(Some(v)) = row.try_get::<Option<Uuid>, _>(i) {
                return json!(v.to_string());
            }
            if let Ok(Some(v)) = row.try_get::<Option<String>, _>(i) {
                return json!(v);
            }
            if let Ok(Some(v)) = row.try_get::<Option<Vec<u8>>, _>(i) {
                return binary_cell(&v);
            }
            json!(format!("‹{ty} 未解码›"))
        }

        pub fn $row_json(row: &$rowty) -> Vec<Value> {
            row.columns()
                .iter()
                .map(|c| $cells(row, c.ordinal(), c.type_info().name()))
                .collect()
        }
    };
}

fn binary_cell(bytes: &[u8]) -> Value {
    let hex = bytes
        .iter()
        .take(256)
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ");
    json!({
        "__bin": true,
        "bytes": bytes.len(),
        "hex": hex,
        "base64": B64.encode(bytes),
    })
}

fn sqlite_uint(row: &sqlx::sqlite::SqliteRow, i: usize) -> Option<Value> {
    if let Ok(Some(v)) = row.try_get::<Option<u64>, _>(i) {
        return Some(json!(v.to_string()));
    }
    None
}

fn sqlite_dec(_row: &sqlx::sqlite::SqliteRow, _i: usize) -> Option<Value> {
    None
}

fn mysql_uint(row: &sqlx::mysql::MySqlRow, i: usize) -> Option<Value> {
    if let Ok(Some(v)) = row.try_get::<Option<u64>, _>(i) {
        return Some(json!(v.to_string()));
    }
    None
}

fn mysql_dec(row: &sqlx::mysql::MySqlRow, i: usize) -> Option<Value> {
    if let Ok(Some(v)) = row.try_get::<Option<Decimal>, _>(i) {
        return Some(json!(v.normalize().to_string()));
    }
    None
}

fn pg_uint(_row: &sqlx::postgres::PgRow, _i: usize) -> Option<Value> {
    None
}

fn pg_dec(row: &sqlx::postgres::PgRow, i: usize) -> Option<Value> {
    if let Ok(Some(v)) = row.try_get::<Option<Decimal>, _>(i) {
        return Some(json!(v.normalize().to_string()));
    }
    None
}

impl_row_json!(sqlite_cells, sqlite_row_json, sqlite_uint, sqlite_dec, sqlx::sqlite::SqliteRow);
impl_row_json!(mysql_cells, mysql_row_json, mysql_uint, mysql_dec, sqlx::mysql::MySqlRow);
impl_row_json!(pg_cells, pg_row_json, pg_uint, pg_dec, sqlx::postgres::PgRow);
