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