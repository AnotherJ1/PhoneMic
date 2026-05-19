//! WebSocket 消息类型定义。
//!
//! 设计来源：`.kiro/specs/phone-mic-voice-input/design.md` §5.1（消息协议）。
//!
//! 协议形态：所有消息为 UTF-8 JSON，统一使用
//! `{ "type": <string>, "id"?: <string>, "payload": <object> }` 结构。
//! 其中 `type` 字段作为 [`serde`] 的内部 tag，`id` 用于客户端补发去重
//! （Requirement 9.4）。
//!
//! 本任务（2.1）仅声明类型、构造辅助函数与基础字段校验；
//! 错误码 [`ErrorCode`] 等更细粒度的语义在任务 2.3 落地，
//! 因此本文件中的 `code` 字段暂时保留为 `String`。
//!
//! 与设计文档的字段对齐参见：
//! - §5.1.1 客户端 → 服务端：`hello` / `text.submit` / `text.preview`
//!   / `audio.chunk` / `audio.end` / `ping`
//! - §5.1.2 服务端 → 客户端：`welcome` / `inject.ack` / `inject.error`
//!   / `transcript.final` / `pong` / `error`

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 单条文本字段允许的最大字节数。
///
/// 用于校验 `text.submit` / `text.preview` / `transcript.final` 等
/// 所有携带文本的消息。16 KiB 是相对宽松的上限：日常一段语音
/// 转写的中文文本不超过几百字节，留出余量以容纳粘贴 / 重发场景。
pub const MAX_TEXT_BYTES: usize = 16 * 1024;

/// 单帧 `audio.chunk` 中 base64 字符串允许的最大字节数。
///
/// 16 kHz 16-bit PCM 单声道每秒约 32 KiB；base64 后约 43 KiB。
/// 设置 256 KiB 可覆盖一条最长 ~6 秒的分片，足以应付实际 chunk
/// 大小（一般 200ms ≤ chunk ≤ 1s）。
pub const MAX_AUDIO_CHUNK_BASE64_BYTES: usize = 256 * 1024;

// ---------- 协议错误 ----------

/// 消息构造 / 校验失败原因。
///
/// 这是一个轻量级、面向"消息层"的错误类型；HTTP / WS 层会在任务
/// 2.3 中引入更完整的 [`ErrorCode`] 枚举与统一错误对象。
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProtocolError {
    /// 文本字段为空（`text.submit` 等场景下不允许）。
    #[error("text field must not be empty")]
    EmptyText,

    /// 文本字段超过 [`MAX_TEXT_BYTES`]。
    #[error("text field exceeds {MAX_TEXT_BYTES} bytes")]
    TextTooLong,

    /// 音频载荷为空。
    #[error("audio chunk payload must not be empty")]
    EmptyAudio,

    /// 音频载荷过大。
    #[error("audio chunk payload exceeds {MAX_AUDIO_CHUNK_BASE64_BYTES} bytes")]
    AudioTooLong,

    /// 时间戳为负。
    #[error("timestamp field `{0}` must be non-negative")]
    NegativeTimestamp(&'static str),

    /// 通用字段校验失败。
    #[error("invalid field `{0}`: {1}")]
    BadField(&'static str, &'static str),
}

// ---------- 公共枚举 ----------

/// 音频编码（Server_ASR 模式下的 `audio.chunk.codec` 字段）。
///
/// - `Pcm16k`：16 kHz / 16-bit / 单声道线性 PCM
/// - `Opus`：Opus over WebM 容器或裸 Opus 包
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioCodec {
    #[serde(rename = "pcm16k")]
    Pcm16k,
    #[serde(rename = "opus")]
    Opus,
}

// ---------- 客户端 → 服务端 payload ----------

/// `hello` 消息载荷。
///
/// 在 WebSocket 升级握手成功后由 Mobile 首先发送，用于报告设备
/// 标签、ASR 语言以及是否启用 Server_ASR。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelloPayload {
    /// 用户可读的设备标签（如"Pixel 8 (Chrome)"）。
    pub device_label: String,
    /// 客户端期望使用的 ASR 语言（BCP-47，如 `zh-CN`）。
    pub lang: String,
    /// 是否请求使用 Server_ASR；为 false 时表示客户端使用浏览器原生 ASR。
    ///
    /// 与设计文档 §5.1.1 一致，wire 字段名为 `useServerASR`（保留缩写大小写）。
    #[serde(rename = "useServerASR")]
    pub use_server_asr: bool,
}

/// `text.submit` 消息载荷。
///
/// Mobile 把一段最终识别得到的文本提交给桌面端进行键盘注入。
/// 这是 Property 11（Unicode round-trip）的核心结构。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextSubmitPayload {
    /// UTF-8 文本，禁止为空，长度 ≤ [`MAX_TEXT_BYTES`]。
    pub text: String,
    /// 文本语言（BCP-47）。
    pub lang: String,
    /// 客户端发送时的本地时间戳（毫秒，Unix 纪元）。
    pub client_ts: i64,
}

/// `text.preview` 消息载荷。
///
/// 中间识别结果，仅用于 UI 同步，不会触发注入。`interim` 字段在
/// 构造时永远为 `true`，以便接收端可以快速识别这是中间态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextPreviewPayload {
    pub text: String,
    /// 永远为 `true`；保留字段以便协议演进。
    pub interim: bool,
}

/// `audio.chunk` 消息载荷（Server_ASR 模式）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioChunkPayload {
    /// 单调递增的分片序号；服务端校验 monotonic 在会话层完成。
    pub seq: u64,
    /// 编码格式。
    pub codec: AudioCodec,
    /// base64 编码后的音频载荷字符串。
    pub data: String,
    /// 可选的客户端时间戳（毫秒）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ts_ms: Option<u64>,
}

/// `audio.end` 消息载荷。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioEndPayload {
    pub seq: u64,
}

/// `ping` 消息载荷。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PingPayload {
    pub client_ts: i64,
}

// ---------- 服务端 → 客户端 payload ----------

/// `welcome` 消息中的 `feature` 字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WelcomeFeatures {
    /// 是否启用 Server_ASR。wire 字段名为 `serverASR`（与设计 §5.1.2 对齐）。
    #[serde(rename = "serverASR")]
    pub server_asr: bool,
}

/// `welcome` 消息载荷。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WelcomePayload {
    /// 服务端发送时的时间戳（毫秒，Unix 纪元）。
    pub server_ts: i64,
    /// 协议版本号（与 [`crate::PROTOCOL_VERSION`] 对应）。
    pub protocol: String,
    /// 服务端能力位图。
    pub feature: WelcomeFeatures,
}

/// `inject.ack` 消息载荷。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InjectAckPayload {
    /// 对应的 `text.submit.id`。
    pub id: String,
    /// 实际注入的字符数（按 Unicode scalar 计）。
    pub chars: u32,
}

/// `inject.error` 消息载荷。
///
/// `code` 字段在任务 2.3 中将切换为强类型枚举 `ErrorCode`；
/// 当前阶段保留 `String` 以解耦任务依赖。
// TODO(2.3): 切换为强类型 ErrorCode 枚举。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InjectErrorPayload {
    /// 对应的 `text.submit.id`。
    pub id: String,
    /// 错误码，例如 `INJECT_NO_FOCUS_TARGET`。
    pub code: String,
    /// 给用户阅读的错误描述。
    pub message: String,
}

/// `transcript.final` 消息载荷（Server_ASR 返回最终识别结果）。
///
/// 注意：`conf` 字段为 `Option<f32>`，因此本结构与下游
/// [`ServerMessageKind`] / [`ServerMessage`] 一概不派生 `Eq`
/// （`f32` 不是全序）；测试中需做相等比较时改用 `PartialEq`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptFinalPayload {
    pub text: String,
    pub lang: String,
    /// 可选置信度（0.0–1.0）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conf: Option<f32>,
}

/// `pong` 消息载荷。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PongPayload {
    pub server_ts: i64,
}

/// `error` 消息载荷（通用错误）。
// TODO(2.3): 切换为强类型 ErrorCode 枚举。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorPayload {
    pub code: String,
    pub message: String,
}

// ---------- 顶层消息：客户端 → 服务端 ----------

/// 客户端 → 服务端消息的 `type` + `payload` 部分。
///
/// 使用 [`serde`] adjacently tagged 表示：`type` 字段作为 tag，
/// `payload` 字段作为内容。`id` 字段在外层 [`ClientMessage`] 中
/// 单独承载（与设计 §5.1 的 `{ type, id?, payload }` 结构一致）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum ClientMessageKind {
    #[serde(rename = "hello")]
    Hello(HelloPayload),
    #[serde(rename = "text.submit")]
    TextSubmit(TextSubmitPayload),
    #[serde(rename = "text.preview")]
    TextPreview(TextPreviewPayload),
    #[serde(rename = "audio.chunk")]
    AudioChunk(AudioChunkPayload),
    #[serde(rename = "audio.end")]
    AudioEnd(AudioEndPayload),
    #[serde(rename = "ping")]
    Ping(PingPayload),
}

/// 客户端 → 服务端的顶层消息。
///
/// 序列化形态：
/// ```json
/// { "type": "text.submit", "id": "...", "payload": { ... } }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientMessage {
    /// 可选的消息 ID，用于断线补发去重（Requirement 9.4）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(flatten)]
    pub kind: ClientMessageKind,
}

// ---------- 顶层消息：服务端 → 客户端 ----------

/// 服务端 → 客户端消息的 `type` + `payload` 部分。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum ServerMessageKind {
    #[serde(rename = "welcome")]
    Welcome(WelcomePayload),
    #[serde(rename = "inject.ack")]
    InjectAck(InjectAckPayload),
    #[serde(rename = "inject.error")]
    InjectError(InjectErrorPayload),
    #[serde(rename = "transcript.final")]
    TranscriptFinal(TranscriptFinalPayload),
    #[serde(rename = "pong")]
    Pong(PongPayload),
    #[serde(rename = "error")]
    Error(ErrorPayload),
}

/// 服务端 → 客户端的顶层消息。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerMessage {
    /// 可选的消息 ID（与对应客户端请求的 `id` 关联，便于客户端追踪）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(flatten)]
    pub kind: ServerMessageKind,
}

// ---------- 字段校验 ----------

fn validate_text(field: &'static str, text: &str) -> Result<(), ProtocolError> {
    if text.is_empty() {
        return Err(ProtocolError::EmptyText);
    }
    if text.len() > MAX_TEXT_BYTES {
        return Err(ProtocolError::TextTooLong);
    }
    // 进一步禁止裸控制字符（除 \n / \r / \t）出现在 text.submit 中
    // 不在此强制：设计文档允许文本透传，控制字符将在桌面端注入器层
    // 自行处理。这里只保证 UTF-8（由 `String` 类型保证）。
    let _ = field;
    Ok(())
}

fn validate_lang(lang: &str) -> Result<(), ProtocolError> {
    if lang.is_empty() {
        return Err(ProtocolError::BadField("lang", "must not be empty"));
    }
    Ok(())
}

impl HelloPayload {
    /// 字段校验。
    ///
    /// # Errors
    ///
    /// - `device_label` 为空
    /// - `lang` 为空
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.device_label.is_empty() {
            return Err(ProtocolError::BadField("deviceLabel", "must not be empty"));
        }
        validate_lang(&self.lang)
    }
}

impl TextSubmitPayload {
    /// 字段校验。
    ///
    /// # Errors
    ///
    /// - `text` 为空或超过 [`MAX_TEXT_BYTES`]
    /// - `lang` 为空
    /// - `client_ts` 为负数
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_text("text", &self.text)?;
        validate_lang(&self.lang)?;
        if self.client_ts < 0 {
            return Err(ProtocolError::NegativeTimestamp("clientTs"));
        }
        Ok(())
    }
}

impl TextPreviewPayload {
    /// 字段校验。
    ///
    /// # Errors
    ///
    /// - `text` 为空或超长
    /// - `interim` 不为 `true`
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_text("text", &self.text)?;
        if !self.interim {
            return Err(ProtocolError::BadField("interim", "must be true"));
        }
        Ok(())
    }
}

impl AudioChunkPayload {
    /// 字段校验。
    ///
    /// # Errors
    ///
    /// - `data` 为空或超过 [`MAX_AUDIO_CHUNK_BASE64_BYTES`]
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.data.is_empty() {
            return Err(ProtocolError::EmptyAudio);
        }
        if self.data.len() > MAX_AUDIO_CHUNK_BASE64_BYTES {
            return Err(ProtocolError::AudioTooLong);
        }
        Ok(())
    }
}

impl PingPayload {
    /// # Errors
    ///
    /// - `client_ts` 为负数
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.client_ts < 0 {
            return Err(ProtocolError::NegativeTimestamp("clientTs"));
        }
        Ok(())
    }
}

impl WelcomePayload {
    /// # Errors
    ///
    /// - `protocol` 为空
    /// - `server_ts` 为负数
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.protocol.is_empty() {
            return Err(ProtocolError::BadField("protocol", "must not be empty"));
        }
        if self.server_ts < 0 {
            return Err(ProtocolError::NegativeTimestamp("serverTs"));
        }
        Ok(())
    }
}

impl InjectAckPayload {
    /// # Errors
    ///
    /// - `id` 为空
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.id.is_empty() {
            return Err(ProtocolError::BadField("id", "must not be empty"));
        }
        Ok(())
    }
}

impl InjectErrorPayload {
    /// # Errors
    ///
    /// - `id` 为空
    /// - `code` 为空
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.id.is_empty() {
            return Err(ProtocolError::BadField("id", "must not be empty"));
        }
        if self.code.is_empty() {
            return Err(ProtocolError::BadField("code", "must not be empty"));
        }
        Ok(())
    }
}

impl TranscriptFinalPayload {
    /// # Errors
    ///
    /// - `text` 为空或超长
    /// - `lang` 为空
    /// - `conf`（若提供）不在 [0.0, 1.0]
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_text("text", &self.text)?;
        validate_lang(&self.lang)?;
        if let Some(c) = self.conf {
            if !(0.0..=1.0).contains(&c) {
                return Err(ProtocolError::BadField("conf", "must be in [0.0, 1.0]"));
            }
        }
        Ok(())
    }
}

impl PongPayload {
    /// # Errors
    ///
    /// - `server_ts` 为负数
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.server_ts < 0 {
            return Err(ProtocolError::NegativeTimestamp("serverTs"));
        }
        Ok(())
    }
}

impl ErrorPayload {
    /// # Errors
    ///
    /// - `code` 为空
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.code.is_empty() {
            return Err(ProtocolError::BadField("code", "must not be empty"));
        }
        Ok(())
    }
}

impl ClientMessage {
    /// 转发到具体 payload 的校验逻辑。
    ///
    /// # Errors
    ///
    /// 任意一条 payload 校验失败时返回对应错误。
    pub fn validate(&self) -> Result<(), ProtocolError> {
        match &self.kind {
            ClientMessageKind::Hello(p) => p.validate(),
            ClientMessageKind::TextSubmit(p) => p.validate(),
            ClientMessageKind::TextPreview(p) => p.validate(),
            ClientMessageKind::AudioChunk(p) => p.validate(),
            ClientMessageKind::AudioEnd(_) => Ok(()),
            ClientMessageKind::Ping(p) => p.validate(),
        }
    }
}

impl ServerMessage {
    /// 转发到具体 payload 的校验逻辑。
    ///
    /// # Errors
    ///
    /// 任意一条 payload 校验失败时返回对应错误。
    pub fn validate(&self) -> Result<(), ProtocolError> {
        match &self.kind {
            ServerMessageKind::Welcome(p) => p.validate(),
            ServerMessageKind::InjectAck(p) => p.validate(),
            ServerMessageKind::InjectError(p) => p.validate(),
            ServerMessageKind::TranscriptFinal(p) => p.validate(),
            ServerMessageKind::Pong(p) => p.validate(),
            ServerMessageKind::Error(p) => p.validate(),
        }
    }
}

// ---------- 构造辅助函数 ----------

impl ClientMessage {
    /// 构造 `hello` 消息。`id` 字段对 `hello` 一般为空。
    #[must_use]
    pub fn hello(device_label: impl Into<String>, lang: impl Into<String>, use_server_asr: bool) -> Self {
        Self {
            id: None,
            kind: ClientMessageKind::Hello(HelloPayload {
                device_label: device_label.into(),
                lang: lang.into(),
                use_server_asr,
            }),
        }
    }

    /// 构造 `text.submit` 消息。
    #[must_use]
    pub fn text_submit(
        id: impl Into<String>,
        text: impl Into<String>,
        lang: impl Into<String>,
        client_ts: i64,
    ) -> Self {
        Self {
            id: Some(id.into()),
            kind: ClientMessageKind::TextSubmit(TextSubmitPayload {
                text: text.into(),
                lang: lang.into(),
                client_ts,
            }),
        }
    }

    /// 构造 `text.preview` 消息（`interim` 永远为 true）。
    #[must_use]
    pub fn text_preview(text: impl Into<String>) -> Self {
        Self {
            id: None,
            kind: ClientMessageKind::TextPreview(TextPreviewPayload {
                text: text.into(),
                interim: true,
            }),
        }
    }

    /// 构造 `audio.chunk` 消息。`data` 应为 base64 字符串。
    #[must_use]
    pub fn audio_chunk(seq: u64, codec: AudioCodec, data: impl Into<String>, ts_ms: Option<u64>) -> Self {
        Self {
            id: None,
            kind: ClientMessageKind::AudioChunk(AudioChunkPayload {
                seq,
                codec,
                data: data.into(),
                ts_ms,
            }),
        }
    }

    /// 构造 `audio.end` 消息。
    #[must_use]
    pub fn audio_end(seq: u64) -> Self {
        Self {
            id: None,
            kind: ClientMessageKind::AudioEnd(AudioEndPayload { seq }),
        }
    }

    /// 构造 `ping` 消息。
    #[must_use]
    pub fn ping(client_ts: i64) -> Self {
        Self {
            id: None,
            kind: ClientMessageKind::Ping(PingPayload { client_ts }),
        }
    }
}

impl ServerMessage {
    /// 构造 `welcome` 消息。`protocol` 字段会自动填充为 [`crate::PROTOCOL_VERSION`]。
    #[must_use]
    pub fn welcome(server_ts: i64, server_asr: bool) -> Self {
        Self {
            id: None,
            kind: ServerMessageKind::Welcome(WelcomePayload {
                server_ts,
                protocol: crate::PROTOCOL_VERSION.to_string(),
                feature: WelcomeFeatures { server_asr },
            }),
        }
    }

    /// 构造 `inject.ack` 消息。
    #[must_use]
    pub fn inject_ack(id: impl Into<String>, chars: u32) -> Self {
        let id = id.into();
        Self {
            id: Some(id.clone()),
            kind: ServerMessageKind::InjectAck(InjectAckPayload { id, chars }),
        }
    }

    /// 构造 `inject.error` 消息。
    #[must_use]
    pub fn inject_error(
        id: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        let id = id.into();
        Self {
            id: Some(id.clone()),
            kind: ServerMessageKind::InjectError(InjectErrorPayload {
                id,
                code: code.into(),
                message: message.into(),
            }),
        }
    }

    /// 构造 `transcript.final` 消息。
    #[must_use]
    pub fn transcript_final(
        text: impl Into<String>,
        lang: impl Into<String>,
        conf: Option<f32>,
    ) -> Self {
        Self {
            id: None,
            kind: ServerMessageKind::TranscriptFinal(TranscriptFinalPayload {
                text: text.into(),
                lang: lang.into(),
                conf,
            }),
        }
    }

    /// 构造 `pong` 消息。
    #[must_use]
    pub fn pong(server_ts: i64) -> Self {
        Self {
            id: None,
            kind: ServerMessageKind::Pong(PongPayload { server_ts }),
        }
    }

    /// 构造通用 `error` 消息。
    #[must_use]
    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            id: None,
            kind: ServerMessageKind::Error(ErrorPayload {
                code: code.into(),
                message: message.into(),
            }),
        }
    }
}

// ---------- 单元测试 ----------

#[cfg(test)]
mod tests {
    //! 这里的测试只覆盖：
    //! 1. 每种消息类型的 `type` tag 与设计文档 §5.1 完全一致；
    //! 2. JSON round-trip 不丢失字段；
    //! 3. 关键校验函数能识别出非法字段。
    //!
    //! 协议层属性测试（Property 11: Unicode round-trip）在任务 2.6
    //! 中单独完成。

    use super::*;
    use serde_json::{json, Value};

    /// 把消息转为 `serde_json::Value`，便于断言 `type` 字段。
    fn to_value<T: serde::Serialize>(v: &T) -> Value {
        serde_json::to_value(v).expect("serialize")
    }

    #[test]
    fn client_hello_tag_and_round_trip() {
        let msg = ClientMessage::hello("Pixel 8 (Chrome)", "zh-CN", false);
        let v = to_value(&msg);
        assert_eq!(v["type"], "hello");
        assert_eq!(v["payload"]["deviceLabel"], "Pixel 8 (Chrome)");
        assert_eq!(v["payload"]["lang"], "zh-CN");
        assert_eq!(v["payload"]["useServerASR"], false);

        let back: ClientMessage = serde_json::from_value(v).expect("deserialize");
        assert_eq!(back, msg);
        assert!(back.validate().is_ok());
    }

    #[test]
    fn client_text_submit_tag_and_round_trip() {
        let msg = ClientMessage::text_submit("m-1", "你好，世界！🌍", "zh-CN", 1_700_000_000_000);
        let v = to_value(&msg);
        assert_eq!(v["type"], "text.submit");
        assert_eq!(v["id"], "m-1");
        assert_eq!(v["payload"]["text"], "你好，世界！🌍");
        assert_eq!(v["payload"]["clientTs"], 1_700_000_000_000_i64);

        let back: ClientMessage = serde_json::from_value(v).expect("deserialize");
        assert_eq!(back, msg);
        assert!(back.validate().is_ok());
    }

    #[test]
    fn client_text_preview_always_interim() {
        let msg = ClientMessage::text_preview("中间结果…");
        let v = to_value(&msg);
        assert_eq!(v["type"], "text.preview");
        assert_eq!(v["payload"]["interim"], true);

        // 反序列化一条 interim=false 的消息时，校验应失败。
        let bad: ClientMessage = serde_json::from_value(json!({
            "type": "text.preview",
            "payload": { "text": "x", "interim": false }
        }))
        .unwrap();
        assert!(matches!(
            bad.validate().unwrap_err(),
            ProtocolError::BadField("interim", _)
        ));
    }

    #[test]
    fn client_audio_chunk_tag_and_codec() {
        let msg = ClientMessage::audio_chunk(7, AudioCodec::Pcm16k, "AAAA", Some(1_234));
        let v = to_value(&msg);
        assert_eq!(v["type"], "audio.chunk");
        assert_eq!(v["payload"]["seq"], 7);
        assert_eq!(v["payload"]["codec"], "pcm16k");
        assert_eq!(v["payload"]["data"], "AAAA");
        assert_eq!(v["payload"]["tsMs"], 1_234);

        // ts_ms 缺省时不应出现在序列化结果中。
        let msg2 = ClientMessage::audio_chunk(8, AudioCodec::Opus, "BBBB", None);
        let v2 = to_value(&msg2);
        assert_eq!(v2["payload"]["codec"], "opus");
        assert!(v2["payload"].get("tsMs").is_none());

        let back: ClientMessage = serde_json::from_value(v).expect("deserialize");
        assert_eq!(back, msg);
    }

    #[test]
    fn client_audio_end_and_ping_tags() {
        let end = ClientMessage::audio_end(42);
        assert_eq!(to_value(&end)["type"], "audio.end");
        assert_eq!(to_value(&end)["payload"]["seq"], 42);

        let ping = ClientMessage::ping(1_700_000_000_000);
        assert_eq!(to_value(&ping)["type"], "ping");
        assert_eq!(to_value(&ping)["payload"]["clientTs"], 1_700_000_000_000_i64);
    }

    #[test]
    fn server_welcome_tag_and_protocol_field() {
        let msg = ServerMessage::welcome(1_700_000_000_000, true);
        let v = to_value(&msg);
        assert_eq!(v["type"], "welcome");
        assert_eq!(v["payload"]["protocol"], crate::PROTOCOL_VERSION);
        assert_eq!(v["payload"]["feature"]["serverASR"], true);

        let back: ServerMessage = serde_json::from_value(v).expect("deserialize");
        assert_eq!(back, msg);
        assert!(back.validate().is_ok());
    }

    #[test]
    fn server_inject_ack_and_error_tags() {
        let ack = ServerMessage::inject_ack("m-1", 5);
        let v = to_value(&ack);
        assert_eq!(v["type"], "inject.ack");
        assert_eq!(v["id"], "m-1");
        assert_eq!(v["payload"]["id"], "m-1");
        assert_eq!(v["payload"]["chars"], 5);

        let err = ServerMessage::inject_error("m-1", "INJECT_NO_FOCUS_TARGET", "no focus");
        let ve = to_value(&err);
        assert_eq!(ve["type"], "inject.error");
        assert_eq!(ve["payload"]["code"], "INJECT_NO_FOCUS_TARGET");
        assert_eq!(ve["payload"]["message"], "no focus");
    }

    #[test]
    fn server_transcript_final_pong_error_tags() {
        let t = ServerMessage::transcript_final("hello", "en-US", Some(0.95));
        let vt = to_value(&t);
        assert_eq!(vt["type"], "transcript.final");
        assert_eq!(vt["payload"]["text"], "hello");
        assert_eq!(vt["payload"]["lang"], "en-US");
        // f32 序列化为浮点数，做近似比较
        assert!((vt["payload"]["conf"].as_f64().unwrap() - 0.95).abs() < 1e-6);

        // conf 缺省时不应出现在序列化结果中。
        let t2 = ServerMessage::transcript_final("hi", "en-US", None);
        assert!(to_value(&t2)["payload"].get("conf").is_none());

        let pong = ServerMessage::pong(1_700_000_000_000);
        assert_eq!(to_value(&pong)["type"], "pong");
        assert_eq!(to_value(&pong)["payload"]["serverTs"], 1_700_000_000_000_i64);

        let err = ServerMessage::error("MSG_BAD_FORMAT", "bad json");
        assert_eq!(to_value(&err)["type"], "error");
    }

    #[test]
    fn validate_rejects_empty_and_oversize_text() {
        let mut m = ClientMessage::text_submit("m-1", "ok", "zh-CN", 0);
        if let ClientMessageKind::TextSubmit(p) = &mut m.kind {
            p.text.clear();
        }
        assert_eq!(m.validate().unwrap_err(), ProtocolError::EmptyText);

        let big = "a".repeat(MAX_TEXT_BYTES + 1);
        let m2 = ClientMessage::text_submit("m-2", big, "zh-CN", 0);
        assert_eq!(m2.validate().unwrap_err(), ProtocolError::TextTooLong);
    }

    #[test]
    fn validate_rejects_negative_timestamp() {
        let m = ClientMessage::ping(-1);
        assert!(matches!(
            m.validate().unwrap_err(),
            ProtocolError::NegativeTimestamp("clientTs")
        ));

        let m2 = ClientMessage::text_submit("m-1", "x", "zh-CN", -100);
        assert!(matches!(
            m2.validate().unwrap_err(),
            ProtocolError::NegativeTimestamp("clientTs")
        ));

        let s = ServerMessage::pong(-1);
        assert!(matches!(
            s.validate().unwrap_err(),
            ProtocolError::NegativeTimestamp("serverTs")
        ));
    }

    #[test]
    fn validate_audio_chunk_bounds() {
        let mut m = ClientMessage::audio_chunk(0, AudioCodec::Pcm16k, "AAAA", None);
        if let ClientMessageKind::AudioChunk(p) = &mut m.kind {
            p.data.clear();
        }
        assert_eq!(m.validate().unwrap_err(), ProtocolError::EmptyAudio);

        let big = "A".repeat(MAX_AUDIO_CHUNK_BASE64_BYTES + 1);
        let m2 = ClientMessage::audio_chunk(0, AudioCodec::Pcm16k, big, None);
        assert_eq!(m2.validate().unwrap_err(), ProtocolError::AudioTooLong);
    }

    #[test]
    fn validate_transcript_conf_range() {
        let m = ServerMessage::transcript_final("x", "en-US", Some(1.5));
        assert!(matches!(
            m.validate().unwrap_err(),
            ProtocolError::BadField("conf", _)
        ));
    }

    #[test]
    fn full_envelope_shape_matches_design() {
        // 设计 §5 要求所有消息形如 { "type": ..., "id"?: ..., "payload": ... }
        let msg = ClientMessage::text_submit("abc", "你好", "zh-CN", 1);
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains(r#""type":"text.submit""#));
        assert!(s.contains(r#""id":"abc""#));
        assert!(s.contains(r#""payload":{"#));

        // 没有 id 字段时不应出现 "id" key（保证最小载荷）
        let msg2 = ClientMessage::ping(0);
        let s2 = serde_json::to_string(&msg2).unwrap();
        assert!(!s2.contains(r#""id""#));
    }

    /// 一次性枚举所有 12 种 `type` tag，确保设计文档与代码同步。
    #[test]
    fn type_tags_match_design_spec() {
        let cases: &[(&str, Value)] = &[
            ("hello", to_value(&ClientMessage::hello("d", "zh-CN", false))),
            (
                "text.submit",
                to_value(&ClientMessage::text_submit("m", "x", "zh-CN", 0)),
            ),
            ("text.preview", to_value(&ClientMessage::text_preview("x"))),
            (
                "audio.chunk",
                to_value(&ClientMessage::audio_chunk(0, AudioCodec::Pcm16k, "AAAA", None)),
            ),
            ("audio.end", to_value(&ClientMessage::audio_end(0))),
            ("ping", to_value(&ClientMessage::ping(0))),
            ("welcome", to_value(&ServerMessage::welcome(0, false))),
            (
                "inject.ack",
                to_value(&ServerMessage::inject_ack("m", 0)),
            ),
            (
                "inject.error",
                to_value(&ServerMessage::inject_error("m", "X", "y")),
            ),
            (
                "transcript.final",
                to_value(&ServerMessage::transcript_final("x", "en-US", None)),
            ),
            ("pong", to_value(&ServerMessage::pong(0))),
            ("error", to_value(&ServerMessage::error("X", "y"))),
        ];
        for (expected, v) in cases {
            assert_eq!(v["type"], *expected, "tag mismatch for {expected}");
        }
        assert_eq!(cases.len(), 12, "12 个消息类型必须全部覆盖");
    }
}
