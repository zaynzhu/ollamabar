// SQLite 单写者：轮询结果全部经 mpsc 汇入唯一写者任务（见 poller），本类型非 Sync 无妨
use crate::types::{ModelStat, Sample};
use chrono::Utc;
use rusqlite::Connection;

#[derive(Debug, Clone)]
pub struct SampleRow {
    pub alias: String, pub fetched_at: String,
    pub session_pct: f64, pub weekly_pct: f64,
    pub session_models: Vec<ModelStat>, pub weekly_models: Vec<ModelStat>,
    pub server_time: Option<String>,
}

impl From<SampleRow> for Sample {
    fn from(r: SampleRow) -> Sample {
        Sample { ts: r.fetched_at, session_pct: r.session_pct,
                 weekly_pct: r.weekly_pct, session_models: r.session_models }
    }
}

pub enum StoreMsg {
    Sample(SampleRow),
    ResetEvent { alias: String, observed_at: String, kind: String },
}

pub struct Store { pub conn: Connection }

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS samples (
  id INTEGER PRIMARY KEY,
  alias TEXT NOT NULL,
  fetched_at TEXT NOT NULL,
  session_pct REAL, weekly_pct REAL,
  session_models_json TEXT, weekly_models_json TEXT,
  server_time TEXT
);
CREATE INDEX IF NOT EXISTS idx_samples_alias_ts ON samples(alias, fetched_at);
CREATE TABLE IF NOT EXISTS reset_events (
  id INTEGER PRIMARY KEY,
  alias TEXT NOT NULL,
  observed_at TEXT NOT NULL,
  kind TEXT NOT NULL
);
";

impl Store {
    pub fn open(path: &str) -> rusqlite::Result<Store> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Store { conn })
    }

    pub fn open_readonly(path: &str) -> rusqlite::Result<Store> {
        let conn = Connection::open_with_flags(
            path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        Ok(Store { conn })
    }

    pub fn open_in_memory() -> rusqlite::Result<Store> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Ok(Store { conn })
    }

    pub fn insert_sample(&mut self, r: &SampleRow) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO samples (alias, fetched_at, session_pct, weekly_pct, session_models_json, weekly_models_json, server_time)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![r.alias, r.fetched_at, r.session_pct, r.weekly_pct,
                serde_json::to_string(&r.session_models).unwrap_or_default(),
                serde_json::to_string(&r.weekly_models).unwrap_or_default(),
                r.server_time],
        )?;
        Ok(())
    }

    pub fn insert_reset_event(&mut self, alias: &str, observed_at: &str, kind: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO reset_events (alias, observed_at, kind) VALUES (?1, ?2, ?3)",
            rusqlite::params![alias, observed_at, kind])?;
        Ok(())
    }

    pub fn history(&self, alias: &str, hours: i64) -> rusqlite::Result<Vec<SampleRow>> {
        let cutoff = (Utc::now() - chrono::Duration::hours(hours)).to_rfc3339();
        let mut stmt = self.conn.prepare(
            "SELECT fetched_at, session_pct, weekly_pct, session_models_json, server_time
               FROM samples WHERE alias=?1 AND fetched_at >= ?2 ORDER BY fetched_at ASC")?;
        let rows = stmt.query_map(rusqlite::params![alias, cutoff], |r| {
            let models_json: String = r.get(3)?;
            Ok(SampleRow {
                alias: alias.into(), fetched_at: r.get(0)?,
                session_pct: r.get(1)?, weekly_pct: r.get(2)?,
                session_models: serde_json::from_str(&models_json).unwrap_or_default(),
                weekly_models: vec![], server_time: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ModelStat;
    use chrono::Utc;

    fn row(alias: &str, ts: &str, pct: f64) -> SampleRow {
        SampleRow {
            alias: alias.into(), fetched_at: ts.into(),
            session_pct: pct, weekly_pct: pct * 2.0,
            session_models: vec![ModelStat { name: "m".into(), request_count: 1 }],
            weekly_models: vec![], server_time: None,
        }
    }

    #[test]
    fn 读写往返与时间过滤() {
        let mut store = Store::open_in_memory().unwrap();
        // 相对当前时间生成时间戳，避免固定日期过期后假失败（控制器裁定）
        let t0 = (Utc::now() - chrono::Duration::hours(3)).to_rfc3339();
        let t1 = (Utc::now() - chrono::Duration::hours(2)).to_rfc3339();
        store.insert_sample(&row("k1", &t0, 5.0)).unwrap();
        store.insert_sample(&row("k1", &t1, 6.0)).unwrap();
        store.insert_sample(&row("k2", &t1, 1.0)).unwrap();
        let h = store.history("k1", 24).unwrap();
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].fetched_at, t0); // 升序
        let h2 = store.history("k2", 24).unwrap();
        assert_eq!(h2.len(), 1);
    }

    #[test]
    fn 重置事件入库() {
        let mut store = Store::open_in_memory().unwrap();
        store.insert_reset_event("k1", "2026-09-15T05:00:00Z", "session").unwrap();
        let n: i64 = store.conn.query_row(
            "SELECT COUNT(*) FROM reset_events WHERE alias='k1' AND kind='session'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }
}