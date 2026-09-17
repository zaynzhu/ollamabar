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
    // 失败留痕：错误/重置等稀疏事件（成功路径由 samples 承担，不重复存）
    Event { alias: String, ts: String, kind: String, message: String },
    // 保留清理：删除 N 天前的 samples 与 events（重置事件随 events 同寿）
    Cleanup { days: i64 },
    // 失败路径无库写入，仅用于驱动写者任务推送 state-changed（A7）
    StateDirty,
}

/// 详情弹窗的日志行：samples（成功）与 events（错误/重置）合并后的统一形态
#[derive(Debug, Clone, serde::Serialize)]
pub struct LogEntry {
    pub ts: String,
    pub kind: String, // ok | error | reset
    pub session_pct: Option<f64>,
    pub weekly_pct: Option<f64>,
    pub session_req: Option<i64>,
    pub weekly_req: Option<i64>,
    pub message: Option<String>,
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
CREATE TABLE IF NOT EXISTS events (
  id INTEGER PRIMARY KEY,
  alias TEXT NOT NULL,
  ts TEXT NOT NULL,
  kind TEXT NOT NULL,
  message TEXT
);
CREATE INDEX IF NOT EXISTS idx_events_alias_ts ON events(alias, ts);
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

    pub fn insert_event(&mut self, alias: &str, ts: &str, kind: &str, message: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO events (alias, ts, kind, message) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![alias, ts, kind, message])?;
        Ok(())
    }

    /// 保留清理：删除 N 天前的样本与事件，返回 (样本数, 事件数)
    pub fn delete_before(&mut self, days: i64) -> rusqlite::Result<(usize, usize)> {
        let cutoff = (Utc::now() - chrono::Duration::days(days)).to_rfc3339();
        let s = self.conn.execute("DELETE FROM samples WHERE fetched_at < ?1", [&cutoff])?;
        let e = self.conn.execute("DELETE FROM events WHERE ts < ?1", [&cutoff])?;
        // 重置事件沿用旧表，一并按 observed_at 清理
        self.conn.execute("DELETE FROM reset_events WHERE observed_at < ?1", [&cutoff])?;
        Ok((s, e))
    }

    /// 详情日志：samples（成功）+ events（错误）+ reset_events（重置）合并，时间倒序取前 limit 条
    pub fn log_rows(&self, alias: &str, limit: usize) -> rusqlite::Result<Vec<LogEntry>> {
        let mut out: Vec<LogEntry> = Vec::new();
        let mut stmt = self.conn.prepare(
            "SELECT fetched_at, session_pct, weekly_pct, session_models_json, weekly_models_json, server_time
               FROM samples WHERE alias=?1")?;
        let rows = stmt.query_map([alias], |r| {
            let session_json: String = r.get(3)?;
            let weekly_json: String = r.get(4)?;
            let sum_req = |json: &str| -> Option<i64> {
                serde_json::from_str::<Vec<ModelStat>>(json).ok()
                    .map(|ms| ms.iter().map(|m| m.request_count).sum())
            };
            Ok(LogEntry {
                ts: r.get(0)?,
                kind: "ok".into(),
                session_pct: r.get(1)?,
                weekly_pct: r.get(2)?,
                session_req: sum_req(&session_json),
                weekly_req: sum_req(&weekly_json),
                message: r.get(5)?, // server_time 借备注列展示
            })
        })?;
        for row in rows { out.push(row?); }

        let mut stmt = self.conn.prepare("SELECT ts, kind, message FROM events WHERE alias=?1")?;
        let rows = stmt.query_map([alias], |r| Ok(LogEntry {
            ts: r.get(0)?, kind: r.get(1)?,
            session_pct: None, weekly_pct: None, session_req: None, weekly_req: None,
            message: r.get(2)?,
        }))?;
        for row in rows { out.push(row?); }

        let mut stmt = self.conn.prepare("SELECT observed_at, kind FROM reset_events WHERE alias=?1")?;
        let rows = stmt.query_map([alias], |r| {
            let kind: String = r.get(1)?;
            Ok(LogEntry {
                ts: r.get(0)?,
                kind: "reset".into(),
                session_pct: None, weekly_pct: None, session_req: None, weekly_req: None,
                message: Some(if kind == "weekly" { "每周窗口重置（实测）".into() } else { "5h 窗口重置（实测）".into() }),
            })
        })?;
        for row in rows { out.push(row?); }

        out.sort_by(|a, b| b.ts.cmp(&a.ts));
        out.truncate(limit);
        Ok(out)
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

    #[test]
    fn 事件入库与合并日志排序() {
        let mut store = Store::open_in_memory().unwrap();
        let t0 = (Utc::now() - chrono::Duration::hours(2)).to_rfc3339();
        let t1 = (Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
        let t2 = Utc::now().to_rfc3339();
        store.insert_sample(&row("k1", &t0, 5.0)).unwrap(); // 成功行：模型求和 = 1
        store.insert_event("k1", &t1, "error", "网络请求失败或超时").unwrap();
        store.insert_reset_event("k1", &t2, "session").unwrap();
        let log = store.log_rows("k1", 100).unwrap();
        assert_eq!(log.len(), 3);
        assert_eq!(log[0].kind, "reset"); // 时间倒序：最新在前
        assert_eq!(log[0].message.as_deref(), Some("5h 窗口重置（实测）"));
        assert_eq!(log[1].kind, "error");
        assert_eq!(log[2].kind, "ok");
        assert_eq!(log[2].session_req, Some(1));
        // limit 截断
        assert_eq!(store.log_rows("k1", 2).unwrap().len(), 2);
        // 别名隔离
        assert!(store.log_rows("k2", 100).unwrap().is_empty());
    }

    #[test]
    fn 保留清理删过期样本与事件() {
        let mut store = Store::open_in_memory().unwrap();
        let old = (Utc::now() - chrono::Duration::days(8)).to_rfc3339();
        let fresh = (Utc::now() - chrono::Duration::days(1)).to_rfc3339();
        store.insert_sample(&row("k1", &old, 1.0)).unwrap();
        store.insert_sample(&row("k1", &fresh, 2.0)).unwrap();
        store.insert_event("k1", &old, "error", "e").unwrap();
        store.insert_reset_event("k1", &old, "session").unwrap();
        let (s, e) = store.delete_before(7).unwrap();
        assert_eq!((s, e), (1, 1));
        assert_eq!(store.log_rows("k1", 100).unwrap().len(), 1); // 只剩 fresh 样本
    }
}
