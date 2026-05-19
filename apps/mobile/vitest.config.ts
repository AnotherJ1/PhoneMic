/**
 * Vitest 配置 —— 任务 9.5/9.7/9.11/9.13/9.15/9.17/9.19 等属性测试与单元测试入口。
 *
 * 任务来源：tasks.md 9.x 系列
 * 设计来源：design.md §9.2、§9.3
 *
 * 关键设计：
 *  - 复用 vite.config.ts 的 alias，避免别名漂移；
 *  - 默认环境 jsdom（满足 visibilitychange / DOM 事件相关测试）；
 *  - globals: false（保持显式导入 describe/it/expect，便于 ESLint 静态分析）。
 */

import { fileURLToPath, URL } from 'node:url'
import { defineConfig } from 'vitest/config'
import vue from '@vitejs/plugin-vue'

export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  test: {
    environment: 'jsdom',
    globals: false,
    include: ['src/**/*.spec.ts', 'src/**/*.test.ts'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'lcov'],
      include: ['src/**/*.ts', 'src/**/*.vue'],
      exclude: [
        'src/**/*.spec.ts',
        'src/**/*.test.ts',
        'src/**/*.d.ts',
        'src/main.ts',
        'src/env.d.ts',
      ],
    },
  },
})
