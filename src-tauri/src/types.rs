// 契约类型：双端唯一接口（src/types.ts 为逐字段镜像）。
// 2026-10 接口升级：新增 plan_type/balance/统计字段；旧字段名一律不改，前端可平滑过渡
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FailureLevel { None, Degraded, InvalidKey, Dead }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RefreshResult { Updated, RateLimited, Failed }

/// 套餐类型：Legacy（session/weekly 百分比）| UsageBased（美元余额/额度），由 /api/balance 结构判定
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanType { Legacy, UsageBased }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStat { pub name: String, pub request_count: i64 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSnapshot {
    pub plan_type: PlanType,                  // 套餐类型，决定展示分支
    pub session_pct: Option<f64>,             // 0~100 已用 = 100 - remaining_percent；usage_based 时 None
    pub weekly_pct: Option<f64>,
    // 优先服务端 resets_at，缺失时回退 reset.rs 客户端推算（from_server 标记来源）
    pub session_reset_est: Option<String>,    // ISO8601 UTC；usage_based 时 None（无 5h 窗口概念）
    pub weekly_reset_est: Option<String>,
    pub session_reset_from_server: bool,
    pub weekly_reset_from_server: bool,
    // 模型明细：新接口暂不提供（官方 Coming soon），恒空数组 + models_available=false；
    // 字段保留以兼容已存历史数据与未来官方恢复
    pub session_models: Vec<ModelStat>,
    pub weekly_models: Vec<ModelStat>,
    pub models_available: bool,               // false=接口不提供；true 且空数组=本窗口暂无调用
    // 新计费套餐（balance_usd/allowance_usd），Legacy 时为 None
    pub balance_usd: Option<f64>,
    pub allowance_usd: Option<f64>,
    pub period_from: Option<String>,          // ISO8601 UTC
    pub period_until: Option<String>,
    // 历史统计（/api/usage?range=，轮转刷新，最多滞后 STATS_EVERY 个周期）
    pub requests_24h: Option<i64>,
    pub requests_7d: Option<i64>,
    pub requests_30d: Option<i64>,
    pub server_time: Option<String>,          // 旧接口 activity.period.ending_at，保留兼容，恒 None
    pub fetched_at: String,                   // 最近一次抓取时刻 ISO8601 UTC
    pub last_success_at: Option<String>,
    pub failure_level: FailureLevel,
    pub error_message: Option<String>,        // 不含 api_key
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
