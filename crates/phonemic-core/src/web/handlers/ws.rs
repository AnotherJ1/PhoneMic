//! WebSocket `/ws` upgrade + 消息循环（任务 5.9 / 5.11）。
//!
//! 鉴权规则：握手时必须携带 `Sec-WebSocket-Protocol: phonemic.<token>`；
//! token 由 [`super::super::middleware::extract_ws_protocol_token`] 提取，
//! 并由 [`crate::session::SessionRegistry`] 校验。
//!
//! 消息层：把每个文本帧投递给 [`MessageDispatcher`]（默认 [`DefaultDispatcher`]）；
//! 解析失败的帧通过 WS 把结构化 `error` 消息回送给客户端（Property 31）。

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
#[allow(unused_imports)]
use futures_util::{SinkExt, StreamExt};

use crate::session::SessionToken;
use crate::web::dispatcher::{DefaultDispatcher, DispatcherOutcome, MessageDispatcher};
use crate::web::middleware::extract_ws_protocol_token;
use crate::web::state::AppState;

pub async fn upgrade(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
    headers: HeaderMap,
) -> Response {
    let Some(token_str) = extract_ws_protocol_token(&headers) else {
        return (StatusCode::UNAUTHORIZED, "missing phonemic.<token> protocol").into_response();
    };
    if token_str.len() != 43 {
        return (StatusCode::UNAUTHORIZED, "invalid token length").into_response();
    }
    let token = SessionToken::from_validated(token_str.clone());
    {
        let svc = state.pairing().await;
        if svc.sessions().validate(&token).is_err() {
            return (StatusCode::UNAUTHORIZED, "invalid session token").into_response();
        }
    }

    // 协议确认：浏览器要求服务器在 101 响应里 echo 一个 client-offered protocol。
    // axum 0.7 通过 `protocols` API 选择；这里手动设置响应头。
    let mut response = ws.on_upgrade(move |socket| handle_socket(socket, token));
    let echoed = format!("phonemic.{token_str}");
    if let Ok(value) = HeaderValue::from_str(&echoed) {
        response
            .headers_mut()
            .insert(axum::http::header::SEC_WEBSOCKET_PROTOCOL, value);
    }
    response
}

async fn handle_socket(mut socket: WebSocket, _token: SessionToken) {
    let dispatcher = DefaultDispatcher;
    while let Some(frame) = socket.next().await {
        let msg = match frame {
            Ok(m) => m,
            Err(err) => {
                tracing::warn!(error = %err, "ws read error");
                break;
            }
        };
        match msg {
            Message::Text(text) => match dispatcher.handle_text(&text, true) {
                DispatcherOutcome::Routed { reply, .. } => {
                    if let Some(reply) = reply {
                        if let Ok(json) = serde_json::to_string(&reply) {
                            let _ = socket.send(Message::Text(json)).await;
                        }
                    }
                }
                DispatcherOutcome::Reject(server_msg) => {
                    if let Ok(json) = serde_json::to_string(&server_msg) {
                        let _ = socket.send(Message::Text(json)).await;
                    }
                }
            },
            Message::Binary(_) => {
                // 当前协议未定义二进制帧；按 MSG_BAD_FORMAT 丢弃。
                let err = phonemic_protocol::ServerMessage::error(
                    phonemic_protocol::ErrorCode::MsgBadFormat.as_str(),
                    "binary frames not supported",
                );
                if let Ok(json) = serde_json::to_string(&err) {
                    let _ = socket.send(Message::Text(json)).await;
                }
            }
            Message::Ping(payload) => {
                let _ = socket.send(Message::Pong(payload)).await;
            }
            Message::Pong(_) | Message::Close(_) => {}
        }
    }
}
