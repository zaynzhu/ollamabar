// src/main.ts
import { getState, onStateChanged, refreshNow, removeKey, addKey, getHistory } from './api'
import { renderKeyCard } from './components/keyCard'
import { renderModelBars } from './components/modelBars'
import { renderHistoryLine } from './components/historyLine'
import { openDetail } from './components/detail'
import type { AppState } from './types'

const app = document.querySelector<HTMLDivElement>('#app')!

// 渲染序号守卫：fillCharts 期间若有新渲染则放弃旧轮次结果
let renderSeq = 0

// 最近一次渲染的 AppState：详情弹窗需要快照统计（7d/30d/余额），避免二次查询
let lastState: AppState | null = null

// 卡片渲染后异步填充两图：模型条形图 + 24h 锯齿历史曲线
async function fillCharts(state: AppState, seq: number) {
  for (const ks of state.keys) {
    // 渲染已过期就不再填 DOM
    if (seq !== renderSeq) return
    const snap = ks.snapshot
    const sbox = app.querySelector<HTMLElement>(`[data-session-models="${ks.alias}"]`)
    if (sbox && snap) sbox.innerHTML = renderModelBars(snap.session_models, snap.models_available)
    const box = app.querySelector<HTMLElement>(`[data-models="${ks.alias}"]`)
    if (box && snap) box.innerHTML = renderModelBars(snap.weekly_models, snap.models_available)
    const hist = app.querySelector<HTMLElement>(`[data-history="${ks.alias}"]`)
    if (!hist) continue
    try {
      // 单个 key 查询失败不中断其他 key
      hist.innerHTML = renderHistoryLine(await getHistory(ks.alias, 24))
    } catch {
      hist.innerHTML = '<p class="empty">历史读取失败</p>'
    }
  }
}

function render(state: AppState) {
  renderSeq++
  lastState = state
  const cards = state.keys.map(renderKeyCard).join('')
  app.innerHTML = `
    <div class="grid">${cards}</div>
    <form id="addKey"><input name="alias" placeholder="别名" required>
      <input name="apiKey" placeholder="api_key" required>
      <button type="submit">添加 key</button></form>
    <p class="footnote">重置时间以服务端返回为准（缺失时客户端推算并标"预计"）；模型明细官方接口暂未提供</p>`
  fillCharts(state, renderSeq)
}

app.addEventListener('click', async (e) => {
  const t = e.target as HTMLElement
  if (t.dataset.refresh) await refreshNow(t.dataset.refresh)
  if (t.dataset.detail) {
    const snap = lastState?.keys.find((k) => k.alias === t.dataset.detail)?.snapshot ?? null
    await openDetail(t.dataset.detail, snap)
  }
  if (t.dataset.remove && confirm('删除该 key？历史采样保留')) await removeKey(t.dataset.remove)
})

app.addEventListener('submit', async (e) => {
  const form = e.target as HTMLFormElement
  if (form.id !== 'addKey') return
  e.preventDefault()
  const fd = new FormData(form)
  try { await addKey(String(fd.get('alias')), String(fd.get('apiKey'))); form.reset() }
  catch (err) { alert(String(err)) }
})

getState().then(render)
onStateChanged(render)
