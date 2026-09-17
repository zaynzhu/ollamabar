// src/api.ts
import type { AppState, LogEntry, RefreshResult, Sample } from './types'
import { mockGetState, mockGetHistory, mockAddKey, mockRemoveKey, mockRefreshNow, mockOnStateChanged, mockGetLog, mockGetRetention, mockSetRetention, mockExportLog } from './mock'

// 纯前端 dev server 下无 Tauri 注入，自动走 mock；真机走契约
const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

export async function getState(): Promise<AppState> {
  if (!inTauri) return mockGetState()
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke('get_state')
}

export async function getHistory(alias: string, hours: number): Promise<Sample[]> {
  if (!inTauri) return mockGetHistory(alias, hours)
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke('get_history', { alias, hours })
}

export async function addKey(alias: string, apiKey: string): Promise<void> {
  if (!inTauri) return mockAddKey(alias, apiKey)
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke('add_key', { alias, apiKey })
}

export async function removeKey(alias: string): Promise<void> {
  if (!inTauri) return mockRemoveKey(alias)
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke('remove_key', { alias })
}

export async function refreshNow(alias: string): Promise<RefreshResult> {
  if (!inTauri) return mockRefreshNow(alias)
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke('refresh_now', { alias })
}

export async function getLog(alias: string): Promise<LogEntry[]> {
  if (!inTauri) return mockGetLog(alias)
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke('get_log', { alias })
}

export async function getRetention(): Promise<number> {
  if (!inTauri) return mockGetRetention()
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke('get_retention')
}

export async function setRetention(days: number): Promise<void> {
  if (!inTauri) return mockSetRetention(days)
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke('set_retention', { days })
}

// 导出成功返回保存路径，用户取消返回 null
export async function exportLog(alias: string): Promise<string | null> {
  if (!inTauri) return mockExportLog(alias)
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke('export_log', { alias })
}

export function onStateChanged(cb: (s: AppState) => void): () => void {
  if (!inTauri) return mockOnStateChanged(cb)
  let un = () => {}
  import('@tauri-apps/api/event').then(({ listen }) => {
    listen('state-changed', (e) => cb(e.payload as AppState)).then((u) => { un = u })
  })
  return () => un()
}
