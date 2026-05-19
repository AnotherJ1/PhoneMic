/**
 * 自动发送计数器 —— 任务 9.10 + Property 7。
 *
 * 任务来源：tasks.md 9.10
 * 关联需求：R4.6、R4.7、R5.8
 * 设计来源：design.md §4.7
 *
 * 纯函数 `autoSendDispatch(events, autoSend)`：模拟 ASR 事件流到达时
 * 自动发送（auto） / 手动发送（manual）的取舍。
 *
 * - `autoSend = true`：每个 `final` 事件触发一次 `text.submit`，`interim`
 *   事件不触发；返回总提交次数。
 * - `autoSend = false`：所有 `final` 事件入草稿队列等待用户手动点击发送，
 *   返回值固定为 0（提交次数由用户在 UI 中决定，本函数只负责"不自动发"
 *   的语义）。
 *
 * Property 7：当 `autoSend = true`，`autoSendDispatch(events, true) === final 事件个数`。
 */

/** ASR 事件的最小形态（与 BrowserAsr / ServerAsr 统一）。 */
export type AsrEvent =
  | { type: 'interim'; text: string }
  | { type: 'final'; text: string }

/**
 * 计算给定事件流应触发多少次 `text.submit`。
 *
 * Property 7 不变量：
 *  - autoSend === true ⇒ submitCount === count(events where type==='final')
 *  - autoSend === false ⇒ submitCount === 0
 *  - 不依赖事件顺序，幂等：同一输入恒返回同一输出。
 */
export function autoSendDispatch(events: readonly AsrEvent[], autoSend: boolean): number {
  if (!autoSend) return 0
  let count = 0
  for (const e of events) {
    if (e.type === 'final') count += 1
  }
  return count
}

/**
 * 与 {@link autoSendDispatch} 配套的 draft 提取器：当 `autoSend === false`，
 * 返回最近一次 `final` 事件的文本（用户可在 UI 中编辑后手动发送）。
 *
 * 该函数不耦合 UI；仅作为纯函数同时被 reducer 与属性测试复用。
 */
export function latestFinalDraft(events: readonly AsrEvent[]): string {
  for (let i = events.length - 1; i >= 0; i -= 1) {
    const e = events[i]
    if (e.type === 'final') return e.text
  }
  return ''
}
