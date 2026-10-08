import { renderRing } from './ring'
import type { KeyState, UsageSnapshot } from '../types'

function countdown(iso: string | null): string {
  if (!iso) return '--'
  const ms = new Date(iso).getTime() - Date.now()
  if (ms <= 0) return '即将重置'
  const h = Math.floor(ms / 3600e3), m = Math.floor((ms % 3600e3) / 60e3)
  return `${h}h${m}m 后重置`
}

function countdownDays(iso: string | null): string {
  if (!iso) return '--'
  const ms = new Date(iso).getTime() - Date.now()
  if (ms <= 0) return '0 天'
  return `${Math.ceil(ms / 86400e3)} 天`
}

function fmtDate(iso: string | null): string {
  if (!iso) return '--'
  const d = new Date(iso)
  const p = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`
}

// 环区：Legacy 双环；usage_based 一环（本期已用 %）+ 周期剩余信息
function ringsHtml(s: UsageSnapshot): string {
  if (s.plan_type === 'usage_based') {
    const used = s.allowance_usd != null && s.allowance_usd > 0 && s.balance_usd != null
      ? Math.max(0, Math.min(100, (s.allowance_usd - s.balance_usd) * 100 / s.allowance_usd)) : null
    const sub = s.balance_usd != null && s.allowance_usd != null
      ? `$${s.balance_usd.toFixed(2)} / $${s.allowance_usd.toFixed(0)}` : undefined
    return `
    <div class="rings">
      ${renderRing(used, '本期用量', sub)}
      <div class="period-left">本期至 ${fmtDate(s.period_until)}<br>剩 ${countdownDays(s.period_until)}</div>
    </div>`
  }
  return `
  <div class="rings">
    ${renderRing(s.session_pct, '5h 窗口')}
    ${renderRing(s.weekly_pct, '本周')}
  </div>`
}

// 重置行：服务端来源不标"预计"，回退推算才标；usage_based 只显示周期倒计时
function resetsHtml(s: UsageSnapshot): string {
  if (s.plan_type === 'usage_based') {
    return `<p class="resets"><span>计费周期：${countdown(s.period_until)}</span></p>`
  }
  const est = (fromServer: boolean) => fromServer ? '' : '（预计）'
  return `
  <p class="resets">
    <span>5h：${countdown(s.session_reset_est)}${est(s.session_reset_from_server)}</span>
    <span>周：${countdown(s.weekly_reset_est)}${est(s.weekly_reset_from_server)}</span>
  </p>`
}

// 模型区：Legacy 双窗口标题；usage_based 无窗口概念合并为一区
function modelsHtml(s: UsageSnapshot, alias: string): string {
  if (s.plan_type === 'usage_based') {
    return `<p class="models-title">模型调用</p>
    <div class="models" data-session-models="${alias}"></div>`
  }
  return `
  <p class="models-title">5h 模型调用</p>
  <div class="models" data-session-models="${alias}"></div>
  <p class="models-title">本周模型调用</p>
  <div class="models" data-models="${alias}"></div>`
}

export function renderKeyCard(ks: KeyState): string {
  const s = ks.snapshot
  if (!s) return `<section class="card"><h3>${ks.alias}</h3><p>尚无数据</p></section>`
  const banner = s.failure_level !== 'none'
    ? `<div class="banner ${s.failure_level}">${s.error_message ?? ''}</div>` : ''
  return `
  <section class="card" data-alias="${ks.alias}">
    <header><h3>${ks.alias}</h3><button class="refresh" data-refresh="${ks.alias}">刷新</button>
      <button class="detail" data-detail="${ks.alias}">详情</button>
      <button class="remove" data-remove="${ks.alias}">删除</button></header>
    ${banner}
    ${ringsHtml(s)}
    <p class="req-count">近24h 请求 ${s.requests_24h ?? '--'} 次</p>
    ${resetsHtml(s)}
    ${modelsHtml(s, ks.alias)}
    <div class="history" data-history="${ks.alias}"></div>
  </section>`
}