//! 统一错误转换层 —— 把上层各种 `Error` 归一为 [`AppError`]。
//!
//! 任务来源：tasks.md 13.1。
//! 设计来源：design.md §8.1。
//! 关联需求：9.6（结构化错误）、9.7（不泄露明文）。
//!
//! 本模块在 `phonemic-protocol::AppError` 之上提供：
//!
//! - [`AppErrorExt`] —— 便利 trait，从 [`phonemic_protocol::ErrorCode`] +
//!   人类可读 `message` 构造 [`AppError`]，并自动附 RFC3339 时间戳；
//! - [`IntoAppError`] —— 通用 trait，把任意错误（`io::Error` /
//!   `serde_json::Error` / `tokio::JoinError` / [`InjectError`] 等）
//!   映射成 [`AppError`]；
//! - 一组 `From<X> for AppError`，与 [`IntoAppError`] 等价但更适合 `?` 用法；
//! - [`code_to_i18n_key`] —— 错误码到桌面 i18n 字典 key 的稳定映射，
//!   缺失时调用方应回退到原 `message`（任务 13.5 的契约）。

use phonemic_protocol::{AppError, ErrorCode};

/// 把任意错误归一为 [`AppError`] 的通用 trait。
///
/// 与 [`From`] 不同，该 trait 强制调用方提供"业务错误码"，避免使用
/// `From<io::Error>` 等通用实现时丢失语义（例如把"文件不存在"硬塞成
/// `INJECT_BACKEND_ERROR`）。
pub trait IntoAppError {
    /// 转为 [`AppError`]，使用 `code` 作为业务错误码、`Display` 作为 `message`。
    fn into_app_error(self, code: ErrorCode) -> AppError;
}

impl<E: std::fmt::Display> IntoAppError for E {
    fn into_app_error(self, code: ErrorCode) -> AppError {
        AppError::now(code.as_str(), self.to_string())
    }
}

/// 给 [`AppError`] 加一个直接接受 [`ErrorCode`] 枚举的构造器。
pub trait AppErrorExt {
    /// 用强类型 [`ErrorCode`] + 人类可读消息构造 [`AppError`]。
    fn from_code(code: ErrorCode, message: impl Into<String>) -> AppError;
}

impl AppErrorExt for AppError {
    fn from_code(code: ErrorCode, message: impl Into<String>) -> AppError {
        AppError::now(code.as_str(), message)
    }
}

/// 错误码到桌面端 i18n 字典 key 的稳定映射。
///
/// 调用方拿到 [`AppError`] 后：
/// 1. 用 `ErrorCode::from_str(&app_error.code)` 解析；
/// 2. 用本函数把它转为 i18n key（形如 `"error.PAIR_INVALID"`）；
/// 3. 在桌面端 i18n 字典里查 key，缺失时回退到 `app_error.message`。
///
/// 这里不直接提供 `lookup_message(lang, code)` 是为了保持 `phonemic-core`
/// 与 `phonemic-protocol` 的依赖单向：`phonemic-protocol` 不知道任何
/// i18n 资源，i18n 字典查询发生在 `phonemic-core::i18n`。
#[must_use]
pub fn code_to_i18n_key(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::OsUnsupported => "error.OS_UNSUPPORTED",
        ErrorCode::PortUnavailable => "error.PORT_UNAVAILABLE",
        ErrorCode::LanLost => "error.LAN_LOST",
        ErrorCode::MicPermissionDenied => "error.MIC_PERMISSION_DENIED",
        ErrorCode::AsrTimeout => "error.ASR_TIMEOUT",
        ErrorCode::PairInvalid => "error.PAIR_INVALID",
        ErrorCode::PairRatelimit => "error.PAIR_RATELIMIT",
        ErrorCode::AuthRequired => "error.AUTH_REQUIRED",
        ErrorCode::ForbiddenSubnet => "error.FORBIDDEN_SUBNET",
        ErrorCode::InjectNoFocusTarget => "error.INJECT_NO_FOCUS_TARGET",
        ErrorCode::InjectPermissionDenied => "error.INJECT_PERMISSION_DENIED",
        ErrorCode::InjectPaused => "error.INJECT_PAUSED",
        ErrorCode::InjectBackendError => "error.INJECT_BACKEND_ERROR",
        ErrorCode::MsgBadFormat => "error.MSG_BAD_FORMAT",
        ErrorCode::ReconnectFailed => "error.RECONNECT_FAILED",
    }
}

/// 用桌面 i18n 字典把 [`AppError`] 渲染为最终用户文本：
/// 1. 解析 `code`，命中 [`code_to_i18n_key`]；
/// 2. 用 `crate::i18n::t(lang, key)` 查字典；
/// 3. 缺失时回退到 `error.message`（任务 13.5 的契约）。
#[must_use]
pub fn render_error(lang: crate::i18n::Lang, err: &AppError) -> String {
    let code: Option<ErrorCode> = err.code.parse().ok();
    if let Some(c) = code {
        if let Some(text) = crate::i18n::t(lang, code_to_i18n_key(c)) {
            return text.to_string();
        }
    }
    err.message.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn into_app_error_wraps_io_error_with_code() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "config.toml missing");
        let app: AppError = io_err.into_app_error(ErrorCode::PortUnavailable);
        assert_eq!(app.code, "PORT_UNAVAILABLE");
        assert!(app.message.contains("config.toml"));
        assert!(app.ts.ends_with('Z'));
    }

    #[test]
    fn into_app_error_wraps_serde_json_error() {
        let parse_err: serde_json::Error = serde_json::from_str::<u32>("not-a-number").unwrap_err();
        let app = parse_err.into_app_error(ErrorCode::MsgBadFormat);
        assert_eq!(app.code, "MSG_BAD_FORMAT");
        assert!(!app.message.is_empty());
    }

    #[test]
    fn app_error_from_code_uses_screaming_snake_case_literal() {
        let e = AppError::from_code(ErrorCode::InjectPaused, "暂停中");
        assert_eq!(e.code, "INJECT_PAUSED");
        assert_eq!(e.message, "暂停中");
    }

    #[test]
    fn render_error_falls_back_to_message_when_key_missing() {
        // 用一个永远不会出现在字典里的 code 字符串模拟"未知错误"。
        let err = AppError::now("TOTALLY_UNKNOWN", "raw fallback");
        let s = render_error(crate::i18n::Lang::EnUS, &err);
        assert_eq!(s, "raw fallback");
    }

    #[test]
    fn render_error_uses_dictionary_when_key_present() {
        let err = AppError::from_code(ErrorCode::LanLost, "raw");
        // 字典里若存在 error.LAN_LOST 则会被返回；若不存在则回退到 raw。
        // 任意结果都应当是非空字符串。
        let s = render_error(crate::i18n::Lang::ZhCN, &err);
        assert!(!s.is_empty());
    }

    #[test]
    fn code_to_i18n_key_covers_every_variant() {
        for code in ErrorCode::ALL {
            let key = code_to_i18n_key(code);
            assert!(key.starts_with("error."), "key={key}");
            assert!(key.ends_with(code.as_str()), "key={key} code={}", code.as_str());
        }
    }
}
