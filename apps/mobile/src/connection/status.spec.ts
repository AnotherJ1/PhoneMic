/**
 * 任务 9.5：连接状态文本映射 —— Property 9 + 单元测试。
 *
 * 任务来源：tasks.md 9.5
 * 关联需求：R4.5
 * 设计来源：design.md §7 Property 9
 *
 * 不变量：对任意 `(status, lang)` 组合：
 *  - statusLabel 返回值等于 i18n[lang][STATUS_KEY[status]]；
 *  - statusLabel 是幂等的（同入参恒同结果）；
 *  - 不抛异常。
 */

import { describe, it, expect } from 'vitest'
import * as fc from 'fast-check'
import {
  CONNECTION_STATUSES,
  statusLabel,
  type ConnectionStatus,
} from '@/connection/status'
import { DICTS, UI_LANGS, type UiLang } from '@/i18n'

const STATUS_KEY: Record<ConnectionStatus, string> = {
  Disconnected: 'status.disconnected',
  Connecting: 'status.connecting',
  Connected: 'status.connected',
  Reconnecting: 'status.reconnecting',
}

describe('statusLabel (task 9.4)', () => {
  it('returns the dictionary value for every (status, lang) pair', () => {
    for (const status of CONNECTION_STATUSES) {
      for (const lang of UI_LANGS) {
        expect(statusLabel(status, lang)).toBe(DICTS[lang][STATUS_KEY[status]])
      }
    }
  })
})

describe('Property 9: connection status text mapping (task 9.5)', () => {
  const statusArb = fc.constantFrom<ConnectionStatus>(...CONNECTION_STATUSES)
  const langArb = fc.constantFrom<UiLang>(...UI_LANGS)

  it('always equals the corresponding dictionary entry', () => {
    fc.assert(
      fc.property(statusArb, langArb, (status, lang) => {
        const expected = DICTS[lang][STATUS_KEY[status]]
        expect(statusLabel(status, lang)).toBe(expected)
      }),
    )
  })

  it('is idempotent (pure)', () => {
    fc.assert(
      fc.property(statusArb, langArb, (status, lang) => {
        const a = statusLabel(status, lang)
        const b = statusLabel(status, lang)
        expect(a).toBe(b)
      }),
    )
  })

  it('returns a non-empty string for every valid input', () => {
    fc.assert(
      fc.property(statusArb, langArb, (status, lang) => {
        const v = statusLabel(status, lang)
        expect(typeof v).toBe('string')
        expect(v.length).toBeGreaterThan(0)
      }),
    )
  })
})
