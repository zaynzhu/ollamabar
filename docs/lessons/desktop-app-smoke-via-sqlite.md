# Tauri 托盘应用冒烟验证：不截图，直接查 SQLite 采样库

**结论速览**：后台启动 release exe → 用 python sqlite3 只读查应用的采样库最新行（samples/events）→ 与预期字段值对照；SQLite 库就是应用行为的 ground truth。
适用条件：本项目（或任何"轮询外部 API → 落库"的后台/桌面应用）的真实取数冒烟。

## ✅ 用 history.db 验证新接口三数据源真机可用（2026-10-08）

- **为何值得记**：桌面托盘应用没有 CLI 出口，UI 验证又受本仓"纯文本主模型不收截图"规则限制（见根 CLAUDE.md TMPI 条款）——SQLite 采样库是唯一无侵入、可脚本化的端到端验证面；本次靠它在发布前闭环了"接口字段名未经真机复核"这一最大风险项。
- **最终方案**：
```bash
# 1. 后台启动正式包（首拍立即轮询，第 5 拍带统计，间隔 60s）
./src-tauri/target/release/ollamabar.exe &
# 2. 等首拍后查最新采样（只读连接，与 WAL 写者并发安全）
E:/program/anaconda3/python.exe -c "
import sqlite3
db = sqlite3.connect(r'C:\Users\OMEN\AppData\Roaming\com.ollamabar.app\history.db')
for r in db.execute('SELECT fetched_at, session_pct, requests_24h, server_time FROM samples ORDER BY fetched_at DESC LIMIT 3'):
    print(r)
# 3. 对照错误表：确认启动后无新增 error 事件
for r in db.execute(\"SELECT ts, message FROM events WHERE kind='error' ORDER BY ts DESC LIMIT 3\"):
    print(r)
"
```
- **为什么这样做**：采样行的每个字段都来自一条完整链路（HTTP → serde 解析 → 换算 → 状态机 → 入库），比单测更接近真实；`server_time` 列的值能直接证明服务端 `resets_at` 拿到了（未来时间）；`requests_24h` 只有统计端点轮转成功才会有值——一个查询同时验证三个数据源。
- **适用条件**：Windows（`%APPDATA%\com.ollamabar.app\history.db`；macOS 为 `~/Library/Application Support/com.ollamabar.app/`）；python 侧只用标准库 sqlite3，只读连接不干扰写者。
- **验证证据**（2026-10-08，v0.2.0 发布前冒烟）：
  - 首拍：`session_pct` 26.43 / 27.34（remaining_percent 换算的已用值）、`server_time = 2026-10-08T04:00:00Z`（服务端 resets_at，未来时间 ✓）
  - 第 5 拍后：`requests_24h` 374 / 345（统计端点轮转 ✓）
  - `events` 表最近的"返回结构解析失败"全部是升级前旧版产生（01:45~01:47），新实例启动后零新增
  - 接口字段口径与官方文档的逐项对照记录见 `docs/verify-usage.md`（权威源，本条不复制）
- **交叉验证**：单 agent 单次跑通；验证结论与 `docs/verify-usage.md` 真机复核清单四条一一对应。
- **关键步骤**：确认无旧实例在跑（`tasklist | grep -i ollamabar`，避免双写库）→ 启动新 exe → 等 ≥1 拍查 samples → 等 ≥5 拍再查统计列 → 对照 events 零新增。
- **易错点**：最容易再错的是忘记先查旧实例（双实例同写一个库，数据归属说不清）；以及只看 samples 不看 events——解析失败也照样有"看起来正常"的旧数据躺着。

> 修订 2026-10-08：初版漏了"先确认无旧实例"步骤，补入关键步骤与易错点。