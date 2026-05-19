/**
 * 识别历史队列 —— 任务 9.12 + Property 8。
 *
 * 任务来源：tasks.md 9.12
 * 关联需求：R4.9
 * 设计来源：design.md §4.7
 *
 * 数据结构 `pushBounded(queue, item, capacity)`：FIFO 队列，超过容量时丢弃
 * 最早条目，保证保留最近 N 条。
 *
 * 实现选择：返回新数组（而不是 in-place mutate），便于和 Pinia / Vue
 * reactive 配合，避免引用相等导致 UI 不刷新。
 */

/** 默认历史容量（与 R4.9 同步）。 */
export const HISTORY_CAPACITY = 50

/**
 * 把 `item` 追加到 `queue`，长度超过 `capacity` 时丢弃最早条目。
 *
 * Property 8 不变量：
 *  - `result.length === Math.min(queue.length + 1, capacity)`
 *  - 当 `queue.length + 1 <= capacity`：`result === [...queue, item]`
 *  - 当 `queue.length + 1 > capacity`：`result` 等于 `[...queue, item]` 的最后 capacity 条
 *  - 不修改输入 `queue`（纯函数）
 *  - `capacity <= 0` 时返回空数组（无限丢弃，等价于禁用历史）
 */
export function pushBounded<T>(queue: readonly T[], item: T, capacity: number): T[] {
  if (capacity <= 0) return []
  const next = queue.concat([item])
  if (next.length <= capacity) return next
  return next.slice(next.length - capacity)
}
