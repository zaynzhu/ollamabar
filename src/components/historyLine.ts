// src/components/historyLine.ts —— 24h 份额锯齿折线：原样连接不平滑，虚线竖线为推算 5h 窗口重置边界
import type { Sample } from '../types'

export function renderHistoryLine(samples: Sample[]): string {
  if (samples.length < 2) return '<p class="empty">攒数据中…</p>'
  const W = 560, H = 120, PAD = 24
  const xs = samples.map((s) => new Date(s.ts).getTime())
  const minT = Math.min(...xs), maxT = Math.max(...xs)
  const maxP = Math.max(10, ...samples.map((s) => s.session_pct))
  const x = (t: number) => PAD + (t - minT) / Math.max(1, maxT - minT) * (W - 2 * PAD)
  const y = (p: number) => H - PAD - p / maxP * (H - 2 * PAD)
  // 折线：原样连接，不插值不平均
  const pts = samples.map((s) => `${x(new Date(s.ts).getTime()).toFixed(1)},${y(s.session_pct).toFixed(1)}`).join(' ')
  // 5h 窗口边界竖线：按推算规则从采样范围头开始每 5h 画一条
  const WINDOW = 5 * 3600e3
  let boundaries = ''
  const start = Math.ceil((minT + 1) / WINDOW) * WINDOW  // 与后端 (t/18000+1)*18000 同规则
  for (let t = start; t <= maxT; t += WINDOW) {
    boundaries += `<line x1="${x(t).toFixed(1)}" y1="${PAD}" x2="${x(t).toFixed(1)}" y2="${H - PAD}" stroke="#30363d" stroke-dasharray="4 3"/>`
  }
  return `<svg class="sparkline" viewBox="0 0 ${W} ${H}" preserveAspectRatio="none">
    <line x1="${PAD}" y1="${H - PAD}" x2="${W - PAD}" y2="${H - PAD}" stroke="#30363d"/>
    ${boundaries}
    <polyline points="${pts}" fill="none" stroke="#58a6ff" stroke-width="1.5"/>
  </svg>
  <p class="axis">5h 窗口内累计份额（锯齿为真实形状，虚线为推算重置边界）</p>`
}