// src/main.ts
import { getState, onStateChanged } from './api'
import type { AppState } from './types'

const app = document.querySelector<HTMLDivElement>('#app')!

function render(state: AppState) {
  app.innerHTML = `<pre>${JSON.stringify(state, null, 2)}</pre>` // B2 换真 UI
}

getState().then(render)
onStateChanged(render)