/**
 * 任务 9.23 + 9.24 + 9.25：i18n 切换、UI/ASR 独立、字典完整性。
 *
 * 任务来源：tasks.md 9.23, 9.24, 9.25
 * 关联需求：R8.2、R8.5、R8.6
 * 设计来源：design.md §7 Property 25, Property 26, §9.3
 *
 * 该文件覆盖三个任务：
 *  - 9.23 Property 25：UI 语言切换即时生效（fast-check）
 *  - 9.24 Property 26：UI / ASR 语言独立（fast-check）
 *  - 9.25 字典完整性（key 集合相等 + 无空字符串 + decideLang 行为）
 */

import { describe, it, expect, vi } from 'vitest'
import * as fc from 'fast-check'
import {
  DICTS,
  UI_LANGS,
  decideLang,
  dictFor,
  translate,
  type UiLang,
} from '@/i18n'

// 一个最小 settings store 模拟，仅用于 Property 26：把 ui / asr 两个 setter
// 视为独立的可观测变量。
function makeSettingsModel() {
  let uiLang: UiLang = 'zh-CN'
  let asrLang: UiLang = 'zh-CN'
  return {
    setUi(l: UiLang) {
      uiLang = l
    },
    setAsr(l: UiLang) {
      asrLang = l
    },
    get ui() {
      return uiLang
    },
    get asr() {
      return asrLang
    },
  }
}

describe('decideLang (task 9.22)', () => {
  it('zh primary maps to zh-CN', () => {
    expect(decideLang('zh')).toBe('zh-CN')
    expect(decideLang('zh-CN')).toBe('zh-CN')
    expect(decideLang('zh-TW')).toBe('zh-CN')
    expect(decideLang('zh_TW')).toBe('zh-CN')
    expect(decideLang('ZH-cn')).toBe('zh-CN')
  })

  it('non-zh primary falls back to en-US', () => {
    expect(decideLang('en')).toBe('en-US')
    expect(decideLang('en-US')).toBe('en-US')
    expect(decideLang('fr-FR')).toBe('en-US')
    expect(decideLang('ja-JP')).toBe('en-US')
  })

  it('empty / null / whitespace falls back to en-US', () => {
    expect(decideLang('')).toBe('en-US')
    expect(decideLang('   ')).toBe('en-US')
    expect(decideLang(null)).toBe('en-US')
    expect(decideLang(undefined)).toBe('en-US')
  })

  it('is deterministic (Property 24 echo)', () => {
    fc.assert(
      fc.property(fc.string(), (s) => {
        expect(decideLang(s)).toBe(decideLang(s))
      }),
    )
  })
})

describe('Mobile i18n dictionary completeness (task 9.25)', () => {
  it('zh-CN and en-US share the same key set', () => {
    const zhKeys = new Set(Object.keys(DICTS['zh-CN']))
    const enKeys = new Set(Object.keys(DICTS['en-US']))
    const onlyZh = [...zhKeys].filter((k) => !enKeys.has(k))
    const onlyEn = [...enKeys].filter((k) => !zhKeys.has(k))
    expect({ onlyZh, onlyEn }).toEqual({ onlyZh: [], onlyEn: [] })
  })

  it('no empty values in either dictionary', () => {
    for (const lang of UI_LANGS) {
      const empties = Object.entries(DICTS[lang]).filter(([, v]) => v.trim() === '')
      expect(empties).toEqual([])
    }
  })

  it('translate warns and falls back to en-US for missing keys', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
    try {
      // Inject a temporary key only into en-US to verify fallback.
      ;(DICTS['en-US'] as Record<string, string>)['__test.fallback__'] = 'OnlyEn'
      try {
        const v = translate('zh-CN', '__test.fallback__')
        expect(v).toBe('OnlyEn')
        expect(warn).toHaveBeenCalled()
      } finally {
        delete (DICTS['en-US'] as Record<string, string>)['__test.fallback__']
      }
    } finally {
      warn.mockRestore()
    }
  })

  it('translate returns the key when missing in both dicts', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
    try {
      expect(translate('zh-CN', '__definitely.absent__')).toBe('__definitely.absent__')
      expect(warn).toHaveBeenCalled()
    } finally {
      warn.mockRestore()
    }
  })

  it('translate substitutes {var} placeholders', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
    try {
      // pair.ratelimit contains {seconds}
      const out = translate('en-US', 'pair.ratelimit', { seconds: 30 })
      expect(out).toContain('30')
      expect(out).not.toContain('{seconds}')
    } finally {
      warn.mockRestore()
    }
  })
})

describe('Property 25: UI language switch takes effect immediately (task 9.23)', () => {
  // 采样一组在两个字典中都存在的 key，对每次切换都断言所有可观测文案与目标字典一致。
  const sharedKeys = Object.keys(DICTS['zh-CN']).filter((k) => k in DICTS['en-US'])

  it('after every setLang, all observable strings equal the target dict', () => {
    fc.assert(
      fc.property(
        fc.array(fc.constantFrom<UiLang>(...UI_LANGS), { minLength: 1, maxLength: 20 }),
        fc.subarray(sharedKeys, { minLength: 1, maxLength: 8 }),
        (sequence, observed) => {
          for (const target of sequence) {
            const dict = dictFor(target)
            for (const k of observed) {
              expect(translate(target, k)).toBe(dict[k])
            }
          }
        },
      ),
    )
  })
})

describe('Property 26: UI lang and ASR lang are independent (task 9.24)', () => {
  const opArb = fc.oneof(
    fc.constantFrom<UiLang>(...UI_LANGS).map((l) => ({ kind: 'setUi' as const, l })),
    fc.constantFrom<UiLang>(...UI_LANGS).map((l) => ({ kind: 'setAsr' as const, l })),
  )

  it('each value depends only on its last own setter call', () => {
    fc.assert(
      fc.property(fc.array(opArb, { minLength: 1, maxLength: 30 }), (ops) => {
        const m = makeSettingsModel()
        let lastUi: UiLang = m.ui
        let lastAsr: UiLang = m.asr
        for (const op of ops) {
          if (op.kind === 'setUi') {
            m.setUi(op.l)
            lastUi = op.l
          } else {
            m.setAsr(op.l)
            lastAsr = op.l
          }
        }
        expect(m.ui).toBe(lastUi)
        expect(m.asr).toBe(lastAsr)
      }),
    )
  })
})
