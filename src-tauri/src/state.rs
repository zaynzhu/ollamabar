// 三级失效状态机（H4）+ 服务端 resets_at 过期未推进漂移检测（H5）+ resets_at 变化/骤降重置事件检测（H6）
use crate::ollama::{BalanceData, FetchError, UsageStatsData};
use crate::reset::{next_session_reset, next_weekly_reset};
use crate::types::{FailureLevel, PlanType, UsageSnapshot};
use chrono::{DateTime, Duration, Utc};

pub const DEAD_AFTER: Duration = Duration::hours(24);
pub const STALL_GRACE: Duration = Duration::minutes(10); // H5：resets_at 过期后的宽限
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
    prev_session_resets_at: Option<String>, // H6 主判据：session resets_at 变化即实测重置
    prev_weekly_resets_at: Option<String>,
    prev_session_pct: Option<f64>, // H6 fallback：resets_at 缺失时的骤降检测
    resets_stalled: bool,          // H5：session resets_at 过期未推进
    req_24h: Option<i64>,          // 统计端点轮转累计，跨 tick 保留
    req_7d: Option<i64>,
    req_30d: Option<i64>,
}

/// resets_at 是否发生实测变化：仅两侧都有值且不同才算（None→Some 是首拍，不是重置）
fn changed(prev: &Option<String>, new: &Option<String>) -> bool {
    prev.is_some() && new.is_some() && prev != new
}

impl KeyRuntime {
    pub fn new(alias: &str) -> KeyRuntime {
        KeyRuntime {
            alias: alias.into(), last_snapshot: None, last_success_at: None,
            first_failure_at: None, last_err: None,
            prev_session_resets_at: None, prev_weekly_resets_at: None,
            prev_session_pct: None, resets_stalled: false,
            req_24h: None, req_7d: None, req_30d: None,
        }
    }

    /// 成功采样：组装快照、检测 H5 停滞与 H6 重置，返回可能的实测重置事件
    pub fn apply_success(&mut self, balance: &BalanceData, stats: Option<&UsageStatsData>, now: DateTime<Utc>) -> Option<ResetEvent> {
        let session_changed = changed(&self.prev_session_resets_at, &balance.session_resets_at);
        let weekly_changed = changed(&self.prev_weekly_resets_at, &balance.weekly_resets_at);
        self.prev_session_resets_at = balance.session_resets_at.clone();
        self.prev_weekly_resets_at = balance.weekly_resets_at.clone();
        let session_pct = balance.session_remaining_pct.map(|r| 100.0 - r);

        let reset_event = if session_changed || weekly_changed {
            Some(ResetEvent {
                kind: if session_changed { ResetKind::Session } else { ResetKind::Weekly },
                observed_at: now,
            })
        } else {
            // H6 fallback：resets_at 缺失时 session 从高位骤降视为实测窗口重置
            match (self.prev_session_pct, session_pct) {
                (Some(prev), Some(cur)) if prev - cur > DROP_THRESHOLD && cur < prev =>
                    Some(ResetEvent { kind: ResetKind::Session, observed_at: now }),
                _ => None,
            }
        };
        self.prev_session_pct = session_pct;

        // H5：session resets_at 过期超宽限仍未推进 → 服务端语义漂移（resets_at 前进即恢复）
        self.resets_stalled = match (&balance.session_resets_at, session_changed) {
            (Some(t), false) => DateTime::parse_from_rfc3339(t)
                .map(|d| now > d.with_timezone(&Utc) + STALL_GRACE).unwrap_or(false),
            _ => false,
        };

        // 统计轮转合并：本次 range 命中的档位更新，其余保留上次值
        if let Some(s) = stats {
            match s.range.as_str() {
                "24h" => self.req_24h = s.request_count,
                "7d" => self.req_7d = s.request_count,
                "30d" => self.req_30d = s.request_count,
                _ => {}
            }
        }

        let legacy = balance.plan_type == PlanType::Legacy;
        self.last_snapshot = Some(UsageSnapshot {
            plan_type: balance.plan_type,
            session_pct,
            weekly_pct: balance.weekly_remaining_pct.map(|r| 100.0 - r),
            // Legacy：优先服务端 resets_at，缺失回退客户端推算并标 from_server=false
            // UsageBased：无 5h/周窗口概念，不推算（周期由 period_until 承担）
            session_reset_est: if legacy {
                balance.session_resets_at.clone().or_else(|| Some(next_session_reset(now).to_rfc3339()))
            } else { None },
            weekly_reset_est: if legacy {
                balance.weekly_resets_at.clone().or_else(|| Some(next_weekly_reset(now).to_rfc3339()))
            } else { None },
            session_reset_from_server: legacy && balance.session_resets_at.is_some(),
            weekly_reset_from_server: legacy && balance.weekly_resets_at.is_some(),
            // 新接口暂不提供模型明细（官方 Coming soon）：恒空 + false，不得把缺失当零次调用
            session_models: vec![],
            weekly_models: vec![],
            models_available: false,
            balance_usd: balance.balance_usd,
            allowance_usd: balance.allowance_usd,
            period_from: balance.period_from.clone(),
            period_until: balance.period_until.clone(),
            requests_24h: self.req_24h,
            requests_7d: self.req_7d,
            requests_30d: self.req_30d,
            server_time: None, // 旧接口字段保留兼容，新接口无对应值
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
                plan_type: PlanType::Legacy,
                session_pct: None, weekly_pct: None,
                session_reset_est: None, weekly_reset_est: None,
                session_reset_from_server: false, weekly_reset_from_server: false,
                session_models: vec![], weekly_models: vec![],
                models_available: false,
                balance_usd: None, allowance_usd: None,
                period_from: None, period_until: None,
                requests_24h: None, requests_7d: None, requests_30d: None,
                server_time: None, fetched_at: now.to_rfc3339(),
                last_success_at: None, failure_level: FailureLevel::None,
                error_message: None,
            },
        };
        let mut level = self.failure_level(now);
        if self.resets_stalled && level == FailureLevel::None {
            level = FailureLevel::Degraded; // H5：HTTP 成功但服务端重置时间未随窗口推进
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
    use chrono::TimeZone;

    fn bal(session_remaining: f64, session_resets: Option<&str>) -> BalanceData {
        bal_w(session_remaining, session_resets, "2026-10-05T00:00:00Z")
    }

    fn bal_w(session_remaining: f64, session_resets: Option<&str>, weekly_resets: &str) -> BalanceData {
        BalanceData {
            plan_type: PlanType::Legacy,
            session_remaining_pct: Some(session_remaining),
            weekly_remaining_pct: Some(40.0),
            session_resets_at: session_resets.map(String::from),
            weekly_resets_at: Some(weekly_resets.into()),
            balance_usd: None, allowance_usd: None, period_from: None, period_until: None,
        }
    }

    fn stats(range: &str, n: i64) -> UsageStatsData {
        UsageStatsData { range: range.into(), request_count: Some(n), window_until: None }
    }

    #[test]
    fn 成功后失败再到dead三级状态() {
        let mut rt = KeyRuntime::new("k1");
        let t0 = Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap();
        rt.apply_success(&bal(90.0, Some("2026-10-01T05:00:00Z")), None, t0);
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
        let t0 = Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap();
        rt.apply_failure(FetchError::Auth, t0);
        assert_eq!(rt.failure_level(t0), FailureLevel::InvalidKey);
    }

    #[test]
    fn session与weekly的resets_at变化触发重置事件() {
        let mut rt = KeyRuntime::new("k1");
        let t0 = Utc.with_ymd_and_hms(2026, 10, 1, 4, 0, 0).unwrap();
        // 首拍：prev 为 None，不产生事件
        assert!(rt.apply_success(&bal(40.0, Some("2026-10-01T05:00:00Z")), None, t0).is_none());
        // session resets_at 前进 → Session 重置
        let ev = rt.apply_success(&bal(90.0, Some("2026-10-01T10:00:00Z")), None, t0 + chrono::Duration::hours(1)).unwrap();
        assert!(matches!(ev.kind, ResetKind::Session));
        // session 不变、weekly resets_at 前进 → Weekly 重置
        let ev2 = rt.apply_success(&bal_w(80.0, Some("2026-10-01T10:00:00Z"), "2026-10-12T00:00:00Z"), None, t0 + chrono::Duration::hours(2)).unwrap();
        assert!(matches!(ev2.kind, ResetKind::Weekly));
        // 两侧都不变 → 无事件
        assert!(rt.apply_success(&bal_w(75.0, Some("2026-10-01T10:00:00Z"), "2026-10-12T00:00:00Z"), None, t0 + chrono::Duration::hours(3)).is_none());
    }

    #[test]
    fn resets_at过期未推进判漂移且前进即恢复() {
        let mut rt = KeyRuntime::new("k1");
        let t0 = Utc.with_ymd_and_hms(2026, 10, 1, 5, 20, 0).unwrap();
        // resets_at 已过 20 分钟（> 10 分钟宽限）且未推进
        rt.apply_success(&bal(40.0, Some("2026-10-01T05:00:00Z")), None, t0);
        rt.apply_success(&bal(45.0, Some("2026-10-01T05:00:00Z")), None, t0 + chrono::Duration::minutes(1));
        let snap = rt.build_snapshot(t0 + chrono::Duration::minutes(1));
        assert_eq!(snap.failure_level, FailureLevel::Degraded);
        // resets_at 前进 → 恢复 None
        rt.apply_success(&bal(50.0, Some("2026-10-01T10:00:00Z")), None, t0 + chrono::Duration::minutes(2));
        let snap2 = rt.build_snapshot(t0 + chrono::Duration::minutes(2));
        assert_eq!(snap2.failure_level, FailureLevel::None);
    }

    #[test]
    fn resets_at缺失回退推算且不标服务端来源() {
        let mut rt = KeyRuntime::new("k1");
        let t0 = Utc.with_ymd_and_hms(2026, 10, 1, 3, 10, 0).unwrap();
        rt.apply_success(&bal(40.0, None), None, t0);
        let snap = rt.build_snapshot(t0);
        assert!(!snap.session_reset_from_server);
        assert_eq!(snap.session_reset_est, Some(next_session_reset(t0).to_rfc3339()));
        // 有服务端值：直接采用并标来源
        let mut rt2 = KeyRuntime::new("k1");
        rt2.apply_success(&bal(40.0, Some("2026-10-01T05:00:00Z")), None, t0);
        let snap2 = rt2.build_snapshot(t0);
        assert!(snap2.session_reset_from_server);
        assert_eq!(snap2.session_reset_est, Some("2026-10-01T05:00:00Z".into()));
    }

    #[test]
    fn 无resets_at时骤降fallback触发session重置() {
        let mut rt = KeyRuntime::new("k1");
        let t0 = Utc.with_ymd_and_hms(2026, 10, 1, 4, 0, 0).unwrap();
        rt.apply_success(&bal(40.0, None), None, t0); // 已用 60%
        let ev = rt.apply_success(&bal(98.0, None), None, t0 + chrono::Duration::hours(1)).unwrap(); // 骤降至已用 2%
        assert!(matches!(ev.kind, ResetKind::Session));
        // 小幅波动（已用 2% → 5%）不算重置
        assert!(rt.apply_success(&bal(95.0, None), None, t0 + chrono::Duration::hours(1)).is_none());
    }

    #[test]
    fn usage_based快照不推算窗口重置() {
        let mut rt = KeyRuntime::new("k1");
        let t0 = Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap();
        let balance = BalanceData {
            plan_type: PlanType::UsageBased,
            session_remaining_pct: None, weekly_remaining_pct: None,
            session_resets_at: None, weekly_resets_at: None,
            balance_usd: Some(4.2), allowance_usd: Some(20.0),
            period_from: Some("2026-09-15T09:30:00Z".into()),
            period_until: Some("2026-10-15T09:30:00Z".into()),
        };
        rt.apply_success(&balance, None, t0);
        let snap = rt.build_snapshot(t0);
        assert_eq!(snap.plan_type, PlanType::UsageBased);
        assert_eq!(snap.session_pct, None);
        assert_eq!(snap.weekly_pct, None);
        assert_eq!(snap.session_reset_est, None); // 无 5h/周窗口概念
        assert_eq!(snap.weekly_reset_est, None);
        assert_eq!(snap.balance_usd, Some(4.2));
        assert_eq!(snap.allowance_usd, Some(20.0));
        assert_eq!(snap.period_until, Some("2026-10-15T09:30:00Z".into()));
        assert!(!snap.models_available);
        assert!(snap.session_models.is_empty());
    }

    #[test]
    fn 统计轮转合并三档请求数() {
        let mut rt = KeyRuntime::new("k1");
        let t0 = Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap();
        let b = bal(90.0, Some("2026-10-01T05:00:00Z"));
        rt.apply_success(&b, Some(&stats("24h", 15)), t0);
        rt.apply_success(&b, Some(&stats("7d", 612)), t0 + chrono::Duration::minutes(1));
        rt.apply_success(&b, Some(&stats("30d", 2048)), t0 + chrono::Duration::minutes(2));
        // 未轮转到的档位保留：再次只更新 24h，7d/30d 不丢
        rt.apply_success(&b, Some(&stats("24h", 16)), t0 + chrono::Duration::minutes(3));
        let snap = rt.build_snapshot(t0 + chrono::Duration::minutes(3));
        assert_eq!(snap.requests_24h, Some(16));
        assert_eq!(snap.requests_7d, Some(612));
        assert_eq!(snap.requests_30d, Some(2048));
    }
}