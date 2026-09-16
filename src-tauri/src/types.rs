// 契约类型：形状与 spec §5.3 一字不差，双端唯一接口
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FailureLevel { None, Degraded, InvalidKey, Dead }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RefreshResult { Updated, RateLimited, Failed }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStat { pub name: String, pub request_count: i64 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSnapshot {
    pub session_pct: Option<f64>,      // 0~100，口径以官方未文档化接口为准（H1）
    pub weekly_pct: Option<f64>,
    pub session_reset_est: Option<String>,  // ISO8601 UTC，客户端推算（H2）
    pub weekly_reset_est: Option<String>,
    pub session_models: Vec<ModelStat>,
    pub weekly_models: Vec<ModelStat>,
    pub server_time: Option<String>,        // activity.period.ending_at 原样（H5）
    pub fetched_at: String,                 // 最近一次抓取时刻 ISO8601 UTC
    pub last_success_at: Option<String>,
    pub failure_level: FailureLevel,
    pub error_message: Option<String>,      // 不含 api_key
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyState { pub alias: String, pub snapshot: Option<UsageSnapshot> }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState { pub keys: Vec<KeyState>, pub generated_at: String }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sample {
    pub ts: String,                         // ISO8601 UTC
    pub session_pct: f64,
    pub weekly_pct: f64,
    pub session_models: Vec<ModelStat>,
}