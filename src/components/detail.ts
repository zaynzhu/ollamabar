// src/components/detail.ts —— 详情弹窗：7d/30d 统计、合并日志（成功/错误/重置）、保留期切换、导出
import { getLog, getRetention, setRetention, exportLog } from '../api'
import type { LogEntry, UsageSnapshot } from '../types'

function fmtTime(ts: string): string {
  const d = new Date(ts)
  if (isNaN(d.getTime())) return ts
  const p = (n: number) => String(n).padStart(2, '0')
  return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`
}

function fmtPct(v: number | null): string {
  return v === null ? '--' : `${v.toFixed(1)}%`
}

// 历史模型请求数（升级前的旧行有值，新行显示 --）
function fmtReq(a: number | null, b: number | null): string {
  return `${a ?? '--'} / ${b ?? '--'}`
}

function resultCell(r: LogEntry): string {
  if (r.kind === 'ok') return '<span class="log-ok">✔ 成功</span>'
  if (r.kind === 'reset') return `<span class="log-reset">↺ 重置</span><span class="log-msg">${r.message ?? ''}</span>`
  return `<span class="log-err">✘ 错误</span><span class="log-msg">${r.message ?? ''}</span>`
}

function renderRows(rows: LogEntry[], onlyIssues: boolean): string {
  const shown = onlyIssues ? rows.filter((r) => r.kind !== 'ok') : rows
  if (!shown.length) return '<tr><td colspan="6" class="log-empty">没有符合条件的记录</td></tr>'
  return shown.map((r) => `
    <tr class="log-${r.kind}">
      <td class="log-ts">${fmtTime(r.ts)}</td>
      <td>${resultCell(r)}</td>
      <td>${r.kind === 'ok' ? fmtPct(r.session_pct) : '--'}</td>
      <td>${r.kind === 'ok' ? fmtPct(r.weekly_pct) : '--'}</td>
      <td>${r.kind === 'ok' ? r.requests_24h ?? '--' : '--'}</td>
      <td>${r.kind === 'ok' ? fmtReq(r.session_req, r.weekly_req) : '--'}</td>
    </tr>`).join('')
}

// 弹窗头部统计区：7d/30d 请求数；usage_based 附余额/额度
function statsHtml(snap: UsageSnapshot | null): string {
  if (!snap) return ''
  let s = `近7d 请求 ${snap.requests_7d ?? '--'} 次 · 近30d 请求 ${snap.requests_30d ?? '--'} 次`
  if (snap.plan_type === 'usage_based' && snap.balance_usd != null) {
    s += ` · 余额 $${snap.balance_usd.toFixed(2)} / 额度 $${snap.allowance_usd != null ? snap.allowance_usd.toFixed(2) : '--'}`
  }
  return `<p class="detail-stats">${s}</p>`
}

export async function openDetail(alias: string, snap: UsageSnapshot | null): Promise<void> {
  // 防重复打开：已有弹窗先移除
  document.querySelector('.modal-back')?.remove()
  const back = document.createElement('div')
  back.className = 'modal-back'
  back.innerHTML = `
    <div class="modal" role="dialog" aria-label="运行日志">
      <header><h3>${alias} · 运行日志</h3>
        <label class="log-retention">保留 <select id="logRetention"><option value="7">7 天</option><option value="30">30 天</option></select></label>
        <label class="log-filter"><input type="checkbox" id="logOnlyIssues"> 仅看异常</label>
        <button class="log-export">导出日志</button>
        <button class="log-close" title="关闭">×</button>
      </header>
      ${statsHtml(snap)}
      <p class="log-hint">每分钟一条成功采样；错误与重置另行留痕。日志按保留期自动清理，导出可长期留存。</p>
      <div class="log-table-wrap"><table class="log-table">
        <thead><tr><th>时间</th><th>结果</th><th>5h 窗口</th><th>本周</th><th>近24h</th><th>模型请求(5h/周)</th></tr></thead>
        <tbody><tr><td colspan="6" class="log-empty">加载中…</td></tr></tbody>
      </table></div>
    </div>`
  document.body.appendChild(back)

  const tbody = back.querySelector('tbody')!
  const onlyBox = back.querySelector<HTMLInputElement>('#logOnlyIssues')!
  const select = back.querySelector<HTMLSelectElement>('#logRetention')!
  let allRows: LogEntry[] = []
  const redraw = () => { tbody.innerHTML = renderRows(allRows, onlyBox.checked) }

  select.value = String(await getRetention())
  try {
    allRows = await getLog(alias)
  } catch (e) {
    tbody.innerHTML = `<tr><td colspan="6" class="log-empty">日志读取失败：${String(e)}</td></tr>`
  }
  redraw()

  onlyBox.addEventListener('change', redraw)
  select.addEventListener('change', async () => {
    const days = Number(select.value)
    try { await setRetention(days) } catch (e) { alert(String(e)) }
  })
  back.querySelector('.log-export')!.addEventListener('click', async () => {
    try {
      const path = await exportLog(alias)
      if (path) alert(`已导出到：\n${path}`)
    } catch (e) { alert(String(e)) }
  })
  const close = () => back.remove()
  back.querySelector('.log-close')!.addEventListener('click', close)
  // 点遮罩空白处关闭（点弹窗本身不关）
  back.addEventListener('click', (e) => { if (e.target === back) close() })
}