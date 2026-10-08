# 遗留项清单（Deferred Minors）

实现过程中经代码审查记录、经裁定暂不处理的低优先级事项。全部不阻塞使用，按优先级大致排列；处理时逐条核对仍然成立再动手。

## 建议择机处理

| # | 位置 | 事项 | 备注 |
|---|------|------|------|
| 1 | `src/api.ts` | `getHistory` 为串行 await 且无超时，某个 key 的请求若长期挂起（不 reject）会阻塞后续 key 的图表填充 | 联调期间未观察到，可加超时或并发化 |
| 2 | `src/api.ts` | `onStateChanged` 动态 import 无 rejection 处理（真机下依赖静态打包不会失败，记录在案） | |
| 3 | `src/main.ts` | `fillCharts` 内 `querySelector` 按 alias 匹配，alias 含 CSS 选择器特殊字符（如引号）会失配——自用别名场景风险极低 | |
| 4 | `src/components/modelBars.ts` | 模型 `name` 未 HTML 转义，遇引号会破坏标记（真实模型名不含引号） | |
| 5 | `src/components/modelBars.ts` | 模型全部 `request_count = 0` 时 `max = 0`，宽度渲染为 `NaN%`（后端不太可能给 0 计数模型） | |
| 6 | `src/components/keyCard.ts` | `countdown` 对非法 ISO 输出 `NaNhNaNm`（契约保证合法，不可达）；`failure_level` 非 none 而 `error_message` 为 null 时渲染空 banner 块（契约上二者同现） | |
| 7 | `src/components/ring.ts` | 0% 卡的零长 `stroke-dasharray` + `stroke-linecap="round"` 在部分渲染器可能在弧起点画出一个圆点（SVG 已知怪癖）——真机观察项 | |
| 8 | `src-tauri/src/state.rs` | `build_snapshot(now)` 的 now 早于 `last_success_at` 时"数据滞后"输出负分钟数（实际调用方不触发，可 `max(0)` 兜底） | |
| 9 | `src-tauri/src/state.rs` | H5 时间戳停滞时 Degraded 文案显示"数据滞后 0 分钟"，语义略困惑（HTTP 成功但语义漂移的场景）——前端文案层可细化 | |
| 10 | `src-tauri/src/state.rs` | `last_err` 为单槽：Auth 失败后再遇 Network 失败，`InvalidKey` 会被 `Degraded` 覆盖显示（网络恢复后下次 Auth 会再翻回） | |
| 11 | `src-tauri/src/store.rs` | `history` 测试只覆盖窗口内侧，建议补一条窗口外旧样本断言其被排除 | |
| 12 | `src-tauri/src/store.rs` | models 序列化用 `unwrap_or_default` 属静默降级路径（当前 `ModelStat` 仅有 String+i64，不可能触发；未来演化时留意） | |
| 13 | `src-tauri/src/commands.rs` | `get_history` 每次调用开新读写连接并执行一遍建表 SCHEMA，已存在的 `open_readonly` 更贴合只读场景 | WAL 下并发安全，纯优化项 |
| 14 | `src-tauri/src/lib.rs` | `KeyRuntime` 在 `spawn_key_task` 与 `run_key_task` 各插入一次（均为全新状态，当前无影响；未来若预热状态会被抹掉） | 观察项 |
| 15 | `src-tauri/src/commands.rs` | `remove_key` 与在途 fetch 存在窗口：任务正在取数时被停，可能补写一条该 alias 的采样进 DB（UI 已不显示，处于"历史保留"语义内） | |
| 16 | `src-tauri/src/poller.rs` | 前端以极高频灌 `RefreshNow` 理论上可推迟该 key 的定时拍（每次限流拒绝即时返回）；UI 交互频率兜底 | |
| 17 | `src-tauri/Cargo.toml` | `description = "A Tauri App"`、`authors = ["you"]` 为模板占位残留 | 一行可改 |
| 18 | `package.json` | `"vite"` 行为 tab 缩进（与其余 2 空格不一致），纯外观 | |
| 19 | `package-lock.json` | registry 指向 npmmirror 镜像（换 registry 时 lock 仍可复用，integrity 校验兜底） | |
| 20 | `src/components/detail.ts` | 弹窗标题与导出提示的 alias 插值进 innerHTML 未转义（与 #3/#4 同类，真实别名可控自用场景风险低） | 2026-09-17 详情弹窗新增 |
| 21 | `src-tauri/src/commands.rs` | `export_log` 的保存对话框与写文件仅在编译+mock 层验证，未真机实测保存路径 | 2026-09-17 新增；下次真机点"导出日志"确认 |
| 22 | `src-tauri/src/store.rs` | samples 表 `session_pct` 列对新计费套餐存"本期已用%"，与 Legacy 的"5h 已用%"语义混用一列（历史曲线目前只画 session_pct，无视觉影响） | 2026-10-08 接口升级引入；长期方案为加专列，见 verify-usage.md |
| 23 | 数据 | 24h/7d/30d 请求统计未入历史曲线（仅卡片/详情/日志行展示）；如需按请求次数画趋势线需扩展 samples 表 | 2026-10-08 新增；官方 buckets 接口已具备数据源 |
| 24 | `src-tauri/src/state.rs` | session 与 weekly 的 resets_at 同 tick 变化时只记录 Session 重置事件（如长时间离线后双窗口同时推进），Weekly 事件丢失 | 2026-10-08 终审发现；apply_success 返回 Vec 可解 |
| 25 | `src-tauri/src/state.rs` | resets_at 字符串不可解析时：from_server 仍标 true、H5 不告警、前端 countdown 显示 `--`，三态均无异常提示 | 2026-10-08 终审发现；低概率（服务端给非法时间戳） |
| 26 | `src-tauri/src/store.rs` | `Sample` 转换层 `weekly_pct.unwrap_or(0.0)` 把缺失周窗口写成 0.0（曲线目前不消费 weekly_pct，无视觉影响） | 2026-10-08 终审发现；与 #22 同类 |
| 27 | `src-tauri/src/ollama.rs` | `UsageStatsData.window_until` 解析后暂无消费方（保留备用字段） | 2026-10-08 终审发现；如长期无用可删 |
| 28 | `src/components/keyCard.ts` | "近24h 请求 N 次"未标注统计滞后（实际最多滞后 15 分钟，docs 已声明） | 2026-10-08 终审发现；可加"约"字或 title 提示 |
| 29 | `src/main.ts` | `refreshNow` 返回的 RefreshResult 未在 UI 消费：429 被拒时点击刷新无反馈（既有行为，非本次引入） | 2026-10-08 终审发现；可加 toast |

## 记录在案、无需处理

- **负时间戳语义偏差**（`reset.rs` session 推算）：Rust 整数除法向零截断与 Python `floor` 除法对 ts < 0 有语义差——实际不可能出现 1970 前的时间戳，继承自参考实现，不返工。
- **契约多类型往返测试未覆盖**：契约测试仅断言 `UsageSnapshot` 序列化形状；`RefreshResult` 的 `rate_limited`、`KeyState.snapshot` 为 null、`Sample` 形状无断言。后续用到时补一个多类型往返测试即可。
- **双 reqwest 版本共存**：本项目 0.12 与 Tauri 内部依赖 0.13 并存于 Cargo.lock（仅记录，收敛需整体升级）。
- **12 字超长别名在当前列宽恰好放得下**：`.card h3` 的 ellipsis 是规则在位兜底，当前数据下不实际截断。
- **mock 相位伪影**（仅开发期）：mock 锯齿重置点按"相对当前时刻"的 5h 网格生成，而边界竖线画在"绝对 Unix epoch"5h 整点网格上——mock 下虚线不压锯齿跳崖是数据相位差，不是绘制公式 bug（公式与后端推算规则严格等价，经数学验证）。

_整理自实现期审查台账（2026-09-16），已完成项（fillCharts 竞态修复、EOF 补齐、动态 import 警告消除、模板残留清理等）不在本清单。_