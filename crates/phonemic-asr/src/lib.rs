//! `phonemic-asr` —— Server_ASR 引擎与桥接（任务 8.x）。
//!
//! 设计来源：`.kiro/specs/phone-mic-voice-input/design.md` §3.8 / §4.6
//!
//! 任务覆盖：
//! - 8.1 [`AsrEngine`] trait + [`AudioFrame`] / [`TranscriptFinal`] 类型
//! - 8.2 [`pick_asr_engine`] 引擎决策纯函数
//! - 8.3 Property 10：决策表 4 种布尔组合的属性化覆盖
//! - 8.4 默认实现 [`NoopAsr`]（compile-time 开关，feature `whisper` 关闭时
//!   一律返回 `ASR_ENGINE_DISABLED`）；whisper-rs 适配位于 `whisper`
//!   feature 之后。
//! - 8.5 [`watchdog::AsrWatchdog`] 单段 10 秒超时，触发
//!   [`phonemic_core::bridge_events::BridgeEvent::AsrTimeout`]。
//! - 8.6 集成测试见 `tests/asr_watchdog.rs`（默认 features 触发超时路径）。

#![forbid(unsafe_code)]

pub mod engine;
pub mod watchdog;
pub mod whisper;

pub use engine::{
    pick_asr_engine, AsrEngine, AsrError, AsrTarget, AudioCodec, AudioFrame, NoopAsr,
    TranscriptFinal, ASR_ENGINE_DISABLED,
};
pub use watchdog::AsrWatchdog;

#[cfg(feature = "whisper")]
pub use whisper::{WhisperAsr, WhisperConfig};
