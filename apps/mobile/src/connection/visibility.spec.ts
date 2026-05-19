/**
 * 任务 9.21：Property 34 后台返回触发重连 —— fast-check。
 *
 * 任务来源：tasks.md 9.21
 * 关联需求：R9.9
 * 设计来源：design.md §7 Property 34
 */

import { describe, it, expect } from 'vitest'
import * as fc from 'fast-check'
import { shouldReconnect, type Visibility } from '@/connection/visibility'
import { CONNECTION_STATUSES, type ConnectionStatus } from '@/connection/status'

const visArb = fc.constantFrom<Visibility>('hidden', 'visible')
const statusArb = fc.constantFrom<ConnectionStatus>(...CONNECTION_STATUSES)

describe('Property 34: visibility -> reconnect trigger (task 9.21)', () => {
  it('triggers when prev=hidden, curr=visible, and status !== Connected', () => {
    fc.assert(
      fc.property(visArb, visArb, statusArb, (prev, curr, status) => {
        const expected = prev === 'hidden' && curr === 'visible' && status !== 'Connected'
        expect(shouldReconnect(prev, curr, status)).toBe(expected)
      }),
    )
  })

  it('over a sequence, reconnect count equals number of hidden->visible transitions while disconnected', () => {
    fc.assert(
      fc.property(
        fc.array(visArb, { minLength: 1, maxLength: 50 }),
        statusArb,
        (sequence, status) => {
          let count = 0
          for (let i = 1; i < sequence.length; i += 1) {
            if (shouldReconnect(sequence[i - 1], sequence[i], status)) count += 1
          }
          if (status === 'Connected') {
            expect(count).toBe(0)
          } else {
            // Count transitions hidden -> visible
            let expected = 0
            for (let i = 1; i < sequence.length; i += 1) {
              if (sequence[i - 1] === 'hidden' && sequence[i] === 'visible') expected += 1
            }
            expect(count).toBe(expected)
          }
        },
      ),
    )
  })
})
