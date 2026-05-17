//! `phonemic-protocol` —— 桌面端与移动端共享的协议与类型。
//!
//! 本 crate 是 Rust 端与 TypeScript 端之间的"单一来源"：
//! 所有跨进程 / 跨设备传输的 WebSocket 消息、HTTP API 请求与
//! 响应、错误码、配置 schema 均在此定义。任务 2.x 逐步充实
//! 各子模块：
//! - 任务 2.1：WebSocket 消息层（[`ws`]）
//! - 任务 2.2：HTTP API 类型（[`http`]）与统一错误对象（[`error_obj`]）
//! - 任务 2.3：错误码枚举（[`error`]）
//! - 任务 2.4：配置 schema（[`config`]）
//!
//! 设计来源：`.kiro/specs/phone-mic-voice-input/design.md` §5、§8.1

#![forbid(unsafe_code)]

pub mod error;
pub mod error_obj;
pub mod http;

pub use error::{ErrorCode, ProtocolErrorCodeParseError};
pub use error_obj::AppError;

/// 协议版本号，写入 `welcome` 消息的 `protocol` 字段。
pub const PROTOCOL_VERSION: &str = "1";

pub mod config;
pub mod ws;

pub use config::{AppConfig, AsrCfg, AsrLang, ConfigError, InputCfg, SecurityCfg, ServerCfg, UiCfg, UiLanguage};

pub use ws::{
    AudioChunkPayload, AudioCodec, AudioEndPayload, ClientMessage, ClientMessageKind, ErrorPayload,
    HelloPayload, InjectAckPayload, InjectErrorPayload, PingPayload, PongPayload, ProtocolError,
    ServerMessage, ServerMessageKind, TextPreviewPayload, TextSubmitPayload, TranscriptFinalPayload,
    WelcomeFeatures, WelcomePayload, MAX_AUDIO_CHUNK_BASE64_BYTES, MAX_TEXT_BYTES,
};
