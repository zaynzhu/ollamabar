// 取数与结构校验：接口未文档化，任何结构漂移都走 Parse 失败而非 panic
use crate::types::ModelStat;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct RawUsage {
    pub session_usage: f64,   // 0~1
    pub weekly_usage: f64,
    pub session_models: Vec<ModelStat>,
    pub weekly_models: Vec<ModelStat>,
    pub server_time: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FetchError { Network, Auth, Http, Parse }

impl FetchError {
    /// 日志与横幅共用的中文文案
    pub fn text(&self) -> &'static str {
        match self {
            FetchError::Network => "网络请求失败或超时",
            FetchError::Auth => "key 无效或已撤销（HTTP 401/403）",
            FetchError::Http => "HTTP 状态异常",
            FetchError::Parse => "返回结构解析失败（接口可能已变更）",
        }
    }
}

#[derive(Deserialize)]
struct ModelRaw { name: String, request_count: i64 }

#[derive(Deserialize)]
struct WindowRaw { usage: serde_json::Number, #[serde(default)] models: Vec<ModelRaw> }

#[derive(Deserialize)]
struct PeriodRaw { #[serde(default)] ending_at: Option<String> }

#[derive(Deserialize)]
struct ActivityRaw { period: PeriodRaw }

#[derive(Deserialize)]
struct LimitsRaw { session: WindowRaw, weekly: WindowRaw }

#[derive(Deserialize)]
struct RootRaw {
    limits: LimitsRaw,
    activity: ActivityRaw,
}

fn valid_usage(n: &serde_json::Number) -> bool {
    // 显式排除布尔：JSON 的 true 不会被 serde 解为 Number，双保险在 Number 侧再验一次
    match n.as_f64() {
        Some(v) => (0.0..=1.0).contains(&v) && !v.is_nan(),
        None => false,
    }
}

pub fn parse_usage(body: &str) -> Option<RawUsage> {
    let root: RootRaw = serde_json::from_str(body).ok()?;
    if !valid_usage(&root.limits.session.usage) || !valid_usage(&root.limits.weekly.usage) {
        return None;
    }
    Some(RawUsage {
        session_usage: root.limits.session.usage.as_f64()?,
        weekly_usage: root.limits.weekly.usage.as_f64()?,
        session_models: root.limits.session.models.into_iter()
            .map(|m| ModelStat { name: m.name, request_count: m.request_count }).collect(),
        weekly_models: root.limits.weekly.models.into_iter()
            .map(|m| ModelStat { name: m.name, request_count: m.request_count }).collect(),
        server_time: root.activity.period.ending_at,
    })
}

pub async fn fetch_usage(client: &reqwest::Client, url: &str, api_key: &str) -> Result<RawUsage, FetchError> {
    let resp = client.get(url)
        .bearer_auth(api_key)
        .timeout(std::time::Duration::from_secs(10))
        .send().await.map_err(|_| FetchError::Network)?;
    match resp.status().as_u16() {
        200 => {}
        401 | 403 => return Err(FetchError::Auth),
        _ => return Err(FetchError::Http),
    }
    let body = resp.text().await.map_err(|_| FetchError::Network)?;
    parse_usage(&body).ok_or(FetchError::Parse)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{
      "limits": {
        "session": {"usage": 0.092, "models": [{"name": "gpt-oss:120b", "request_count": 242}]},
        "weekly": {"usage": 0.049, "models": [{"name": "gpt-oss:120b", "request_count": 575}]}
      },
      "activity": {"period": {"ending_at": "2026-09-15T06:29:56Z"}}
    }"#;

    #[test]
    fn 正常解析() {
        let raw = parse_usage(FIXTURE).unwrap();
        assert!((raw.session_usage - 0.092).abs() < 1e-9);
        assert_eq!(raw.session_models[0].request_count, 242);
        assert_eq!(raw.server_time.as_deref(), Some("2026-09-15T06:29:56Z"));
    }

    #[test]
    fn models缺失容错为空数组() {
        let body = r#"{"limits":{"session":{"usage":0.1},"weekly":{"usage":0.2}},"activity":{"period":{}}}"#;
        let raw = parse_usage(body).unwrap();
        assert!(raw.session_models.is_empty());
        assert_eq!(raw.server_time, None);
    }

    #[test]
    fn usage越界拒收() {
        let body = r#"{"limits":{"session":{"usage":1.5},"weekly":{"usage":0.2}},"activity":{"period":{}}}"#;
        assert!(parse_usage(body).is_none());
    }

    #[test]
    fn usage为布尔拒收() {
        let body = r#"{"limits":{"session":{"usage":true},"weekly":{"usage":0.2}},"activity":{"period":{}}}"#;
        assert!(parse_usage(body).is_none());
    }

    #[test]
    fn 结构缺失返回none() {
        assert!(parse_usage("{}").is_none());
        assert!(parse_usage("not json").is_none());
    }

    // 集成测试：本地手写 HTTP mock，URL 可注入
    #[test]
    fn fetch对本地mock_server集成() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut s, _) = listener.accept().unwrap();
                let mut buf = [0u8; 1024];
                let _ = std::io::Read::read(&mut s, &mut buf);
                let body = FIXTURE;
                let resp = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}", body.len(), body);
                let _ = std::io::Write::write_all(&mut s, resp.as_bytes());
            });
            let client = reqwest::Client::new();
            let raw = fetch_usage(&client, &format!("http://{addr}/api/usage"), "test-key").await.unwrap();
            assert_eq!(raw.session_models.len(), 1);
            server.join().unwrap();
        });
    }

    #[test]
    fn fetch_401判定为auth错误() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut s, _) = listener.accept().unwrap();
                let mut buf = [0u8; 1024];
                let _ = std::io::Read::read(&mut s, &mut buf);
                let resp = "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n";
                let _ = std::io::Write::write_all(&mut s, resp.as_bytes());
            });
            let client = reqwest::Client::new();
            let err = fetch_usage(&client, &format!("http://{addr}/api/usage"), "bad").await.unwrap_err();
            assert!(matches!(err, FetchError::Auth));
            server.join().unwrap();
        });
    }
}
