/**
 * 任务 9.15：心跳频率 + 断线阈值 fast-check + 受控时钟测试。
 *
 * 任务来源：tasks.md 9.15
 * 关联需求：R9.1、R9.2
 * 设计来源：design.md §7 Property 27, Property 28
 */

import { describe, it, expect } from 'vitest'
import * as fc from 'fast-check'
import {
  startHeartbeat,
  startWatchdog,
  PING_INTERVAL_MS,
  WATCHDOG_THRESHOLD_MS,
  type Clock,
} from '@/connection/heartbeat'

interface ScheduledTask {
  id: number
  fireAt: number
  interval: number | null // null === one-shot
  handler: () => void
  cancelled: boolean
}

/** 受控时钟：调用方手动 advance(ms) 才推进时间。 */
function makeMockClock(): Clock & {
  advance: (ms: number) => void
  current: () => number
  scheduledCount: () => number
} {
  let now = 0
  let nextId = 1
  const tasks: ScheduledTask[] = []

  function schedule(handler: () => void, ms: number, interval: number | null): number {
    const id = nextId++
    tasks.push({ id, fireAt: now + ms, interval, handler, cancelled: false })
    return id
  }

  function cancel(id: number): void {
    const t = tasks.find((x) => x.id === id)
    if (t) t.cancelled = true
  }

  function advance(ms: number): void {
    const target = now + ms
    // Loop because fired interval tasks reschedule themselves into the same window
    while (true) {
      const next = tasks
        .filter((t) => !t.cancelled && t.fireAt <= target)
        .sort((a, b) => a.fireAt - b.fireAt)[0]
      if (!next) break
      now = next.fireAt
      next.handler()
      if (next.interval != null) {
        next.fireAt = now + next.interval
      } else {
        next.cancelled = true
      }
    }
    now = target
  }

  return {
    now: () => now,
    setInterval: (h, ms) => schedule(h, ms, ms),
    setTimeout: (h, ms) => schedule(h, ms, null),
    clearInterval: (id) => cancel(id),
    clearTimeout: (id) => cancel(id),
    advance,
    current: () => now,
    scheduledCount: () => tasks.filter((t) => !t.cancelled).length,
  }
}

describe('Property 27: heartbeat send frequency (task 9.15)', () => {
  it('sends exactly floor(t / PING_INTERVAL_MS) pings over t ms', () => {
    fc.assert(
      fc.property(fc.integer({ min: 0, max: 600_000 }), (t) => {
        const clock = makeMockClock()
        let pings = 0
        const stop = startHeartbeat(() => (pings += 1), clock)
        clock.advance(t)
        stop()
        expect(pings).toBe(Math.floor(t / PING_INTERVAL_MS))
      }),
    )
  })

  it('exact-multiple boundary: at t = 5*INTERVAL we have 5 pings', () => {
    const clock = makeMockClock()
    let pings = 0
    const stop = startHeartbeat(() => (pings += 1), clock)
    clock.advance(5 * PING_INTERVAL_MS)
    stop()
    expect(pings).toBe(5)
  })
})

describe('Property 28: watchdog disconnect threshold (task 9.15)', () => {
  it('fires expired exactly once after 30s of silence', () => {
    const clock = makeMockClock()
    let expired = 0
    const wd = startWatchdog(() => (expired += 1), clock)

    clock.advance(WATCHDOG_THRESHOLD_MS - 1)
    expect(expired).toBe(0)
    clock.advance(2)
    expect(expired).toBe(1)

    // Subsequent ticks must not re-fire (one-shot semantics).
    clock.advance(60_000)
    expect(expired).toBe(1)
    wd.stop()
  })

  it('mark() resets the countdown', () => {
    const clock = makeMockClock()
    let expired = 0
    const wd = startWatchdog(() => (expired += 1), clock)

    clock.advance(WATCHDOG_THRESHOLD_MS - 5_000) // 25s
    wd.mark() // reset
    clock.advance(WATCHDOG_THRESHOLD_MS - 1) // 29.999s after mark
    expect(expired).toBe(0)
    clock.advance(2)
    expect(expired).toBe(1)
    wd.stop()
  })

  it('property: any sequence of marks separated by < THRESHOLD never expires', () => {
    fc.assert(
      fc.property(
        fc.array(fc.integer({ min: 1, max: WATCHDOG_THRESHOLD_MS - 1 }), {
          minLength: 1,
          maxLength: 30,
        }),
        (gaps) => {
          const clock = makeMockClock()
          let expired = 0
          const wd = startWatchdog(() => (expired += 1), clock)
          for (const g of gaps) {
            clock.advance(g)
            wd.mark()
          }
          expect(expired).toBe(0)
          wd.stop()
        },
      ),
    )
  })
})
