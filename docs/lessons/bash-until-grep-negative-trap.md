# bash until + grep -v 负向枚举监控会提前误退出

**结论速览**：轮询等待外部状态时用正向等值判断 `until [ "$(cmd --jq '.status')" = "completed" ]; do sleep 60; done`，不要用 `until cmd | grep -qv "枚举1\|枚举2"` 负向枚举。
适用条件：bash 轮询监控外部任务状态（CI、远端队列等）。

## ⛔ grep -qv 负向枚举判断导致监控 7 秒即误判完成（2026-10-08）

- **报错原文**：监控 `gh run view` 的 until 循环在构建仍在 queued 时退出，最终输出 `{"conclusion":"","jobs":[{"conclusion":"",...}]}`（结论为空，说明远端任务根本没跑完）。
- **报错稳定片段**：`conclusion":"`（jq 输出里空 conclusion 字符串，可作为"监控误退出"的标志物）
- **错误原因**（事实与推断分开）：
  - 事实：写了 `until gh run view ... --jq '.status + "/" + (.conclusion // "running")' | grep -qv "in_progress/running\|queued/running"; do sleep 60; done`，7 秒后循环退出，此时远端 status 未 completed。
  - 推断（未核对当时的中间输出）：`grep -v` 的语义是"选出不匹配的行"——只有当输出**恰好**落在两个枚举组合内才返回失败（循环继续）；任何第三种输出（gh 启动初期的空输出、限流重试、字段缺失产生的其他组合）都会让 grep 成功、循环立刻退出。负向枚举永远枚举不全。
- **为何不可再采用**：外部系统的状态空间不受你控制，负向枚举漏一个值就静默误判——且误判方向是"提前宣布完成"，最危险的方向。
- **替代方案**（✅ 已验证，2026-10-08 同日）：
```bash
until [ "$(gh run view <ID> --repo <owner/repo> --json status --jq '.status' 2>/dev/null)" = "completed" ]; do sleep 60; done
gh run view <ID> --json conclusion --jq '.conclusion'
```
正向等待唯一终态，任何中间状态（queued / in_progress / 网络错误导致的空输出）都会让 `[ ... = "completed" ]` 为假、循环继续。用替代方案后监控正确等待约 6 分钟到 `success`。
- **判定时效**：基于 gh CLI 2.x / bash（Git Bash on Windows）/ 2026-10；属 shell 语义而非版本行为，长期有效。