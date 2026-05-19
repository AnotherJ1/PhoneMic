//! 把 phonemic 的错误码映射为 axum 响应（任务 5.4 / 5.5 / 5.6 / 5.11）。
//!
//! 设计来源：design.md §8.1 "结构化错误对象"。

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use phonemic_protocol::{AppError, ErrorCode};
use serde_json::json;

/// 通用错误响应：HTTP 状态码 + 结构化 [`AppError`] 体。
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: ErrorCode,
    pub message: String,
    pub detail: Option<serde_json::Value>,
}

impl ApiError {
    #[must_use]
    pub fn new(status: StatusCode, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            detail: None,
        }
    }

    #[must_use]
    pub fn with_detail(mut self, detail: serde_json::Value) -> Self {
        self.detail = Some(detail);
        self
    }

    /// 401 AUTH_REQUIRED：缺少 / 非法的 Session_Token。
    #[must_use]
    pub fn auth_required() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            ErrorCode::AuthRequired,
            "authentication required",
        )
    }

    /// 403 FORBIDDEN_SUBNET：来源 IP 不在 RFC1918 / loopback 子网内。
    #[must_use]
    pub fn forbidden_subnet() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            ErrorCode::ForbiddenSubnet,
            "client IP not in LAN subnet",
        )
    }

    /// 401 PAIR_INVALID：配对码错误或已失效。
    #[must_use]
    pub fn pair_invalid() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            ErrorCode::PairInvalid,
            "invalid pairing code",
        )
    }

    /// 429 PAIR_RATELIMIT：配对失败次数过多。
    #[must_use]
    pub fn pair_rate_limit(retry_after_secs: u64) -> Self {
        Self::new(
            StatusCode::TOO_MANY_REQUESTS,
            ErrorCode::PairRatelimit,
            "too many failed pairing attempts",
        )
        .with_detail(json!({ "retryAfter": retry_after_secs }))
    }

    /// 400 MSG_BAD_FORMAT：协议解析失败。
    #[must_use]
    pub fn bad_format(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, ErrorCode::MsgBadFormat, msg)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut err = AppError::now(self.code.as_str(), self.message);
        if let Some(detail) = self.detail {
            err = err.with_detail(detail);
        }
        let mut resp = (self.status, Json(err)).into_response();
        // PAIR_RATELIMIT 时给出 Retry-After（秒）头部，方便 Mobile 直接读取。
        if matches!(self.code, ErrorCode::PairRatelimit) {
            if let Ok(value) = "300".parse() {
                resp.headers_mut().insert(axum::http::header::RETRY_AFTER, value);
            }
        }
        resp
    }
}
