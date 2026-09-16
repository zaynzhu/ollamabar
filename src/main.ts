// src/main.ts
import { getState, onStateChanged, refreshNow, removeKey } from './api'
import { renderKeyCard } from './components/keyCard'
import type { AppState } from './types'

const app = document.querySelector<HTMLDivElement>('#app')!

function render(state: AppState) {
  const cards = state.keys.map(renderKeyCard).join('')
  app.innerHTML = `
    <div class="grid">${cards}</div>
    <form id="addKey"><input name="alias" placeholder="别名" required>
      <input name="apiKey" placeholder="api_key" required>
      <button type="submit">添加 key</button></form>
    <p class="footnote">重置时间为客户端推算（预计）；数字口径以官方未文档化接口为准</p>`
}

app.addEventListener('click', async (e) => {
  const t = e.target as HTMLElement
  if (t.dataset.refresh) await refreshNow(t.dataset.refresh)
  if (t.dataset.remove && confirm('删除该 key？历史采样保留')) await removeKey(t.dataset.remove)
})

app.addEventListener('submit', async (e) => {
  const form = e.target as HTMLFormElement
  if (form.id !== 'addKey') return
  e.preventDefault()
  const fd = new FormData(form)
  const { addKey } = await import('./api')
  try { await addKey(String(fd.get('alias')), String(fd.get('apiKey'))); form.reset() }
  catch (err) { alert(String(err)) }
})

getState().then(render)
onStateChanged(render)