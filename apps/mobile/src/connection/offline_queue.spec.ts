/**
 * 任务 9.19：离线消息补发保序 —— Property 30 + 单元测试。
 *
 * 任务来源：tasks.md 9.19
 * 关联需求：R9.4
 * 设计来源：design.md §7 Property 30
 */

import { describe, it, expect } from 'vitest'
import * as fc from 'fast-check'
import {
  createOfflineQueue,
  enqueue,
  drainQueue,
  dedupById,
  type QueuedSubmit,
} from '@/connection/offline_queue'

function makeItem(id: string, text = 'hello'): QueuedSubmit {
  return {
    id,
    payload: { text, lang: 'en-US', clientTs: 0 },
  }
}

describe('offline queue — basic cases (task 9.18)', () => {
  it('enqueue does not mutate the source', () => {
    const a = createOfflineQueue()
    const b = enqueue(a, makeItem('1'))
    expect(a).toEqual([])
    expect(b.length).toBe(1)
  })

  it('drainQueue returns ordered items and empties remaining', () => {
    let q = createOfflineQueue()
    q = enqueue(q, makeItem('1'))
    q = enqueue(q, makeItem('2'))
    q = enqueue(q, makeItem('3'))
    const { ordered, remaining } = drainQueue(q)
    expect(ordered.map((x) => x.id)).toEqual(['1', '2', '3'])
    expect(remaining).toEqual([])
  })

  it('dedupById keeps the first occurrence of each id', () => {
    const q = [makeItem('a'), makeItem('b'), makeItem('a'), makeItem('c'), makeItem('b')]
    expect(dedupById(q).map((x) => x.id)).toEqual(['a', 'b', 'c'])
  })
})

describe('Property 30: offline replay preserves order (task 9.19)', () => {
  const idArb = fc.uuid()
  const itemArb = idArb.map((id) => makeItem(id))

  it('replay order equals enqueue order, each item exactly once', () => {
    fc.assert(
      fc.property(fc.array(itemArb, { maxLength: 30 }), (items) => {
        let q = createOfflineQueue()
        for (const x of items) q = enqueue(q, x)
        const { ordered, remaining } = drainQueue(q)
        expect(remaining).toEqual([])
        expect(ordered.map((x) => x.id)).toEqual(items.map((x) => x.id))
      }),
    )
  })

  it('after dedupById, ids are unique while order of first-seen is preserved', () => {
    fc.assert(
      fc.property(fc.array(itemArb, { maxLength: 30 }), (items) => {
        const deduped = dedupById(items)
        const ids = deduped.map((x) => x.id)
        expect(new Set(ids).size).toBe(ids.length)

        // Order of first-seen
        const firstSeen: string[] = []
        const seen = new Set<string>()
        for (const x of items) {
          if (!seen.has(x.id)) {
            seen.add(x.id)
            firstSeen.push(x.id)
          }
        }
        expect(ids).toEqual(firstSeen)
      }),
    )
  })
})
