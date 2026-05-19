//! 单段 ASR 看门狗（任务 8.5）。
//!
//! 当 `feed` 启动后，若在 `timeout` 内（默认 10 秒）没有 `transcript.final`
//! 信号，就通过 [`BridgeEventTx`] 投递 [`BridgeEvent::AsrTimeout`]。
//!
//! 设计来源：`.kiro/specs/phone-mic-voice-input/design.md` §4.6 / §8.1。

use std::time::Duration;

use phonemic_core::bridge_events::{AsrTimeoutEvent, BridgeEvent, BridgeEventTx};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

/// 默认看门狗超时：10 秒（任务 8.5）。
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// 一次 ASR 段的看门狗。
///
/// 通过 [`AsrWatchdog::start`] 创建后，调用 [`AsrWatchdog::cancel`] 表示段已
/// 在超时前自然结束（拿到 `transcript.final`）；否则定时器触发并向事件总线
/// 投递 [`BridgeEvent::AsrTimeout`]。
pub struct AsrWatchdog {
    cancel: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}

impl AsrWatchdog {
    /// 启动一个看门狗。
    ///
    /// `segment_id` 仅用于事件载荷；调用方应保证它在并发段间唯一。
    #[must_use]
    pub fn start(segment_id: String, timeout: Duration, events: BridgeEventTx) -> Self {
        let (tx, rx) = oneshot::channel();
        let task = tokio::spawn(async move {
            tokio::select! {
                _ = tokio::time::sleep(timeout) => {
                    let evt = BridgeEvent::AsrTimeout(AsrTimeoutEvent { segment_id });
                    if let Err(err) = events.send(evt).await {
                        tracing::warn!(error = %err, "AsrWatchdog failed to publish timeout event");
                    }
                }
                _ = rx => {
                    // 段在超时前结束 → 不投递事件。
                }
            }
        });
        Self {
            cancel: Some(tx),
            task: Some(task),
        }
    }

    /// 取消计时器。如果段已在超时前结束，调用此方法防止误报。
    pub fn cancel(mut self) {
        if let Some(tx) = self.cancel.take() {
            let _ = tx.send(());
        }
        // task 让其自然结束。
    }
}

impl Drop for AsrWatchdog {
    fn drop(&mut self) {
        // Drop 默认意味着段被 abandon：尝试取消，避免误报。
        if let Some(tx) = self.cancel.take() {
            let _ = tx.send(());
        }
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phonemic_core::bridge_events::channel;

    #[tokio::test]
    async fn watchdog_fires_timeout_event_on_inactivity() {
        let (tx, mut rx) = channel();
        let _wd = AsrWatchdog::start("seg-1".into(), Duration::from_millis(50), tx);

        // 等待真实超时（短间隔，避免拖慢 CI）。
        let evt = tokio::time::timeout(Duration::from_millis(500), rx.recv())
            .await
            .expect("event must arrive within 500ms")
            .expect("channel must not close before event");
        match evt {
            BridgeEvent::AsrTimeout(p) => assert_eq!(p.segment_id, "seg-1"),
            other => panic!("expected AsrTimeout, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn cancel_prevents_timeout_event() {
        let (tx, mut rx) = channel();
        let wd = AsrWatchdog::start("seg-2".into(), Duration::from_millis(50), tx);
        wd.cancel();

        // 给定 timeout 之后通道里仍不应有事件。
        match tokio::time::timeout(Duration::from_millis(200), rx.recv()).await {
            Err(_) => {} // expected：未在窗口内收到事件。
            Ok(Some(evt)) => panic!("watchdog should not have emitted: {evt:?}"),
            Ok(None) => {} // 通道关闭，亦视为未发事件。
        }
    }
}
