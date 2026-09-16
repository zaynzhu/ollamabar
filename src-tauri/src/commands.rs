// 5 个前端 command + 契约事件 state-changed（spec §5.1/§5.2）；错误全走 String，禁 panic
use crate::config::{self, AppConfig, KeyConfig};
use crate::state::KeyRuntime;
use crate::store::{self, StoreMsg};
use crate::types::{AppState, KeyState, RefreshResult, Sample};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};

// 应用级共享状态（在 lib.rs setup 中构造并 manage）
pub struct Ctx {
    pub config_path: PathBuf,
    pub db_path: PathBuf,
    pub runtime: Arc<Mutex<HashMap<String, KeyRuntime>>>, // poller 共用
    pub poll_tx: Arc<Mutex<HashMap<String, tokio::sync::mpsc::Sender<crate::poller::PollerMsg>>>>,
    pub store_tx: tokio::sync::mpsc::Sender<StoreMsg>,
}

pub fn build_state(ctx: &Ctx) -> AppState {
    let mut keys: Vec<KeyState> = vec![];
    {
        let mut rt = ctx.runtime.lock().unwrap();
        let now = chrono::Utc::now();
        for (_, runtime) in rt.iter_mut() {
            keys.push(KeyState { alias: runtime.alias.clone(), snapshot: Some(runtime.build_snapshot(now)) });
        }
    }
    keys.sort_by(|a, b| a.alias.cmp(&b.alias));
    AppState { keys, generated_at: chrono::Utc::now().to_rfc3339() }
}

/// 契约事件：每次轮询完成（写者每处理一条 StoreMsg）都推
pub fn emit_state(app: &AppHandle, ctx: &Ctx) {
    let _ = app.emit("state-changed", build_state(ctx));
}

#[tauri::command]
pub fn get_state(ctx: State<'_, Ctx>) -> AppState { build_state(&ctx) }

#[tauri::command]
pub fn get_history(ctx: State<'_, Ctx>, alias: String, hours: i64) -> Result<Vec<Sample>, String> {
    let store = store::Store::open(&ctx.db_path.to_string_lossy()).map_err(|e| e.to_string())?;
    Ok(store.history(&alias, hours).map_err(|e| e.to_string())?.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub fn add_key(app: AppHandle, ctx: State<'_, Ctx>, alias: String, api_key: String) -> Result<(), String> {
    let alias = alias.trim().to_string();
    if alias.is_empty() { return Err("别名不能为空".into()); }
    if api_key.trim().is_empty() { return Err("api_key 不能为空".into()); }
    let mut cfg: AppConfig = config::load(&ctx.config_path).map_err(|e| e.to_string())?;
    if cfg.keys.iter().any(|k| k.alias == alias) { return Err("别名已存在".into()); }
    let key = KeyConfig { alias: alias.clone(), api_key: api_key.trim().to_string() };
    cfg.keys.push(key.clone());
    config::save(&ctx.config_path, &cfg).map_err(|e| e.to_string())?;
    crate::spawn_key_task(&app, &ctx, &key);
    emit_state(&app, &ctx);
    Ok(())
}

#[tauri::command]
pub fn remove_key(app: AppHandle, ctx: State<'_, Ctx>, alias: String) -> Result<(), String> {
    let mut cfg: AppConfig = config::load(&ctx.config_path).map_err(|e| e.to_string())?;
    cfg.keys.retain(|k| k.alias != alias);
    config::save(&ctx.config_path, &cfg).map_err(|e| e.to_string())?;
    if let Some(tx) = ctx.poll_tx.lock().unwrap().remove(&alias) {
        let _ = tx.try_send(crate::poller::PollerMsg::Stop); // 轮询任务收到 Stop 即 return
    }
    ctx.runtime.lock().unwrap().remove(&alias);
    emit_state(&app, &ctx);
    Ok(())
}

#[tauri::command]
pub async fn refresh_now(ctx: State<'_, Ctx>, alias: String) -> Result<RefreshResult, String> {
    let tx = ctx.poll_tx.lock().unwrap().get(&alias).cloned().ok_or("key 不存在")?;
    let (ack, done) = tokio::sync::oneshot::channel();
    tx.send(crate::poller::PollerMsg::RefreshNow(ack)).await.map_err(|e| e.to_string())?;
    Ok(done.await.map_err(|e| e.to_string())?)
}
