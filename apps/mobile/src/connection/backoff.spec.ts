/**
 * 任务 9.17：重连退避序列 —— Property 29 + 单元测试。
 *
 * 任务来源：tasks.md 9.17
 * 关联需求：R9.3
 * 设计来源：design.md §7 Property 29
 */

import { describe, it, expect } from 'vitest'
import * as fc from 'fast-check'
import {
  nextDelay,
  isReconnectExhausted,
  MAX_RECONNECT_ATTEMPTS,
  BACKOFF_DELAYS_SECONDS,
} from '@/connection/backoff'

describe('nextDelay — basic cases (task 9.16)', () => {
  it('matches the [1, 2, 4, 8, 16] sequence', () => {
    expect(BACKOFF_DELAYS_SECONDS).toEqual([1, 2, 4, 8, 16])
    expect(nextDelay(1)).toBe(1)
    expect(nextDelay(2)).toBe(2)
    expect(nextDelay(3)).toBe(4)
    expect(nextDelay(4)).toBe(8)
    expect(nextDelay(5)).toBe(16)
  })

  it('returns null after MAX_RECONNECT_ATTEMPTS', () => {
    expect(nextDelay(MAX_RECONNECT_ATTEMPTS + 1)).toBeNull()
    expect(nextDelay(99)).toBeNull()
  })

  it('returns null for non-positive or non-integer attempts', () => {
    expect(nextDelay(0)).toBeNull()
    expect(nextDelay(-1)).toBeNull()
    expect(nextDelay(1.5)).toBeNull()
    expect(nextDelay(Number.NaN)).toBeNull()
  })

  it('isReconnectExhausted true only when attempt > MAX', () => {
    expect(isReconnectExhausted(5)).toBe(false)
    expect(isReconnectExhausted(6)).toBe(true)
    expect(isReconnectExhausted(0)).toBe(false)
  })
})

describe('Property 29: reconnect backoff sequence (task 9.17)', () => {
  it('nextDelay(n) === 2^(n-1) for n in [1, 5]', () => {
    fc.assert(
      fc.property(fc.integer({ min: 1, max: MAX_RECONNECT_ATTEMPTS }), (n) => {
        expect(nextDelay(n)).toBe(2 ** (n - 1))
      }),
    )
  })

  it('nextDelay(n) === null for n > 5 or n < 1', () => {
    fc.assert(
      fc.property(
        fc.oneof(fc.integer({ min: -100, max: 0 }), fc.integer({ min: 6, max: 1000 })),
        (n) => {
          expect(nextDelay(n)).toBeNull()
        },
      ),
    )
  })
})
