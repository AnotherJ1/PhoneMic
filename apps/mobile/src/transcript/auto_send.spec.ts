/**
 * 任务 9.11：自动发送计数 —— Property 7 + 单元测试。
 *
 * 任务来源：tasks.md 9.11
 * 关联需求：R4.7
 * 设计来源：design.md §7 Property 7
 */

import { describe, it, expect } from 'vitest'
import * as fc from 'fast-check'
import { autoSendDispatch, latestFinalDraft, type AsrEvent } from '@/transcript/auto_send'

const eventArb = fc.oneof(
  fc.string().map<AsrEvent>((text) => ({ type: 'interim', text })),
  fc.string().map<AsrEvent>((text) => ({ type: 'final', text })),
)

describe('autoSendDispatch (task 9.10)', () => {
  it('returns 0 when autoSend is false regardless of events', () => {
    fc.assert(
      fc.property(fc.array(eventArb, { maxLength: 16 }), (events) => {
        expect(autoSendDispatch(events, false)).toBe(0)
      }),
    )
  })

  it('returns the count of `final` events when autoSend is true', () => {
    fc.assert(
      fc.property(fc.array(eventArb, { maxLength: 32 }), (events) => {
        const finals = events.filter((e) => e.type === 'final').length
        expect(autoSendDispatch(events, true)).toBe(finals)
      }),
    )
  })

  it('ignores interim events for submit count', () => {
    const events: AsrEvent[] = [
      { type: 'interim', text: 'a' },
      { type: 'interim', text: 'ab' },
      { type: 'final', text: 'abc' },
      { type: 'interim', text: 'd' },
      { type: 'final', text: 'de' },
    ]
    expect(autoSendDispatch(events, true)).toBe(2)
  })

  it('returns the latest final text for manual draft', () => {
    const events: AsrEvent[] = [
      { type: 'final', text: 'first' },
      { type: 'interim', text: 'mid' },
      { type: 'final', text: 'last' },
      { type: 'interim', text: 'tail' },
    ]
    expect(latestFinalDraft(events)).toBe('last')
  })

  it('latestFinalDraft returns empty string when no final events', () => {
    expect(latestFinalDraft([])).toBe('')
    expect(latestFinalDraft([{ type: 'interim', text: 'x' }])).toBe('')
  })
})
