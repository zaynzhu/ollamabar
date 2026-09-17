// 5 个前端 command + 契约事件 state-changed（spec §5.1/§5.2）；错误全走 String，禁 panic
use crate::config::{self, AppConfig, KeyConfig};
use crate::state::KeyRuntime;
use crate::store::{self, LogEntry, StoreMsg};
use crate::types::{AppState, KeyState, RefreshResult, Sample};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::DialogExt;

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

/// 详情弹窗：某 key 最近 limit 条合并日志（成功/错误/重置）
#[tauri::command]
pub fn get_log(ctx: State<'_, Ctx>, alias: String) -> Result<Vec<LogEntry>, String> {
    let store = store::Store::open(&ctx.db_path.to_string_lossy()).map_err(|e| e.to_string())?;
    store.log_rows(&alias, 500).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_retention(ctx: State<'_, Ctx>) -> u64 {
    config::load(&ctx.config_path).map(|c| c.log_retention_days).unwrap_or(7)
}

#[tauri::command]
pub fn set_retention(ctx: State<'_, Ctx>, days: u64) -> Result<(), String> {
    if days != 7 && days != 30 { return Err("保留天数仅支持 7 或 30".into()); }
    let mut cfg: AppConfig = config::load(&ctx.config_path).map_err(|e| e.to_string())?;
    cfg.log_retention_days = days;
    config::save(&ctx.config_path, &cfg).map_err(|e| e.to_string())?;
    // 切短保留期时立即触发一次清理，不等下个自然日
    let _ = ctx.store_tx.try_send(StoreMsg::Cleanup { days: days as i64 });
    Ok(())
}

/// 导出某 key 的全部保留日志为 ps1 同款格式 txt；用户取消保存则返回 None
#[tauri::command]
pub async fn export_log(app: AppHandle, ctx: State<'_, Ctx>, alias: String) -> Result<Option<String>, String> {
    let content = {
        let db_path = ctx.db_path.clone();
        let store = store::Store::open(&db_path.to_string_lossy()).map_err(|e| e.to_string())?;
        let rows = store.log_rows(&alias, usize::MAX).map_err(|e| e.to_string())?;
        let mut lines = vec![format!("===== OllamaBar {} 导出于 {} =====", alias, chrono::Local::now().format("%Y-%m-%d %H:%M:%S"))];
        for r in rows.iter().rev() { // 导出按时间正序，与 ps1 日志一致
            let ts = chrono::DateTime::parse_from_rfc3339(&r.ts)
                .map(|t| t.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|_| r.ts.clone());
            match r.kind.as_str() {
                "ok" => lines.push(format!(
                    "{} | 5h={:.2}% req={} | week={:.2}% req={} | server={}",
                    ts, r.session_pct.unwrap_or(0.0), r.session_req.unwrap_or(0),
                    r.weekly_pct.unwrap_or(0.0), r.weekly_req.unwrap_or(0),
                    r.message.as_deref().unwrap_or("-"))),
                "reset" => lines.push(format!("{} | RESET | {}", ts, r.message.as_deref().unwrap_or(""))),
                _ => lines.push(format!("{} | ERROR | {}", ts, r.message.as_deref().unwrap_or("未知错误"))),
            }
        }
        lines.join("\n") + "\n"
    };
    // 保存对话框必须在非主线程阻塞调用，丢给 spawn_blocking
    let file = tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file()
            .add_filter("文本日志", &["txt"])
            .set_file_name(format!("ollamabar-{}.log.txt", sanitize_filename(&alias)))
            .blocking_save_file()
            .map(|p| p.to_string())
    }).await.map_err(|e| e.to_string())?;
    let Some(path) = file else { return Ok(None) };
    std::fs::write(&path, content).map_err(|e| format!("写入失败: {e}"))?;
    Ok(Some(path))
}

/// Windows 文件名非法字符替换为下划线
fn sanitize_filename(name: &str) -> String {
    name.chars().map(|c| if "\\/:*?\"<>|".contains(c) { '_' } else { c }).collect()
}
