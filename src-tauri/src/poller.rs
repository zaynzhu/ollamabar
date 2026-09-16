// 每 key 一个任务：interval + select 手动刷新；MissedTickBehavior::Delay 防休眠唤醒补发
use crate::ollama::{FetchError, RawUsage};
use crate::state::{KeyRuntime, ResetKind};
use crate::store::{SampleRow, StoreMsg};
use crate::types::RefreshResult;
use chrono::Utc;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

pub type FetchFn = Arc<dyn Fn(String) -> Pin<Box<dyn Future<Output = Result<RawUsage, FetchError>> + Send>> + Send + Sync>;

#[derive(Clone)]
pub struct PollConfig { pub interval: Duration, pub min_gap: Duration }

pub enum PollerMsg { RefreshNow(tokio::sync::oneshot::Sender<RefreshResult>), Stop }

pub const DEFAULT_URL: &str = "https://ollama.com/api/usage";

/// 单 key 轮询主循环。app_state 为全部 key 共享的运行时表；每次状态变化后 emit_state 回调负责推前端。
pub async fn run_key_task(
    alias: String,
    api_key: String,
    cfg: PollConfig,
    fetch: FetchFn,
    store_tx: mpsc::Sender<StoreMsg>,
    app_state: Arc<Mutex<HashMap<String, KeyRuntime>>>,
    mut rx: mpsc::Receiver<PollerMsg>,
) {
    app_state.lock().unwrap().insert(alias.clone(), KeyRuntime::new(&alias));
    let mut interval = tokio::time::interval(cfg.interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut last_req = tokio::time::Instant::now() - cfg.min_gap - cfg.min_gap; // 双倍回拨：允许首拍与首个手动刷新立即执行，避免 2s 边界 flaky
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
                        if last_req.elapsed() < cfg.min_gap {
                            let _ = ack.send(RefreshResult::RateLimited);
                            continue;
                        }
                        last_req = tokio::time::Instant::now();
                        let ok = do_fetch(&alias, &api_key, &fetch, &store_tx, &app_state).await;
                        let _ = ack.send(if ok { RefreshResult::Updated } else { RefreshResult::Failed });
                    }
                }
            }
            _ = interval.tick() => {
                if last_req.elapsed() < cfg.min_gap { continue; } // 与手动刷新共享限流窗口
                last_req = tokio::time::Instant::now();
                do_fetch(&alias, &api_key, &fetch, &store_tx, &app_state).await;
            }
        }
    }
}

async fn do_fetch(
    alias: &str, api_key: &str, fetch: &FetchFn,
    store_tx: &mpsc::Sender<StoreMsg>, app_state: &Arc<Mutex<HashMap<String, KeyRuntime>>>,
) -> bool {
    let result = fetch(api_key.to_string()).await; // FetchFn 全权负责怎么取：测试注入假实现，真机注入包 fetch_usage 的闭包
    let now = Utc::now();
    // 锁内只做状态计算并备好待发消息，锁释放后再 await 发送：
    // std::sync::MutexGuard 不能跨 await（future 需 Send 才能 spawn）
    let (ok, msgs) = {
        let mut rt = app_state.lock().unwrap();
        let Some(runtime) = rt.get_mut(alias) else { return false };
        match result {
            Ok(raw) => {
                let mut msgs = Vec::new();
                if let Some(ev) = runtime.apply_success(raw.clone(), now) { // H6：实测重置入库
                    msgs.push(StoreMsg::ResetEvent {
                        alias: alias.into(), observed_at: ev.observed_at.to_rfc3339(),
                        kind: match ev.kind { ResetKind::Session => "session", ResetKind::Weekly => "weekly" }.into(),
                    });
                }
                msgs.push(StoreMsg::Sample(SampleRow {
                    alias: alias.into(), fetched_at: now.to_rfc3339(),
                    session_pct: raw.session_usage * 100.0, weekly_pct: raw.weekly_usage * 100.0,
                    session_models: raw.session_models, weekly_models: raw.weekly_models,
                    server_time: raw.server_time,
                }));
                (true, msgs)
            }
            Err(e) => { runtime.apply_failure(e, now); (false, Vec::new()) }
        }
    };
    for msg in msgs { let _ = store_tx.send(msg).await; }
    ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ollama::{FetchError, RawUsage};
    use crate::types::ModelStat;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::sync::mpsc;

    fn fake_raw() -> RawUsage {
        RawUsage { session_usage: 0.1, weekly_usage: 0.2,
            session_models: vec![ModelStat { name: "m".into(), request_count: 1 }],
            weekly_models: vec![], server_time: Some("2026-09-15T00:00:00Z".into()) }
    }

    #[tokio::test]
    async fn 轮询产出采样并驱动状态() {
        let (store_tx, mut store_rx) = mpsc::channel(16);
        let state = Arc::new(std::sync::Mutex::new(std::collections::HashMap::<String, KeyRuntime>::new()));
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let fetch: FetchFn = Arc::new(move |_key| {
            let c = c.clone();
            Box::pin(async move { c.fetch_add(1, Ordering::SeqCst); Ok(fake_raw()) })
        });
        let (tx, rx) = mpsc::channel::<PollerMsg>(8);
        let handle = tokio::spawn(run_key_task(
            "k1".into(), "sk".into(),
            PollConfig { interval: std::time::Duration::from_millis(20), min_gap: std::time::Duration::from_millis(0) },
            fetch, store_tx, state, rx));
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        handle.abort();
        assert!(calls.load(Ordering::SeqCst) >= 2);
        assert!(store_rx.try_recv().is_ok()); // 有采样写入消息
    }

    #[tokio::test]
    async fn 手动刷新受min_gap限流() {
        let (store_tx, _store_rx) = mpsc::channel(16);
        let state = Arc::new(std::sync::Mutex::new(std::collections::HashMap::<String, KeyRuntime>::new()));
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let fetch: FetchFn = Arc::new(move |_key| {
            let c = c.clone();
            Box::pin(async move { c.fetch_add(1, Ordering::SeqCst); Ok(fake_raw()) })
        });
        let (tx, rx) = mpsc::channel::<PollerMsg>(8);
        let cfg = PollConfig { interval: std::time::Duration::from_secs(3600), min_gap: std::time::Duration::from_secs(2) };
        let handle = tokio::spawn(run_key_task("k1".into(), "sk".into(), cfg.clone(), fetch, store_tx.clone(), state.clone(), rx));
        let (ack, done) = tokio::sync::oneshot::channel();
        tx.send(PollerMsg::RefreshNow(ack)).await.unwrap();
        assert_eq!(done.await.unwrap(), RefreshResult::Updated);
        let (ack2, done2) = tokio::sync::oneshot::channel();
        tx.send(PollerMsg::RefreshNow(ack2)).await.unwrap();
        assert_eq!(done2.await.unwrap(), RefreshResult::RateLimited); // 2s 内第二次被拒
        handle.abort();
    }
}