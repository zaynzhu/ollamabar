// 每 key 一个任务：interval + select 手动刷新；MissedTickBehavior::Delay 防休眠唤醒补发
// 官方限流 10 次/分钟每用户（跨 Key 与设备共享）：全局 RateBudget 滑窗保守计数 + 429 暂停
use crate::ollama::{BalanceData, FetchError, FetchOutcome};
use crate::state::{KeyRuntime, ResetKind};
use crate::store::{SampleRow, StoreMsg};
use crate::types::{PlanType, RefreshResult};
use chrono::Utc;
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

pub const BALANCE_URL: &str = "https://ollama.com/api/balance";
pub const USAGE_URL: &str = "https://ollama.com/api/usage"; // fetch_usage_stats 拼 ?range=

pub const STATS_EVERY: u32 = 5;       // 每 5 个 tick 抓一次统计（24h→7d→30d 轮转）
pub const BUDGET_PER_MIN: usize = 8;  // 官方 10 次/分钟 - 2 余量（手动刷新与其他设备）
const BUDGET_WINDOW: Duration = Duration::from_secs(60);
pub const RETRY_BUFFER_SECS: u64 = 2; // 429 暂停时在 Retry-After 基础上加的缓冲

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatsRange { H24, D7, D30 }

impl StatsRange {
    pub fn as_str(&self) -> &'static str {
        match self { StatsRange::H24 => "24h", StatsRange::D7 => "7d", StatsRange::D30 => "30d" }
    }
    pub fn next(self) -> Self {
        match self { StatsRange::H24 => StatsRange::D7, StatsRange::D7 => StatsRange::D30, StatsRange::D30 => StatsRange::H24 }
    }
}

/// 一次轮询的取数计划：balance 每 tick，统计按 STATS_EVERY 轮转附带
pub enum FetchPlan { BalanceOnly, BalanceAndStats { range: StatsRange } }

/// 取数策略：参数为 (api_key, plan)，测试注入假实现，真机注入包 fetch_balance/fetch_usage_stats 的闭包
pub type FetchFn = Arc<dyn Fn(String, FetchPlan) -> Pin<Box<dyn Future<Output = FetchOutcome> + Send>> + Send + Sync>;

#[derive(Clone)]
pub struct PollConfig { pub interval: Duration, pub min_gap: Duration }

pub enum PollerMsg { RefreshNow(tokio::sync::oneshot::Sender<RefreshResult>), Stop }

/// 跨 key 共享的全局滑动窗口预算：BUDGET_WINDOW 内最多 cap 次 HTTP 请求。
/// 官方限流按"用户"计且跨 Key 共享；不同 Key 可能属不同账号无法区分，
/// 按最坏情况（全部同账号）全局保守计数，宁可跳过一个 tick 也不超限。
pub struct RateBudget { cap: usize, window: Mutex<VecDeque<std::time::Instant>> }

impl RateBudget {
    pub fn new(cap: usize) -> Self { RateBudget { cap, window: Mutex::new(VecDeque::new()) } }

    /// 申请 n 个名额：窗口内还有余量则记账并返回 true，否则拒绝（不等待）
    pub fn acquire(&self, n: usize) -> bool {
        let mut w = self.window.lock().unwrap();
        self.trim(&mut w);
        if w.len() + n <= self.cap {
            for _ in 0..n { w.push_back(std::time::Instant::now()); }
            true
        } else { false }
    }

    /// 429 时补记：把被服务端拒绝的请求计入窗口，惩罚期内预算同步收紧
    pub fn record(&self, n: usize) {
        let mut w = self.window.lock().unwrap();
        self.trim(&mut w);
        for _ in 0..n { w.push_back(std::time::Instant::now()); }
    }

    fn trim(&self, w: &mut VecDeque<std::time::Instant>) {
        let cutoff = std::time::Instant::now() - BUDGET_WINDOW;
        while w.front().map(|t| *t < cutoff).unwrap_or(false) { w.pop_front(); }
    }
}

/// 任务局部状态：限流时间戳、tick 计数、统计轮转、429 暂停期
struct KeyTaskState {
    last_req: tokio::time::Instant,
    tick: u32,
    rotor: StatsRange,
    pause_until: Option<tokio::time::Instant>,
}

/// tick 计数推进并给出本次取数计划（独立成纯函数便于确定性测试）
fn advance_plan(tick: &mut u32, rotor: &mut StatsRange) -> FetchPlan {
    *tick += 1;
    if *tick % STATS_EVERY == 0 {
        let range = *rotor;
        *rotor = rotor.next();
        FetchPlan::BalanceAndStats { range }
    } else { FetchPlan::BalanceOnly }
}

/// 单 key 轮询主循环。app_state 为全部 key 共享的运行时表；
/// 状态变化经 store_tx 消息驱动 lib.rs 装配侧的写者任务推 state-changed（本模块不直接 emit）。
pub async fn run_key_task(
    alias: String,
    api_key: String,
    cfg: PollConfig,
    fetch: FetchFn,
    store_tx: mpsc::Sender<StoreMsg>,
    app_state: Arc<Mutex<HashMap<String, KeyRuntime>>>,
    budget: Arc<RateBudget>,
    mut rx: mpsc::Receiver<PollerMsg>,
) {
    app_state.lock().unwrap().insert(alias.clone(), KeyRuntime::new(&alias));
    let mut interval = tokio::time::interval(cfg.interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut st = KeyTaskState {
        // 双倍回拨：允许首拍与首个手动刷新立即执行，避免 2s 边界 flaky
        last_req: tokio::time::Instant::now() - cfg.min_gap - cfg.min_gap,
        tick: 0, rotor: StatsRange::H24, pause_until: None,
    };
    loop {
        // biased + 手动刷新分支在前：定时首拍与首个 RefreshNow 同帧就绪时，
        // 无 biased 的随机分支会让首个手动刷新有 50% 概率被限流误判（测试 flaky 根源）；
        // 手动刷新优先于定时拍也是合理生产语义
        tokio::select! {
            biased;
            Some(msg) = rx.recv() => {
                match msg {
                    PollerMsg::Stop => return, // remove_key 停任务
                    PollerMsg::RefreshNow(ack) => {
                        let _ = ack.send(poll_once(&alias, &api_key, &cfg, &fetch, &store_tx, &app_state, &budget, &mut st).await);
                    }
                }
            }
            _ = interval.tick() => {
                poll_once(&alias, &api_key, &cfg, &fetch, &store_tx, &app_state, &budget, &mut st).await;
            }
        }
    }
}

/// 单次轮询：暂停期 / min_gap / 全局预算不足都拒绝（RateLimited），否则按计划取数
async fn poll_once(
    alias: &str, api_key: &str, cfg: &PollConfig, fetch: &FetchFn,
    store_tx: &mpsc::Sender<StoreMsg>, app_state: &Arc<Mutex<HashMap<String, KeyRuntime>>>,
    budget: &RateBudget, st: &mut KeyTaskState,
) -> RefreshResult {
    if let Some(until) = st.pause_until {
        if tokio::time::Instant::now() < until { return RefreshResult::RateLimited; }
        st.pause_until = None; // 暂停期满，恢复正常轮询
    }
    if st.last_req.elapsed() < cfg.min_gap { return RefreshResult::RateLimited; } // 与手动刷新共享限流窗口
    // 预算先于 tick/rotor 推进：拒绝时统计档位顺延到下轮，不白耗（与 advance_plan 的档位判定保持同步）
    let permits = if (st.tick + 1) % STATS_EVERY == 0 { 2 } else { 1 };
    if !budget.acquire(permits) { return RefreshResult::RateLimited; } // 全局预算不足：跳过本 tick，下轮自动补
    let plan = advance_plan(&mut st.tick, &mut st.rotor);
    st.last_req = tokio::time::Instant::now();
    do_fetch(alias, api_key, plan, fetch, store_tx, app_state, budget, st).await
}

async fn do_fetch(
    alias: &str, api_key: &str, plan: FetchPlan, fetch: &FetchFn,
    store_tx: &mpsc::Sender<StoreMsg>, app_state: &Arc<Mutex<HashMap<String, KeyRuntime>>>,
    budget: &RateBudget, st: &mut KeyTaskState,
) -> RefreshResult {
    let outcome = fetch(api_key.to_string(), plan).await; // FetchFn 全权负责怎么取
    let now = Utc::now();
    let permits = match outcome.stats { Some(_) => 2, None => 1 };
    // 统计失败不降级主状态：仅留痕事件；但 429 同样进入暂停（同一用户预算）
    if let Some(Err(e)) = outcome.stats.as_ref() {
        if let FetchError::RateLimited { retry_after_secs } = e {
            budget.record(permits);
            st.pause_until = Some(tokio::time::Instant::now()
                + Duration::from_secs(retry_after_secs + RETRY_BUFFER_SECS));
        }
        let _ = store_tx.try_send(StoreMsg::Event {
            alias: alias.into(), ts: now.to_rfc3339(),
            kind: "error".into(), message: format!("统计接口失败：{}", e.text()),
        });
        let _ = store_tx.try_send(StoreMsg::StateDirty);
    }
    // 锁内只做状态计算并备好待发消息，锁释放后再 await 发送：
    // std::sync::MutexGuard 不能跨 await（future 需 Send 才能 spawn）
    let (result, msgs) = {
        let mut rt = app_state.lock().unwrap();
        let Some(runtime) = rt.get_mut(alias) else { return RefreshResult::Failed };
        let stats_ok = outcome.stats.as_ref().and_then(|r| r.as_ref().ok());
        match outcome.balance {
            Ok(balance) => {
                let mut msgs = Vec::new();
                if let Some(ev) = runtime.apply_success(&balance, stats_ok, now) { // H6：实测重置入库
                    msgs.push(StoreMsg::ResetEvent {
                        alias: alias.into(), observed_at: ev.observed_at.to_rfc3339(),
                        kind: match ev.kind { ResetKind::Session => "session", ResetKind::Weekly => "weekly" }.into(),
                    });
                }
                // 历史曲线列复用：Legacy 写 5h 已用%；UsageBased 写本期已用 %（锯齿语义按周期爬升）
                let (session_pct, weekly_pct) = sample_pcts(&balance);
                msgs.push(StoreMsg::Sample(SampleRow {
                    alias: alias.into(), fetched_at: now.to_rfc3339(),
                    session_pct, weekly_pct,
                    session_models: vec![], weekly_models: vec![],
                    // 列复用：server_time 列存服务端 session resets_at（日志备注列展示）
                    server_time: balance.session_resets_at.clone(),
                    requests_24h: runtime.requests_24h(),
                }));
                (RefreshResult::Updated, msgs)
            }
            Err(e) => {
                if let FetchError::RateLimited { retry_after_secs } = e {
                    budget.record(permits);
                    st.pause_until = Some(tokio::time::Instant::now()
                        + Duration::from_secs(retry_after_secs + RETRY_BUFFER_SECS));
                }
                runtime.apply_failure(e, now);
                // 失败留痕（对齐 ps1 脚本的 ERROR 行）：错误也进日志，尽力而为，通道满即弃
                let _ = store_tx.try_send(StoreMsg::Event {
                    alias: alias.into(), ts: now.to_rfc3339(),
                    kind: "error".into(), message: e.text().into(),
                });
                // 失败分支无常规消息产出，补一条脏标记驱动写者推 state-changed（尽力而为，通道满即弃）
                let _ = store_tx.try_send(StoreMsg::StateDirty);
                (RefreshResult::Failed, Vec::new())
            }
        }
    };
    for msg in msgs { let _ = store_tx.send(msg).await; }
    result
}

/// 历史曲线两列的取值：Legacy = 100 - remaining；UsageBased = 本期已用 %（无 session/weekly 概念）
fn sample_pcts(balance: &BalanceData) -> (f64, Option<f64>) {
    match balance.plan_type {
        PlanType::Legacy => (
            balance.session_remaining_pct.map(|r| 100.0 - r).unwrap_or_default(),
            balance.weekly_remaining_pct.map(|r| 100.0 - r),
        ),
        PlanType::UsageBased => (
            match (balance.allowance_usd, balance.balance_usd) {
                (Some(a), Some(b)) if a > 0.0 => ((a - b) * 100.0 / a).clamp(0.0, 100.0),
                _ => 0.0,
            },
            None,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn balance_ok() -> BalanceData {
        // resets_at 必须晚于真实 now：固定的过去时间会触发 H5 漂移误判
        let resets = (Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
        BalanceData {
            plan_type: PlanType::Legacy,
            session_remaining_pct: Some(75.0), weekly_remaining_pct: Some(40.0),
            session_resets_at: Some(resets),
            weekly_resets_at: Some("2026-10-05T00:00:00Z".into()),
            balance_usd: None, allowance_usd: None, period_from: None, period_until: None,
        }
    }

    fn outcome_ok() -> FetchOutcome {
        FetchOutcome { balance: Ok(balance_ok()), stats: None }
    }

    #[test]
    fn budget滑窗容量内记账超容拒绝() {
        let budget = RateBudget::new(2);
        assert!(budget.acquire(1));
        assert!(budget.acquire(1));
        assert!(!budget.acquire(1)); // 60s 内已满
        assert!(!budget.acquire(2));
        budget.record(1); // 补记后依然超容
        assert!(!budget.acquire(1));
    }

    #[test]
    fn budget一次申请多名额原子判定() {
        let budget = RateBudget::new(3);
        assert!(budget.acquire(2));
        assert!(!budget.acquire(2)); // 剩 1 个名额，2 个不够
        assert!(budget.acquire(1));
    }

    #[test]
    fn plan按周期轮转附统计() {
        let mut tick = 0;
        let mut rotor = StatsRange::H24;
        // 前 4 tick 仅 balance
        for _ in 0..4 {
            assert!(matches!(advance_plan(&mut tick, &mut rotor), FetchPlan::BalanceOnly));
        }
        // 第 5 tick 附带 24h 统计，随后轮转 7d → 30d → 24h
        assert!(matches!(advance_plan(&mut tick, &mut rotor),
            FetchPlan::BalanceAndStats { range: StatsRange::H24 }));
        for _ in 0..4 { assert!(matches!(advance_plan(&mut tick, &mut rotor), FetchPlan::BalanceOnly)); }
        assert!(matches!(advance_plan(&mut tick, &mut rotor),
            FetchPlan::BalanceAndStats { range: StatsRange::D7 }));
        for _ in 0..4 { assert!(matches!(advance_plan(&mut tick, &mut rotor), FetchPlan::BalanceOnly)); }
        assert!(matches!(advance_plan(&mut tick, &mut rotor),
            FetchPlan::BalanceAndStats { range: StatsRange::D30 }));
        for _ in 0..4 { assert!(matches!(advance_plan(&mut tick, &mut rotor), FetchPlan::BalanceOnly)); }
        assert!(matches!(advance_plan(&mut tick, &mut rotor),
            FetchPlan::BalanceAndStats { range: StatsRange::H24 }));
    }

    #[tokio::test]
    async fn 轮询产出采样并驱动状态() {
        let (store_tx, mut store_rx) = mpsc::channel(16);
        let state = Arc::new(Mutex::new(HashMap::<String, KeyRuntime>::new()));
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let fetch: FetchFn = Arc::new(move |_key, _plan| {
            let c = c.clone();
            Box::pin(async move { c.fetch_add(1, Ordering::SeqCst); outcome_ok() })
        });
        let (_tx, rx) = mpsc::channel::<PollerMsg>(8);
        let handle = tokio::spawn(run_key_task(
            "k1".into(), "sk".into(),
            PollConfig { interval: Duration::from_millis(20), min_gap: Duration::from_millis(0) },
            fetch, store_tx, state, Arc::new(RateBudget::new(BUDGET_PER_MIN)), rx));
        tokio::time::sleep(Duration::from_millis(100)).await;
        handle.abort();
        assert!(calls.load(Ordering::SeqCst) >= 2);
        // 首条消息为采样：session_pct 已换算为已用 25%
        match store_rx.try_recv().unwrap() {
            StoreMsg::Sample(r) => assert!((r.session_pct - 25.0).abs() < 1e-9),
            _ => panic!("期待采样消息"),
        }
    }

    #[tokio::test]
    async fn 手动刷新受min_gap限流() {
        let (store_tx, _store_rx) = mpsc::channel(16);
        let state = Arc::new(Mutex::new(HashMap::<String, KeyRuntime>::new()));
        let fetch: FetchFn = Arc::new(|_key, _plan| Box::pin(async { outcome_ok() }));
        let (tx, rx) = mpsc::channel::<PollerMsg>(8);
        let cfg = PollConfig { interval: Duration::from_secs(3600), min_gap: Duration::from_secs(2) };
        let handle = tokio::spawn(run_key_task("k1".into(), "sk".into(), cfg, fetch,
            store_tx, state, Arc::new(RateBudget::new(BUDGET_PER_MIN)), rx));
        let (ack, done) = tokio::sync::oneshot::channel();
        tx.send(PollerMsg::RefreshNow(ack)).await.unwrap();
        assert_eq!(done.await.unwrap(), RefreshResult::Updated);
        let (ack2, done2) = tokio::sync::oneshot::channel();
        tx.send(PollerMsg::RefreshNow(ack2)).await.unwrap();
        assert_eq!(done2.await.unwrap(), RefreshResult::RateLimited); // 2s 内第二次被拒
        handle.abort();
    }

    #[tokio::test]
    async fn 全局预算不足跳过本次轮询() {
        let (store_tx, _store_rx) = mpsc::channel(16);
        let state = Arc::new(Mutex::new(HashMap::<String, KeyRuntime>::new()));
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let fetch: FetchFn = Arc::new(move |_key, _plan| {
            let c = c.clone();
            Box::pin(async move { c.fetch_add(1, Ordering::SeqCst); outcome_ok() })
        });
        let (tx, rx) = mpsc::channel::<PollerMsg>(8);
        let handle = tokio::spawn(run_key_task("k1".into(), "sk".into(),
            PollConfig { interval: Duration::from_secs(3600), min_gap: Duration::from_millis(0) },
            fetch, store_tx, state, Arc::new(RateBudget::new(0)), rx)); // 预算恒为 0
        let (ack, done) = tokio::sync::oneshot::channel();
        tx.send(PollerMsg::RefreshNow(ack)).await.unwrap();
        assert_eq!(done.await.unwrap(), RefreshResult::RateLimited);
        assert_eq!(calls.load(Ordering::SeqCst), 0); // 未发出任何请求
        handle.abort();
    }

    #[tokio::test]
    async fn balance返回429进入暂停期手动刷新被拒() {
        let (store_tx, _store_rx) = mpsc::channel(16);
        let state = Arc::new(Mutex::new(HashMap::<String, KeyRuntime>::new()));
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let fetch: FetchFn = Arc::new(move |_key, _plan| {
            let c = c.clone();
            Box::pin(async move {
                c.fetch_add(1, Ordering::SeqCst);
                FetchOutcome {
                    balance: Err(FetchError::RateLimited { retry_after_secs: 30 }),
                    stats: None,
                }
            })
        });
        let (tx, rx) = mpsc::channel::<PollerMsg>(8);
        let handle = tokio::spawn(run_key_task("k1".into(), "sk".into(),
            PollConfig { interval: Duration::from_secs(3600), min_gap: Duration::from_millis(0) },
            fetch, store_tx, state, Arc::new(RateBudget::new(BUDGET_PER_MIN)), rx));
        // 首个手动刷新：发出请求、得到 429、返回 Failed
        let (ack, done) = tokio::sync::oneshot::channel();
        tx.send(PollerMsg::RefreshNow(ack)).await.unwrap();
        assert_eq!(done.await.unwrap(), RefreshResult::Failed);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        // 暂停期内（30s + 缓冲）再次手动刷新：客户端直接拒绝，不再发请求
        let (ack2, done2) = tokio::sync::oneshot::channel();
        tx.send(PollerMsg::RefreshNow(ack2)).await.unwrap();
        assert_eq!(done2.await.unwrap(), RefreshResult::RateLimited);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        handle.abort();
    }

    #[tokio::test]
    async fn 统计失败不降级balance成功状态() {
        let (store_tx, mut store_rx) = mpsc::channel(16);
        let state = Arc::new(Mutex::new(HashMap::<String, KeyRuntime>::new()));
        let fetch: FetchFn = Arc::new(|_key, _plan| Box::pin(async {
            FetchOutcome {
                balance: Ok(balance_ok()),
                stats: Some(Err(FetchError::Parse)),
            }
        }));
        let state2 = state.clone();
        let (_tx, rx) = mpsc::channel::<PollerMsg>(8);
        let handle = tokio::spawn(run_key_task("k1".into(), "sk".into(),
            PollConfig { interval: Duration::from_millis(20), min_gap: Duration::from_millis(0) },
            fetch, store_tx, state2, Arc::new(RateBudget::new(BUDGET_PER_MIN)), rx));
        tokio::time::sleep(Duration::from_millis(60)).await;
        handle.abort();
        // 主状态不降级
        let snap = state.lock().unwrap().get_mut("k1").unwrap().build_snapshot(Utc::now());
        assert_eq!(snap.failure_level, crate::types::FailureLevel::None);
        // 统计失败已留痕为 error 事件
        let mut saw_event = false;
        while let Ok(msg) = store_rx.try_recv() {
            if let StoreMsg::Event { message, .. } = msg {
                assert!(message.starts_with("统计接口失败"));
                saw_event = true;
            }
        }
        assert!(saw_event);
    }
}
