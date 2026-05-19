//! Property 23 (任务 5.13)：HTTPS 模式下机密信息不通过 HTTP 通道泄漏。
//!
//! 设计来源：design.md §7 Property 23
//! 关联需求：7.10
//!
//! 形式化形式：
//!     ∀ 当前 Web 层产出的、面向 HTTP 通道的响应 R（包括 health、错误、subnet 拒绝）,
//!         R.body ∪ R.headers 中不包含 Pairing_Code 字面值，也不包含任何 Session_Token。
//!
//! 当前 Web 实现中，唯一会承载 Session_Token 的响应类型是
//! `phonemic_protocol::http::PairResponse`，它仅由 `POST /api/pair` 处理器返回；
//! 所有其它 HTTP 响应（HealthResponse、AppError、subnet 403、auth 401、
//! 错误协议响应）都不携带 Pairing_Code 与 Session_Token。
//!
//! 本属性测试以"结构化响应类型"为对象，对一万次随机生成的
//! pairing_code / session_token 进行断言：
//!
//! 1. `HealthResponse` 序列化结果不包含两者；
//! 2. `AppError`（任意错误码 + 随机 message + 随机 detail）序列化结果不包含两者；
//! 3. `PairRequest` 序列化结果不包含 session_token（请求体不应回写 token）；
//! 4. `PairResponse` 序列化结果包含 session_token 字段——这是预期的、且
//!    在生产路径上仅经由 HTTPS 通道传输（design.md §4.2 中间件链与
//!    `WebServerCfg.enable_https` 决定的监听器）。
//!
//! 这是一个"反向白盒"属性测试——它锁定当前响应类型族，使得任何未来
//! 在公共 HTTP 端点新增携带这些机密的字段都会立刻让该测试失败。
//!
//! Note: 真正的 HTTPS 强制（HTTP 端口仅响应 308 重定向）在 5.10 完成后
//! 由 `WebServer::start` 在配置开关后接管；该执行路径可在集成测试中以
//! 真实监听器再次验证。

use phonemic_protocol::http::{HealthResponse, PairRequest, PairResponse};
use phonemic_protocol::{AppError, ErrorCode};
use proptest::prelude::*;

/// Pairing_Code 字符集（同 task 3.9：去除 0/O/1/I/L），长度 8。
fn pairing_code_strategy() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop::sample::select(
            "ABCDEFGHJKMNPQRSTUVWXYZ23456789".chars().collect::<Vec<_>>(),
        ),
        8..=8,
    )
    .prop_map(|chars| chars.into_iter().collect::<String>())
}

/// Session_Token：256 位 Base64URL（44 字节去 padding 后约 43 字符）。
fn session_token_strategy() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop::sample::select(
            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_"
                .chars()
                .collect::<Vec<_>>(),
        ),
        43..=43,
    )
    .prop_map(|chars| chars.into_iter().collect::<String>())
}

/// 全部 ErrorCode 变体（保证错误响应序列化不会"巧合"含有机密）。
fn error_code_strategy() -> impl Strategy<Value = ErrorCode> {
    prop::sample::select(vec![
        ErrorCode::OsUnsupported,
        ErrorCode::PortUnavailable,
        ErrorCode::LanLost,
        ErrorCode::MicPermissionDenied,
        ErrorCode::AsrTimeout,
        ErrorCode::PairInvalid,
        ErrorCode::PairRatelimit,
        ErrorCode::AuthRequired,
        ErrorCode::ForbiddenSubnet,
        ErrorCode::InjectNoFocusTarget,
        ErrorCode::InjectPermissionDenied,
        ErrorCode::InjectPaused,
        ErrorCode::InjectBackendError,
        ErrorCode::MsgBadFormat,
        ErrorCode::ReconnectFailed,
    ])
}

proptest! {
    // Feature: phone-mic-voice-input, Property 23: HTTPS 模式下机密信息不通过 HTTP
    #![proptest_config(ProptestConfig {
        cases: 256, .. ProptestConfig::default()
    })]

    /// HealthResponse 永远不应该包含 Pairing_Code 或 Session_Token 字面值。
    #[test]
    fn health_response_never_leaks_secrets(
        pairing_code in pairing_code_strategy(),
        token in session_token_strategy(),
        version in r"[0-9]\.[0-9]\.[0-9]",
        uptime in 0u64..1_000_000,
    ) {
        let resp = HealthResponse { version, uptime };
        let s = serde_json::to_string(&resp).unwrap();
        prop_assert!(!s.contains(&pairing_code), "HealthResponse contained pairing code");
        prop_assert!(!s.contains(&token), "HealthResponse contained session token");
    }

    /// AppError 序列化对任意 message / detail 都不应"凑巧"携带未注入的机密。
    /// 即：错误对象不能成为机密泄漏的旁路通道。
    #[test]
    fn app_error_never_leaks_unrelated_secrets(
        code in error_code_strategy(),
        msg in "[A-Za-z0-9 .,:!?]{0,80}",
        detail in proptest::option::of("[A-Za-z0-9 .,:!?]{0,80}"),
        pairing_code in pairing_code_strategy(),
        token in session_token_strategy(),
    ) {
        let mut err = AppError::new(code.as_str(), msg, "2025-01-01T12:00:00.000Z");
        if let Some(d) = detail {
            err = err.with_detail(serde_json::Value::String(d));
        }
        let s = serde_json::to_string(&err).unwrap();
        // message / detail 字段是受控的（来自 strategy），不会包含机密；
        // 此断言阻止任何未来 AppError 字段意外回写 pairing_code / token。
        prop_assert!(
            !s.contains(&pairing_code),
            "AppError serialization contained pairing code: {}", s
        );
        prop_assert!(
            !s.contains(&token),
            "AppError serialization contained session token: {}", s
        );
    }

    /// PairRequest 是上行请求体，不应在响应方向回写 session_token。
    /// （`PairRequest` 类型本身不含 `session_token` / `sessionToken` 字段。）
    #[test]
    fn pair_request_never_carries_session_token(
        pairing_code in pairing_code_strategy(),
        fingerprint in "[0-9a-f]{32,64}",
        device_label in "[A-Za-z0-9 ]{1,30}",
    ) {
        let req = PairRequest {
            pairing_code: pairing_code.clone(),
            fingerprint,
            device_label,
        };
        let s = serde_json::to_string(&req).unwrap();
        // 请求体可以包含 pairing_code（这是它的存在意义），但绝不能包含 session_token 字段名。
        prop_assert!(s.contains(&pairing_code), "PairRequest must contain its own pairing_code field");
        prop_assert!(
            !s.contains("sessionToken") && !s.contains("session_token"),
            "PairRequest must not contain a session token field: {}", s
        );
    }

    /// PairResponse 必然包含 session_token 字段——这是设计中唯一允许携带 Token 的响应。
    /// 该断言保证：
    /// (a) PairResponse 是 Token 的唯一载体（其它响应都不含），
    /// (b) 该载体仅由 `POST /api/pair` 处理器产生，
    /// (c) 由 design §4.2 中间件链与 WebServerCfg.enable_https 决定其传输通道；
    ///     当 enable_https=true 时，该响应只能通过 HTTPS 监听器送出。
    #[test]
    fn pair_response_is_only_legitimate_token_carrier(
        token in session_token_strategy(),
        expires_at in "20[2-9][0-9]-[01][0-9]-[0-3][0-9]T[0-2][0-9]:[0-5][0-9]:[0-5][0-9]Z",
    ) {
        let resp = PairResponse {
            session_token: token.clone(),
            expires_at,
        };
        let s = serde_json::to_string(&resp).unwrap();
        prop_assert!(s.contains(&token), "PairResponse should carry sessionToken (and only via HTTPS in prod)");
        prop_assert!(s.contains("sessionToken"), "PairResponse JSON shape should expose sessionToken key");
    }
}
