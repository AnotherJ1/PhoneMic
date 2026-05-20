import { createApp } from 'vue'
import { createPinia } from 'pinia'
import 'virtual:uno.css'
import './styles/main.css'
import App from './App.vue'
import { router } from './router'

const app = createApp(App)
app.use(createPinia())
app.use(router)

// 桌面端 Tauri webview 默认进入桌面 splash；
// 手机浏览器保持默认 hash 路由（HomeView）。
// 在 mount 前用 router.replace 调整初始路由，
// 避免在 tauri.conf.json 的 windows[].url 中写 hash
// （Tauri 2.x 相对 URL + hash 在部分版本下被误解析为外部地址，
// 导致 __TAURI_INTERNALS__ 不注入）。
const w = window as unknown as Record<string, unknown>
const isInTauri = Boolean(w.__TAURI_INTERNALS__ ?? w.__TAURI__)
if (isInTauri && !window.location.hash.startsWith('#/desktop')) {
  void router.replace('/desktop/splash')
}

app.mount('#app')
