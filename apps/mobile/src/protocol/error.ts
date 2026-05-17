/**
 * `ErrorCode` —— 跨 HTTP / WebSocket / 桌面端日志统一的错误码常量。
 *
 * Rust 来源：`crates/phonemic-protocol/src/error.rs`
 * 设计来源：`.kiro/specs/phone-mic-voice-input/design.md` §8.1
 * 关联需求：9.6、9.8
 *
 * 与 Rust `ErrorCode` 枚举的 SCREAMING_SNAKE_CASE 序列化字面量严格一一对应。
 * 任何修改 Rust 端枚举的 PR 都必须同步更新本文件并重新生成 stamp
 * （见 `scripts/gen-ts-types.mjs`）。
 */

/**
 * 全部错误码常量数组，作为单一来源派生联合类型 {@link ErrorCode}。
 *
 * 顺序与 Rust 端 `ErrorCode::ALL` 保持一致，便于与 Rust 端逐项比对。
 */
export const ERROR_CODES = [
  /** 当前 OS 或版本不在受支持范围内（关联需求 1.5） */
  'OS_UNSUPPORTED',
  /** 端口选择失败 / 全部占用（关联需求 2.8） */
  'PORT_UNAVAILABLE',
  /** LAN 连接丢失（关联需求 3.6） */
  'LAN_LOST',
  /** 浏览器麦克风权限被拒（关联需求 4.8 / 5.1） */
  'MIC_PERMISSION_DENIED',
  /** ASR 单段超时（关联需求 5.7） */
  'ASR_TIMEOUT',
  /** 配对码错误或已失效（关联需求 7.2 / 7.9） */
  'PAIR_INVALID',
  /** 配对失败次数超过阈值，进入限流冻结（关联需求 7.5） */
  'PAIR_RATELIMIT',
  /** 缺少或非法的 Session_Token（关联需求 7.4 / 9.6） */
  'AUTH_REQUIRED',
  /** 来源 IP 不在 RFC1918 / loopback 子网内（关联需求 7.8） */
  'FORBIDDEN_SUBNET',
  /** 当前没有可注入的前台焦点窗口（关联需求 6.6） */
  'INJECT_NO_FOCUS_TARGET',
  /** 平台辅助功能 / 注入权限被拒（关联需求 6.8） */
  'INJECT_PERMISSION_DENIED',
  /** 注入处于暂停状态（关联需求 6.7） */
  'INJECT_PAUSED',
  /** 平台后端注入失败（关联需求 9.8） */
  'INJECT_BACKEND_ERROR',
  /** WebSocket 报文非法 / 缺字段（关联需求 9.6） */
  'MSG_BAD_FORMAT',
  /** 重连退避序列耗尽仍未成功（关联需求 9.5） */
  'RECONNECT_FAILED',
] as const

/**
 * 错误码字面量联合类型。
 *
 * 通过 `typeof ERROR_CODES[number]` 派生，保证常量数组与类型同步演进，
 * 同时支持 `switch (code)` 的穷尽性检查。
 */
export type ErrorCode = (typeof ERROR_CODES)[number]

/**
 * 类型谓词：判断任意字符串是否为已声明的错误码。
 *
 * 使用场景：从网络收到 `error` / `inject.error` 等消息时，将原始字符串
 * 收窄为 `ErrorCode`，避免下游分支误判未知码。
 */
export function isErrorCode(value: string): value is ErrorCode {
  return (ERROR_CODES as readonly string[]).includes(value)
}
