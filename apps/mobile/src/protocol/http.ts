/**
 * HTTP API 类型定义。
 *
 * Rust 来源：`crates/phonemic-protocol/src/http.rs`
 * 设计来源：`.kiro/specs/phone-mic-voice-input/design.md` §5.2
 * 关联需求：7.2、7.3
 *
 * 字段命名：与 Rust 端 `#[serde(rename_all = "camelCase")]` 一致，
 * 即 wire 字段为 camelCase。TS 接口字段名与 wire 字段一一对应。
 */

/**
 * `POST /api/pair` 请求体（design.md §5.2）。
 *
 * - `pairingCode`：桌面端当前展示的 8 位配对码（`[A-Z0-9]`，去除易混字符）。
 * - `fingerprint`：移动端生成的设备指纹，hex 编码。
 * - `deviceLabel`：人类可读设备标签，用于桌面端列表展示。
 */
export interface PairRequest {
  /** 配对码明文，仅在 LAN 内传输；恒定时间比较由服务端完成。 */
  pairingCode: string
  /** 设备指纹，hex 编码。 */
  fingerprint: string
  /** 设备标签，由移动端从 UA / 屏幕分辨率推导。 */
  deviceLabel: string
}

/**
 * `POST /api/pair` 成功响应体（design.md §5.2）。
 *
 * - `sessionToken`：256 位随机数，Base64URL 编码。
 * - `expiresAt`：过期时间，RFC3339 / ISO-8601 字符串。
 */
export interface PairResponse {
  /** 移动端后续 WebSocket / HTTP 调用使用的 Bearer Token。 */
  sessionToken: string
  /** 过期时间，RFC3339 字符串（如 `"2025-01-01T12:00:00Z"`）。 */
  expiresAt: string
}

/**
 * `GET /api/health` 响应体（design.md §5.2）。
 */
export interface HealthResponse {
  /** 桌面端语义化版本号，形如 `"x.y.z"`。 */
  version: string
  /** 服务进程已运行秒数。 */
  uptime: number
}
