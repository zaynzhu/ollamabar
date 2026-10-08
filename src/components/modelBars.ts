// src/components/modelBars.ts —— 模型调用条形图（按 request_count 占最大值比例）
// available=false：官方接口暂不提供模型明细（占位提示，不得把缺失当零次调用）
// available=true 且空数组：本窗口暂无调用
import type { ModelStat } from '../types'

export function renderModelBars(models: ModelStat[], available: boolean): string {
  if (!available) return '<p class="empty models-unavailable">官方接口暂不提供模型明细（恢复后自动展示）</p>'
  if (!models.length) return '<p class="empty">本窗口暂无调用</p>'
  const max = Math.max(...models.map((m) => m.request_count))
  return `<ol class="bars">${models.slice(0, 20).map((m) => `
    <li><span class="name" title="${m.name}">${m.name}</span>
      <span class="track"><span class="fill" style="width: ${(m.request_count * 100 / max).toFixed(1)}%"></span></span>
      <span class="count">${m.request_count}</span></li>`).join('')}</ol>`
}
