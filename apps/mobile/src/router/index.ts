import { createRouter, createWebHashHistory, type RouteRecordRaw } from 'vue-router'

/**
 * 路由表 —— 任务 9.1 + worker-injector-desktop handoff。
 *
 * 设计来源：design.md §4.7
 *
 * 三套手机视图：
 *  - `/`         HomeView
 *  - `/pair`     PairView
 *  - `/settings` SettingsView
 *
 * 两套 Tauri 桌面视图（同一份 SPA，由桌面端窗口加载 `#/desktop/splash`）：
 *  - `/desktop`        DesktopView
 *  - `/desktop/splash` DesktopSplashView
 *
 * Hash 路由保留：避免在 Tauri 静态分发场景下需要 Web_Server 配置 SPA fallback。
 */
const routes: RouteRecordRaw[] = [
  {
    path: '/',
    name: 'home',
    component: () => import('@/views/HomeView.vue'),
    meta: { title: 'app.home.title' },
  },
  {
    path: '/pair',
    name: 'pair',
    component: () => import('@/views/PairView.vue'),
    meta: { title: 'app.pair.title' },
  },
  {
    path: '/settings',
    name: 'settings',
    component: () => import('@/views/SettingsView.vue'),
    meta: { title: 'app.settings.title' },
  },
  {
    path: '/desktop',
    name: 'desktop',
    component: () => import('@/views/DesktopView.vue'),
    meta: { title: 'app.desktop.title' },
  },
  {
    path: '/desktop/splash',
    name: 'desktop-splash',
    component: () => import('@/views/DesktopSplashView.vue'),
    meta: { title: 'app.desktop.splash.title' },
  },
  {
    path: '/:pathMatch(.*)*',
    redirect: { name: 'home' },
  },
]

export const router = createRouter({
  history: createWebHashHistory(),
  routes,
  scrollBehavior() {
    return { top: 0 }
  },
})
