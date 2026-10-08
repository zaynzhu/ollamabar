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
