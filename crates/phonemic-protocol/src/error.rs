//! `ErrorCode` —— 跨 HTTP / `WebSocket` / 桌面端日志统一的错误码枚举。
//!
//! 任务来源：tasks.md 2.3
//! 设计来源：design.md §8.1
//! 关联需求：9.6、9.8
//!
//! ## 设计要点
//!
//! - 所有变体使用 PascalCase，在 `serde` 序列化时统一转换为大写蛇形
//!   （`SCREAMING_SNAKE_CASE`）字面量，作为跨语言（Rust ↔ TypeScript）
//!   的稳定线缆格式。
//! - 提供 `as_str` 常量路径，避免在仅需字符串的场景（日志、错误对象拼装）
//!   引入 `serde_json` 依赖。
//! - 提供 `Display`、`FromStr`、`ALL` 常量数组以方便迭代与测试。

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// `PhoneMic` 全局错误码（详见 design.md §8.1 完整清单）。
///
/// 序列化为 `SCREAMING_SNAKE_CASE` 字面量；HTTP / `WebSocket` / 日志
/// 三条通道使用同一字符串以便客户端与运维工具消费。
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    /// 当前 `OS` 或版本不在受支持范围内（关联需求 1.5）。
    OsUnsupported,
    /// 端口选择失败 / 全部占用（关联需求 2.8）。
    PortUnavailable,
    /// `LAN` 连接丢失（关联需求 3.6）。
    LanLost,
    /// 浏览器麦克风权限被拒（关联需求 4.8 / 5.1）。
    MicPermissionDenied,
    /// `ASR` 单段超时（关联需求 5.7）。
    AsrTimeout,
    /// 配对码错误或已失效（关联需求 7.2 / 7.9）。
    PairInvalid,
    /// 配对失败次数超过阈值，进入限流冻结（关联需求 7.5）。
    PairRatelimit,
    /// 缺少或非法的 `Session_Token`（关联需求 7.4 / 9.6）。
    AuthRequired,
    /// 来源 `IP` 不在 `RFC1918` / `loopback` 子网内（关联需求 7.8）。
    ForbiddenSubnet,
    /// 当前没有可注入的前台焦点窗口（关联需求 6.6）。
    InjectNoFocusTarget,
    /// 平台辅助功能 / 注入权限被拒（关联需求 6.8）。
    InjectPermissionDenied,
    /// 注入处于暂停状态（关联需求 6.7）。
    InjectPaused,
    /// 平台后端注入失败（关联需求 9.8）。
    InjectBackendError,
    /// `WebSocket` 报文非法 / 缺字段（关联需求 9.6）。
    MsgBadFormat,
    /// 重连退避序列耗尽仍未成功（关联需求 9.5）。
    ReconnectFailed,
}

impl ErrorCode {
    /// 与 `serde` 序列化字面量严格一致的常量字符串。
    ///
    /// 提供 `const` 路径以便日志层、错误对象拼装等热路径无需依赖
    /// `serde_json`；若有偏离将由测试 `as_str_matches_serde_output` 立即拦截。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OsUnsupported => "OS_UNSUPPORTED",
            Self::PortUnavailable => "PORT_UNAVAILABLE",
            Self::LanLost => "LAN_LOST",
            Self::MicPermissionDenied => "MIC_PERMISSION_DENIED",
            Self::AsrTimeout => "ASR_TIMEOUT",
            Self::PairInvalid => "PAIR_INVALID",
            Self::PairRatelimit => "PAIR_RATELIMIT",
            Self::AuthRequired => "AUTH_REQUIRED",
            Self::ForbiddenSubnet => "FORBIDDEN_SUBNET",
            Self::InjectNoFocusTarget => "INJECT_NO_FOCUS_TARGET",
            Self::InjectPermissionDenied => "INJECT_PERMISSION_DENIED",
            Self::InjectPaused => "INJECT_PAUSED",
            Self::InjectBackendError => "INJECT_BACKEND_ERROR",
            Self::MsgBadFormat => "MSG_BAD_FORMAT",
            Self::ReconnectFailed => "RECONNECT_FAILED",
        }
    }

    /// 全部错误码常量数组（按 `as_str` 字面量字典序无关，固定声明顺序）。
    ///
    /// 便于 `FromStr`、迭代式测试以及 i18n 文案完整性校验。
    pub const ALL: [Self; 15] = [
        Self::OsUnsupported,
        Self::PortUnavailable,
        Self::LanLost,
        Self::MicPermissionDenied,
        Self::AsrTimeout,
        Self::PairInvalid,
        Self::PairRatelimit,
        Self::AuthRequired,
        Self::ForbiddenSubnet,
        Self::InjectNoFocusTarget,
        Self::InjectPermissionDenied,
        Self::InjectPaused,
        Self::InjectBackendError,
        Self::MsgBadFormat,
        Self::ReconnectFailed,
    ];
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `ErrorCode::from_str` 解析失败时返回的错误。
///
/// 携带原始输入字符串便于诊断；不直接对外暴露内部拼写表。
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
#[error("unknown ErrorCode literal: {0:?}")]
pub struct ProtocolErrorCodeParseError(pub String);

impl FromStr for ErrorCode {
    type Err = ProtocolErrorCodeParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        for code in Self::ALL {
            if code.as_str() == s {
                return Ok(code);
            }
        }
        Err(ProtocolErrorCodeParseError(s.to_owned()))
    }
}


#[cfg(test)]
mod tests {
    use super::{ErrorCode, ProtocolErrorCodeParseError};
    use std::str::FromStr;

    /// `(变体, 期望的 SCREAMING_SNAKE_CASE 字面量)` 列表，作为单一来源驱动多条断言。
    const CASES: &[(ErrorCode, &str)] = &[
        (ErrorCode::OsUnsupported, "OS_UNSUPPORTED"),
        (ErrorCode::PortUnavailable, "PORT_UNAVAILABLE"),
        (ErrorCode::LanLost, "LAN_LOST"),
        (ErrorCode::MicPermissionDenied, "MIC_PERMISSION_DENIED"),
        (ErrorCode::AsrTimeout, "ASR_TIMEOUT"),
        (ErrorCode::PairInvalid, "PAIR_INVALID"),
        (ErrorCode::PairRatelimit, "PAIR_RATELIMIT"),
        (ErrorCode::AuthRequired, "AUTH_REQUIRED"),
        (ErrorCode::ForbiddenSubnet, "FORBIDDEN_SUBNET"),
        (ErrorCode::InjectNoFocusTarget, "INJECT_NO_FOCUS_TARGET"),
        (ErrorCode::InjectPermissionDenied, "INJECT_PERMISSION_DENIED"),
        (ErrorCode::InjectPaused, "INJECT_PAUSED"),
        (ErrorCode::InjectBackendError, "INJECT_BACKEND_ERROR"),
        (ErrorCode::MsgBadFormat, "MSG_BAD_FORMAT"),
        (ErrorCode::ReconnectFailed, "RECONNECT_FAILED"),
    ];

    #[test]
    fn serialize_produces_screaming_snake_case_literals() {
        for (variant, literal) in CASES {
            let json = serde_json::to_string(variant).expect("serialize ErrorCode");
            assert_eq!(json, format!("\"{literal}\""), "variant {variant:?}");
        }
    }

    #[test]
    fn deserialize_round_trips_every_variant() {
        for (variant, literal) in CASES {
            let quoted = format!("\"{literal}\"");
            let parsed: ErrorCode =
                serde_json::from_str(&quoted).expect("deserialize ErrorCode");
            assert_eq!(parsed, *variant);
        }
    }

    #[test]
    fn as_str_matches_serde_output() {
        // `as_str` 与 `serde_json` 输出（去掉外层引号）必须完全一致，
        // 防止常量路径与序列化派生悄悄发生分歧。
        for (variant, _) in CASES {
            let json = serde_json::to_string(variant).expect("serialize ErrorCode");
            let unquoted = json.trim_matches('"');
            assert_eq!(variant.as_str(), unquoted, "variant {variant:?}");
        }
    }

    #[test]
    fn display_uses_as_str() {
        for (variant, literal) in CASES {
            assert_eq!(variant.to_string(), *literal);
        }
    }

    #[test]
    fn from_str_accepts_canonical_literals() {
        for (variant, literal) in CASES {
            let parsed = ErrorCode::from_str(literal).expect("FromStr canonical");
            assert_eq!(parsed, *variant);
        }
    }

    #[test]
    fn from_str_rejects_unknown_literal() {
        let err = ErrorCode::from_str("definitely-not-a-code").unwrap_err();
        assert_eq!(
            err,
            ProtocolErrorCodeParseError("definitely-not-a-code".to_owned())
        );
    }

    #[test]
    fn from_str_rejects_lowercase_and_partial_matches() {
        // 与设计契约一致：仅大写蛇形被接受，避免客户端按风格自创变体。
        for (_, literal) in CASES {
            let lower = literal.to_ascii_lowercase();
            assert!(
                ErrorCode::from_str(&lower).is_err(),
                "lower-case form {lower} should not parse"
            );
        }
        assert!(ErrorCode::from_str("OS_UNSUPPORTED ").is_err());
        assert!(ErrorCode::from_str("").is_err());
    }

    #[test]
    fn all_constant_lists_every_variant_exactly_once() {
        assert_eq!(ErrorCode::ALL.len(), 15);
        assert_eq!(ErrorCode::ALL.len(), CASES.len());

        for (variant, _) in CASES {
            let occurrences = ErrorCode::ALL.iter().filter(|v| *v == variant).count();
            assert_eq!(occurrences, 1, "variant {variant:?} must appear exactly once");
        }
    }
}
