# Ollama Cloud 接口口径验证记录

日期：2026-10-08（接口重构后首次整理）
来源：官方文档 [balance.mdx](https://github.com/ollama/ollama/blob/main/docs/api/balance.mdx)、[cloud-usage.mdx](https://github.com/ollama/ollama/blob/main/docs/api/cloud-usage.mdx)（commit f8646015，2026-10-06/07 重构）
状态：**口径与官方文档一致；真机对照待用户下次运行时确认**（下方样本为官方文档示例，非真实抓包）

## 接口变迁摘要

| 项 | 旧（2026-09 前，未文档化） | 新（2026-10 起，官方文档化） |
|----|--------------------------|------------------------------|
| 额度端点 | `GET /api/usage` 的 `limits.session/weekly.usage`（0~1 已用比例） | `GET /api/balance`：`included.session/weekly.remaining_percent`（0~100 **剩余**） |
| 重置时间 | 接口不返回，客户端推算（周一 00:00 UTC / 5h Unix 桶） | `included.session/weekly.resets_at`（服务端准确 UTC 时间） |
| 模型明细 | `limits.*.models[]`（name + request_count） | **不提供**（官方列 Coming soon） |
| 历史统计 | `activity.period.ending_at`（当前窗口结束时间戳） | `GET /api/usage?range=24h\|7d\|30d`：totals（request_count 等）+ buckets |
| 限流 | 未文档化 | 每用户 10 次/分钟，跨 Key 与设备共享；429 带 Retry-After（秒） |

## /api/balance 响应样本（官方文档示例）

Legacy 套餐（session/weekly 限额）：

```json
{
  "included": {
    "session": { "remaining_percent": 75, "resets_at": "2026-10-01T07:00:00Z" },
    "weekly": { "remaining_percent": 40, "resets_at": "2026-10-05T00:00:00Z" }
  },
  "purchased": { "balance_usd": 25 }
}
```

新计费套餐（美元余额）：

```json
{
  "included": {
    "balance_usd": 72.5,
    "allowance_usd": 100,
    "period": { "from": "2026-09-15T09:30:00Z", "until": "2026-10-15T09:30:00Z" }
  },
  "purchased": { "balance_usd": 25 }
}
```

要点：
- `remaining_percent` 为 0~100 **剩余**百分比，已用 = `100 - remaining_percent`（75 剩余 → 25% 已用）
- `resets_at` 为服务端 UTC 重置时间，倒计时以此为准；缺失才回退客户端推算并标"预计"
- 两种套餐结构互斥：有 `included.session` 即 Legacy；有 `balance_usd + allowance_usd` 即新计费
- Teams 全员共享额度

## /api/usage?range= 响应样本（官方文档示例，Legacy 套餐仅 request_count）

```json
{
  "range": "24h", "scope": "self", "granularity": "hour",
  "from": "2026-09-30T02:00:00Z", "until": "2026-10-01T02:30:00Z",
  "totals": { "request_count": 15, "usage_usd": 0.01718, "input_tokens": 106000, "cached_input_tokens": 46000, "output_tokens": 13600 },
  "buckets": [
    { "from": "2026-10-01T02:00:00Z", "until": "2026-10-01T02:30:00Z", "partial": true, "request_count": 3, "usage_usd": 0.00318 }
  ]
}
```

要点：
- `range=24h|7d|30d`；重复/不支持查询参数返回 400（实现用 range 回显校验防结构漂移）
- Legacy 套餐只有 `request_count`；若桶内含 legacy 请求则省略 cost/token 字段
- 按模型、按 API Key、按团队成员统计均列 Coming soon（模型明细占位即源于此）
- 统计数字非实时（官方注明"新用量可能延迟出现"）

## 本实现的换算与取数口径

- 已用百分比：Legacy `session_pct = 100 - session.remaining_percent`（weekly 同）；新计费 `本期已用% = (allowance_usd - balance_usd) / allowance_usd × 100`
- 历史曲线列：Legacy 存 5h 已用%，新计费存本期已用%（锯齿按周期爬升）
- 统计轮询：每 5 个轮询周期取一个 range，按 24h→7d→30d 轮转（数字最多滞后 15 分钟）
- 限流：全局滑窗 60s 内 ≤8 次（官方 10 减 2 余量），429 按 Retry-After + 2s 暂停该 key

## 真机复核清单（待办）

下次用真实 Key 运行应用时核对：

1. Legacy Key：卡片已用% 与 ollama.com 官网一致；倒计时不带"预计"
2. 倒计时到点后 `resets_at` 前进、无 429 告警
3. （若有新计费 Key）美元余额与官网一致
4. 升级前的旧 history.db 打开后历史曲线与日志正常（迁移自动补列）

## 历史记录

- 2026-09-16：旧 `GET /api/usage`（未文档化）真机对照官网验证通过（`limits.session.usage` 0~1 已用比例与官网百分比一致，模型次数对得上）——该记录对应旧接口，接口已于 2026-10 重构，样本与结论仅存档参考。