/**
 * 任务 9.13：历史队列上限 —— Property 8 + 单元测试。
 *
 * 任务来源：tasks.md 9.13
 * 关联需求：R4.9
 * 设计来源：design.md §7 Property 8
 */

import { describe, it, expect } from 'vitest'
import * as fc from 'fast-check'
import { pushBounded, HISTORY_CAPACITY } from '@/transcript/history'

describe('pushBounded — basic cases (task 9.12)', () => {
  it('appends within capacity', () => {
    const q = pushBounded([1, 2], 3, 5)
    expect(q).toEqual([1, 2, 3])
  })

  it('drops earliest when exceeding capacity', () => {
    const q = pushBounded([1, 2, 3, 4, 5], 6, 5)
    expect(q).toEqual([2, 3, 4, 5, 6])
  })

  it('does not mutate the input', () => {
    const orig = [1, 2, 3]
    const q = pushBounded(orig, 4, 3)
    expect(orig).toEqual([1, 2, 3])
    expect(q).toEqual([2, 3, 4])
  })

  it('capacity 0 returns empty array', () => {
    expect(pushBounded([1, 2], 3, 0)).toEqual([])
  })

  it('default HISTORY_CAPACITY is 50', () => {
    expect(HISTORY_CAPACITY).toBe(50)
  })
})

describe('Property 8: history queue capacity bound (task 9.13)', () => {
  it('length === min(|R|, capacity) and last items are preserved', () => {
    fc.assert(
      fc.property(
        fc.array(fc.integer(), { maxLength: 200 }),
        fc.integer({ min: 1, max: 100 }),
        (items, capacity) => {
          let q: readonly number[] = []
          for (const x of items) q = pushBounded(q, x, capacity)
          // Length matches min(|R|, capacity)
          expect(q.length).toBe(Math.min(items.length, capacity))
          // Equals the last `q.length` of items
          expect([...q]).toEqual(items.slice(items.length - q.length))
        },
      ),
    )
  })

  it('capacity = 50 holds the last 50 items exactly', () => {
    fc.assert(
      fc.property(fc.array(fc.integer(), { minLength: 51, maxLength: 200 }), (items) => {
        let q: readonly number[] = []
        for (const x of items) q = pushBounded(q, x, 50)
        expect(q.length).toBe(50)
        expect([...q]).toEqual(items.slice(items.length - 50))
      }),
    )
  })
})
