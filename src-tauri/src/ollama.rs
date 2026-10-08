// 取数与结构校验：接口文档化（/api/balance、/api/usage），任何结构漂移都走 Parse 失败而非 panic
use crate::types::PlanType;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FetchError { Network, Auth, Http, RateLimited { retry_after_secs: u64 }, Parse }

impl FetchError {
    /// 日志与横幅共用的中文文案
    pub fn text(&self) -> &'static str {
        match self {
            FetchError::Network => "网络请求失败或超时",
            FetchError::Auth => "key 无效或已撤销（HTTP 401/403）",
            FetchError::Http => "HTTP 状态异常",
            FetchError::RateLimited { .. } => "请求过于频繁（官方限流，稍后自动重试）",
            FetchError::Parse => "返回结构解析失败（接口可能已变更）",
        }
    }
}

// ============ 新接口取数层 ============

/// /api/balance 的统一中间形态：remaining_percent 为 0~100 剩余，已用换算在 state/poller 层做
#[derive(Debug, Clone)]
pub struct BalanceData {
    pub plan_type: PlanType,
    pub session_remaining_pct: Option<f64>,  // Legacy 0~100 剩余
    pub weekly_remaining_pct: Option<f64>,
    pub session_resets_at: Option<String>,   // 服务端准确 UTC 重置时间
    pub weekly_resets_at: Option<String>,
    pub balance_usd: Option<f64>,            // 新计费套餐
    pub allowance_usd: Option<f64>,
    pub period_from: Option<String>,
    pub period_until: Option<String>,
}

/// /api/usage?range= 的历史统计（Legacy 套餐仅有 request_count）
#[derive(Debug, Clone)]
pub struct UsageStatsData {
    pub range: String,
    pub request_count: Option<i64>,
    pub window_until: Option<String>,
}

/// 一次轮询的取数结果：balance 为主状态源，stats 为辅助统计（None=本次未请求）
#[derive(Debug, Clone)]
pub struct FetchOutcome {
    pub balance: Result<BalanceData, FetchError>,
    pub stats: Option<Result<UsageStatsData, FetchError>>,
}

#[derive(Deserialize)]
struct BalanceRootRaw {
    included: IncludedRaw,
    // 刻意保留解析但不消费：防官方新增 purchased 结构造成整体解析失败
    #[serde(default)] #[allow(dead_code)] purchased: Option<PurchasedRaw>,
}

#[derive(Deserialize)]
struct IncludedRaw {
    #[serde(default)] session: Option<WindowRaw2>,
    #[serde(default)] weekly: Option<WindowRaw2>,
    #[serde(default)] balance_usd: Option<serde_json::Number>,
    #[serde(default)] allowance_usd: Option<serde_json::Number>,
    #[serde(default)] period: Option<BillingPeriodRaw>,
}

#[derive(Deserialize)]
struct WindowRaw2 { remaining_percent: serde_json::Number, #[serde(default)] resets_at: Option<String> }

#[derive(Deserialize)]
struct BillingPeriodRaw { #[serde(default)] from: Option<String>, #[serde(default)] until: Option<String> }

#[derive(Deserialize)]
struct PurchasedRaw { #[serde(default)] #[allow(dead_code)] balance_usd: Option<serde_json::Number> }

#[derive(Deserialize)]
struct StatsRootRaw {
    range: String,
    #[serde(default)] totals: Option<TotalsRaw>,
    #[serde(default)] buckets: Vec<serde_json::Value>, // 只验证存在性，暂不逐桶解析
    #[serde(default)] until: Option<String>,
}

#[derive(Deserialize)]
struct TotalsRaw { #[serde(default)] request_count: Option<i64> }

/// 0~100 百分比校验：排除布尔与 NaN（serde 不会把 true 解为 Number，此处双保险）
fn valid_percent(n: &serde_json::Number) -> Option<f64> {
    match n.as_f64() {
        Some(v) if (0.0..=100.0).contains(&v) && !v.is_nan() => Some(v),
        _ => None,
    }
}

/// 美元金额校验：有限数且非负
fn valid_usd(n: &serde_json::Number) -> Option<f64> {
    n.as_f64().filter(|v| v.is_finite() && *v >= 0.0)
}

/// 解析 /api/balance：session 存在→Legacy；否则 balance_usd+allowance_usd 齐→UsageBased；都不满足→None
pub fn parse_balance(body: &str) -> Option<BalanceData> {
    let root: BalanceRootRaw = serde_json::from_str(body).ok()?;
    let inc = root.included;
    let (period_from, period_until) = match inc.period {
        Some(p) => (p.from, p.until),
        None => (None, None),
    };
    if let Some(session) = inc.session {
        // Legacy 套餐：session 必须有合法 remaining_percent，weekly 可缺
        let session_remaining = valid_percent(&session.remaining_percent)?;
        let weekly_remaining = match &inc.weekly {
            Some(w) => Some(valid_percent(&w.remaining_percent)?),
            None => None,
        };
        return Some(BalanceData {
            plan_type: PlanType::Legacy,
            session_remaining_pct: Some(session_remaining),
            weekly_remaining_pct: weekly_remaining,
            session_resets_at: session.resets_at,
            weekly_resets_at: inc.weekly.and_then(|w| w.resets_at),
            balance_usd: inc.balance_usd.and_then(|n| valid_usd(&n)),
            allowance_usd: inc.allowance_usd.and_then(|n| valid_usd(&n)),
            period_from,
            period_until,
        });
    }
    let (balance_usd, allowance_usd) = (
        inc.balance_usd.as_ref().and_then(valid_usd),
        inc.allowance_usd.as_ref().and_then(valid_usd),
    );
    if let (Some(b), Some(a)) = (balance_usd, allowance_usd) {
        return Some(BalanceData {
            plan_type: PlanType::UsageBased,
            session_remaining_pct: None,
            weekly_remaining_pct: None,
            session_resets_at: None,
            weekly_resets_at: None,
            balance_usd: Some(b),
            allowance_usd: Some(a),
            period_from,
            period_until,
        });
    }
    None // 两种套餐结构都不满足：接口可能已再变更
}

/// 解析 /api/usage?range=：range 回显必须等于请求值（重复/不支持参数官方返 400），totals/buckets 至少一项存在
pub fn parse_usage_stats(body: &str, expect_range: &str) -> Option<UsageStatsData> {
    let root: StatsRootRaw = serde_json::from_str(body).ok()?;
    if root.range != expect_range { return None; }
    if root.totals.is_none() && root.buckets.is_empty() { return None; }
    Some(UsageStatsData {
        range: root.range,
        request_count: root.totals.and_then(|t| t.request_count),
        window_until: root.until,
    })
}

/// 共用请求层：200 通过 / 401|403→Auth / 429→RateLimited（解析 Retry-After 整数秒，缺失或非法默认 60）/ 其余→Http
async fn send_and_read(client: &reqwest::Client, url: &str, api_key: &str) -> Result<reqwest::Response, FetchError> {
    let resp = client.get(url)
        .bearer_auth(api_key)
        .timeout(std::time::Duration::from_secs(10))
        .send().await.map_err(|_| FetchError::Network)?;
    match resp.status().as_u16() {
        200 => Ok(resp),
        401 | 403 => Err(FetchError::Auth),
        429 => {
            // Retry-After 官方口径为秒数；HTTP 日期格式或非法值回退 60s
            let secs = resp.headers().get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.trim().parse::<u64>().ok())
                .unwrap_or(60);
            Err(FetchError::RateLimited { retry_after_secs: secs })
        }
        _ => Err(FetchError::Http),
    }
}

pub async fn fetch_balance(client: &reqwest::Client, url: &str, api_key: &str) -> Result<BalanceData, FetchError> {
    let resp = send_and_read(client, url, api_key).await?;
    let body = resp.text().await.map_err(|_| FetchError::Network)?;
    parse_balance(&body).ok_or(FetchError::Parse)
}

/// base_url 拼 ?range=（scope 缺省 self 不显式传）
pub async fn fetch_usage_stats(client: &reqwest::Client, base_url: &str, api_key: &str, range: &str) -> Result<UsageStatsData, FetchError> {
    let resp = send_and_read(client, &format!("{base_url}?range={range}"), api_key).await?;
    let body = resp.text().await.map_err(|_| FetchError::Network)?;
    parse_usage_stats(&body, range).ok_or(FetchError::Parse)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BALANCE_LEGACY: &str = r#"{
      "included": {
        "session": {"remaining_percent": 75, "resets_at": "2026-10-01T07:00:00Z"},
        "weekly": {"remaining_percent": 40, "resets_at": "2026-10-05T00:00:00Z"}
      },
      "purchased": {"balance_usd": 25}
    }"#;

    const BALANCE_USAGE_BASED: &str = r#"{
      "included": {
        "balance_usd": 72.5, "allowance_usd": 100,
        "period": {"from": "2026-09-15T09:30:00Z", "until": "2026-10-15T09:30:00Z"}
      },
      "purchased": {"balance_usd": 25}
    }"#;

    const USAGE_STATS_24H: &str = r#"{
      "range": "24h", "scope": "self", "granularity": "hour",
      "from": "2026-09-30T02:00:00Z", "until": "2026-10-01T02:30:00Z",
      "totals": {"request_count": 15},
      "buckets": [{"from": "2026-10-01T02:00:00Z", "until": "2026-10-01T02:30:00Z", "partial": true, "request_count": 3}]
    }"#;

    /// 本地手写 HTTP mock server：返回预构造响应，地址可注入各 fetch 函数
    fn mock_server(resp: String) -> (std::net::SocketAddr, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = std::io::Read::read(&mut s, &mut buf);
            let _ = std::io::Write::write_all(&mut s, resp.as_bytes());
        });
        (addr, handle)
    }

    // —— 新接口测试 ——

    #[test]
    fn balance_legacy套餐解析() {
        let d = parse_balance(BALANCE_LEGACY).unwrap();
        assert_eq!(d.plan_type, PlanType::Legacy);
        assert!((d.session_remaining_pct.unwrap() - 75.0).abs() < 1e-9);
        assert!((d.weekly_remaining_pct.unwrap() - 40.0).abs() < 1e-9);
        assert_eq!(d.session_resets_at.as_deref(), Some("2026-10-01T07:00:00Z"));
        assert_eq!(d.weekly_resets_at.as_deref(), Some("2026-10-05T00:00:00Z"));
        assert_eq!(d.balance_usd, None); // legacy 无美元字段
    }

    #[test]
    fn balance_新计费套餐解析() {
        let d = parse_balance(BALANCE_USAGE_BASED).unwrap();
        assert_eq!(d.plan_type, PlanType::UsageBased);
        assert_eq!(d.session_remaining_pct, None);
        assert_eq!(d.weekly_remaining_pct, None);
        assert!((d.balance_usd.unwrap() - 72.5).abs() < 1e-9);
        assert!((d.allowance_usd.unwrap() - 100.0).abs() < 1e-9);
        assert_eq!(d.period_from.as_deref(), Some("2026-09-15T09:30:00Z"));
        assert_eq!(d.period_until.as_deref(), Some("2026-10-15T09:30:00Z"));
    }

    #[test]
    fn balance_两种结构皆缺拒收() {
        assert!(parse_balance(r#"{"included": {}}"#).is_none());
        assert!(parse_balance(r#"{"included": {"balance_usd": 72.5}}"#).is_none()); // 缺 allowance
        assert!(parse_balance("not json").is_none());
    }

    #[test]
    fn balance_剩余越界或布尔拒收() {
        let over = r#"{"included": {"session": {"remaining_percent": 150}}}"#;
        assert!(parse_balance(over).is_none());
        let neg = r#"{"included": {"session": {"remaining_percent": -1}}}"#;
        assert!(parse_balance(neg).is_none());
        let boolean = r#"{"included": {"session": {"remaining_percent": true}}}"#;
        assert!(parse_balance(boolean).is_none());
    }

    #[test]
    fn balance_resets缺失与weekly缺失容错() {
        let body = r#"{"included": {"session": {"remaining_percent": 80}}}"#;
        let d = parse_balance(body).unwrap();
        assert_eq!(d.session_resets_at, None);
        assert_eq!(d.weekly_remaining_pct, None);
    }

    #[test]
    fn usage_stats_legacy仅有请求数() {
        let d = parse_usage_stats(USAGE_STATS_24H, "24h").unwrap();
        assert_eq!(d.range, "24h");
        assert_eq!(d.request_count, Some(15));
        assert_eq!(d.window_until.as_deref(), Some("2026-10-01T02:30:00Z"));
    }

    #[test]
    fn usage_stats_totals与buckets全缺拒收() {
        assert!(parse_usage_stats(r#"{"range": "24h"}"#, "24h").is_none());
        assert!(parse_usage_stats("not json", "24h").is_none());
    }

    #[test]
    fn usage_stats_range回显不匹配拒收() {
        assert!(parse_usage_stats(USAGE_STATS_24H, "7d").is_none()); // 防 400/结构漂移
    }

    #[test]
    fn fetch_balance_429带retry_after() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (addr, server) = mock_server(
                "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 30\r\nContent-Length: 0\r\n\r\n".into());
            let client = reqwest::Client::new();
            let err = fetch_balance(&client, &format!("http://{addr}/api/balance"), "k").await.unwrap_err();
            assert!(matches!(err, FetchError::RateLimited { retry_after_secs: 30 }));
            server.join().unwrap();
        });
    }

    #[test]
    fn fetch_balance_429无retry_after默认60秒() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (addr, server) = mock_server("HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\n\r\n".into());
            let client = reqwest::Client::new();
            let err = fetch_balance(&client, &format!("http://{addr}/api/balance"), "k").await.unwrap_err();
            assert!(matches!(err, FetchError::RateLimited { retry_after_secs: 60 }));
            server.join().unwrap();
        });
    }

    #[test]
    fn fetch_balance_400判定为http错误() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (addr, server) = mock_server("HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n".into());
            let client = reqwest::Client::new();
            let err = fetch_balance(&client, &format!("http://{addr}/api/balance"), "k").await.unwrap_err();
            assert!(matches!(err, FetchError::Http));
            server.join().unwrap();
        });
    }

    #[test]
    fn fetch_stats_401判定为auth错误() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (addr, server) = mock_server("HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n".into());
            let client = reqwest::Client::new();
            let err = fetch_usage_stats(&client, &format!("http://{addr}/api/usage"), "bad", "24h").await.unwrap_err();
            assert!(matches!(err, FetchError::Auth));
            server.join().unwrap();
        });
    }
}
