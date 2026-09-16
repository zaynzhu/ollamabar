// 环形进度：纯 SVG，无依赖
export function renderRing(pct: number | null, label: string): string {
  const v = pct == null ? 0 : Math.max(0, Math.min(100, pct))
  const r = 42, c = 2 * Math.PI * r
  const color = v >= 90 ? '#f85149' : v >= 70 ? '#d29922' : '#3fb950'
  return `
  <div class="ring">
    <svg viewBox="0 0 100 100" width="96" height="96">
      <circle cx="50" cy="50" r="${r}" fill="none" stroke="#21262d" stroke-width="10"/>
      <circle cx="50" cy="50" r="${r}" fill="none" stroke="${color}" stroke-width="10"
        stroke-dasharray="${(c * v / 100).toFixed(1)} ${c.toFixed(1)}"
        stroke-linecap="round" transform="rotate(-90 50 50)"/>
      <text x="50" y="47" text-anchor="middle" fill="#e6edf3" font-size="18" font-weight="600">${pct == null ? '--' : Math.round(v)}%</text>
      <text x="50" y="66" text-anchor="middle" fill="#8b949e" font-size="11">${label}</text>
    </svg>
    <span class="caliber">口径以官方未文档化接口为准</span>
  </div>`
}