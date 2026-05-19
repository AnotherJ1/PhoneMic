/**
 * 离线消息队列与重连后保序补发 —— 任务 9.18 + Property 30。
 *
 * 任务来源：tasks.md 9.18
 * 关联需求：R9.4
 * 设计来源：design.md §4.7、§6.4
 *
 * 行为：
 *  - 当 ConnectionService 不在 `Connected` 状态时，`text.submit` 入队；
 *  - 重连成功后按入队顺序逐条发送；
 *  - 每条消息的 `id` 字段用于服务端去重。
 *
 * 此模块只承担"队列数据结构 + 纯保序发送计算"两项职责，副作用（实际
 * `ws.send`）由 ConnectionService 在 9.3 中调用 {@link drainQueue} 时按返
 * 回顺序逐条投递。
 */

import type { TextSubmitPayload } from '@/protocol'

/** 入队条目：为 `text.submit` 消息附加 `id` 字段，便于去重。 */
export interface QueuedSubmit {
  /** 服务端去重用的稳定 ID（建议 UUIDv4）。 */
  readonly id: string
  /** 真正的 `text.submit` 载荷。 */
  readonly payload: TextSubmitPayload
}

/** 队列实例：使用普通数组承载，避免链表实现的额外复杂度。 */
export type OfflineQueue = readonly QueuedSubmit[]

/** 创建空队列。 */
export function createOfflineQueue(): OfflineQueue {
  return []
}

/**
 * 入队：返回新数组（保持纯函数语义）。
 *
 * 不变量（Property 30）：
 *  - `enqueue(q, item).length === q.length + 1`
 *  - `enqueue(q, item)[i] === q[i]` for i < q.length
 *  - `enqueue(q, item)[q.length] === item`
 */
export function enqueue(q: OfflineQueue, item: QueuedSubmit): OfflineQueue {
  return q.concat([item])
}

/**
 * 一次性取出全部条目（重连成功后调用）。
 *
 * 返回 `{ ordered, remaining }`：
 *  - `ordered`：按入队顺序排列的待发送条目；
 *  - `remaining`：调用方如果只想发送前 K 条，可使用此值（当前默认全发，
 *    `remaining = []`）。
 *
 * Property 30 不变量：发送顺序与入队顺序一致；每条恰好一次。
 */
export function drainQueue(q: OfflineQueue): {
  ordered: readonly QueuedSubmit[]
  remaining: OfflineQueue
} {
  return { ordered: q.slice(), remaining: [] }
}

/** 基于 `id` 的去重过滤（防御性，正常路径下 id 由调用方保证唯一）。 */
export function dedupById(q: OfflineQueue): OfflineQueue {
  const seen = new Set<string>()
  const out: QueuedSubmit[] = []
  for (const item of q) {
    if (seen.has(item.id)) continue
    seen.add(item.id)
    out.push(item)
  }
  return out
}
