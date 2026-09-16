// 三级失效状态机（H4）+ 服务器时间戳漂移检测（H5）+ 骤降重置事件检测（H6）
use crate::ollama::{FetchError, RawUsage};
use crate::reset::{next_session_reset, next_weekly_reset};
use crate::types::{FailureLevel, UsageSnapshot};
use chrono::{DateTime, Duration, Utc};

pub const DEAD_AFTER: Duration = Duration::hours(24);
pub const STALL_TICKS: usize = 5;
pub const DROP_THRESHOLD: f64 = 5.0; // 百分点

#[derive(Debug, Clone)]
pub enum ResetKind { Session, Weekly }
#[derive(Debug, Clone)]
pub struct ResetEvent { pub kind: ResetKind, pub observed_at: DateTime<Utc> }

#[derive(Debug, Clone, Copy, PartialEq)]
enum LastErr { Auth, Other }

pub struct KeyRuntime {
    pub alias: String,
    last_snapshot: Option<UsageSnapshot>,
    last_success_at: Option<DateTime<Utc>>,
    first_failure_at: Option<DateTime<Utc>>,
    last_err: Option<LastErr>,
    recent_server_times: Vec<String>,
    prev_session_pct: Option<f64>,
}

impl KeyRuntime {
    pub fn new(alias: &str) -> KeyRuntime {
        KeyRuntime { alias: alias.into(), last_snapshot: None, last_success_at: None,
                     first_failure_at: None, last_err: None,
                     recent_server_times: vec![], prev_session_pct: None }
    }

    /// 成功采样：组装快照、检测 H5 停滞与 H6 骤降，返回可能的实测重置事件
    pub fn apply_success(&mut self, raw: RawUsage, now: DateTime<Utc>) -> Option<ResetEvent> {
        let session_pct = raw.session_usage * 100.0;
        let weekly_pct = raw.weekly_usage * 100.0;

        // H6：session 从高位骤降视为实测窗口重置
        let reset_event = match self.prev_session_pct {
            Some(prev) if prev - session_pct > DROP_THRESHOLD && session_pct < prev =>
                Some(ResetEvent { kind: ResetKind::Session, observed_at: now }),
            _ => None,
        };
        self.prev_session_pct = Some(session_pct);

        // H5：ending_at 连续 STALL_TICKS 次不变
        match &raw.server_time {
            Some(t) if self.recent_server_times.last().map(|x| x == t).unwrap_or(false) => {
                self.recent_server_times.push(t.clone());
            }
            _ => self.recent_server_times = raw.server_time.clone().into_iter().collect(),
        }

        self.last_snapshot = Some(UsageSnapshot {
            session_pct: Some(session_pct),
            weekly_pct: Some(weekly_pct),
            session_reset_est: Some(next_session_reset(now).to_rfc3339()),
            weekly_reset_est: Some(next_weekly_reset(now).to_rfc3339()),
            session_models: raw.session_models,
            weekly_models: raw.weekly_models,
            server_time: raw.server_time,
            fetched_at: now.to_rfc3339(),
            last_success_at: Some(now.to_rfc3339()),
            failure_level: FailureLevel::None, // 在 build_snapshot 里按当下时间重判
            error_message: None,
        });
        self.last_success_at = Some(now);
        self.first_failure_at = None;
        self.last_err = None;
        reset_event
    }

    pub fn apply_failure(&mut self, err: FetchError, now: DateTime<Utc>) {
        if self.first_failure_at.is_none() { self.first_failure_at = Some(now); }
        self.last_err = match err {
            FetchError::Auth => Some(LastErr::Auth),
            _ => Some(LastErr::Other),
        };
    }

    pub fn server_time_stalled(&self) -> bool {
        self.recent_server_times.len() >= STALL_TICKS
            && self.recent_server_times.iter().all(|t| Some(t) == self.recent_server_times.first())
    }

    pub fn failure_level(&self, now: DateTime<Utc>) -> FailureLevel {
        match self.last_err {
            Some(LastErr::Auth) if self.first_failure_at.is_some() => FailureLevel::InvalidKey,
            Some(_) if self.first_failure_at.map(|t| now - t >= DEAD_AFTER).unwrap_or(false) => FailureLevel::Dead,
            Some(_) => FailureLevel::Degraded,
            None => FailureLevel::None,
        }
    }

    /// 组装给前端的快照：失败时保留上次成功数据并附滞后信息（H4 禁止静默空壳）
    pub fn build_snapshot(&mut self, now: DateTime<Utc>) -> UsageSnapshot {
        let mut snap = match self.last_snapshot.take() {
            Some(s) => s,
            None => UsageSnapshot {
                session_pct: None, weekly_pct: None,
                session_reset_est: None, weekly_reset_est: None,
                session_models: vec![], weekly_models: vec![],
                server_time: None, fetched_at: now.to_rfc3339(),
                last_success_at: None, failure_level: FailureLevel::None,
                error_message: None,
            },
        };
        let mut level = self.failure_level(now);
        if self.server_time_stalled() && level == FailureLevel::None {
            level = FailureLevel::Degraded; // H5：HTTP 成功但语义漂移
        }
        snap.failure_level = level.clone(); // FailureLevel 无 Copy（types.rs 仅派生 Clone）
        snap.error_message = match level {
            FailureLevel::InvalidKey => Some("key 无效或已撤销".into()),
            FailureLevel::Dead => Some(format!(
                "接口可能已失效，上次成功：{}", self.last_success_at.map(|t| t.to_rfc3339()).unwrap_or_default())),
            FailureLevel::Degraded => Some(match self.last_success_at {
                Some(t) => format!("数据滞后 {} 分钟", (now - t).num_minutes()),
                None => "尚无成功数据".into(),
            }),
            FailureLevel::None => None,
        };
        snap.fetched_at = now.to_rfc3339();
        if let Some(t) = self.last_success_at { snap.last_success_at = Some(t.to_rfc3339()); }
        self.last_snapshot = Some(snap.clone());
        snap
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ollama::{RawUsage, FetchError};
    use crate::types::ModelStat;
    use chrono::{TimeZone, Utc};

    fn raw(session: f64, server: &str) -> RawUsage {
        RawUsage { session_usage: session, weekly_usage: 0.1,
            session_models: vec![ModelStat { name: "m".into(), request_count: 3 }],
            weekly_models: vec![], server_time: Some(server.into()) }
    }

    #[test]
    fn 成功后失败再到dead三级状态() {
        let mut rt = KeyRuntime::new("k1");
        let t0 = Utc.with_ymd_and_hms(2026, 9, 14, 0, 0, 0).unwrap();
        rt.apply_success(raw(0.1, "2026-09-14T00:00:10Z"), t0);
        assert_eq!(rt.failure_level(t0), FailureLevel::None);

        let t1 = t0 + chrono::Duration::hours(2);
        rt.apply_failure(FetchError::Network, t1);
        assert_eq!(rt.failure_level(t1), FailureLevel::Degraded);

        // 首次失败发生在 t1，距 t2 必须 ≥24h 才判 dead
        let t2 = t0 + chrono::Duration::hours(27);
        assert_eq!(rt.failure_level(t2), FailureLevel::Dead);
    }

    #[test]
    fn auth错误单列为invalid_key() {
        let mut rt = KeyRuntime::new("k1");
        let t0 = Utc.with_ymd_and_hms(2026, 9, 14, 0, 0, 0).unwrap();
        rt.apply_failure(FetchError::Auth, t0);
        assert_eq!(rt.failure_level(t0), FailureLevel::InvalidKey);
    }

    #[test]
    fn 服务器时间戳连续5次不前进判漂移() {
        let mut rt = KeyRuntime::new("k1");
        let t0 = Utc.with_ymd_and_hms(2026, 9, 14, 0, 0, 0).unwrap();
        for i in 0..5 {
            rt.apply_success(raw(0.1, "2026-09-14T00:00:10Z"), t0 + chrono::Duration::minutes(i));
        }
        assert!(rt.server_time_stalled());
        let snap = rt.build_snapshot(t0);
        assert_eq!(snap.failure_level, FailureLevel::Degraded);
    }

    #[test]
    fn session骤降超5个百分点触发重置事件() {
        let mut rt = KeyRuntime::new("k1");
        let t0 = Utc.with_ymd_and_hms(2026, 9, 14, 4, 0, 0).unwrap();
        rt.apply_success(raw(0.4, "2026-09-14T04:00:00Z"), t0);
        let ev = rt.apply_success(raw(0.02, "2026-09-14T05:00:00Z"), t0 + chrono::Duration::hours(1));
        assert!(matches!(ev, Some(ResetEvent { kind: ResetKind::Session, .. })));
        // 小幅波动（0.02 → 0.05）不算重置
        let ev2 = rt.apply_success(raw(0.05, "2026-09-14T05:00:10Z"), t0 + chrono::Duration::hours(1));
        assert!(ev2.is_none());
    }
}