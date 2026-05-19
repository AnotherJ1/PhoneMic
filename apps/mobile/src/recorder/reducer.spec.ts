/**
 * 任务 9.7：录音状态机 —— Property 6 + 单元测试。
 *
 * 任务来源：tasks.md 9.7
 * 关联需求：R4.2
 * 设计来源：design.md §7 Property 6
 *
 * Property 6 不变量：
 *  - press 模式：isRecording ⇔ pressedIds.length > 0；
 *  - toggle 模式：isRecording ⇔ tapCount % 2 === 1；
 *  - Blur 后 isRecording === false 且 pressedIds.length === 0；
 *  - PointerUp 对未按下 pointerId 是空操作；
 *  - 重复 PointerDown 同一 pointerId 不重复入队。
 */

import { describe, it, expect } from 'vitest'
import * as fc from 'fast-check'
import {
  initialRecorderState,
  recorderReduce,
  isRecording,
  type RecorderEvent,
  type RecorderMode,
} from '@/recorder/reducer'

function applyAll(mode: RecorderMode, events: readonly RecorderEvent[]) {
  let s = initialRecorderState(mode)
  for (const e of events) s = recorderReduce(s, e)
  return s
}

const pressEventArb = fc.oneof(
  fc.integer({ min: 0, max: 5 }).map<RecorderEvent>((id) => ({ type: 'PointerDown', pointerId: id })),
  fc.integer({ min: 0, max: 5 }).map<RecorderEvent>((id) => ({ type: 'PointerUp', pointerId: id })),
  fc.constant<RecorderEvent>({ type: 'Blur' }),
)

const toggleEventArb = fc.oneof(
  fc.constant<RecorderEvent>({ type: 'Tap' }),
  fc.constant<RecorderEvent>({ type: 'Blur' }),
)

describe('recorderReduce — basic cases (task 9.6)', () => {
  it('press: PointerDown starts recording, PointerUp stops', () => {
    let s = initialRecorderState('press')
    s = recorderReduce(s, { type: 'PointerDown', pointerId: 1 })
    expect(isRecording(s)).toBe(true)
    s = recorderReduce(s, { type: 'PointerUp', pointerId: 1 })
    expect(isRecording(s)).toBe(false)
  })

  it('press: holds recording while any finger remains down', () => {
    let s = initialRecorderState('press')
    s = recorderReduce(s, { type: 'PointerDown', pointerId: 1 })
    s = recorderReduce(s, { type: 'PointerDown', pointerId: 2 })
    s = recorderReduce(s, { type: 'PointerUp', pointerId: 1 })
    expect(isRecording(s)).toBe(true)
    s = recorderReduce(s, { type: 'PointerUp', pointerId: 2 })
    expect(isRecording(s)).toBe(false)
  })

  it('press: PointerUp on never-pressed id is a no-op', () => {
    let s = initialRecorderState('press')
    s = recorderReduce(s, { type: 'PointerUp', pointerId: 99 })
    expect(s.pressedIds).toEqual([])
  })

  it('press: duplicate PointerDown for same id does not double-count', () => {
    let s = initialRecorderState('press')
    s = recorderReduce(s, { type: 'PointerDown', pointerId: 1 })
    s = recorderReduce(s, { type: 'PointerDown', pointerId: 1 })
    expect(s.pressedIds).toEqual([1])
  })

  it('toggle: tap count parity drives isRecording', () => {
    let s = initialRecorderState('toggle')
    expect(isRecording(s)).toBe(false)
    s = recorderReduce(s, { type: 'Tap' })
    expect(isRecording(s)).toBe(true)
    s = recorderReduce(s, { type: 'Tap' })
    expect(isRecording(s)).toBe(false)
  })

  it('Blur forces stop in press mode', () => {
    let s = initialRecorderState('press')
    s = recorderReduce(s, { type: 'PointerDown', pointerId: 1 })
    s = recorderReduce(s, { type: 'PointerDown', pointerId: 2 })
    s = recorderReduce(s, { type: 'Blur' })
    expect(isRecording(s)).toBe(false)
    expect(s.pressedIds).toEqual([])
  })

  it('Blur forces stop in toggle mode', () => {
    let s = initialRecorderState('toggle')
    s = recorderReduce(s, { type: 'Tap' }) // recording
    s = recorderReduce(s, { type: 'Blur' })
    expect(isRecording(s)).toBe(false)
    expect(s.tapCount).toBe(0)
  })
})

describe('Property 6: recorder state machine invariants (task 9.7)', () => {
  it('press mode: isRecording ⇔ pressedIds.length > 0', () => {
    fc.assert(
      fc.property(fc.array(pressEventArb, { maxLength: 32 }), (events) => {
        const s = applyAll('press', events)
        expect(isRecording(s)).toBe(s.pressedIds.length > 0)
      }),
    )
  })

  it('press mode: pressedIds is a deduplicated set (no duplicates)', () => {
    fc.assert(
      fc.property(fc.array(pressEventArb, { maxLength: 32 }), (events) => {
        const s = applyAll('press', events)
        const set = new Set(s.pressedIds)
        expect(set.size).toBe(s.pressedIds.length)
      }),
    )
  })

  it('toggle mode: isRecording ⇔ tapCount % 2 === 1', () => {
    fc.assert(
      fc.property(fc.array(toggleEventArb, { maxLength: 32 }), (events) => {
        const s = applyAll('toggle', events)
        expect(isRecording(s)).toBe(s.tapCount % 2 === 1)
      }),
    )
  })

  it('Blur always forces isRecording === false and clears counters', () => {
    fc.assert(
      fc.property(
        fc.constantFrom<RecorderMode>('press', 'toggle'),
        fc.array(pressEventArb, { maxLength: 16 }),
        (mode, events) => {
          const s = applyAll(mode, events)
          const after = recorderReduce(s, { type: 'Blur' })
          expect(isRecording(after)).toBe(false)
          expect(after.pressedIds).toEqual([])
          expect(after.tapCount).toBe(0)
        },
      ),
    )
  })
})
