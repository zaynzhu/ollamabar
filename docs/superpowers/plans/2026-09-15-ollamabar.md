# OllamaBar 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **分工约定（用户手动切模型）**：执行时按顺序推进——前置任务 → 后端线（Task A1~A8）→ UI 线（Task B1~B4）→ 联调（Task C1）。**每条线开始前停下，提示用户切换模型，得到确认后再继续。** 两条线代码零相互依赖，只依赖 spec §5 契约。

**Goal:** 跨 Win/macOS 的 Tauri 2 托盘应用，多 Ollama key 用量只读仪表盘（额度环形图、重置倒计时、模型调用次数、诚实锯齿曲线）。

**Architecture:** Rust 侧每 key 独立 tokio 轮询任务（60s，防休眠补发），取数经严格结构校验后汇入 mpsc 单写者写 SQLite(WAL)；三级失效状态机 + 时间戳漂移/重置规则自校验；前端 vanilla TS 纯展示，开发期走 mock，invoke 契约见 spec §5。

**Tech Stack:** Tauri 2 / Rust(tokio, reqwest+rustls, rusqlite bundled, serde, chrono) / vanilla TS + Vite + 手绘 SVG。

**Spec:** `docs/superpowers/specs/2026-09-15-ollamabar-design.md`（契约 §5、诚实性条款 §3 必读）

## Global Constraints

- 数据源仅 `GET https://ollama.com/api/usage`，`Authorization: Bearer <key>`，未文档化接口
- 诚实性条款 H1~H6 逐条满足（见 spec §3）：usage 口径标注、重置标"预计"、锯齿不修饰、三级失效禁止静默空壳、ending_at 停滞检测、骤降重置事件入库
- 重置推算：weekly = 严格晚于 now 的下一个周一 00:00 UTC；session = `(ts // 18000 + 1) * 18000`；禁止写死每日小时列表
- 契约（spec §5）是双端唯一接口，字段名/类型一字不差：`get_state` / `get_history(alias, hours)` / `add_key(alias, api_key)` / `remove_key(alias)` / `refresh_now(alias)`、事件 `state-changed`
- command 出错返回串错误信息，禁止 panic、禁止 api_key 进日志/错误信息
- 同一 key 任意来源请求间隔 ≥2s（轮询与手动刷新共享）
- JS 不使用分号，2 空格缩进，camelCase；注释中文，标识符英文
- Commit 格式 `type: 中文描述`，每任务一提交
- 无 CI、无签名、无自动更新、无前端框架

---

### Task 0: 前置——Rust 工具链 + Tauri 脚手架

**Files:**
- Create: 仓库根下 `package.json`、`vite.config.ts`、`tsconfig.json`、`index.html`、`src/`（模板默认页）、`src-tauri/`（Tauri Rust 工程与配置）、`src-tauri/icons/`

**Interfaces:**
- Produces: 可 `cargo tauri dev` 起跑的空壳工程；`src-tauri/src/` 供线 A 使用，`src/` 供线 B 重写

- [ ] **Step 1: 安装 Rust 工具链**

从 https://rustup.rs 下载并安装 rustup（默认 stable-msvc），Windows 需先装 Visual Studio Build Tools（含 "Desktop development with C++" 工作负载）。装完新开终端验证：

```bash
cargo --version && rustc --version
```

Expected: 两个版本号正常输出。

- [ ] **Step 2: 脚手架落地**

仓库根已有 `docs/`，不能在原地生成。在临时目录生成 vanilla-ts 模板后移入：

```bash
cd /e/claudecode/project
npm create tauri-app@latest scaffold-tmp -- --template vanilla-ts --manager npm --yes
# 将 scaffold-tmp 内除 .git 外全部移入 ollamabar/，确认 tauri.conf.json productName 为 "OllamaBar"
mv scaffold-tmp/* ollamabar/ && rm -rf scaffold-tmp
```

- [ ] **Step 3: 验证 dev 起跑**

```bash
cd ollamabar && npm install && npm run tauri dev
```

Expected: 弹出默认 Hello World 窗口（首次编译约 5~15 分钟），Ctrl+C 退出后无残留进程。

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "build: 落地 Tauri 2 vanilla-ts 脚手架"
```

---

### Task 1: 前置——usage 语义对照官网验证（spec H1，用户参与）

**Files:**
- Create: `docs/verify-usage.md`

- [ ] **Step 1: 用真实 key 抓一次接口**

```bash
curl -s -H "Authorization: Bearer <用户真实key>" https://ollama.com/api/usage
```

- [ ] **Step 2: 同日登录 ollama.com 官网 usage 页，人工对照 session/weekly 两个数字与官网展示**

把 curl 原始响应（脱敏 key）、官网截图路径、对照结论写进 `docs/verify-usage.md`。三种可能结论：口径一致（继续）；只给份额无绝对值（UI 标注文案定为"官方未文档化接口的份额口径"）；对不上（停下回报用户重新决策展示口径——spec §9 退出条件）。

- [ ] **Step 3: Commit**

```bash
git add docs/verify-usage.md && git commit -m "docs: usage 语义对照官网验证记录"
```

---

## 后端线（Task A1~A8）——开始前提示用户切换模型

---

### Task A1: 契约类型 types.rs

**Files:**
- Create: `src-tauri/src/types.rs`
- Modify: `src-tauri/src/main.rs`（加 `mod types;`）
- Test: `src-tauri/tests/contract_types.rs`

**Interfaces:**
- Produces: `ModelStat` / `UsageSnapshot` / `KeyState` / `AppState` / `Sample` / `FailureLevel` / `RefreshResult`（serde snake_case，序列化形状 = spec §5.3，供本线全部后续任务使用）

- [ ] **Step 1: 写失败测试**

```rust
// src-tauri/tests/contract_types.rs
use serde_json::json;

#[test]
fn usage_snapshot_serializes_to_contract_shape() {
    let snap = ollamabar_lib::types::UsageSnapshot {
        session_pct: Some(9.2),
        weekly_pct: None,
        session_reset_est: Some("2026-09-15T10:00:00Z".into()),
        weekly_reset_est: None,
        session_models: vec![ollamabar_lib::types::ModelStat { name: "gpt-oss:120b".into(), request_count: 242 }],
        weekly_models: vec![],
        server_time: Some("2026-09-15T06:29:56Z".into()),
        fetched_at: "2026-09-15T06:30:00Z".into(),
        last_success_at: Some("2026-09-15T06:30:00Z".into()),
        failure_level: ollamabar_lib::types::FailureLevel::None,
        error_message: None,
    };
    let v = serde_json::to_value(&snap).unwrap();
    assert_eq!(v["session_pct"], json!(9.2));
    assert_eq!(v["weekly_pct"], json!(null));
    assert_eq!(v["failure_level"], json!("none"));
    assert_eq!(v["session_models"][0]["request_count"], json!(242));
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: 编译失败，找不到 `ollamabar_lib::types`。

- [ ] **Step 3: 最小实现**

脚手架的 `src-tauri/src/lib.rs` 里 `pub mod` 声明各模块（后续任务逐个加）。先建 `types.rs`：

```rust
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
```

注意：脚手架默认 `lib.rs` crate 名是 `<appname>_lib`，确认 `Cargo.toml` 中 `name = "ollamabar_lib"`（`main.rs` 的 `generate_context!` 与此联动，改名后同步检查编译）。

- [ ] **Step 4: 运行确认通过** — `cargo test --manifest-path src-tauri/Cargo.toml` 全绿。

- [ ] **Step 5: Commit**

```bash
git add src-tauri && git commit -m "feat: 契约类型与序列化形状"
```

---

### Task A2: reset.rs 重置推算（TDD，移植已验证规则）

**Files:**
- Create: `src-tauri/src/reset.rs`
- Test: `src-tauri/src/reset.rs` 内 `#[cfg(test)]`（用例移植自 cc-switch-hub 已验证边界）

**Interfaces:**
- Produces: `next_weekly_reset(now: DateTime<Utc>) -> DateTime<Utc>`、`next_session_reset(now: DateTime<Utc>) -> DateTime<Utc>`

- [ ] **Step 1: 写失败测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc, Datelike};

    #[test]
    fn weekly_恰在周一零点取下周一() {
        // 2026-09-14 是周一
        let now = Utc.with_ymd_and_hms(2026, 9, 14, 0, 0, 0).unwrap();
        let r = next_weekly_reset(now);
        assert_eq!(r, Utc.with_ymd_and_hms(2026, 9, 21, 0, 0, 0).unwrap());
    }

    #[test]
    fn weekly_周日深夜取次日周一零点() {
        let now = Utc.with_ymd_and_hms(2026, 9, 13, 23, 0, 0).unwrap(); // 周日
        let r = next_weekly_reset(now);
        assert_eq!(r.weekday(), chrono::Weekday::Mon);
        assert!(r > now);
    }

    #[test]
    fn session_桶边界前取本桶上沿() {
        // 桶边界随 Unix epoch 对齐且逐日漂移，不假设某日 05:00 恰是边界，从桶直接构造
        let boundary = Utc.timestamp_opt(18000i64 * 12345, 0).unwrap();
        let now = boundary - chrono::Duration::seconds(1);
        assert_eq!(next_session_reset(now), boundary);
    }

    #[test]
    fn session_恰在边界取下一桶() {
        let boundary = Utc.timestamp_opt(18000i64 * 12345, 0).unwrap();
        assert_eq!(next_session_reset(boundary), boundary + chrono::Duration::seconds(18000));
    }

    #[test]
    fn session_跨日漂移不写死小时() {
        // 桶边界不与每日固定小时列表对齐：任意桶的下一沿就是 +5h，不依赖当天墙钟
        let b = Utc.timestamp_opt(18000i64 * 12345, 0).unwrap();
        let now = b + chrono::Duration::seconds(1);
        assert_eq!(next_session_reset(now), b + chrono::Duration::seconds(18000));
    }
}
```

- [ ] **Step 2: 运行确认失败** — `cargo test --manifest-path src-tauri/Cargo.toml reset` 编译失败。

- [ ] **Step 3: 实现（移植 cc-switch-hub quota_fetcher.py 已验证规则）**

```rust
// 重置时间推算：接口不返回 reset_at，属客户端推算（H2），显示层须标"预计"
use chrono::{DateTime, Datelike, TimeZone, Utc};

/// 严格晚于 now 的下一个周一 00:00 UTC
pub fn next_weekly_reset(now: DateTime<Utc>) -> DateTime<Utc> {
    let mut d = Utc.from_utc_datetime(&now.date_naive().and_hms_opt(0, 0, 0).unwrap());
    while d.weekday() != chrono::Weekday::Mon || d <= now {
        d += chrono::Duration::days(1);
    }
    d
}

/// 严格晚于 now 的下一个 5 小时 Unix bucket 边界
pub fn next_session_reset(now: DateTime<Utc>) -> DateTime<Utc> {
    const WINDOW: i64 = 5 * 3600;
    let ts = now.timestamp();
    Utc.timestamp_opt((ts / WINDOW + 1) * WINDOW, 0).unwrap()
}
```

`lib.rs` 加 `pub mod reset;`。

- [ ] **Step 4: 运行确认通过** — 全部 reset 测试绿。

- [ ] **Step 5: Commit** — `git commit -m "feat: 重置时间推算与边界测试"`

---

### Task A3: ollama.rs 取数与结构校验

**Files:**
- Create: `src-tauri/src/ollama.rs`
- Test: `src-tauri/src/ollama.rs` 内 `#[cfg(test)]`

**Interfaces:**
- Produces: `RawUsage { session_usage, weekly_usage, session_models, weekly_models, server_time }`、`FetchError { Network, Auth, Http, Parse }`、`parse_usage(body: &str) -> Option<RawUsage>`、`fetch_usage(client: &reqwest::Client, url: &str, api_key: &str) -> Result<RawUsage, FetchError>`

- [ ] **Step 1: 写失败测试（正常 fixture + 四类拒绝）**

```rust
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
```

- [ ] **Step 2: 运行确认失败**

- [ ] **Step 3: 实现**

```rust
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

#[derive(Deserialize)]
struct ModelRaw { name: String, request_count: i64 }

#[derive(Deserialize)]
struct WindowRaw { usage: serde_json::Number, #[serde(default)] models: Vec<ModelRaw> }

#[derive(Deserialize)]
struct RootRaw {
    limits: struct { session: WindowRaw, weekly: WindowRaw },
    activity: struct { period: struct { #[serde(default)] ending_at: Option<String> } },
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
```

`Cargo.toml` 增依赖：`reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "json"] }`（`serde_json`、`tokio` 脚手架已有则复用）。`lib.rs` 加 `pub mod ollama;`。

- [ ] **Step 4: 运行确认通过** — `cargo test` 全绿。

- [ ] **Step 5: Commit** — `git commit -m "feat: ollama 取数与结构校验"`

---

### Task A4: store.rs SQLite 单写者

**Files:**
- Create: `src-tauri/src/store.rs`
- Test: `src-tauri/src/store.rs` 内 `#[cfg(test)]`

**Interfaces:**
- Produces: `Store::open(path) -> rusqlite::Result<Store>`（建表 + WAL）、`Store::open_readonly(path)`、`insert_sample(&SampleRow)`、`insert_reset_event(alias, observed_at, kind)`、`history(alias, hours) -> Vec<SampleRow>`；`SampleRow { alias, fetched_at, session_pct, weekly_pct, session_models, weekly_models, server_time }`

- [ ] **Step 1: 写失败测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ModelStat;

    fn row(alias: &str, ts: &str, pct: f64) -> SampleRow {
        SampleRow {
            alias: alias.into(), fetched_at: ts.into(),
            session_pct: pct, weekly_pct: pct * 2.0,
            session_models: vec![ModelStat { name: "m".into(), request_count: 1 }],
            weekly_models: vec![], server_time: None,
        }
    }

    #[test]
    fn 读写往返与时间过滤() {
        let mut store = Store::open_in_memory().unwrap();
        store.insert_sample(&row("k1", "2026-09-15T06:00:00Z", 5.0)).unwrap();
        store.insert_sample(&row("k1", "2026-09-15T07:00:00Z", 6.0)).unwrap();
        store.insert_sample(&row("k2", "2026-09-15T07:00:00Z", 1.0)).unwrap();
        let h = store.history("k1", 24).unwrap();
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].fetched_at, "2026-09-15T06:00:00Z"); // 升序
        let h2 = store.history("k2", 24).unwrap();
        assert_eq!(h2.len(), 1);
    }

    #[test]
    fn 重置事件入库() {
        let mut store = Store::open_in_memory().unwrap();
        store.insert_reset_event("k1", "2026-09-15T05:00:00Z", "session").unwrap();
        let n: i64 = store.conn.query_row(
            "SELECT COUNT(*) FROM reset_events WHERE alias='k1' AND kind='session'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }
}
```

- [ ] **Step 2: 运行确认失败**

- [ ] **Step 3: 实现**

```rust
// SQLite 单写者：轮询结果全部经 mpsc 汇入唯一写者任务（见 poller），本类型非 Sync 无妨
use crate::types::{ModelStat, Sample};
use chrono::Utc;
use rusqlite::Connection;

#[derive(Debug, Clone)]
pub struct SampleRow {
    pub alias: String, pub fetched_at: String,
    pub session_pct: f64, pub weekly_pct: f64,
    pub session_models: Vec<ModelStat>, pub weekly_models: Vec<ModelStat>,
    pub server_time: Option<String>,
}

impl From<SampleRow> for Sample {
    fn from(r: SampleRow) -> Sample {
        Sample { ts: r.fetched_at, session_pct: r.session_pct,
                 weekly_pct: r.weekly_pct, session_models: r.session_models }
    }
}

pub struct Store { pub conn: Connection }

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS samples (
  id INTEGER PRIMARY KEY,
  alias TEXT NOT NULL,
  fetched_at TEXT NOT NULL,
  session_pct REAL, weekly_pct REAL,
  session_models_json TEXT, weekly_models_json TEXT,
  server_time TEXT
);
CREATE INDEX IF NOT EXISTS idx_samples_alias_ts ON samples(alias, fetched_at);
CREATE TABLE IF NOT EXISTS reset_events (
  id INTEGER PRIMARY KEY,
  alias TEXT NOT NULL,
  observed_at TEXT NOT NULL,
  kind TEXT NOT NULL
);
";

impl Store {
    pub fn open(path: &str) -> rusqlite::Result<Store> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Store { conn })
    }

    pub fn open_in_memory() -> rusqlite::Result<Store> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Ok(Store { conn })
    }

    pub fn insert_sample(&mut self, r: &SampleRow) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO samples (alias, fetched_at, session_pct, weekly_pct, session_models_json, weekly_models_json, server_time)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![r.alias, r.fetched_at, r.session_pct, r.weekly_pct,
                serde_json::to_string(&r.session_models).unwrap_or_default(),
                serde_json::to_string(&r.weekly_models).unwrap_or_default(),
                r.server_time],
        )?;
        Ok(())
    }

    pub fn insert_reset_event(&mut self, alias: &str, observed_at: &str, kind: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO reset_events (alias, observed_at, kind) VALUES (?1, ?2, ?3)",
            rusqlite::params![alias, observed_at, kind])?;
        Ok(())
    }

    pub fn history(&self, alias: &str, hours: i64) -> rusqlite::Result<Vec<SampleRow>> {
        let cutoff = (Utc::now() - chrono::Duration::hours(hours)).to_rfc3339();
        let mut stmt = self.conn.prepare(
            "SELECT fetched_at, session_pct, weekly_pct, session_models_json, server_time
               FROM samples WHERE alias=?1 AND fetched_at >= ?2 ORDER BY fetched_at ASC")?;
        let rows = stmt.query_map(rusqlite::params![alias, cutoff], |r| {
            let models_json: String = r.get(3)?;
            SampleRow {
                alias: alias.into(), fetched_at: r.get(0)?,
                session_pct: r.get(1)?, weekly_pct: r.get(2)?,
                session_models: serde_json::from_str(&models_json).unwrap_or_default(),
                weekly_models: vec![], server_time: r.get(4)?,
            }
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}
```

`Cargo.toml` 增 `rusqlite = { version = "0.32", features = ["bundled"] }`。`lib.rs` 加 `pub mod store;`。

- [ ] **Step 4: 运行确认通过**

- [ ] **Step 5: Commit** — `git commit -m "feat: SQLite 存储与历史查询"`

---

### Task A5: config.rs + state.rs 状态机（H4/H5/H6）

**Files:**
- Create: `src-tauri/src/config.rs`、`src-tauri/src/state.rs`
- Test: 各自内嵌 `#[cfg(test)]`

**Interfaces:**
- Consumes: `RawUsage`/`FetchError`（A3）、`reset.rs`（A2）、`UsageSnapshot` 等（A1）
- Produces: `AppConfig { keys: Vec<KeyConfig{alias, api_key}>, poll_interval_secs }` + `load/save`；`KeyRuntime`（apply_success / apply_failure / build_snapshot / detect_reset_event / server_time_stalled）

- [ ] **Step 1: 写失败测试**

```rust
// config.rs 内嵌测试
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 配置读写往返() {
        let dir = std::env::temp_dir().join(format!("ollamabar-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let cfg = AppConfig { keys: vec![KeyConfig { alias: "工作".into(), api_key: "sk-x".into() }], poll_interval_secs: 60 };
        save(&path, &cfg).unwrap();
        assert_eq!(load(&path).unwrap(), cfg);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn 缺文件返回默认空配置() {
        let path = std::env::temp_dir().join("ollamabar-nonexist-xyz.json");
        assert!(load(&path).unwrap().keys.is_empty());
    }
}

// state.rs 内嵌测试
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ollama::{RawUsage, FetchError};
    use crate::types::ModelStat;
    use chrono::{TimeZone, Utc};

    fn raw(session: f64, server: &str) -> RawUsage {
        RawUsage { session_usage: session, weekly_usage: 0.1,
            session_models: vec![ModelStat { name: "m".into(), request_count: 3 }],
            weekly_models: vec![], server_time: Some(server.into()) }
    }

    #[test]
    fn 成功后失败再到dead三级状态() {
        let mut rt = KeyRuntime::new("k1");
        let t0 = Utc.with_ymd_and_hms(2026, 9, 14, 0, 0, 0).unwrap();
        rt.apply_success(raw(0.1, "2026-09-14T00:00:10Z"), t0);
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
        let t0 = Utc.with_ymd_and_hms(2026, 9, 14, 0, 0, 0).unwrap();
        rt.apply_failure(FetchError::Auth, t0);
        assert_eq!(rt.failure_level(t0), FailureLevel::InvalidKey);
    }

    #[test]
    fn 服务器时间戳连续5次不前进判漂移() {
        let mut rt = KeyRuntime::new("k1");
        let t0 = Utc.with_ymd_and_hms(2026, 9, 14, 0, 0, 0).unwrap();
        for i in 0..5 {
            rt.apply_success(raw(0.1, "2026-09-14T00:00:10Z"), t0 + chrono::Duration::minutes(i));
        }
        assert!(rt.server_time_stalled());
        let snap = rt.build_snapshot(t0);
        assert_eq!(snap.failure_level, FailureLevel::Degraded);
    }

    #[test]
    fn session骤降超5个百分点触发重置事件() {
        let mut rt = KeyRuntime::new("k1");
        let t0 = Utc.with_ymd_and_hms(2026, 9, 14, 4, 0, 0).unwrap();
        rt.apply_success(raw(0.4, "2026-09-14T04:00:00Z"), t0);
        let ev = rt.apply_success(raw(0.02, "2026-09-14T05:00:00Z"), t0 + chrono::Duration::hours(1));
        assert!(matches!(ev, Some(ResetEvent { kind: ResetKind::Session, .. })));
        // 小幅波动（0.02 → 0.05）不算重置
        let ev2 = rt.apply_success(raw(0.05, "2026-09-14T05:00:10Z"), t0 + chrono::Duration::hours(1));
        assert!(ev2.is_none());
    }
}
```

- [ ] **Step 2: 运行确认失败**

- [ ] **Step 3: 实现 config.rs**

```rust
// 明文配置（用户已知情接受，spec 决策3）：写入尽力设 owner-only 权限
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KeyConfig { pub alias: String, pub api_key: String }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppConfig {
    pub keys: Vec<KeyConfig>,
    pub poll_interval_secs: u64,
}

impl Default for AppConfig {
    fn default() -> Self { AppConfig { keys: vec![], poll_interval_secs: 60 } }
}

pub fn load(path: &Path) -> std::io::Result<AppConfig> {
    match std::fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(AppConfig::default()),
        Err(e) => Err(e),
    }
}

pub fn save(path: &Path, cfg: &AppConfig) -> std::io::Result<()> {
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
    std::fs::write(path, serde_json::to_string_pretty(cfg)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    // Windows：无简洁的可移植 owner-only 方案，接受明文本就是决策3的立场
    Ok(())
}
```

- [ ] **Step 4: 实现 state.rs**

```rust
// 三级失效状态机（H4）+ 服务器时间戳漂移检测（H5）+ 骤降重置事件检测（H6）
use crate::ollama::{FetchError, RawUsage};
use crate::reset::{next_session_reset, next_weekly_reset};
use crate::types::{FailureLevel, ModelStat, UsageSnapshot};
use chrono::{DateTime, Duration, Utc};

pub const DEAD_AFTER: Duration = Duration::hours(24);
pub const STALL_TICKS: usize = 5;
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
    recent_server_times: Vec<String>,
    prev_session_pct: Option<f64>,
}

impl KeyRuntime {
    pub fn new(alias: &str) -> KeyRuntime {
        KeyRuntime { alias: alias.into(), last_snapshot: None, last_success_at: None,
                     first_failure_at: None, last_err: None,
                     recent_server_times: vec![], prev_session_pct: None }
    }

    /// 成功采样：组装快照、检测 H5 停滞与 H6 骤降，返回可能的实测重置事件
    pub fn apply_success(&mut self, raw: RawUsage, now: DateTime<Utc>) -> Option<ResetEvent> {
        let session_pct = raw.session_usage * 100.0;
        let weekly_pct = raw.weekly_usage * 100.0;

        // H6：session 从高位骤降视为实测窗口重置
        let reset_event = match self.prev_session_pct {
            Some(prev) if prev - session_pct > DROP_THRESHOLD && session_pct < prev =>
                Some(ResetEvent { kind: ResetKind::Session, observed_at: now }),
            _ => None,
        };
        self.prev_session_pct = Some(session_pct);

        // H5：ending_at 连续 STALL_TICKS 次不变
        match &raw.server_time {
            Some(t) if self.recent_server_times.last().map(|x| x == t).unwrap_or(false) => {
                self.recent_server_times.push(t.clone());
            }
            _ => self.recent_server_times = raw.server_time.clone().into_iter().collect(),
        }

        self.last_snapshot = Some(UsageSnapshot {
            session_pct: Some(session_pct),
            weekly_pct: Some(weekly_pct),
            session_reset_est: Some(next_session_reset(now).to_rfc3339()),
            weekly_reset_est: Some(next_weekly_reset(now).to_rfc3339()),
            session_models: raw.session_models,
            weekly_models: raw.weekly_models,
            server_time: raw.server_time,
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

    pub fn server_time_stalled(&self) -> bool {
        self.recent_server_times.len() >= STALL_TICKS
            && self.recent_server_times.iter().all(|t| Some(t) == self.recent_server_times.first())
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
                session_pct: None, weekly_pct: None,
                session_reset_est: None, weekly_reset_est: None,
                session_models: vec![], weekly_models: vec![],
                server_time: None, fetched_at: now.to_rfc3339(),
                last_success_at: None, failure_level: FailureLevel::None,
                error_message: None,
            },
        };
        let mut level = self.failure_level(now);
        if self.server_time_stalled() && level == FailureLevel::None {
            level = FailureLevel::Degraded; // H5：HTTP 成功但语义漂移
        }
        snap.failure_level = level;
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
```

`lib.rs` 加 `pub mod config; pub mod state;`。

- [ ] **Step 5: 运行确认通过** — 全绿（含既有测试）。

- [ ] **Step 6: Commit** — `git commit -m "feat: 配置读写与三级失效状态机"`

---

### Task A6: poller.rs 每 key 轮询任务

**Files:**
- Create: `src-tauri/src/poller.rs`
- Test: `src-tauri/src/poller.rs` 内 `#[cfg(test)]`（tokio::test）

**Interfaces:**
- Consumes: `ollama::fetch_usage`、`state::KeyRuntime`、`store::Store`（A3/A5/A4）
- Produces: `start_key_task(alias, api_key, cfg, fetch, store_tx, app_state, app_handle)`；`PollerMsg::RefreshNow(oneshot::Sender<RefreshResult>)` 经 per-key mpsc 发送；`StoreMsg { Sample(SampleRow), ResetEvent { alias, observed_at, kind } }`

- [ ] **Step 1: 写失败测试**

```rust
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
```

- [ ] **Step 2: 运行确认失败**

- [ ] **Step 3: 实现**

```rust
// 每 key 一个任务：interval + select 手动刷新；MissedTickBehavior::Delay 防休眠唤醒补发
use crate::ollama::{fetch_usage, FetchError, RawUsage};
use crate::state::{KeyRuntime, ResetKind};
use crate::store::{SampleRow, StoreMsg};
use crate::types::{AppState, KeyState, RefreshResult};
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
    let client = reqwest::Client::new(); // 仅用于构造下方真实 FetchFn；do_fetch 只认注入的 FetchFn
    let mut last_req = tokio::time::Instant::now() - cfg.min_gap - cfg.min_gap; // 双倍回拨：允许首拍与首个手动刷新立即执行，避免 2s 边界 flaky
    loop {
        tokio::select! {
            _ = interval.tick() => {
                if last_req.elapsed() < cfg.min_gap { continue; } // 与手动刷新共享限流窗口
                last_req = tokio::time::Instant::now();
                do_fetch(&alias, &api_key, &fetch, &store_tx, &app_state).await;
            }
            Some(msg) = rx.recv() => match msg {
                PollerMsg::Stop => return, // remove_key 停任务
                PollerMsg::RefreshNow(ack) => {
                    if last_req.elapsed() < cfg.min_gap {
                        let _ = ack.send(RefreshResult::RateLimited);
                        continue;
                    }
                    last_req = tokio::time::Instant::now();
                    let ok = do_fetch(&alias, &api_key, &fetch, &client, &store_tx, &app_state).await;
                    let _ = ack.send(if ok { RefreshResult::Updated } else { RefreshResult::Failed });
                }
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
    let mut rt = app_state.lock().unwrap();
    let Some(runtime) = rt.get_mut(alias) else { return false };
    match result {
        Ok(raw) => {
            if let Some(ev) = runtime.apply_success(raw.clone(), now) { // H6：实测重置入库
                let _ = store_tx.send(StoreMsg::ResetEvent {
                    alias: alias.into(), observed_at: ev.observed_at.to_rfc3339(),
                    kind: match ev.kind { ResetKind::Session => "session", ResetKind::Weekly => "weekly" }.into(),
                }).await;
            }
            let _ = store_tx.send(StoreMsg::Sample(SampleRow {
                alias: alias.into(), fetched_at: now.to_rfc3339(),
                session_pct: raw.session_usage * 100.0, weekly_pct: raw.weekly_usage * 100.0,
                session_models: raw.session_models, weekly_models: raw.weekly_models,
                server_time: raw.server_time,
            })).await;
            true
        }
        Err(e) => { runtime.apply_failure(e, now); false }
    }
}
```

**注意**：`FetchFn` 签名统一为 `Arc<dyn Fn(String) -> Pin<Box<dyn Future<Output = Result<RawUsage, FetchError>> + Send>>>`——测试注入假实现（忽略 key 返回固定 `RawUsage`）；真机任务在 A7 装配时构造真实闭包：捕获自己的 `reqwest::Client`，内部调 `fetch_usage(&client, DEFAULT_URL, &key)`。`StoreMsg` 定义在 `store.rs`：

```rust
// store.rs 追加
pub enum StoreMsg {
    Sample(SampleRow),
    ResetEvent { alias: String, observed_at: String, kind: String },
}
```

`lib.rs` 加 `pub mod poller;`。

- [ ] **Step 4: 运行确认通过** — 两个 poller 测试绿。

- [ ] **Step 5: Commit** — `git commit -m "feat: 每 key 轮询任务与限流"`

---

### Task A7: commands.rs + tray.rs + main.rs 装配

**Files:**
- Create: `src-tauri/src/commands.rs`、`src-tauri/src/tray.rs`
- Modify: `src-tauri/src/lib.rs`（装配）、`src-tauri/tauri.conf.json`（窗口/托盘配置）、`src-tauri/icons/`（托盘图标）

**Interfaces:**
- Consumes: 线 A 前全部任务；契约（spec §5）
- Produces: 5 个 command + `state-changed` 事件 + 托盘（含关窗 hide）

- [ ] **Step 1: commands.rs（错误返回串，禁 panic，禁 key 泄漏）**

```rust
use crate::config::{self, AppConfig, KeyConfig};
use crate::state::KeyRuntime;
use crate::store::{self, StoreMsg};
use crate::types::{AppState, KeyState, RefreshResult, Sample};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

// 应用级共享状态（在 lib.rs setup 中构造，字段见下）
pub struct Ctx {
    pub config_path: PathBuf,
    pub db_path: PathBuf,
    pub runtime: Arc<Mutex<HashMap<String, KeyRuntime>>>,      // poller 共用
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

pub fn emit_state(app: &AppHandle, ctx: &Ctx) {
    let _ = app.emit("state-changed", build_state(ctx)); // 契约事件：每次轮询后都推
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
    cfg.keys.push(KeyConfig { alias: alias.clone(), api_key: api_key.trim().to_string() });
    config::save(&ctx.config_path, &cfg).map_err(|e| e.to_string())?;
    crate::spawn_key_task(&app, ctx, &cfg.keys.iter().find(|k| k.alias == alias).unwrap());
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
```

- [ ] **Step 2: tray.rs（托盘菜单纯文字归后端；关窗拦截为 hide）**

```rust
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, WindowEvent};

pub fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "打开仪表盘", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

pub fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") { let _ = w.show(); let _ = w.set_focus(); }
}

/// 关窗不退出（hide），托盘常驻继续轮询——这是 spec 架构承诺的验收项
pub fn handle_window_event(window: &tauri::WebviewWindow, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        let _ = window.hide();
    }
}
```

- [ ] **Step 3: lib.rs 装配 + tauri.conf.json 调整**

`lib.rs` 的 `run()` 中：构造 `Ctx`（`config_path`/`db_path` 用 `app.path().app_config_dir()` / `app_config_dir().join("history.db")`）；启动 SQLite 单写者任务（`tokio::spawn`，持有 `Store::open`，循环 `store_rx.recv()` 分发 `insert_sample`/`insert_reset_event`）；读配置为每个 key `spawn_key_task`（构造真实 `FetchFn` 闭包包 `fetch_usage(&client, DEFAULT_URL, &key)`，`PollConfig { interval: poll_interval_secs, min_gap: 2s }`，存 tx 进 `poll_tx`）；注册 5 个 command；`.on_window_event(tray::handle_window_event)`；setup 回调里 `build_tray`。每 key 轮询完成后调 `emit_state`——把 `emit_state` 挂在 store 写者收到消息后（或 poller 侧回调），保证"每次轮询完成都推"。

`tauri.conf.json`：`productName: "OllamaBar"`；窗口配置加 `"resizable": true`；确认 `bundle.icon` 已指向 icons。托盘图标：Windows 用 ico、macOS 用 template PNG（脚手架自带 icons 可先用，mac 构建时再换 template 版）。

- [ ] **Step 4: 验证**

```bash
cargo test --manifest-path src-tauri/Cargo.toml && cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: 全绿 + check 无错。

- [ ] **Step 5: Commit** — `git commit -m "feat: commands、托盘与装配"`

---

### Task A8: 后端线自验（手动清单前半）

- [ ] **Step 1: `npm run tauri dev` 起跑**

Expected: 托盘图标出现；窗口可开；**点窗口 X → 进程不退、托盘仍在、日志显示轮询循环继续**（验收项 1 前半）。

- [ ] **Step 2: 在默认前端页开 DevTools console 手动验证契约**

先在 `tauri.conf.json` 设 `"app": { "withGlobalTauri": true }`（重跑 dev 生效），console 中执行：

```js
window.__TAURI__.core.invoke('get_state')
```

Expected: 返回 `AppState` JSON，字段与 spec §5.3 一致；`add_key` 后 `state-changed` 事件能被 `window.__TAURI__.event.listen('state-changed', console.log)` 收到。

- [ ] **Step 3: Commit（如有微调）** — `git commit -m "test: 后端契约手动自验通过"`

---

## UI 线（Task B1~B4）——开始前提示用户切换模型

---

### Task B1: 前端骨架 + 契约类型 + mock 层

**Files:**
- Create: `src/types.ts`、`src/api.ts`、`src/mock.ts`、`src/main.ts`、`src/styles.css`
- Modify: `index.html`（挂 `#app` 容器）；删模板示例文件

**Interfaces:**
- Consumes: spec §5.3 契约（TS 镜像，字段一字不差）
- Produces: `getState()` / `getHistory(alias, hours)` / `addKey` / `removeKey` / `refreshNow` / `onStateChanged(cb)`（mock 与真机同签名，C1 切换零改动）

- [ ] **Step 1: types.ts（契约镜像，注意无分号风格）**

```ts
export interface ModelStat { name: string, request_count: number }

export interface UsageSnapshot {
  session_pct: number | null
  weekly_pct: number | null
  session_reset_est: string | null
  weekly_reset_est: string | null
  session_models: ModelStat[]
  weekly_models: ModelStat[]
  server_time: string | null
  fetched_at: string
  last_success_at: string | null
  failure_level: 'none' | 'degraded' | 'invalid_key' | 'dead'
  error_message: string | null
}

export interface KeyState { alias: string, snapshot: UsageSnapshot | null }

export interface AppState { keys: KeyState[], generated_at: string }

export interface Sample {
  ts: string
  session_pct: number
  weekly_pct: number
  session_models: ModelStat[]
}

export type RefreshResult = 'updated' | 'rate_limited' | 'failed'
```

- [ ] **Step 2: api.ts（真机/mock 自动切换）+ mock.ts**

```ts
// src/api.ts
import type { AppState, RefreshResult, Sample } from './types'
import { mockGetState, mockGetHistory, mockAddKey, mockRemoveKey, mockRefreshNow, mockOnStateChanged } from './mock'

// 纯前端 dev server 下无 Tauri 注入，自动走 mock；真机走契约
const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

export async function getState(): Promise<AppState> {
  if (!inTauri) return mockGetState()
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke('get_state')
}

export async function getHistory(alias: string, hours: number): Promise<Sample[]> {
  if (!inTauri) return mockGetHistory(alias, hours)
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke('get_history', { alias, hours })
}

export async function addKey(alias: string, apiKey: string): Promise<void> {
  if (!inTauri) return mockAddKey(alias, apiKey)
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke('add_key', { alias, apiKey })
}

export async function removeKey(alias: string): Promise<void> {
  if (!inTauri) return mockRemoveKey(alias)
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke('remove_key', { alias })
}

export async function refreshNow(alias: string): Promise<RefreshResult> {
  if (!inTauri) return mockRefreshNow(alias)
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke('refresh_now', { alias })
}

export function onStateChanged(cb: (s: AppState) => void): () => void {
  if (!inTauri) return mockOnStateChanged(cb)
  let un = () => {}
  import('@tauri-apps/api/event').then(({ listen }) => {
    listen('state-changed', (e) => cb(e.payload)).then((u) => { un = u })
  })
  return () => un()
}
```

```ts
// src/mock.ts —— 覆盖四种 failure_level、空 models、多 key、滞后场景的假数据工厂
import type { AppState, Sample, UsageSnapshot } from './types'

const snap = (over: Partial<UsageSnapshot>): UsageSnapshot => ({
  session_pct: 9.2, weekly_pct: 41.3,
  session_reset_est: new Date(Date.now() + 3 * 3600e3).toISOString(),
  weekly_reset_est: new Date(Date.now() + 2 * 86400e3).toISOString(),
  session_models: [{ name: 'gpt-oss:120b', request_count: 87 }, { name: 'qwen3-coder', request_count: 155 }],
  weekly_models: [{ name: 'qwen3-coder', request_count: 575 }, { name: 'gpt-oss:120b', request_count: 400 }, { name: 'deepseek-v3', request_count: 33 }],
  server_time: new Date().toISOString(), fetched_at: new Date().toISOString(),
  last_success_at: new Date().toISOString(),
  failure_level: 'none', error_message: null,
  ...over,
})

export const mockGetState = (): AppState => ({
  keys: [
    { alias: '工作', snapshot: snap({}) },
    { alias: '个人', snapshot: snap({ session_pct: 0, failure_level: 'degraded', error_message: '数据滞后 12 分钟', session_models: [] }) },
    { alias: '过期key', snapshot: snap({ failure_level: 'invalid_key', error_message: 'key 无效或已撤销' }) },
    { alias: '接口失效', snapshot: snap({ failure_level: 'dead', error_message: '接口可能已失效，上次成功：2026-09-14T08:00:00Z' }) },
  ],
  generated_at: new Date().toISOString(),
})

export const mockGetHistory = (_alias: string, _hours: number): Sample[] => {
  // 24h 假锯齿：每 10 分钟一点，窗口重置处从高跌 0
  const pts: Sample[] = []
  for (let i = 144; i >= 0; i--) {
    const inWindow = i % 30   // 30 点 = 5h 模拟窗口
    pts.push({ ts: new Date(Date.now() - i * 600e3).toISOString(),
      session_pct: inWindow * 1.2, weekly_pct: 30 + (i % 60) * 0.5, session_models: [] })
  }
  return pts
}

export const mockAddKey = async () => {}
export const mockRemoveKey = async () => {}
export const mockRefreshNow = async (): Promise<'updated'> => 'updated'
export const mockOnStateChanged = (cb: (s: AppState) => void) => {
  const t = setInterval(() => cb(mockGetState()), 5000)
  return () => clearInterval(t)
}
```

- [ ] **Step 3: main.ts + styles.css 渲染骨架（先打通数据→DOM）**

```ts
// src/main.ts
import { getState, onStateChanged } from './api'
import type { AppState } from './types'

const app = document.querySelector<HTMLDivElement>('#app')!

function render(state: AppState) {
  app.innerHTML = `<pre>${JSON.stringify(state, null, 2)}</pre>` // B2 换真 UI
}

getState().then(render)
onStateChanged(render)
```

```css
/* src/styles.css */
* { box-sizing: border-box; }
body { margin: 0; font-family: system-ui, sans-serif; background: #11151c; color: #e6edf3; }
#app { padding: 16px; }
```

- [ ] **Step 4: 验证**

```bash
npm run dev
```

Expected: 浏览器显示 mock AppState JSON，每 5s 刷新一次。

- [ ] **Step 5: Commit** — `git commit -m "feat: 前端契约类型与 mock 数据层"`

---

### Task B2: key 卡片 + 环形图 + 重置倒计时（含 H1/H2 标注）

**Files:**
- Create: `src/components/ring.ts`、`src/components/keyCard.ts`
- Modify: `src/main.ts`、`src/styles.css`

**Interfaces:**
- Consumes: B1 的 api/types
- Produces: `renderRing(pct, label)` 返回 SVG 字符串；`renderKeyCard(state: KeyState)` 返回 HTML 字符串

- [ ] **Step 1: ring.ts 手绘 SVG 环形图**

```ts
// 环形进度：纯 SVG，无依赖
export function renderRing(pct: number | null, label: string): string {
  const v = pct == null ? 0 : Math.max(0, Math.min(100, pct))
  const r = 42, c = 2 * Math.PI * r
  const color = v >= 90 ? '#f85149' : v >= 70 ? '#d29922' : '#3fb950'
  return `
  <div class="ring">
    <svg viewBox="0 0 100 100" width="96" height="96">
      <circle cx="50" cy="50" r="${r}" fill="none" stroke="#21262d" stroke-width="10"/>
      <circle cx="50" cy="50" r="${r}" fill="none" stroke="${color}" stroke-width="10"
        stroke-dasharray="${(c * v / 100).toFixed(1)} ${c.toFixed(1)}"
        stroke-linecap="round" transform="rotate(-90 50 50)"/>
      <text x="50" y="47" text-anchor="middle" fill="#e6edf3" font-size="18" font-weight="600">${pct == null ? '--' : Math.round(v)}%</text>
      <text x="50" y="66" text-anchor="middle" fill="#8b949e" font-size="11">${label}</text>
    </svg>
    <span class="caliber">口径以官方未文档化接口为准</span>
  </div>`
}
```

- [ ] **Step 2: keyCard.ts 组装卡片**

```ts
import { renderRing } from './ring'
import type { KeyState } from '../types'

function countdown(iso: string | null): string {
  if (!iso) return '--'
  const ms = new Date(iso).getTime() - Date.now()
  if (ms <= 0) return '即将重置'
  const h = Math.floor(ms / 3600e3), m = Math.floor((ms % 3600e3) / 60e3)
  return `${h}h${m}m 后重置`
}

export function renderKeyCard(ks: KeyState): string {
  const s = ks.snapshot
  if (!s) return `<section class="card"><h3>${ks.alias}</h3><p>尚无数据</p></section>`
  const banner = s.failure_level !== 'none'
    ? `<div class="banner ${s.failure_level}">${s.error_message ?? ''}</div>` : ''
  return `
  <section class="card" data-alias="${ks.alias}">
    <header><h3>${ks.alias}</h3><button class="refresh" data-refresh="${ks.alias}">刷新</button>
      <button class="remove" data-remove="${ks.alias}">删除</button></header>
    ${banner}
    <div class="rings">
      ${renderRing(s.session_pct, '5h 窗口')}
      ${renderRing(s.weekly_pct, '本周')}
    </div>
    <p class="resets">5h：${countdown(s.session_reset_est)}（预计）｜周：${countdown(s.weekly_reset_est)}（预计）</p>
    <div class="models" data-models="${ks.alias}"></div>
  </section>`
}
```

- [ ] **Step 3: main.ts 换真渲染（卡片网格 + 事件委托刷新/删除/查看历史）**

```ts
import { getState, onStateChanged, refreshNow, removeKey } from './api'
import { renderKeyCard } from './components/keyCard'
import type { AppState } from './types'

const app = document.querySelector<HTMLDivElement>('#app')!

function render(state: AppState) {
  const cards = state.keys.map(renderKeyCard).join('')
  app.innerHTML = `
    <div class="grid">${cards}</div>
    <form id="addKey"><input name="alias" placeholder="别名" required>
      <input name="apiKey" placeholder="api_key" required>
      <button type="submit">添加 key</button></form>
    <p class="footnote">重置时间为客户端推算（预计）；数字口径以官方未文档化接口为准</p>`
}

app.addEventListener('click', async (e) => {
  const t = e.target as HTMLElement
  if (t.dataset.refresh) await refreshNow(t.dataset.refresh)
  if (t.dataset.remove && confirm('删除该 key？历史采样保留')) await removeKey(t.dataset.remove)
})

app.addEventListener('submit', async (e) => {
  const form = e.target as HTMLFormElement
  if (form.id !== 'addKey') return
  e.preventDefault()
  const fd = new FormData(form)
  const { addKey } = await import('./api')
  try { await addKey(String(fd.get('alias')), String(fd.get('apiKey'))); form.reset() }
  catch (err) { alert(String(err)) }
})

getState().then(render)
onStateChanged(render)
```

- [ ] **Step 4: 样式**

```css
/* 追加到 src/styles.css */
.grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 12px; }
.card { background: #161b22; border: 1px solid #30363d; border-radius: 8px; padding: 12px; }
.rings { display: flex; gap: 8px; }
.caliber { font-size: 10px; color: #8b949e; }
.banner { border-radius: 6px; padding: 6px 10px; margin-bottom: 8px; font-size: 13px; }
.banner.degraded { background: #3d2e00; color: #d29922; }
.banner.invalid_key { background: #3d1418; color: #f85149; }
.banner.dead { background: #2d0a0d; color: #f85149; font-weight: 600; }
.footnote { font-size: 11px; color: #8b949e; }
```

- [ ] **Step 5: 验证** — `npm run dev`：四张 mock 卡片渲染，两种颜色阈值（0% 灰圈、9.2% 绿、41.3% 绿）、invalid_key/dead 横幅、"（预计）"标注、口径脚注都在。倒计时数字随时间走动。

- [ ] **Step 6: Commit** — `git commit -m "feat: key 卡片与环形图"`

---

### Task B3: 模型条形图 + 锯齿曲线 + 重置边界竖线（H3）

**Files:**
- Create: `src/components/modelBars.ts`、`src/components/historyLine.ts`
- Modify: `src/components/keyCard.ts`（挂载两个图）、`src/main.ts`（渲染后填数据）、`src/styles.css`

**Interfaces:**
- Consumes: B1 `getHistory`、B2 keyCard 挂载点 `div[data-models]` / 新增 `div[data-history]`

- [ ] **Step 1: modelBars.ts**

```ts
import type { ModelStat } from '../types'

export function renderModelBars(models: ModelStat[]): string {
  if (!models.length) return '<p class="empty">本窗口暂无调用</p>'
  const max = Math.max(...models.map((m) => m.request_count))
  return `<ol class="bars">${models.slice(0, 20).map((m) => `
    <li><span class="name" title="${m.name}">${m.name}</span>
      <span class="track"><span class="fill" style="width: ${(m.request_count * 100 / max).toFixed(1)}%"></span></span>
      <span class="count">${m.request_count}</span></li>`).join('')}</ol>`
}
```

- [ ] **Step 2: historyLine.ts（诚实锯齿：折线原样 + 推算窗口边界竖线，不做平滑）**

```ts
import type { Sample } from '../types'

export function renderHistoryLine(samples: Sample[]): string {
  if (samples.length < 2) return '<p class="empty">攒数据中…</p>'
  const W = 560, H = 120, PAD = 24
  const xs = samples.map((s) => new Date(s.ts).getTime())
  const minT = Math.min(...xs), maxT = Math.max(...xs)
  const maxP = Math.max(10, ...samples.map((s) => s.session_pct))
  const x = (t: number) => PAD + (t - minT) / Math.max(1, maxT - minT) * (W - 2 * PAD)
  const y = (p: number) => H - PAD - p / maxP * (H - 2 * PAD)
  // 折线：原样连接，不插值不平均
  const pts = samples.map((s) => `${x(new Date(s.ts).getTime()).toFixed(1)},${y(s.session_pct).toFixed(1)}`).join(' ')
  // 5h 窗口边界竖线：按推算规则从采样范围头开始每 5h 画一条
  const WINDOW = 5 * 3600e3
  let boundaries = ''
  const start = Math.ceil((minT + 1) / WINDOW) * WINDOW  // 与后端 (t/18000+1)*18000 同规则
  for (let t = start; t <= maxT; t += WINDOW) {
    boundaries += `<line x1="${x(t).toFixed(1)}" y1="${PAD}" x2="${x(t).toFixed(1)}" y2="${H - PAD}" stroke="#30363d" stroke-dasharray="4 3"/>`
  }
  return `<svg class="sparkline" viewBox="0 0 ${W} ${H}" preserveAspectRatio="none">
    <line x1="${PAD}" y1="${H - PAD}" x2="${W - PAD}" y2="${H - PAD}" stroke="#30363d"/>
    ${boundaries}
    <polyline points="${pts}" fill="none" stroke="#58a6ff" stroke-width="1.5"/>
  </svg>
  <p class="axis">5h 窗口内累计份额（锯齿为真实形状，虚线为推算重置边界）</p>`
}
```

- [ ] **Step 3: keyCard 挂图** — 卡片 `<div class="models" data-models>` 后追加：

```ts
    <p class="models-title">本周模型调用</p>
    <div class="models" data-models="${ks.alias}"></div>
    <div class="history" data-history="${ks.alias}"></div>
```

`main.ts` 渲染后异步填：

```ts
import { getHistory } from './api'
import { renderModelBars } from './components/modelBars'
import { renderHistoryLine } from './components/historyLine'

async function fillCharts(state: AppState) {
  for (const ks of state.keys) {
    const snap = ks.snapshot
    const box = app.querySelector<HTMLElement>(`[data-models="${ks.alias}"]`)
    if (box && snap) box.innerHTML = renderModelBars(snap.weekly_models)
    const hist = app.querySelector<HTMLElement>(`[data-history="${ks.alias}"]`)
    if (hist) hist.innerHTML = renderHistoryLine(await getHistory(ks.alias, 24))
  }
}
```

`render()` 里卡片注入完成后调 `fillCharts(state)`。

- [ ] **Step 4: 验证** — `npm run dev`：每卡 3 条 weekly 模型条形（qwen3-coder 575 最长）、锯齿折线有可见跳崖与虚线边界、"攒数据中…"（invalid_key 卡无历史时）兜底、20+ 模型只显前 20 条。

- [ ] **Step 5: Commit** — `git commit -m "feat: 模型条形图与锯齿曲线"`

---

### Task B4: 状态矩阵自验 + 边界场景打磨

**Files:**
- Modify: `src/mock.ts`（补边界 mock）、`src/styles.css`（打磨）

- [ ] **Step 1: mock 补齐边界** — 20+ 模型 key、models 空、`session_pct: null`（首拍未成功）、单 key、8 key 超长别名。

- [ ] **Step 2: 浏览器逐一走查**

Expected：空 models 显示"本窗口暂无调用"；null pct 显示 `--` 灰圈；8 卡网格换行不溢出；超长别名 ellipsis；dead 横幅文案含上次成功时间；`session_reset_est` 过期瞬间显示"即将重置"。全部走查通过。

- [ ] **Step 3: Commit** — `git commit -m "test: UI 状态矩阵与边界走查"`

---

## 联调（Task C1）

---

### Task C1: 切真实 invoke + 手动验收清单

**Files:**
- Modify: 无代码改动（api.ts 的 inTauri 判定自动切换）；如出问题修复对应侧

- [ ] **Step 1: 起跑** — `npm run tauri dev`，通过设置表单添加真实 key。

- [ ] **Step 2: 执行 spec §7 验收清单 6 项**

1. 关窗 → 进程存活、托盘在、轮询继续（日志可见）
2. 断网 3 分钟 → "数据滞后"标注、旧数字保留
3. 填错误 key → `invalid_key` 横幅
4. mock 篡改骤降 → `reset_events` 表新增 session 记录（`sqlite3` 查 `history.db`）
5. 休眠 10 分钟唤醒 → 无唤醒瞬间请求风暴（日志时间戳间隔仍 ≥60s）
6. 数字对照 ollama.com 官网 usage 页（复用 Task 1 记录）

- [ ] **Step 3: 修复发现的问题，逐项 commit（`fix: ...`）**

- [ ] **Step 4: 终验**

```bash
cargo test --manifest-path src-tauri/Cargo.toml && npm run build
```

Expected: 全绿，`git log` 每任务一提交，工作区干净。

- [ ] **Step 5: Commit** — `git commit -m "test: 联调验收清单全部通过"`