export interface ModelStat { name: string, request_count: number }

// 套餐类型：legacy（session/weekly 百分比）| usage_based（美元余额/额度）
export type PlanType = 'legacy' | 'usage_based'

export interface UsageSnapshot {
  plan_type: PlanType
  session_pct: number | null                  // 0~100 已用 = 100 - remaining_percent
  weekly_pct: number | null
  session_reset_est: string | null            // 优先服务端 resets_at，缺失回退客户端推算
  weekly_reset_est: string | null
  session_reset_from_server: boolean           // false 时显示层标"（预计）"
  weekly_reset_from_server: boolean
  session_models: ModelStat[]                 // 官方暂不提供，恒空；字段保留待恢复
  weekly_models: ModelStat[]
  models_available: boolean                    // false=接口不提供；true 且空数组=本窗口暂无调用
  balance_usd: number | null                   // 新计费套餐
  allowance_usd: number | null
  period_from: string | null
  period_until: string | null
  requests_24h: number | null                 // 历史统计（轮转刷新，可能滞后）
  requests_7d: number | null
  requests_30d: number | null
  server_time: string | null                   // 旧接口字段保留兼容，恒 null
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

// 详情弹窗的日志行：kind ok=成功采样 error=取数失败 reset=实测重置
export interface LogEntry {
  ts: string
  kind: 'ok' | 'error' | 'reset'
  session_pct: number | null
  weekly_pct: number | null
  session_req: number | null   // 旧接口模型求和（升级前历史行有值，新行 null）
  weekly_req: number | null
  requests_24h: number | null  // 近 24h 总请求数（升级后新增）
  message: string | null
}
