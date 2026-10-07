//! JSON persistence for connection profiles and SQL history, stored in the
//! application data directory next to the executable's own state.

use crate::model::{ConnCfg, Res};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

const HISTORY_CAP: usize = 200;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HistoryItem {
    pub sql: String,
    pub driver: String,
    pub database: String,
    pub at: String,
    pub ok: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Snapshot {
    pub profiles: Vec<ConnCfg>,
    pub history: Vec<HistoryItem>,
}

pub struct Repo {
    path: PathBuf,
    cache: Mutex<Snapshot>,
}

impl Repo {
    pub fn new(dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        Self { path: dir.join("connections.json"), cache: Mutex::new(Snapshot::default()) }
    }

    /// Load once at startup and keep the snapshot in memory afterwards.
    pub fn init(&self) {
        let loaded = std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|s| serde_json::from_str::<Snapshot>(&s).ok())
            .unwrap_or_default();
        if let Ok(mut c) = self.cache.lock() {
            *c = loaded;
        }
    }

    fn flush(&self) -> Res<()> {
        let json = {
            let guard = self.cache.lock().map_err(|_| "状态被污染".to_string())?;
            serde_json::to_string_pretty(&*guard).map_err(crate::model::err)?
        };
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(crate::model::err)?;
        let _ = std::fs::remove_file(&self.path);
        std::fs::rename(&tmp, &self.path).map_err(crate::model::err)?;
        Ok(())
    }

    pub fn profiles(&self) -> Vec<ConnCfg> {
        self.cache.lock().map(|c| c.profiles.clone()).unwrap_or_default()
    }

    pub fn upsert(&self, mut cfg: ConnCfg) -> Res<ConnCfg> {
        let now = chrono::Local::now().to_rfc3339();
        if cfg.id.trim().is_empty() {
            cfg.id = format!("{}-{}", now.replace([':', '.'], "-"), rand_suffix());
        }
        if cfg.created_at.is_empty() {
            cfg.created_at = now.clone();
        }
        cfg.updated_at = now;
        let mut guard = self.cache.lock().map_err(|_| "状态被污染".to_string())?;
        match guard.profiles.iter().position(|p| p.id == cfg.id) {
            Some(i) => guard.profiles[i] = cfg.clone(),
            None => guard.profiles.push(cfg.clone()),
        }
        drop(guard);
        self.flush()?;
        Ok(cfg)
    }

    pub fn remove(&self, id: &str) -> Res<bool> {
        let mut guard = self.cache.lock().map_err(|_| "状态被污染".to_string())?;
        let before = guard.profiles.len();
        guard.profiles.retain(|p| p.id != id);
        let removed = guard.profiles.len() != before;
        drop(guard);
        if removed {
            self.flush()?;
        }
        Ok(removed)
    }

    pub fn history(&self) -> Vec<HistoryItem> {
        self.cache.lock().map(|c| c.history.clone()).unwrap_or_default()
    }

    /// Record executed SQL; consecutive duplicates collapse into one entry.
    pub fn remember(&self, sql: &str, driver: &str, database: &str, ok: bool) -> Res<()> {
        let sql = sql.trim().to_string();
        if sql.is_empty() {
            return Ok(());
        }
        {
            let mut guard = self.cache.lock().map_err(|_| "状态被污染".to_string())?;
            if guard.history.first().map(|h| h.sql == sql).unwrap_or(false) {
                return Ok(());
            }
            guard.history.retain(|h| h.sql != sql);
            guard.history.insert(0, HistoryItem { sql, driver: driver.to_string(), database: database.to_string(), at: chrono::Local::now().to_rfc3339(), ok });
            guard.history.truncate(HISTORY_CAP);
        }
        self.flush()
    }

    pub fn clear_history(&self) -> Res<()> {
        {
            let mut guard = self.cache.lock().map_err(|_| "状态被污染".to_string())?;
            guard.history.clear();
        }
        self.flush()
    }
}

fn rand_suffix() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("{:04x}", nanos & 0xffff)
}
