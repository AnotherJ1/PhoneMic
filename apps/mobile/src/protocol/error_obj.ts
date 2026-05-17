/**
 * 统一错误对象 `AppError`。
 *
 * Rust 来源：`crates/phonemic-protocol/src/error_obj.rs`
 * 设计来源：`.kiro/specs/phone-mic-voice-input/design.md` §8.1
 * 关联需求：7.2、7.3、9.6
 *
 * 所有跨 HTTP / WebSocket / 桌面端日志的错误都以此结构序列化，确保前端
 * 集中处理（design.md §8.2 "结构化优先"）。`detail` 字段为可选；当 Rust
 * 端为 `None` 时不会出现在 JSON 中，因此 TS 端使用 `?:`（可选属性）镜像。
 */

import type { ErrorCode } from './error'

/**
 * `AppError.detail` 字段的形态：任意 JSON 值。
 *
 * Rust 端使用 `serde_json::Value`；在 TS 端我们用 `unknown` 收紧入口，
 * 由消费者自行收窄。如确需结构化访问，可使用类型守卫或运行时校验。
 */
export type AppErrorDetail = unknown

/**
 * 统一错误对象，与 Rust `AppError` 严格对应。
 *
 * 字段说明（与 Rust 端注释保持一致）：
 * - `code`：错误码，详见 {@link ErrorCode}。Rust 端当前仍为 `String`，
 *   这里保留为字符串字面量联合，提升前端类型安全。如未来 Rust 切换
 *   为强类型枚举，本字段保持不变即可。
 * - `message`：用户可见的错误消息，不得包含敏感信息。
 * - `detail`：结构化补充信息；为 `None` / 缺省时不出现。
 * - `ts`：错误时间戳，RFC3339 字符串（如 `"2025-01-01T12:00:00.123Z"`）。
 */
export interface AppError {
  /** 错误码，参见 {@link ErrorCode}。 */
  code: ErrorCode
  /** 用户可见的错误消息，不得包含敏感信息。 */
  message: string
  /** 结构化补充信息；与 Rust 端 `Option<serde_json::Value>` 对齐。 */
  detail?: AppErrorDetail
  /** RFC3339 / ISO-8601 时间戳，UTC，毫秒精度，`Z` 后缀。 */
  ts: string
}
