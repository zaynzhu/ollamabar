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
