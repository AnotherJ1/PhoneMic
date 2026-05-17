/**
 * 配置 schema 镜像（`config.toml`）。
 *
 * Rust 来源：`crates/phonemic-protocol/src/config.rs`
 * 设计来源：`.kiro/specs/phone-mic-voice-input/design.md` §5.3
 *
 * 移动端不直接读写 `config.toml`，但仍需引用部分类型（如 `AsrLang`、
 * `UiLanguage`）以保证设置 UI 与桌面端约束同步演进。当 Rust 端调整
 * 默认值或字段时，这里必须同步更新；fingerprint stamp 会拦截漂移。
 */

/**
 * 桌面端 UI 显示语言（关联 Req 8.1）。
 *
 * - `auto`：跟随系统语言。
 * - `zh-CN`：简体中文。
 * - `en-US`：美式英语。
 */
export type UiLanguage = 'auto' | 'zh-CN' | 'en-US'

/**
 * ASR 默认语种（关联 Req 5.3）。
 *
 * 当前仅支持 `zh-CN` / `en-US`，未来如需扩展将由设计文档先行声明。
 */
export type AsrLang = 'zh-CN' | 'en-US'

/**
 * `[server]` —— Web_Server 监听相关配置（design.md §5.3）。
 */
export interface ServerCfg {
  /** 首选 TCP 端口；占用时由端口选择器在 1024–65535 内回退（Req 2.2）。默认 18080。 */
  preferredPort: number
  /** 是否启用 HTTPS（自签证书）；默认关闭（Req 2.3 / 2.4）。 */
  enableHttps: boolean
  /** 是否仅监听 LAN 接口；默认 `true`，避免误绑定公网地址。 */
  bindLanOnly: boolean
}

/** `[ui]` —— 桌面端 UI 配置（Req 8.1）。 */
export interface UiCfg {
  /** 桌面端 UI 语言；`auto` 表示跟随系统语言。默认 `auto`。 */
  language: UiLanguage
}

/** `[asr]` —— ASR 偏好配置（Req 5.3 / 5.4）。 */
export interface AsrCfg {
  /** 默认识别语种；默认 `zh-CN`。 */
  defaultLang: AsrLang
  /** 是否优先使用 Server_ASR；默认 `false`（优先 Browser_ASR）。 */
  preferServerAsr: boolean
}

/**
 * `[input]` —— 键盘注入相关配置。
 *
 * 注意：`injectDelayMs` 必须在 [0, 500] 内（Req 6.5）。
 */
export interface InputCfg {
  /** 字符间注入延迟，单位毫秒；范围 0–500（Req 6.5）。默认 0。 */
  injectDelayMs: number
  /** 是否暂停注入（Req 6.7）。默认 `false`。 */
  paused: boolean
}

/** `[security]` —— 配对设备的安全策略。 */
export interface SecurityCfg {
  /** 闲置自动撤销天数（design.md §5.3）；默认 30。 */
  autoRevokeIdleDays: number
}

/**
 * 顶层应用配置：聚合 `[server] / [ui] / [asr] / [input] / [security]` 五个段。
 */
export interface AppConfig {
  server: ServerCfg
  ui: UiCfg
  asr: AsrCfg
  input: InputCfg
  security: SecurityCfg
}

/**
 * `inject_delay_ms` 上限（Req 6.5：0–500 ms）。
 *
 * 与 Rust 端 `MAX_INJECT_DELAY_MS` 保持一致。
 */
export const MAX_INJECT_DELAY_MS = 500
