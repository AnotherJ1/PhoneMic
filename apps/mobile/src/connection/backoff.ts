/**
 * 重连退避 —— 任务 9.16 + Property 29。
 *
 * 任务来源：tasks.md 9.16
 * 关联需求：R9.3、R9.5
 * 设计来源：design.md §4.7、§6.4、§8.1
 *
 * 退避序列固定为 `[1, 2, 4, 8, 16]` 秒，最多 5 次。第 6 次以后切换到
 * `RECONNECT_FAILED` 错误视图，并提供"手动重试"按钮重置 attempt。
 *
 * 纯函数 `nextDelay(attempt) -> seconds | null`：
 *  - `attempt ∈ {1,2,3,4,5}` ⇒ 返回 `2^(attempt-1)`；
 *  - 其它（含 0、负数、>=6）⇒ 返回 `null`。
 *
 * 这与 Property 29 描述完全一致。
 */

/** 最大重连尝试次数。 */
export const MAX_RECONNECT_ATTEMPTS = 5

/** 退避序列（秒）。导出供测试和文档复用，不应被修改。 */
export const BACKOFF_DELAYS_SECONDS: readonly number[] = Object.freeze([1, 2, 4, 8, 16])

/**
 * 返回第 `attempt` 次重连应等待的秒数；超过最大次数返回 `null`。
 *
 * 注意 `attempt` 从 1 开始计数：第 1 次重连等 1 秒，第 2 次等 2 秒……
 *
 * Property 29：`nextDelay(n) === 2^(n-1)` (n ∈ [1, 5])；超过 5 返回 null。
 */
export function nextDelay(attempt: number): number | null {
  if (!Number.isInteger(attempt)) return null
  if (attempt < 1 || attempt > MAX_RECONNECT_ATTEMPTS) return null
  // 等价于 BACKOFF_DELAYS_SECONDS[attempt - 1]，但用 2^(n-1) 表达让 Property 29 直接可读。
  return 2 ** (attempt - 1)
}

/** 是否已进入"重连失败"终态（即 attempt 超过最大次数）。 */
export function isReconnectExhausted(attempt: number): boolean {
  return Number.isInteger(attempt) && attempt > MAX_RECONNECT_ATTEMPTS
}
