/**
 * visibilitychange 触发重连 —— 任务 9.20 + Property 34（task 9.21）。
 *
 * 任务来源：tasks.md 9.20
 * 关联需求：R9.9
 * 设计来源：design.md §4.7、§6.4
 *
 * 监听 `document.visibilitychange`：从 `hidden` 变为 `visible` 时若状态非
 * `Connected`，立即触发一次重连。本模块只暴露纯逻辑：状态机 reducer，
 * 由调用方（ConnectionService）订阅事件并把 visibility 转换为事件传入。
 */

import type { ConnectionStatus } from './status'

/** 可见性事件流：只有 hidden / visible 两种。 */
export type Visibility = 'hidden' | 'visible'

/**
 * 决定 visibilitychange 事件是否应触发重连。
 *
 * Property 34 不变量：当且仅当 (前一帧 visibility === hidden) ∧
 *   (当前 visibility === visible) ∧ (status !== 'Connected') 时返回 true。
 *
 * 该函数纯函数化设计便于属性测试；副作用（实际调用 `connect()`）由
 * ConnectionService 在收到 true 时执行一次。
 */
export function shouldReconnect(
  prev: Visibility,
  curr: Visibility,
  status: ConnectionStatus,
): boolean {
  return prev === 'hidden' && curr === 'visible' && status !== 'Connected'
}
