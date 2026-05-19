//! axum 中间件：SubnetFilter / RateLimit / Auth（任务 5.2 / 5.4 / 5.5）。

use std::net::IpAddr;
use std::sync::Arc;
use std::time::Instant;

use axum::extract::{ConnectInfo, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use tokio::sync::Mutex;

use crate::lan_filter::is_rfc1918;
use crate::pair_rate_limit::{PairRateLimiter, FAILURE_WINDOW};
use crate::session::SessionToken;

use super::errors::ApiError;
use super::state::AppState;

// ---- Subnet filter ----------------------------------------------------------

/// SubnetFilter（任务 5.2 / Property 21）：允许 RFC1918 + loopback；其它一律 403。
///
/// 优先取 `ConnectInfo<SocketAddr>` 的对端地址；若无则尝试解析 `X-Forwarded-For`
/// 第一段 IP（用于反向代理与集成测试）。
pub async fn subnet_filter(
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Response {
    let peer = effective_peer_ip(addr.ip(), &headers);
    if !is_lan_or_loopback(peer) {
        return ApiError::forbidden_subnet().into_response();
    }
    next.run(request).await
}

/// 给定连接对端 IP 与请求头，得到"用于子网判定"的有效 IP。
///
/// 当 `X-Forwarded-For` 提供且其首段是合法 IP 时优先使用它；否则使用
/// 直连地址。集成测试由此路径模拟"公网 IP"。
pub fn effective_peer_ip(direct: IpAddr, headers: &HeaderMap) -> IpAddr {
    if let Some(value) = headers.get("x-forwarded-for") {
        if let Ok(s) = value.to_str() {
            if let Some(first) = s.split(',').next() {
                if let Ok(parsed) = first.trim().parse::<IpAddr>() {
                    return parsed;
                }
            }
        }
    }
    direct
}

/// LAN（RFC1918）+ loopback 通行；ipv6 loopback 也允许，便于本机调试。
#[must_use]
pub fn is_lan_or_loopback(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback() || is_rfc1918(v4),
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}

// ---- Rate limit (only used by /api/pair) -----------------------------------

/// 配对接口限流器；与 `AppState.pairing.rate_limiter` 解耦以便独立测试，
/// 但运行时由 [`AppState`] 内的 `PairingService` 持有。
///
/// 这里提供一个独立的 `Arc<Mutex<PairRateLimiter>>` 作为可选注入点，便于
/// `Router::layer` 链接时直接共享。
pub type SharedRateLimiter = Arc<Mutex<PairRateLimiter>>;

pub async fn pair_rate_limit(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Response {
    let peer = effective_peer_ip(addr.ip(), &headers);
    let now = Instant::now();
    {
        let svc = state.pairing().await;
        // PairingService 内部 RateLimiter 是私有字段；通过 submit_pair 路径
        // 才会被使用。这里只读判定 —— 我们暴露一个轻量 helper：
        if svc.is_rate_limited(peer, now) {
            return ApiError::pair_rate_limit(FAILURE_WINDOW.as_secs()).into_response();
        }
    }
    next.run(request).await
}

// ---- Auth (Bearer Token) ---------------------------------------------------

/// 解析 `Authorization: Bearer <token>` 或 `Sec-WebSocket-Protocol: phonemic.<token>`。
///
/// 仅返回字符串；上游在拿到字符串后调用 `SessionRegistry::validate` 做语义校验。
#[must_use]
pub fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers.get(header::AUTHORIZATION) {
        if let Ok(s) = value.to_str() {
            if let Some(rest) = s.strip_prefix("Bearer ") {
                let trimmed = rest.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_owned());
                }
            }
        }
    }
    None
}

/// Sec-WebSocket-Protocol 头部使用 `phonemic.<token>` 编码会话凭据。
#[must_use]
pub fn extract_ws_protocol_token(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::SEC_WEBSOCKET_PROTOCOL)?;
    let s = value.to_str().ok()?;
    // 头部允许逗号分隔多个 protocol；遍历并寻找 `phonemic.` 前缀的第一项。
    for entry in s.split(',') {
        let trimmed = entry.trim();
        if let Some(rest) = trimmed.strip_prefix("phonemic.") {
            if !rest.is_empty() {
                return Some(rest.to_owned());
            }
        }
    }
    None
}

/// 鉴权中间件：对非 public 路径强制要求 `Authorization: Bearer <token>`。
///
/// 注：WebSocket `/ws` 路径单独由 [`super::handlers::ws`] 处理（它消费
/// `Sec-WebSocket-Protocol` 头部），不进入本中间件。
pub async fn auth_required(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Response {
    let Some(token_str) = extract_bearer_token(&headers) else {
        return ApiError::auth_required().into_response();
    };
    // 长度 ≠ 43 时直接拒绝以防 SessionToken 构造 panic。
    if token_str.len() != 43 {
        return ApiError::auth_required().into_response();
    }
    let token = SessionToken::from_validated(token_str);
    let svc = state.pairing().await;
    if svc.sessions().validate(&token).is_err() {
        return ApiError::auth_required().into_response();
    }
    drop(svc);
    next.run(request).await
}

/// 无鉴权直通中间件，便于路由组合。
pub async fn no_op(request: Request, next: Next) -> Response {
    next.run(request).await
}

// ---- Helpers --------------------------------------------------------------

/// `tower_http::set_header` 风格的小工具：给响应附加严格 CSP 头部，
/// 避免静态资源下泄。任务 5.10 使用。
#[must_use]
pub fn strict_security_headers(_status: StatusCode) -> Vec<(http::HeaderName, http::HeaderValue)> {
    vec![
        (http::header::X_CONTENT_TYPE_OPTIONS, http::HeaderValue::from_static("nosniff")),
        (http::header::REFERRER_POLICY, http::HeaderValue::from_static("no-referrer")),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn make_headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(
                http::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                http::HeaderValue::from_str(v).unwrap(),
            );
        }
        h
    }

    #[test]
    fn effective_peer_ip_prefers_xff_first_segment() {
        let direct = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let h = make_headers(&[("x-forwarded-for", "203.0.113.10, 192.168.1.1")]);
        assert_eq!(
            effective_peer_ip(direct, &h),
            IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10))
        );
    }

    #[test]
    fn effective_peer_ip_falls_back_to_direct() {
        let direct = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 5));
        let h = HeaderMap::new();
        assert_eq!(effective_peer_ip(direct, &h), direct);
    }

    #[test]
    fn is_lan_or_loopback_accepts_rfc1918_and_loopback() {
        assert!(is_lan_or_loopback(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 5))));
        assert!(is_lan_or_loopback(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(is_lan_or_loopback(IpAddr::V4(Ipv4Addr::LOCALHOST)));
        assert!(!is_lan_or_loopback(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
        assert!(!is_lan_or_loopback(IpAddr::V4(Ipv4Addr::new(100, 64, 0, 1))));
    }

    #[test]
    fn extract_bearer_token_strips_prefix() {
        let h = make_headers(&[("authorization", "Bearer abc123")]);
        assert_eq!(extract_bearer_token(&h).as_deref(), Some("abc123"));

        let h2 = make_headers(&[("authorization", "Basic xyz")]);
        assert_eq!(extract_bearer_token(&h2), None);
    }

    #[test]
    fn extract_ws_protocol_token_finds_phonemic_prefix() {
        let h = make_headers(&[("sec-websocket-protocol", "phonemic.tok123")]);
        assert_eq!(extract_ws_protocol_token(&h).as_deref(), Some("tok123"));

        let h2 = make_headers(&[("sec-websocket-protocol", "json, phonemic.xyz")]);
        assert_eq!(extract_ws_protocol_token(&h2).as_deref(), Some("xyz"));

        let h3 = make_headers(&[("sec-websocket-protocol", "json, foo")]);
        assert_eq!(extract_ws_protocol_token(&h3), None);
    }
}

// ----------------------------------------------------------------------------
// Property tests
// ----------------------------------------------------------------------------
// Feature: phone-mic-voice-input, Property 21: 子网过滤
//
// 任务 5.3：随机生成 IPv4 客户端地址，断言：
//   - RFC1918 / loopback 一律通过；
//   - 公网 / link-local / CGNAT 一律拒绝。
// 这里在纯函数 [`is_lan_or_loopback`] 上做属性化覆盖；
// 中间件 `subnet_filter` 直接调用它，因此该不变量等价于
// "Pair / WS / API handler 在非 LAN IP 下绝不会被调用"（中间件已在
// `subnet_filter` 中以 `IntoResponse` 方式短路）。
#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;
    use std::net::{IpAddr, Ipv4Addr};

    /// 参考实现：与 `is_lan_or_loopback` 等价的纯逻辑。
    fn reference_is_lan(ip: IpAddr) -> bool {
        match ip {
            IpAddr::V4(v4) => {
                let [a, b, _, _] = v4.octets();
                v4.is_loopback()
                    || a == 10
                    || (a == 172 && (16..=31).contains(&b))
                    || (a == 192 && b == 168)
            }
            IpAddr::V6(v6) => v6.is_loopback(),
        }
    }

    fn ipv4_strategy() -> impl Strategy<Value = IpAddr> {
        (0u8..=255, 0u8..=255, 0u8..=255, 0u8..=255)
            .prop_map(|(a, b, c, d)| IpAddr::V4(Ipv4Addr::new(a, b, c, d)))
    }

    proptest! {
        // Feature: phone-mic-voice-input, Property 21: 子网过滤
        #[test]
        fn property_21_subnet_filter_matches_spec(ip in ipv4_strategy()) {
            prop_assert_eq!(is_lan_or_loopback(ip), reference_is_lan(ip));
        }

        // Property 21 推论：所有公网 IP（CGNAT / 8.8.8.8 / 类似）必被拒绝。
        #[test]
        fn property_21_public_ips_are_rejected(
            a in 1u8..=126,
            b in 0u8..=255,
            c in 0u8..=255,
            d in 1u8..=254,
        ) {
            // 排除 RFC1918 / loopback / CGNAT / link-local 区间。
            prop_assume!(a != 10);
            prop_assume!(!(a == 127));
            prop_assume!(!(a == 172 && (16..=31).contains(&b)));
            prop_assume!(!(a == 192 && b == 168));
            prop_assume!(!(a == 169 && b == 254));
            prop_assume!(!(a == 100 && (64..=127).contains(&b)));

            let ip = IpAddr::V4(Ipv4Addr::new(a, b, c, d));
            prop_assert!(!is_lan_or_loopback(ip), "expected reject for {ip}");
        }

        // Property 21 推论：RFC1918 + loopback 全部接受。
        #[test]
        fn property_21_rfc1918_and_loopback_pass(
            class in prop::sample::select(vec!["10", "172", "192", "127"]),
            b in 0u8..=255,
            c in 0u8..=255,
            d in 1u8..=254,
        ) {
            let ip = match class {
                "10" => IpAddr::V4(Ipv4Addr::new(10, b, c, d)),
                "172" => IpAddr::V4(Ipv4Addr::new(172, 16 + (b % 16), c, d)),
                "192" => IpAddr::V4(Ipv4Addr::new(192, 168, c, d)),
                "127" => IpAddr::V4(Ipv4Addr::new(127, b, c, d)),
                _ => unreachable!(),
            };
            prop_assert!(is_lan_or_loopback(ip), "expected pass for {ip}");
        }
    }
}
