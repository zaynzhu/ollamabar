import { renderRing } from './ring'
import type { KeyState } from '../types'

function countdown(iso: string | null): string {
  if (!iso) return '--'
  const ms = new Date(iso).getTime() - Date.now()
  if (ms <= 0) return '即将重置'
  const h = Math.floor(ms / 3600e3), m = Math.floor((ms % 3600e3) / 60e3)
  return `${h}h${m}m 后重置`
}

export function renderKeyCard(ks: KeyState): string {
  const s = ks.snapshot
  if (!s) return `<section class="card"><h3>${ks.alias}</h3><p>尚无数据</p></section>`
  const banner = s.failure_level !== 'none'
    ? `<div class="banner ${s.failure_level}">${s.error_message ?? ''}</div>` : ''
  return `
  <section class="card" data-alias="${ks.alias}">
    <header><h3>${ks.alias}</h3><button class="refresh" data-refresh="${ks.alias}">刷新</button>
      <button class="remove" data-remove="${ks.alias}">删除</button></header>
    ${banner}
    <div class="rings">
      ${renderRing(s.session_pct, '5h 窗口')}
      ${renderRing(s.weekly_pct, '本周')}
    </div>
    <p class="resets">5h：${countdown(s.session_reset_est)}（预计）｜周：${countdown(s.weekly_reset_est)}（预计）</p>
    <p class="models-title">本周模型调用</p>
    <div class="models" data-models="${ks.alias}"></div>
    <div class="history" data-history="${ks.alias}"></div>
  </section>`
}
