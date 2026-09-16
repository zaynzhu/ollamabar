pub mod types;
pub mod reset;
pub mod ollama;
pub mod store;
pub mod config;
pub mod state;
pub mod poller;
pub mod commands;
pub mod tray;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::Manager;

use commands::Ctx;

/// 包装 poller::run_key_task：构造真机 FetchFn（闭包自带 reqwest::Client）、
/// 往 runtime 表插入 KeyRuntime、启动任务并把命令通道存入 poll_tx。
/// 注意：不用 tokio::spawn（command 线程无运行时上下文会 panic），统一走 tauri::async_runtime。
pub fn spawn_key_task(_app: &tauri::AppHandle, ctx: &Ctx, key: &config::KeyConfig) {
    let poll_interval_secs = config::load(&ctx.config_path)
        .map(|c| c.poll_interval_secs)
        .unwrap_or(60); // 配置读不出时退回默认间隔
    let client = reqwest::Client::new();
    let fetch: poller::FetchFn = Arc::new(move |api_key| {
        let client = client.clone();
        Box::pin(async move { ollama::fetch_usage(&client, poller::DEFAULT_URL, &api_key).await })
    });
    ctx.runtime.lock().unwrap().insert(key.alias.clone(), state::KeyRuntime::new(&key.alias));
    let (tx, rx) = tokio::sync::mpsc::channel::<poller::PollerMsg>(8);
    ctx.poll_tx.lock().unwrap().insert(key.alias.clone(), tx);
    let cfg = poller::PollConfig {
        interval: std::time::Duration::from_secs(poll_interval_secs),
        min_gap: std::time::Duration::from_secs(2),
    };
    // 任务句柄即弃：停止靠 poll_tx 发 Stop，不持有 JoinHandle
    let _task = tauri::async_runtime::spawn(poller::run_key_task(
        key.alias.clone(), key.api_key.clone(), cfg, fetch,
        ctx.store_tx.clone(), ctx.runtime.clone(), rx));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::get_history,
            commands::add_key,
            commands::remove_key,
            commands::refresh_now
        ])
        .on_window_event(tray::handle_window_event)
        .setup(|app| {
            tray::build_tray(app.handle())?;

            let dir = app.path().app_config_dir()?;
            std::fs::create_dir_all(&dir)?;
            let config_path = dir.join("config.json");
            let db_path = dir.join("history.db");

            let mut store = store::Store::open(&db_path.to_string_lossy())?;
            let (store_tx, mut store_rx) = tokio::sync::mpsc::channel::<store::StoreMsg>(64);
            let runtime: Arc<Mutex<HashMap<String, state::KeyRuntime>>> =
                Arc::new(Mutex::new(HashMap::new()));
            app.manage(Ctx {
                config_path: config_path.clone(),
                db_path,
                runtime: runtime.clone(),
                poll_tx: Arc::new(Mutex::new(HashMap::new())),
                store_tx,
            });

            // SQLite 单写者任务：唯一持写连接；每处理完一条 StoreMsg 推一次 state-changed
            // （spec §5.2 硬要求：每次轮询完成都推，含失败路径的 StateDirty）
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while let Some(msg) = store_rx.recv().await {
                    let res = match msg {
                        store::StoreMsg::Sample(r) => store.insert_sample(&r),
                        store::StoreMsg::ResetEvent { alias, observed_at, kind } => {
                            store.insert_reset_event(&alias, &observed_at, &kind)
                        }
                        store::StoreMsg::StateDirty => Ok(()), // 仅触发推送，不写库
                    };
                    if let Err(e) = res { eprintln!("store 写入失败: {e}"); }
                    commands::emit_state(&handle, &handle.state::<Ctx>());
                }
            });

            // 启动时为已配置的每个 key 起轮询任务
            let cfg = config::load(&config_path).unwrap_or_default();
            let ctx_state = app.state::<Ctx>();
            for key in cfg.keys {
                spawn_key_task(app.handle(), &ctx_state, &key);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}