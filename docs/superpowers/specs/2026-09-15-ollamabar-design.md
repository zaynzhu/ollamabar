# OllamaBar 设计规格

日期：2026-09-15
状态：待用户审批
项目目录：`E:\claudecode\project\ollamabar`（启动时为空目录，本文件为首个文档）

## 1. 项目定位

跨 Windows / macOS 的 Ollama Cloud 用量查看器。托盘常驻 + 仪表盘窗口，支持多个 Ollama API key，展示用量份额、重置时间倒计时、各模型调用次数、历史用量锯齿曲线。**纯只读**：不修改任何工具配置、不代理请求、不埋点上报。

参考形态：steipete/CodexBar（菜单栏用量条，Swift/macOS only）。本项目跨平台，技术选型不同。

## 2. 已定决策（grill 结论，全部经用户确认）

| # | 决策点 | 结论 |
|---|--------|------|
| 1 | 技术栈 | Tauri 2（Rust 后端 + vanilla TS WebView 前端）。本机无 Rust 工具链，实施前需安装 rustup + MSVC Build Tools（任务 #0） |
| 2 | 历史数据立场 | 诚实锯齿曲线：60s 轮询攒 SQLite，原样画窗口内累计值 + 窗口重置边界竖线，不美化不平滑 |
| 3 | key 存储 | 明文 `config.json`（用户已知情接受风险：可读用户目录的程序可拿到 key）。写入时尽量设 owner-only 权限（Unix 0600；Windows 尽力而为并文档标注） |
| 4 | 轮询归属 | Rust 侧 tokio 任务 + rusqlite，窗口关闭（hide）后轮询持续；前端纯展示 |
| 5 | 前端 | vanilla TS + 手绘 SVG（环形/条形）+ uPlot（折线，可选）；无 React/Vue 等框架 |
| 6 | 分发 | 纯自用零分发：不配 CI、不签名、不自动更新。Windows 本机 `cargo tauri build`；macOS 版届时 clone 仓库构建 |

## 3. 数据源与诚实性条款（红队审查产物，具有最高优先级）

唯一数据源：`GET https://ollama.com/api/usage`，Header `Authorization: Bearer <api_key>`。该接口**未文档化**（legacy），前身实现见 `E:\codex\cc-switch-hub\src\quota_fetcher.py` 与 `E:\claudecode\project\ollama\watch-ollama.ps1`。

响应结构（已观测）：

```json
{
  "limits": {
    "session": { "usage": 0.092, "models": [{"name": "...", "request_count": 242}] },
    "weekly":  { "usage": 0.049, "models": [{"name": "...", "request_count": 575}] }
  },
  "activity": { "period": { "ending_at": "2026-09-15T06:29:56Z" } }
}
```

诚实性条款（违反任何一条视为验收不通过）：

- **H1 usage 语义未验证**：`usage`(0~1) 是什么资源的份额未文档化（token？请求数？）。**开发任务 #1**：用真实 key 同日对照 ollama.com 官网 usage 页数字，一致才继续 UI 联调；UI 上所有份额数字旁永久标注"口径以官方未文档化接口为准"。
- **H2 重置时间是推算不是事实**：接口不返回 reset_at。推算规则移植自已验证的 cc-switch-hub 实现：weekly = 严格晚于当前时间的下一个周一 00:00 UTC；session = 下一个 5 小时 Unix bucket 边界（`(ts // 18000 + 1) * 18000`，5h 不整除 24h，窗口跨日漂移，禁止写死每日固定小时列表）。UI 统一标注"预计"。
- **H3 锯齿曲线不修饰**：历史曲线原样呈现窗口内累计值，重置处画竖线；不做平滑、不伪装趋势。
- **H4 失效三级判定，禁止静默成空壳**：
  - `none` → `degraded`：单次/连续请求失败，保留上次成功数据，UI 标注"数据滞后 N 分钟"；
  - `degraded` → `dead`：连续 24 小时无一次成功（任务 #1 验证期可用更短阈值手测），托盘图标变警告态，UI 显眼文案"接口可能已失效，上次成功：X"；
  - `invalid_key`：HTTP 401/403 单列，文案"key 无效或已撤销"。
- **H5 服务器时间戳漂移检测**：HTTP 成功但 `activity.period.ending_at` 连续 5 次不前进，视为接口语义漂移，计为 `degraded` 并告警。
- **H6 推算规则自校验**：轮询中观测到 session_pct 从 >5 个百分点骤降到更低值（窗口实测重置），记录 `reset_events`；实测时刻与推算边界偏差超过 30 分钟则告警"推算规则可能已失效"。

## 4. 架构总览

```
┌─ Rust 后端 ────────────────────────────────┐
│ poller (每 key 一个 tokio task, 60s,        │
│         MissedTickBehavior::Delay,          │
│         每 key 2s 限流带反馈)               │
│   → ollama (取数 + 结构校验, URL 可注入)    │
│   → state (三级失效状态机 + H5/H6 检测)     │
│   → mpsc 单写者 → store (rusqlite, WAL)     │
│ config / reset / commands / tray            │
│ 关窗 close_requested → hide (验收项)        │
└──────────────┬─────────────────────────────┘
               │ invoke / emit(state-changed)
┌──────────────┴─────────────────────────────┐
│ vanilla TS 前端：环形图 / 条形图 / uPlot    │
│ 锯齿折线 / key 管理表单 / 告警与标注        │
│ 开发期用 mock 数据跑纯前端 dev server       │
└────────────────────────────────────────────┘
```

SQLite schema：

```sql
CREATE TABLE samples (
  id INTEGER PRIMARY KEY,
  alias TEXT NOT NULL,
  fetched_at TEXT NOT NULL,        -- ISO8601 UTC
  session_pct REAL, weekly_pct REAL,
  session_models_json TEXT,        -- [{name, request_count}]
  weekly_models_json TEXT,
  server_time TEXT                 -- activity.period.ending_at 原样
);
CREATE INDEX idx_samples_alias_ts ON samples(alias, fetched_at);

CREATE TABLE reset_events (
  id INTEGER PRIMARY KEY,
  alias TEXT NOT NULL,
  observed_at TEXT NOT NULL,
  kind TEXT NOT NULL               -- 'session' | 'weekly'
);
```

`config.json`（用户配置目录，如 `%APPDATA%/ollamabar/`）：

```json
{ "keys": [{"alias": "工作", "api_key": "..."}], "poll_interval_secs": 60 }
```

## 5. 冻结契约（两个实施者只认本节，实现分歧以本节为准）

### 5.1 Commands（前端 invoke）

| command | 参数 | 返回 | 说明 |
|---------|------|------|------|
| `get_state` | — | `AppState` | 所有 key 当前快照 |
| `get_history` | `alias: string, hours: number` | `Sample[]` | 锯齿曲线数据点，按时间升序 |
| `add_key` | `alias: string, api_key: string` | `null` | 写入 config，立即起轮询任务 |
| `remove_key` | `alias: string` | `null` | 停任务、删 key；历史 samples 保留 |
| `refresh_now` | `alias: string` | `RefreshResult` | 立即取数；2s 限流拒绝返回 `rate_limited` |

约定：所有 command 出错时返回串错误信息（如 alias 为空、alias 重复、api_key 为空），不得 panic、不得携带 api_key。2s 限流对同一 key 适用任意来源请求（定时轮询与手动刷新共享该间隔）。

### 5.2 事件（后端 emit → 前端 listen）

| 事件 | payload | 时机 |
|------|---------|------|
| `state-changed` | `AppState` | 每次轮询完成（成功/失败/状态迁移均推） |

### 5.3 数据形状（TypeScript 定义，双端共用语义）

```ts
interface ModelStat { name: string; request_count: number }

interface UsageSnapshot {
  session_pct: number | null        // 0~100
  weekly_pct: number | null
  session_reset_est: string | null  // ISO8601 UTC，推算（H2）
  weekly_reset_est: string | null
  session_models: ModelStat[]
  weekly_models: ModelStat[]
  server_time: string | null        // ending_at 原样（H5）
  fetched_at: string                // 最近一次抓取时刻 ISO8601 UTC
  last_success_at: string | null
  failure_level: 'none' | 'degraded' | 'invalid_key' | 'dead'  // H4
  error_message: string | null      // 不含 key
}

interface KeyState { alias: string; snapshot: UsageSnapshot | null }
interface AppState { keys: KeyState[]; generated_at: string }

interface Sample {
  ts: string                        // ISO8601 UTC
  session_pct: number; weekly_pct: number
  session_models: ModelStat[]
}

type RefreshResult = 'updated' | 'rate_limited' | 'failed'
```

### 5.4 职责边界

- **后端（实施者 A）**：config/ollama/reset/store/poller/state/commands/tray 全部 Rust 代码、托盘图标资源（macOS template PNG + Windows ico）、Rust 单元测试。**托盘菜单的纯文字内容归后端**（Tauri 托盘只能在 Rust 侧构建）。
- **UI（实施者 B）**：窗口内一切——`index.html`/TS/CSS、图表渲染、key 管理表单、H1/H2/H4 标注文案、开发期 mock。
- 两边互不 import，只认 5.1~5.3。

## 6. 错误处理

- 网络/HTTP/JSON/结构校验任何失败：本次写库跳过，`UsageSnapshot` 保留上次成功值，freshness 由 `last_success_at` 与当前时间差推导，错误信息进 `error_message`（不含 key）。
- 结构校验：`usage` 必须是 [0,1] 的 number（排除 bool）；`models` 缺省视为空数组而非失败；`ending_at` 缺失置 null 并记 degraded 依据之一。
- 任何 panic 路径禁止携带 api_key 进日志。

## 7. 测试与验收

**后端单元测试（实施者 A，`cargo test`）**：

- `reset.rs`：移植 cc-switch-hub 已验证边界用例——恰在周一 00:00 UTC 取下周；恰在 bucket 边界取下一 bucket；跨日漂移（连续两天边界列表不同）。
- `ollama.rs`：fixture JSON 正常解析；usage 越界/bool 拒收；models 缺失容错；ending_at 缺失。URL 可注入，对本地 mock HTTP server 做一轮集成测试。
- `store.rs`：内存 SQLite 读写往返；单写者并发写入不丢数据。

**手动验收清单（联调期）**：

1. 关窗 → 进程存活，托盘在，polling 日志继续（对应 H 架构承诺）。
2. 断网 3 分钟 → UI 显示"数据滞后"，旧数字仍在。
3. 填错误 key → `invalid_key` 文案出现。
4. 篡改 mock 使 usage 从 40% 跳 2% → `reset_events` 多一条 session 记录。
5. 电脑休眠 10 分钟唤醒 → 不出现唤醒瞬间连续多请求。
6. 对照 ollama.com 官网 usage 页数字（任务 #1, H1）。

**UI 自验（实施者 B）**：mock 数据下四种 failure_level、空 models、超多模型（20+）、单 key/多 key 布局均渲染正常。

## 8. 交付物清单

- Cargo workspace（Tauri 2 app）+ 前端源码 + 托盘图标（macOS template PNG / Windows ico）
- 本 spec + git 仓库初始化（首个 commit 即本文件）
- 手动验收清单执行记录（联调后填写）

## 9. 退出条件

- `/api/usage` 改结构或下线且无法通过解析层适配恢复 → 项目进入维护终态，`dead` 状态即最终交付形态，不再投入。
- 任务 #1 发现 usage 语义与"份额"假设严重不符且无法确定真实口径 → 暂停 UI 联调，回到用户处重新决策展示口径。

## 10. 明确不做（YAGNI）

- 自动更新、签名、公证、CI/CD、发布渠道
- keychain/加密存储（决策 3 已定明文）
- 跨周长期"最常用模型"统计（samples 表已留 models_json 数据口子，第二版再加聚合查询）
- 用量速率推算曲线、用量预测、通知推送、多语言
- 代理/转发/改写任何 API 请求
