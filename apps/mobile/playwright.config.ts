/**
 * Playwright 配置 —— 任务 12.1 / 12.2 端到端测试。
 *
 * 任务来源：tasks.md 12.1, 12.2
 * 关联需求：R4.x、R5.x、R6.1、R9.3、R9.4、R9.5
 * 设计来源：design.md §6.3、§9.5
 *
 * 该配置驱动 Chromium（移动 UA 模拟）执行手机视图 E2E。Tauri 桌面端
 * 走 WebDriver 是任务 10.9 的范围；本配置只覆盖移动侧 + 真实后端。
 *
 * 测试假设：
 *  - 由 `pnpm dev` 或 `vite preview` 提供 SPA（默认 http://localhost:4173）；
 *  - 由桌面端二进制（或测试 stub）提供 `/api/pair`（默认 http://localhost:18080）；
 *  - 桌面端在 PHONEMIC_TEST_INJECT_FILE 环境变量指向的文件中追加注入文本
 *    （TestSink 后端，见 tasks.md 12.1）。
 */
import { defineConfig, devices } from '@playwright/test'

const SPA_BASE = process.env.PHONEMIC_SPA_URL ?? 'http://localhost:4173'

export default defineConfig({
  testDir: './e2e',
  timeout: 30_000,
  fullyParallel: false, // E2E 共享桌面进程，串行更安全
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: 1,
  reporter: [['list'], ['html', { outputFolder: 'playwright-report', open: 'never' }]],
  use: {
    baseURL: SPA_BASE,
    trace: 'on-first-retry',
    video: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  projects: [
    {
      name: 'mobile-chromium',
      use: { ...devices['Pixel 7'] },
    },
  ],
})
