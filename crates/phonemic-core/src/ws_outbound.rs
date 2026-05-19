//! WebSocket 出站消息抽象（任务 5.x / 7.11 / 8.5）。
//!
//! - 设计来源：`.kiro/specs/phone-mic-voice-input/design.md` §4.2 / §6.3
//! - 任务来源：`tasks.md` 5.11、7.11、8.5
//!
//! Web Server（任务 5.9 / 5.11）持有具体的 `tokio_tungstenite` 连接表，
//! Input_Injector 与 ASR Bridge 不应当直接依赖它。本模块定义抽象 trait
//! [`WsOutbound`]，由 Web Server 的 `MessageDispatcher` 实现，使得：
//!
//! - Input_Injector 可以在注入失败时调用 `send_inject_error` 把消息回送
//!   到正确的 mobile 会话；
//! - ASR Bridge 在拿到最终识别结果时调用 `send_transcript_final` 把文本
//!   推回客户端；
//! - 测试与桌面 UI mock 都可以提供实现而不必启动真实 axum。
//!
//! ## 路由策略
//!
//! 每个发送方法都接受 [`SessionTarget`]：要么向某个具体 token 发送，要么
//! 广播到所有当前在线会话。具体路由在 Web Server 的 dispatcher 中维护，
//! 实现细节不暴露给本 trait 的消费者。

use async_trait::async_trait;
use phonemic_protocol::{
    ErrorCode, InjectAckPayload, InjectErrorPayload, ServerMessage, ServerMessageKind,
    TranscriptFinalPayload,
};

use crate::session::SessionToken;

/// 出站消息的目标会话集合。
#[derive(Debug, Clone)]
pub enum SessionTarget {
    /// 单一会话：只发送给该 token 对应的 WS 连接。
    One(SessionToken),
    /// 广播到所有在线会话。
    Broadcast,
}

/// `WsOutbound` 错误。Web Server 实现可在路由失败 / 通道关闭时返回。
#[derive(Debug, thiserror::Error)]
pub enum WsOutboundError {
    /// 目标 token 当前不在线（已断开 / 未连接）。
    #[error("session not connected")]
    NotConnected,
    /// WebSocket 写入失败（连接被对端关闭等）。
    #[error("websocket send failed: {0}")]
    SendFailed(String),
    /// 服务器已关停。
    #[error("server shutting down")]
    ServerStopped,
}

/// 抽象的"向 Mobile 推送消息"接口。
///
/// 所有方法都是 `async`：由 [`async_trait`] 转换为 trait object 友好的
/// `Pin<Box<dyn Future>>` 形式，供 Web Server 与子系统在运行时通过
/// `Arc<dyn WsOutbound>` 调用。
#[async_trait]
pub trait WsOutbound: Send + Sync + 'static {
    /// 向目标会话发送任意 [`ServerMessage`]。
    ///
    /// # Errors
    ///
    /// - [`WsOutboundError::NotConnected`]：目标 token 未连接；
    /// - [`WsOutboundError::SendFailed`]：底层 WS 写入失败；
    /// - [`WsOutboundError::ServerStopped`]：dispatcher 已 drop。
    async fn send(&self, target: SessionTarget, msg: ServerMessage) -> Result<(), WsOutboundError>;

    /// 便捷方法：发送 `inject.ack`。
    async fn send_inject_ack(
        &self,
        target: SessionTarget,
        submit_id: String,
        chars: u32,
    ) -> Result<(), WsOutboundError> {
        let id_for_payload = submit_id.clone();
        let msg = ServerMessage {
            id: Some(submit_id),
            kind: ServerMessageKind::InjectAck(InjectAckPayload {
                id: id_for_payload,
                chars,
            }),
        };
        self.send(target, msg).await
    }

    /// 便捷方法：发送 `inject.error`。
    async fn send_inject_error(
        &self,
        target: SessionTarget,
        submit_id: String,
        code: ErrorCode,
        message: String,
    ) -> Result<(), WsOutboundError> {
        let id_for_payload = submit_id.clone();
        let msg = ServerMessage {
            id: Some(submit_id),
            kind: ServerMessageKind::InjectError(InjectErrorPayload {
                id: id_for_payload,
                code: code.as_str().to_owned(),
                message,
            }),
        };
        self.send(target, msg).await
    }

    /// 便捷方法：发送 `transcript.final`。
    async fn send_transcript_final(
        &self,
        target: SessionTarget,
        text: String,
        lang: String,
        conf: Option<f32>,
    ) -> Result<(), WsOutboundError> {
        let msg = ServerMessage {
            id: None,
            kind: ServerMessageKind::TranscriptFinal(TranscriptFinalPayload {
                text,
                lang,
                conf,
            }),
        };
        self.send(target, msg).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// 简单 mock：把所有发送过的消息记录在 `Vec` 中，便于测试断言。
    #[derive(Default)]
    struct MockOutbound {
        sent: Mutex<Vec<(String, ServerMessage)>>,
    }

    #[async_trait]
    impl WsOutbound for MockOutbound {
        async fn send(
            &self,
            target: SessionTarget,
            msg: ServerMessage,
        ) -> Result<(), WsOutboundError> {
            let key = match target {
                SessionTarget::Broadcast => "*broadcast*".to_owned(),
                SessionTarget::One(t) => t.as_str().to_owned(),
            };
            self.sent.lock().unwrap().push((key, msg));
            Ok(())
        }
    }

    #[tokio::test]
    async fn send_inject_error_uses_error_code_string() {
        let mock = MockOutbound::default();
        mock.send_inject_error(
            SessionTarget::Broadcast,
            "m-1".into(),
            ErrorCode::InjectPaused,
            "paused".into(),
        )
        .await
        .unwrap();

        let log = mock.sent.lock().unwrap();
        assert_eq!(log.len(), 1);
        let (target_key, msg) = &log[0];
        assert_eq!(target_key, "*broadcast*");
        match &msg.kind {
            ServerMessageKind::InjectError(p) => {
                assert_eq!(p.id, "m-1");
                assert_eq!(p.code, "INJECT_PAUSED");
                assert_eq!(p.message, "paused");
            }
            other => panic!("expected InjectError, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn send_transcript_final_round_trips_fields() {
        let mock = MockOutbound::default();
        mock.send_transcript_final(
            SessionTarget::Broadcast,
            "你好".into(),
            "zh-CN".into(),
            Some(0.9),
        )
        .await
        .unwrap();

        let log = mock.sent.lock().unwrap();
        match &log[0].1.kind {
            ServerMessageKind::TranscriptFinal(p) => {
                assert_eq!(p.text, "你好");
                assert_eq!(p.lang, "zh-CN");
                assert_eq!(p.conf, Some(0.9));
            }
            other => panic!("expected TranscriptFinal, got {other:?}"),
        }
    }
}
