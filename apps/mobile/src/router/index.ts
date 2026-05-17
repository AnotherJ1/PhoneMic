import { createRouter, createWebHashHistory, type RouteRecordRaw } from 'vue-router'

// 使用 Hash 路由：避免在 Tauri 静态分发场景下需要 Web_Server 配置 SPA fallback。
// 路由结构与 design.md §4.7 中 OnboardingView / MainView 对应；具体视图实现在后续任务补齐。
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
