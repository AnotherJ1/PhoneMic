/**
 * 连接状态文本映射 —— 任务 9.4 + Property 9。
 *
 * 任务来源：tasks.md 9.4
 * 关联需求：R4.5
 * 设计来源：design.md §4.7
 *
 * 纯函数：`statusLabel(status, lang) -> string`。
 *
 * 从 i18n 字典读取，与 store 解耦，便于属性测试。
 */

import { translate, type UiLang } from '@/i18n'

/**
 * 连接状态机的所有取值（与 ConnectionService 状态机对齐 —— tasks 9.3）。
 */
export const CONNECTION_STATUSES = [
  'Disconnected',
  'Connecting',
  'Connected',
  'Reconnecting',
] as const

export type ConnectionStatus = (typeof CONNECTION_STATUSES)[number]

/** 状态枚举到 i18n key 的稳定映射。 */
const STATUS_KEY: Readonly<Record<ConnectionStatus, string>> = {
  Disconnected: 'status.disconnected',
  Connecting: 'status.connecting',
  Connected: 'status.connected',
  Reconnecting: 'status.reconnecting',
}

/**
 * 把连接状态映射为给定语言下的文案。
 *
 * 不变量（Property 9）：
 *  - 同一 `(status, lang)` 多次调用结果一致；
 *  - 返回值 = `i18n[lang][STATUS_KEY[status]]`；
 *  - 不抛异常。
 */
export function statusLabel(status: ConnectionStatus, lang: UiLang): string {
  return translate(lang, STATUS_KEY[status])
}
