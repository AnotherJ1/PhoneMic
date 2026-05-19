//! ASR 引擎抽象与决策（任务 8.1 / 8.2 / 8.4）。

use async_trait::async_trait;
use bytes::Bytes;
use thiserror::Error;

/// 错误码字面量：whisper feature 未启用时返回。
pub const ASR_ENGINE_DISABLED: &str = "ASR_ENGINE_DISABLED";

/// 音频编码（与 `phonemic_protocol::AudioCodec` 同步）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioCodec {
    Pcm16k,
    Opus,
}

/// 单帧音频。
#[derive(Debug, Clone, PartialEq)]
pub struct AudioFrame {
    pub codec: AudioCodec,
    pub payload: Bytes,
    pub ts_ms: u64,
}

/// 最终识别结果。
#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptFinal {
    pub text: String,
    pub lang: String,
    pub conf: Option<f32>,
}

/// ASR 引擎错误。
#[derive(Debug, Clone, PartialEq, Error)]
pub enum AsrError {
    /// `whisper` feature 关闭：编译期就被禁用。
    #[error("ASR_ENGINE_DISABLED: whisper feature not compiled in")]
    EngineDisabled,
    /// 模型初始化失败（路径错误 / 不可读）。
    #[error("ASR_INIT_ERROR: {0}")]
    InitError(String),
    /// 喂入帧时编码不匹配 / payload 异常。
    #[error("ASR_FEED_ERROR: {0}")]
    FeedError(String),
    /// 识别超时（与 [`crate::watchdog`] 协作）。
    #[error("ASR_TIMEOUT")]
    Timeout,
    /// 其它后端错误（whisper-rs 内部错误等）。
    #[error("ASR_BACKEND_ERROR: {0}")]
    Backend(String),
}

/// `AsrEngine` —— 增量喂入 + 终结产出最终识别。
#[async_trait]
pub trait AsrEngine: Send + Sync + 'static {
    /// 喂入一帧音频。
    async fn feed(&self, frame: AudioFrame) -> Result<(), AsrError>;
    /// 结束当前段：触发最终识别。
    async fn end(&self) -> Result<TranscriptFinal, AsrError>;
}

/// 引擎选择结果（任务 8.2 / Property 10）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsrTarget {
    /// 浏览器原生 ASR（Web Speech API）。
    Browser,
    /// 本地服务端 ASR（whisper.cpp）。
    Server,
}

/// 引擎决策纯函数（任务 8.2）。
///
/// 仅当 `supports_browser_asr ∧ ¬prefer_server_asr` 时返回 `Browser`，
/// 其它一切情况返回 `Server`。
#[must_use]
pub fn pick_asr_engine(supports_browser_asr: bool, prefer_server_asr: bool) -> AsrTarget {
    if supports_browser_asr && !prefer_server_asr {
        AsrTarget::Browser
    } else {
        AsrTarget::Server
    }
}

/// 默认 ASR 引擎：feature `whisper` 关闭时一律返回 `ASR_ENGINE_DISABLED`。
///
/// 这是 design §4.6 关于"compile-time 开关，非运行时回退"的体现：
/// CI / 默认 build 不携带 whisper 后端，但代码 path 始终可用。
#[derive(Debug, Default, Clone)]
pub struct NoopAsr;

#[async_trait]
impl AsrEngine for NoopAsr {
    async fn feed(&self, _frame: AudioFrame) -> Result<(), AsrError> {
        Err(AsrError::EngineDisabled)
    }
    async fn end(&self) -> Result<TranscriptFinal, AsrError> {
        Err(AsrError::EngineDisabled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_asr_engine_returns_browser_only_when_supported_and_not_preferring_server() {
        assert_eq!(pick_asr_engine(true, false), AsrTarget::Browser);
        assert_eq!(pick_asr_engine(true, true), AsrTarget::Server);
        assert_eq!(pick_asr_engine(false, false), AsrTarget::Server);
        assert_eq!(pick_asr_engine(false, true), AsrTarget::Server);
    }

    #[tokio::test]
    async fn noop_asr_returns_engine_disabled_in_default_features() {
        let asr = NoopAsr;
        let frame = AudioFrame {
            codec: AudioCodec::Pcm16k,
            payload: Bytes::new(),
            ts_ms: 0,
        };
        assert_eq!(asr.feed(frame).await.unwrap_err(), AsrError::EngineDisabled);
        assert_eq!(asr.end().await.unwrap_err(), AsrError::EngineDisabled);
    }
}

// ----------------------------------------------------------------------------
// Property tests
// ----------------------------------------------------------------------------
// Feature: phone-mic-voice-input, Property 10: ASR 引擎决策
#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        // Feature: phone-mic-voice-input, Property 10: ASR 引擎决策
        #[test]
        fn property_10_decision_matches_spec(
            supports_browser in any::<bool>(),
            prefer_server in any::<bool>(),
        ) {
            let actual = pick_asr_engine(supports_browser, prefer_server);
            let expected = if supports_browser && !prefer_server {
                AsrTarget::Browser
            } else {
                AsrTarget::Server
            };
            prop_assert_eq!(actual, expected);
        }
    }
}
