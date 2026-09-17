// src/mock.ts —— 覆盖四种 failure_level、空 models、多 key、滞后场景的假数据工厂
import type { AppState, LogEntry, ModelStat, Sample, UsageSnapshot } from './types'

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

// —— B4 追加的边界场景 ——
// 22 个模型、request_count 从 500 递减到 5（前端条形图只渲染 top 20）
const manyModels: ModelStat[] = Array.from({ length: 22 }, (_, i) => ({
  name: `model-${String(i + 1).padStart(2, '0')}`,
  request_count: 500 - Math.floor((495 / 21) * i),
}))
const LONG_ALIAS_A = '很久之前的备用账号十二字' // 12 字超长别名，混 22 模型场景
const LONG_ALIAS_B = '很久之前没再用的备用账号' // 12 字超长别名，混 null pct 场景

export const mockGetState = (): AppState => ({
  keys: [
    { alias: '工作', snapshot: snap({}) },
    { alias: '个人', snapshot: snap({ session_pct: 0, failure_level: 'degraded', error_message: '数据滞后 12 分钟', session_models: [] }) },
    { alias: '过期key', snapshot: snap({ failure_level: 'invalid_key', error_message: 'key 无效或已撤销' }) },
    { alias: '接口失效', snapshot: snap({ failure_level: 'dead', error_message: '接口可能已失效，上次成功：2026-09-14T08:00:00Z' }) },
    // 22 个模型：验证条形图 top-20 截断与长列表排版
    { alias: LONG_ALIAS_A, snapshot: snap({ session_models: manyModels.slice(0, 5), weekly_models: manyModels }) },
    // 首拍未成功：session_pct 为 null，环形图显示 '--'
    { alias: LONG_ALIAS_B, snapshot: snap({ session_pct: null, failure_level: 'none', last_success_at: null, error_message: null }) },
    // 两个列表全空：显示"本窗口暂无调用"
    { alias: '空窗口', snapshot: snap({ session_models: [], weekly_models: [] }) },
    // 组合场景：degraded + null pct + 空 session 模型 + 重置时间已过期（"即将重置"）
    { alias: '混合滞后', snapshot: snap({ session_pct: null, session_models: [],
      weekly_models: manyModels.slice(0, 3), failure_level: 'degraded',
      error_message: '数据滞后 12 分钟', session_reset_est: new Date(Date.now() - 3600e3).toISOString() }) },
  ],
  generated_at: new Date().toISOString(),
})

export const mockGetHistory = (_alias: string, _hours: number): Sample[] => {
  // 24h 假锯齿：每 10 分钟一点，窗口内爬升、重置跌 0
  const pts: Sample[] = []
  for (let i = 144; i >= 0; i--) {
    const inWindow = i % 30   // 30 点 = 5h 模拟窗口，正向锯齿：值随时间爬升
    pts.push({ ts: new Date(Date.now() - i * 600e3).toISOString(),
      session_pct: ((30 - inWindow) % 30) * 1.2, weekly_pct: 30 + (i % 60) * 0.5, session_models: [] })
  }
  return pts
}

export const mockAddKey = async (_alias: string, _apiKey: string) => {}
export const mockRemoveKey = async (_alias: string) => {}
export const mockRefreshNow = async (_alias: string): Promise<'updated'> => 'updated'

export const mockGetLog = (_alias: string): LogEntry[] => {
  // 三类行混合：成功采样、取数错误、实测重置（错误文案与后端 FetchError::text 对齐）
  const mk = (minAgo: number, over: Partial<LogEntry>): LogEntry => ({
    ts: new Date(Date.now() - minAgo * 60e3).toISOString(),
    kind: 'ok', session_pct: 4.2, weekly_pct: 12.7,
    session_req: 106, weekly_req: 328, message: null,
    ...over,
  })
  return [
    mk(1, {}),
    mk(2, { kind: 'error', message: '网络请求失败或超时' }),
    mk(3, {}),
    mk(5, { kind: 'reset', message: '5h 窗口重置（实测）' }),
    mk(6, { session_pct: 0.2, weekly_pct: 12.5, session_req: 2, weekly_req: 326 }),
    mk(7, { kind: 'error', message: 'key 无效或已撤销（HTTP 401/403）' }),
    mk(10, { session_pct: 12.4, weekly_pct: 12.3, session_req: 310, weekly_req: 322 }),
  ]
}
export const mockGetRetention = (): number => 7
export const mockSetRetention = async (_days: number) => {}
export const mockExportLog = async (_alias: string): Promise<string> => 'C:\\mock\\ollamabar-export.txt'

export const mockOnStateChanged = (cb: (s: AppState) => void) => {
  const t = setInterval(() => cb(mockGetState()), 5000)
  return () => clearInterval(t)
}
