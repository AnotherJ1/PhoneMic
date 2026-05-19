/**
 * 心跳与看门狗 —— 任务 9.14 + Property 27/28（受控时钟）。
 *
 * 任务来源：tasks.md 9.14
 * 关联需求：R9.1、R9.2
 * 设计来源：design.md §4.7、§6.4
 *
 * 设计：抽象出"时钟"接口，让 fast-check + 受控时钟在 9.15 中能完整覆盖
 * Property 27（每 20 秒发一次 ping）与 Property 28（30 秒未响应即断线）。
 */

/** 时钟抽象：暴露 now() / setInterval / setTimeout / clearInterval / clearTimeout。 */
export interface Clock {
  now(): number
  setInterval(handler: () => void, ms: number): number
  setTimeout(handler: () => void, ms: number): number
  clearInterval(id: number): void
  clearTimeout(id: number): void
}

/** 真实环境时钟：直接转发到全局对象。 */
export const realClock: Clock = {
  now: () => Date.now(),
  setInterval: (h, ms) => globalThis.setInterval(h, ms) as unknown as number,
  setTimeout: (h, ms) => globalThis.setTimeout(h, ms) as unknown as number,
  clearInterval: (id) => globalThis.clearInterval(id),
  clearTimeout: (id) => globalThis.clearTimeout(id),
}

export const PING_INTERVAL_MS = 20_000
export const WATCHDOG_THRESHOLD_MS = 30_000

/**
 * 启动心跳。每 {@link PING_INTERVAL_MS} 毫秒调用 `sendPing`。
 *
 * 返回 stop 函数。Property 27：当连接保持 t 秒，恰好发送 `floor(t / 20)` 次 ping。
 */
export function startHeartbeat(
  sendPing: () => void,
  clock: Clock = realClock,
): () => void {
  const id = clock.setInterval(sendPing, PING_INTERVAL_MS)
  return () => clock.clearInterval(id)
}

/** 看门狗状态：调用方在每次收到 pong/ack 时调用 `mark()` 更新 lastSeen。 */
export interface Watchdog {
  /** 收到任意服务端消息时调用，刷新 lastSeen。 */
  mark(): void
  /** 检查 `now - lastSeen >= WATCHDOG_THRESHOLD_MS`；返回 `true` 表示已超时。 */
  isExpired(): boolean
  /** 停止周期检查并释放资源。 */
  stop(): void
}

/**
 * 启动看门狗：每 1 秒检查一次。超时调用 `onExpired` 一次。
 *
 * Property 28：从最近一次 mark() 起 30 秒未再 mark()，状态进入 `Disconnected`。
 */
export function startWatchdog(
  onExpired: () => void,
  clock: Clock = realClock,
  thresholdMs: number = WATCHDOG_THRESHOLD_MS,
): Watchdog {
  let lastSeen = clock.now()
  let fired = false
  const intervalId = clock.setInterval(() => {
    if (fired) return
    if (clock.now() - lastSeen >= thresholdMs) {
      fired = true
      onExpired()
    }
  }, 1_000)
  return {
    mark(): void {
      lastSeen = clock.now()
      fired = false
    },
    isExpired(): boolean {
      return clock.now() - lastSeen >= thresholdMs
    },
    stop(): void {
      clock.clearInterval(intervalId)
    },
  }
}
