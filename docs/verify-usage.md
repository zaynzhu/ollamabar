# usage 语义对照官网验证记录（spec H1）

日期：2026-09-16
验证人：用户本人目测官网 + 本记录
结论：**口径一致，H1 满足**——`limits.session.usage` / `limits.weekly.usage`（0~1）与 ollama.com 官网 usage 页展示的窗口百分比一致，可直接作为份额百分比展示；UI 永久标注文案照常保留（"口径以官方未文档化接口为准"）。

## 抓取样本

命令（key 已脱敏）：

```bash
curl -sS https://ollama.com/api/usage -H "Authorization: Bearer <REDACTED>" | jq .
```

时间：2026-09-16 00:20 UTC 前后

```json
{
  "activity": {
    "cost": "0.00000",
    "period": { "type": "last_4_weeks", "starting_at": "2026-08-24T00:00:00Z",
                "ending_at": "2026-09-16T00:20:14.411095649Z" },
    "models": []
  },
  "limits": {
    "session": {
      "usage": 0.021,
      "models": [
        { "name": "glm-5.3-flash", "request_count": 8 },
        { "name": "glm-5.3", "request_count": 4 },
        { "name": "deepseek-v4-flash:0731", "request_count": 17 }
      ]
    },
    "weekly": {
      "usage": 0.388,
      "models": [
        { "name": "glm-5.3-flash", "request_count": 3308 },
        { "name": "kimi-k3", "request_count": 364 },
        { "name": "glm-5.3", "request_count": 486 },
        { "name": "glm-5.2", "request_count": 45 },
        { "name": "deepseek-v4-flash:0731", "request_count": 37 }
      ]
    }
  }
}
```

## 对照结果（用户同日目测 ollama.com 官网 usage 页）

| 指标 | API 值 | 官网显示 | 一致性 |
|------|--------|----------|--------|
| 5h 窗口百分比 | 0.021 → 2.1% | 2.1% | ✅ |
| 本周百分比 | 0.388 → 38.8% | 38.8% | ✅ |
| 模型请求次数（周） | glm-5.3-flash 3308 / kimi-k3 364 / glm-5.3 486 等 | 对得上 | ✅ |

用户原话确认："没问题"（2026-09-16）。

## 附带观察（供实现参考，非结论）

- `activity.period.type` 实为 `last_4_weeks`（4 周窗口），`ending_at` 为服务器当前时间戳，随每次请求走动——H5 漂移检测可依赖其"持续前进"特性。
- `activity.models` 为空数组（该字段与 limits.models 不同源，未展示数据）。
- `usage` 是份额比例（0~1），接口无绝对 token/请求数上限值，`models[].request_count` 是仅有的绝对计数。