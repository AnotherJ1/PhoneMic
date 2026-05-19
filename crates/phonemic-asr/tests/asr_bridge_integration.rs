// ASR Bridge integration tests（任务 8.6）。
//
// - 默认 features：仅触发 watchdog 超时路径（NoopAsr 永不产出最终识别）；
// - feature = "whisper"：进入真实模型识别 path（需要模型文件）。
//
// 注：whisper 路径在本仓库 MVP 阶段尚未集成模型文件，因此该测试只验证
//   "feed → end → 一定时间内必有 transcript.final" 的接口契约（占位）。
//   后续接入 whisper-rs 后改为读取真实样本。

use std::time::Duration;

use phonemic_asr::{AsrEngine, AsrError, AudioCodec, AudioFrame, AsrWatchdog, NoopAsr};
use phonemic_core::bridge_events::{channel as events_channel, BridgeEvent};

#[tokio::test]
async fn watchdog_emits_timeout_when_noop_engine_never_produces_final() {
    let (tx, mut rx) = events_channel();
    // 启动一个段：超时设短一点（150ms）以加快 CI。
    let _wd = AsrWatchdog::start("seg-it-1".into(), Duration::from_millis(150), tx);

    // NoopAsr 永远返回 EngineDisabled，且永不会触发 final。
    let asr = NoopAsr;
    let frame = AudioFrame {
        codec: AudioCodec::Pcm16k,
        payload: bytes::Bytes::new(),
        ts_ms: 0,
    };
    assert_eq!(asr.feed(frame).await.unwrap_err(), AsrError::EngineDisabled);
    assert_eq!(asr.end().await.unwrap_err(), AsrError::EngineDisabled);

    // 等到 watchdog 超时事件抵达。
    let evt = tokio::time::timeout(Duration::from_millis(500), rx.recv())
        .await
        .expect("event must arrive within 500ms")
        .expect("channel must not be closed");
    match evt {
        BridgeEvent::AsrTimeout(p) => assert_eq!(p.segment_id, "seg-it-1"),
        other => panic!("expected AsrTimeout, got {other:?}"),
    }
}

#[cfg(feature = "whisper")]
mod live {
    //! 真实 whisper 集成测试占位。当 `whisper-rs` 适配器接入并提供模型
    //! 文件后，这里改为 "feed PCM → end → assert transcript.final 文本不空"。

    #[tokio::test]
    async fn whisper_returns_non_empty_transcript_within_3s() {
        // TODO: 接入 whisper-rs；MVP 阶段保持占位以避免 CI 失败。
        eprintln!("whisper feature enabled but adapter not yet implemented; placeholder");
    }
}
