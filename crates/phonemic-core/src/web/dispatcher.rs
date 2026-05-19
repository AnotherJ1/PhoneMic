//! WebSocket 消息分发器（任务 5.11 / 5.12）。
//!
//! - 设计来源：design.md §4.2 / §5.1
//! - 任务来源：tasks.md 5.11
//!
//! 接收原始 WebSocket 文本帧（来自任务 5.9），尝试解析为
//! [`ClientMessage`]；解析成功后做权限校验并转发给具体子系统接口。
//! 解析失败 / 缺 token 必须返回结构化 [`ServerMessage::error`] 而非 panic
//! （Property 31）。

use phonemic_protocol::{
    ClientMessage, ClientMessageKind, ErrorCode, HelloPayload, ServerMessage,
};

/// 处理一帧文本消息后产出的"出站动作"集合。
///
/// 调用方（WS handler）拿到结果后再决定具体如何把消息回送给客户端。
/// 这种"决策与 IO 分离"的设计便于在 Property 31 的属性测试中无副作用驱动。
#[derive(Debug, Clone, PartialEq)]
pub enum DispatcherOutcome {
    /// 一切正常：客户端消息被分发到了具体子系统；可能还有需要立刻回送的
    /// 服务端消息（如 `welcome` / `pong`）。
    Routed {
        /// 路由后的消息分类，仅供观测 / 日志使用。
        kind: RoutedKind,
        /// 需要立刻回送给该会话的服务端消息（可空）。
        reply: Option<ServerMessage>,
    },
    /// 协议层错误：消息被丢弃，调用方应回送给客户端。
    Reject(ServerMessage),
}

/// 路由后消息的分类，用于日志 / 测试断言。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutedKind {
    Hello,
    TextSubmit,
    TextPreview,
    AudioChunk,
    AudioEnd,
    Ping,
}

/// 抽象的消息分发器：纯函数 / 状态由实现自行持有。
///
/// MVP 阶段仅提供基本路由 + 解析鲁棒性；具体投递到 Input_Injector / ASR
/// Bridge 的桥接（5.11 全功能）在后续任务通过 [`MessageDispatcher::handle_with_sinks`]
/// 扩展。本 trait 已先行落地以便单测与属性测试。
pub trait MessageDispatcher {
    /// 解析并分发一帧文本，无副作用。
    fn handle_text(&self, raw_text: &str, has_authenticated_session: bool) -> DispatcherOutcome;
}

/// 默认实现：纯文本协议解析 + 鉴权检查；不持有具体 sink。
#[derive(Debug, Default, Clone)]
pub struct DefaultDispatcher;

impl MessageDispatcher for DefaultDispatcher {
    fn handle_text(&self, raw_text: &str, has_authenticated_session: bool) -> DispatcherOutcome {
        // 1. JSON 解析。
        let msg: ClientMessage = match serde_json::from_str(raw_text) {
            Ok(m) => m,
            Err(e) => {
                return DispatcherOutcome::Reject(error_message(
                    ErrorCode::MsgBadFormat,
                    format!("invalid JSON: {e}"),
                ));
            }
        };

        // 2. payload 字段校验（如 text 非空、ts 非负等）。
        if let Err(e) = msg.validate() {
            return DispatcherOutcome::Reject(error_message(
                ErrorCode::MsgBadFormat,
                format!("invalid payload: {e}"),
            ));
        }

        // 3. Hello 不要求已认证（它本身只是握手补充信息）；其它需要 token。
        let needs_session = !matches!(&msg.kind, ClientMessageKind::Hello(_));
        if needs_session && !has_authenticated_session {
            return DispatcherOutcome::Reject(error_message(
                ErrorCode::AuthRequired,
                "session token required",
            ));
        }

        // 4. 路由分类，并对部分消息直接生成回复。
        match msg.kind {
            ClientMessageKind::Hello(p) => {
                let reply = build_welcome_reply(&p);
                DispatcherOutcome::Routed {
                    kind: RoutedKind::Hello,
                    reply: Some(reply),
                }
            }
            ClientMessageKind::Ping(p) => {
                let reply = ServerMessage::pong(p.client_ts);
                DispatcherOutcome::Routed {
                    kind: RoutedKind::Ping,
                    reply: Some(reply),
                }
            }
            ClientMessageKind::TextSubmit(_) => DispatcherOutcome::Routed {
                kind: RoutedKind::TextSubmit,
                reply: None,
            },
            ClientMessageKind::TextPreview(_) => DispatcherOutcome::Routed {
                kind: RoutedKind::TextPreview,
                reply: None,
            },
            ClientMessageKind::AudioChunk(_) => DispatcherOutcome::Routed {
                kind: RoutedKind::AudioChunk,
                reply: None,
            },
            ClientMessageKind::AudioEnd(_) => DispatcherOutcome::Routed {
                kind: RoutedKind::AudioEnd,
                reply: None,
            },
        }
    }
}

fn build_welcome_reply(_hello: &HelloPayload) -> ServerMessage {
    // server_asr 暂时禁用（whisper feature off-by-default，任务 8.4），
    // 后续会按 AppConfig 决定。
    let server_asr = false;
    let now_ms = current_unix_ms();
    ServerMessage::welcome(now_ms, server_asr)
}

fn error_message(code: ErrorCode, message: impl Into<String>) -> ServerMessage {
    ServerMessage::error(code.as_str(), message)
}

fn current_unix_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let dur = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    i64::try_from(dur.as_millis()).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use phonemic_protocol::ServerMessageKind;

    #[test]
    fn invalid_json_yields_msg_bad_format() {
        let d = DefaultDispatcher;
        let outcome = d.handle_text("not json", true);
        match outcome {
            DispatcherOutcome::Reject(msg) => match msg.kind {
                ServerMessageKind::Error(e) => {
                    assert_eq!(e.code, "MSG_BAD_FORMAT");
                }
                other => panic!("expected error, got {other:?}"),
            },
            other => panic!("expected Reject, got {other:?}"),
        }
    }

    #[test]
    fn empty_text_submit_yields_msg_bad_format() {
        let d = DefaultDispatcher;
        let raw = r#"{"type":"text.submit","id":"m-1","payload":{"text":"","lang":"zh-CN","clientTs":0}}"#;
        let outcome = d.handle_text(raw, true);
        assert!(matches!(outcome, DispatcherOutcome::Reject(_)));
    }

    #[test]
    fn unauthenticated_text_submit_yields_auth_required() {
        let d = DefaultDispatcher;
        let raw = r#"{"type":"text.submit","id":"m-1","payload":{"text":"x","lang":"zh-CN","clientTs":0}}"#;
        let outcome = d.handle_text(raw, false);
        match outcome {
            DispatcherOutcome::Reject(msg) => match msg.kind {
                ServerMessageKind::Error(e) => assert_eq!(e.code, "AUTH_REQUIRED"),
                other => panic!("expected Error, got {other:?}"),
            },
            other => panic!("expected Reject, got {other:?}"),
        }
    }

    #[test]
    fn hello_does_not_require_authenticated_session() {
        let d = DefaultDispatcher;
        let raw = r#"{"type":"hello","payload":{"deviceLabel":"d","lang":"zh-CN","useServerASR":false}}"#;
        let outcome = d.handle_text(raw, false);
        match outcome {
            DispatcherOutcome::Routed { kind, reply } => {
                assert_eq!(kind, RoutedKind::Hello);
                let reply = reply.expect("hello must produce welcome reply");
                assert!(matches!(reply.kind, ServerMessageKind::Welcome(_)));
            }
            other => panic!("expected Routed, got {other:?}"),
        }
    }

    #[test]
    fn ping_replies_with_pong() {
        let d = DefaultDispatcher;
        let raw = r#"{"type":"ping","payload":{"clientTs":42}}"#;
        let outcome = d.handle_text(raw, true);
        match outcome {
            DispatcherOutcome::Routed { kind, reply } => {
                assert_eq!(kind, RoutedKind::Ping);
                match reply.unwrap().kind {
                    ServerMessageKind::Pong(p) => assert!(p.server_ts >= 0),
                    other => panic!("expected Pong, got {other:?}"),
                }
            }
            other => panic!("expected Routed, got {other:?}"),
        }
    }

    #[test]
    fn audio_chunk_routes_without_reply() {
        let d = DefaultDispatcher;
        let raw = r#"{"type":"audio.chunk","payload":{"seq":0,"codec":"pcm16k","data":"AAAA"}}"#;
        let outcome = d.handle_text(raw, true);
        match outcome {
            DispatcherOutcome::Routed { kind, reply } => {
                assert_eq!(kind, RoutedKind::AudioChunk);
                assert!(reply.is_none());
            }
            other => panic!("expected Routed, got {other:?}"),
        }
    }
}

// ----------------------------------------------------------------------------
// Property 31: 错误协议鲁棒性（任务 5.12）
// ----------------------------------------------------------------------------
// 任意字节序列（合法 / 非法 / 缺字段 / 缺 token）都不应让 dispatcher panic，
// 也必须返回结构化错误而非空响应。
#[cfg(test)]
mod proptests {
    use super::*;
    use phonemic_protocol::ServerMessageKind;
    use proptest::prelude::*;

    proptest! {
        // Feature: phone-mic-voice-input, Property 31: 错误协议鲁棒性
        #[test]
        fn property_31_dispatcher_never_panics_on_arbitrary_input(
            raw in prop::string::string_regex("[\\x00-\\xff]{0,256}").unwrap(),
            authed in any::<bool>(),
        ) {
            let d = DefaultDispatcher;
            // 必须返回 Routed / Reject 之一；不会 panic。
            let _ = d.handle_text(&raw, authed);
        }

        // 几乎所有非空非法输入都应得到 Reject + Error。
        #[test]
        fn property_31_invalid_input_is_rejected_with_error(
            raw in prop::string::string_regex("[a-z0-9]{1,16}").unwrap(),
        ) {
            let d = DefaultDispatcher;
            let outcome = d.handle_text(&raw, true);
            match outcome {
                DispatcherOutcome::Reject(msg) => match msg.kind {
                    ServerMessageKind::Error(_) => {}
                    other => prop_assert!(false, "expected Error reject, got {:?}", other),
                },
                other => prop_assert!(false, "expected Reject, got {:?}", other),
            }
        }
    }
}
