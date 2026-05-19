/**
 * 任务 12.1 端到端测试：扫码配对 → 注入文本。
 *
 * 流程：
 *  1. 启动桌面端二进制（PHONEMIC_DESKTOP_BIN 环境变量），它会监听
 *     `PHONEMIC_TEST_INJECT_FILE` 并以 TestSink 后端把所有注入写入该文件。
 *  2. 通过 `/api/pair` 模拟手机端配对。
 *  3. 通过 WebSocket 发送预录文本 `text.submit`。
 *  4. 断言注入文件包含该文本。
 *
 * 这是 design §6.3 描述的最小闭环；TestSink 是真实新增的 InjectorEventSink
 * 实现（worker-injector-desktop 在 7.11 中提供），不是运行时 workaround。
 */

import { test, expect } from '@playwright/test'
import { readFileSync, existsSync, unlinkSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const DESKTOP_API = process.env.PHONEMIC_DESKTOP_API ?? 'http://localhost:18080'
const DESKTOP_WS = process.env.PHONEMIC_DESKTOP_WS ?? 'ws://localhost:18080/ws'
const PAIR_CODE = process.env.PHONEMIC_TEST_PAIR_CODE ?? 'TESTCODE'
const INJECT_FILE =
  process.env.PHONEMIC_TEST_INJECT_FILE ?? join(tmpdir(), 'phonemic-e2e-inject.txt')

test.beforeEach(() => {
  if (existsSync(INJECT_FILE)) unlinkSync(INJECT_FILE)
})

test('pair → submit text → desktop injects to TestSink file', async ({ page, request }) => {
  test.skip(
    !process.env.PHONEMIC_DESKTOP_BIN && !process.env.PHONEMIC_E2E_DESKTOP_RUNNING,
    'Desktop binary not provided (set PHONEMIC_DESKTOP_BIN or PHONEMIC_E2E_DESKTOP_RUNNING=1)',
  )

  // 1. Pair via HTTP
  const pair = await request.post(`${DESKTOP_API}/api/pair`, {
    data: {
      pairingCode: PAIR_CODE,
      fingerprint: 'e2e-fp',
      deviceLabel: 'Playwright Mobile',
    },
  })
  expect(pair.ok()).toBeTruthy()
  const { sessionToken } = (await pair.json()) as { sessionToken: string }
  expect(sessionToken.length).toBeGreaterThan(0)

  // 2. Open mobile SPA (sanity: ensure /pair route loads)
  await page.goto('/#/pair')
  await expect(page).toHaveTitle(/PhoneMic/i)

  // 3. Send text.submit over WS using the session token
  const sample = 'hello phonemic 测试 ✨'
  await page.evaluate(
    async ([wsUrl, token, text]) => {
      const sub = `phonemic.${token}`
      await new Promise<void>((resolve, reject) => {
        const ws = new WebSocket(wsUrl, [sub])
        const timer = setTimeout(() => reject(new Error('ws timeout')), 5000)
        ws.onopen = () => {
          ws.send(
            JSON.stringify({
              type: 'hello',
              payload: { deviceLabel: 'Playwright', lang: 'en-US', useServerASR: false },
            }),
          )
          ws.send(
            JSON.stringify({
              type: 'text.submit',
              id: 'e2e-1',
              payload: { text, lang: 'en-US', clientTs: Date.now() },
            }),
          )
        }
        ws.onmessage = (ev) => {
          try {
            const msg = JSON.parse(String(ev.data)) as { type: string }
            if (msg.type === 'inject.ack') {
              clearTimeout(timer)
              ws.close(1000)
              resolve()
            }
          } catch {
            // Skip non-JSON frames; only inject.ack drives the success path.
          }
        }
        ws.onerror = (e) => {
          clearTimeout(timer)
          reject(new Error(`ws error: ${String(e)}`))
        }
      })
    },
    [DESKTOP_WS, sessionToken, sample] as const,
  )

  // 4. TestSink writes injected text to the file
  expect(existsSync(INJECT_FILE)).toBeTruthy()
  const written = readFileSync(INJECT_FILE, 'utf8')
  expect(written).toContain(sample)
})
