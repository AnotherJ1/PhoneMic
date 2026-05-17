/**
 * WebSocket 消息类型镜像。
 *
 * Rust 来源：`crates/phonemic-protocol/src/ws.rs`
 * 设计来源：`.kiro/specs/phone-mic-voice-input/design.md` §5.1
 *
 * 协议形态：所有消息为 UTF-8 JSON，统一使用
 * `{ "type": <string>, "id"?: <string>, "payload": <object> }` 结构。
 * Rust 端使用 `#[serde(tag = "type", content = "payload")]` adjacently
 * tagged，TS 端以可辨识联合（discriminated union）镜像，`type` 字段为
 * 字面量类型，便于 `switch (msg.type)` 的穷尽性检查。
 *
 * 任何修改 Rust 端 `ws.rs` 的 PR 都必须同步更新本文件并重新生成 stamp
 * （见 `scripts/gen-ts-types.mjs`）。
 */

import type { ErrorCode } from './error'

/** 单条文本字段允许的最大字节数（与 Rust `MAX_TEXT_BYTES` 对齐）。 */
export const MAX_TEXT_BYTES = 16 * 1024

/** 单帧 `audio.chunk` 中 base64 字符串允许的最大字节数（与 Rust `MAX_AUDIO_CHUNK_BASE64_BYTES` 对齐）。 */
export const MAX_AUDIO_CHUNK_BASE64_BYTES = 256 * 1024

/**
 * 音频编码（Server_ASR 模式下的 `audio.chunk.codec` 字段）。
 *
 * - `pcm16k`：16 kHz / 16-bit / 单声道线性 PCM
 * - `opus`：Opus over WebM 容器或裸 Opus 包
 */
export type AudioCodec = 'pcm16k' | 'opus'

// ---------- 客户端 → 服务端 payload ----------

/**
 * `hello` 消息载荷。
 *
 * 在 WebSocket 握手成功后由 Mobile 首先发送。`useServerASR` 字段名保留
 * 缩写大小写，与设计文档 §5.1.1 严格一致。
 */
export interface HelloPayload {
  /** 用户可读的设备标签（如 "Pixel 8 (Chrome)"）。 */
  deviceLabel: string
  /** 客户端期望使用的 ASR 语言（BCP-47，如 "zh-CN"）。 */
  lang: string
  /** 是否请求使用 Server_ASR；为 false 表示客户端使用浏览器原生 ASR。 */
  useServerASR: boolean
}

/**
 * `text.submit` 消息载荷。
 *
 * Mobile 把一段最终识别得到的文本提交给桌面端进行键盘注入。
 * 这是 Property 11（Unicode round-trip）的核心结构。
 */
export interface TextSubmitPayload {
  /** UTF-8 文本，禁止为空，长度 ≤ {@link MAX_TEXT_BYTES}。 */
  text: string
  /** 文本语言（BCP-47）。 */
  lang: string
  /** 客户端发送时的本地时间戳（毫秒，Unix 纪元）。 */
  clientTs: number
}

/**
 * `text.preview` 消息载荷。
 *
 * 中间识别结果，仅用于 UI 同步，不会触发注入。`interim` 字段在
 * 构造时永远为 `true`。
 */
export interface TextPreviewPayload {
  text: string
  /** 永远为 `true`；保留字段以便协议演进。 */
  interim: true
}

/** `audio.chunk` 消息载荷（Server_ASR 模式）。 */
export interface AudioChunkPayload {
  /** 单调递增的分片序号；服务端校验 monotonic 在会话层完成。 */
  seq: number
  /** 编码格式。 */
  codec: AudioCodec
  /** base64 编码后的音频载荷字符串。 */
  data: string
  /** 可选的客户端时间戳（毫秒）；Rust 端 `None` 时不出现在 JSON 中。 */
  tsMs?: number
}

/** `audio.end` 消息载荷。 */
export interface AudioEndPayload {
  seq: number
}

/** `ping` 消息载荷。 */
export interface PingPayload {
  clientTs: number
}

// ---------- 服务端 → 客户端 payload ----------

/** `welcome` 消息中的 `feature` 字段。 */
export interface WelcomeFeatures {
  /** 是否启用 Server_ASR；wire 字段名为 `serverASR`。 */
  serverASR: boolean
}

/** `welcome` 消息载荷。 */
export interface WelcomePayload {
  /** 服务端发送时的时间戳（毫秒，Unix 纪元）。 */
  serverTs: number
  /** 协议版本号；与 `PROTOCOL_VERSION` 对应。 */
  protocol: string
  /** 服务端能力位图。 */
  feature: WelcomeFeatures
}

/** `inject.ack` 消息载荷。 */
export interface InjectAckPayload {
  /** 对应的 `text.submit.id`。 */
  id: string
  /** 实际注入的字符数（按 Unicode scalar 计）。 */
  chars: number
}

/**
 * `inject.error` 消息载荷。
 *
 * Rust 端的 `code` 字段当前仍为 `String`（任务 2.3 留作切换强类型枚举
 * 时的占位），TS 端这里直接收紧到 {@link ErrorCode} 联合，让上层逻辑
 * 第一时间获得类型保护。
 */
export interface InjectErrorPayload {
  /** 对应的 `text.submit.id`。 */
  id: string
  /** 错误码，例如 `INJECT_NO_FOCUS_TARGET`。 */
  code: ErrorCode
  /** 给用户阅读的错误描述。 */
  message: string
}

/** `transcript.final` 消息载荷（Server_ASR 返回最终识别结果）。 */
export interface TranscriptFinalPayload {
  text: string
  lang: string
  /** 可选置信度（0.0–1.0）；Rust 端 `None` 时不出现在 JSON 中。 */
  conf?: number
}

/** `pong` 消息载荷。 */
export interface PongPayload {
  serverTs: number
}

/** `error` 消息载荷（通用错误）。 */
export interface ErrorPayload {
  code: ErrorCode
  message: string
}

// ---------- 顶层消息：客户端 → 服务端 ----------

/**
 * 客户端 → 服务端的可辨识联合。
 *
 * Rust 端的 `id?: string` 字段在 wire 层为 `{ "id"?: string }`，被外层
 * `ClientMessage` struct 通过 `#[serde(flatten)]` 与 `kind` 合并。TS 端
 * 直接把 `id?` 加到每个变体上，避免引入额外的包装层。
 */
export type ClientMessage =
  | ({ type: 'hello'; payload: HelloPayload } & WithOptionalId)
  | ({ type: 'text.submit'; payload: TextSubmitPayload } & WithOptionalId)
  | ({ type: 'text.preview'; payload: TextPreviewPayload } & WithOptionalId)
  | ({ type: 'audio.chunk'; payload: AudioChunkPayload } & WithOptionalId)
  | ({ type: 'audio.end'; payload: AudioEndPayload } & WithOptionalId)
  | ({ type: 'ping'; payload: PingPayload } & WithOptionalId)

// ---------- 顶层消息：服务端 → 客户端 ----------

/** 服务端 → 客户端的可辨识联合，结构与 {@link ClientMessage} 类似。 */
export type ServerMessage =
  | ({ type: 'welcome'; payload: WelcomePayload } & WithOptionalId)
  | ({ type: 'inject.ack'; payload: InjectAckPayload } & WithOptionalId)
  | ({ type: 'inject.error'; payload: InjectErrorPayload } & WithOptionalId)
  | ({ type: 'transcript.final'; payload: TranscriptFinalPayload } & WithOptionalId)
  | ({ type: 'pong'; payload: PongPayload } & WithOptionalId)
  | ({ type: 'error'; payload: ErrorPayload } & WithOptionalId)

/** 提取所有客户端消息的 `type` 字面量集合。 */
export type ClientMessageType = ClientMessage['type']

/** 提取所有服务端消息的 `type` 字面量集合。 */
export type ServerMessageType = ServerMessage['type']

/**
 * 全部客户端消息的 `type` 字面量数组（与 Rust 端 `ClientMessageKind` 顺序一致）。
 *
 * 用于 fingerprint stamp 校验与运行时枚举：任何拼写漂移都能被静态类型检查捕获。
 */
export const CLIENT_MESSAGE_TYPES = [
  'hello',
  'text.submit',
  'text.preview',
  'audio.chunk',
  'audio.end',
  'ping',
] as const satisfies readonly ClientMessageType[]

/**
 * 全部服务端消息的 `type` 字面量数组（与 Rust 端 `ServerMessageKind` 顺序一致）。
 */
export const SERVER_MESSAGE_TYPES = [
  'welcome',
  'inject.ack',
  'inject.error',
  'transcript.final',
  'pong',
  'error',
] as const satisfies readonly ServerMessageType[]

/** 仅包含可选 `id` 字段的辅助类型，便于在联合中复用。 */
interface WithOptionalId {
  /** 可选的消息 ID，用于断线补发去重（Requirement 9.4）。 */
  id?: string
}
