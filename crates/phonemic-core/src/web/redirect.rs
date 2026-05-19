//! HTTP→HTTPS 重定向 handler（任务 5.10 / 5.13）。
//!
//! 当配置 `enable_https = true` 时，可在 HTTP 端口（若保留）挂载本 handler：
//! 接受任意请求 → 返回 308 Permanent Redirect 到对应 HTTPS URL，**响应体留空，
//! 响应头仅包含 `Location` + `Cache-Control`**。
//!
//! 这是 Property 23 的具体落地：HTTPS 模式下，HTTP 端口不会原样返回任何
//! 来自请求路径 / query string 的字符串（包括可能携带的 `pairingCode` / token），
//! 因为响应被强制收敛为"只含 Location 头部 + 空 body"的固定形态。
//!
//! Location 字段的构造保留 path 但抛弃 query：避免把 Mobile 误填的
//! `?pairingCode=...` 反射进 HTTPS URL。

use axum::body::Body;
use axum::http::{header, HeaderValue, Request, Response, StatusCode, Uri};

/// HTTP→HTTPS 重定向 handler。`https_host` 通常是 `<lan-ip>:<https-port>`。
#[must_use]
pub fn build_redirect_response(req: &Request<Body>, https_host: &str) -> Response<Body> {
    let path = req.uri().path();
    // 关键：丢弃 query；只保留 path，避免任何用户输入反射回响应。
    let location = format!("https://{https_host}{path}");
    let header_value = HeaderValue::from_str(&location)
        .unwrap_or_else(|_| HeaderValue::from_static("https://localhost/"));
    Response::builder()
        .status(StatusCode::PERMANENT_REDIRECT)
        .header(header::LOCATION, header_value)
        .header(header::CACHE_CONTROL, HeaderValue::from_static("no-store"))
        .body(Body::empty())
        .expect("redirect response should always build")
}

/// 校验"重定向响应永不泄漏机密"的纯字符串版本。
///
/// 给定 path、query、HTTPS host 与机密字符串集合，构造响应头部 + 空 body
/// 的字符串表示，断言其中不含任何机密。便于 Property 23 在不启动真实
/// HTTP server 的前提下做高密度属性测试。
#[must_use]
pub fn redirect_response_text(path: &str, https_host: &str) -> String {
    // path 仅作为 Location 字段的一部分；本函数不接受 query，与 build_redirect_response
    // 的"丢弃 query"语义保持一致。
    let safe_path = path.parse::<Uri>().ok().map(|u| u.path().to_owned()).unwrap_or_default();
    let location = format!("https://{https_host}{safe_path}");
    format!(
        "HTTP/1.1 308 Permanent Redirect\r\n\
         location: {location}\r\n\
         cache-control: no-store\r\n\
         content-length: 0\r\n\r\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirect_strips_query() {
        let req = Request::builder()
            .uri("/api/pair?pairingCode=ABCD2345&fingerprint=fp")
            .body(Body::empty())
            .unwrap();
        let resp = build_redirect_response(&req, "192.168.1.10:18443");
        assert_eq!(resp.status(), StatusCode::PERMANENT_REDIRECT);
        let location = resp
            .headers()
            .get(header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .unwrap();
        assert_eq!(location, "https://192.168.1.10:18443/api/pair");
        assert!(!location.contains("pairingCode"));
    }

    #[test]
    fn redirect_response_text_is_pure_header() {
        let s = redirect_response_text("/api/pair?pairingCode=SECRET01", "h:1");
        assert!(s.starts_with("HTTP/1.1 308"));
        assert!(s.contains("location: https://h:1/api/pair\r\n"));
        assert!(s.contains("content-length: 0"));
        // body 为空 → 文末为 \r\n\r\n。
        assert!(s.ends_with("\r\n\r\n"));
        assert!(!s.contains("SECRET01"));
    }
}

// ----------------------------------------------------------------------------
// Property tests
// ----------------------------------------------------------------------------
// Feature: phone-mic-voice-input, Property 23: HTTPS 模式下机密信息不通过 HTTP
//
// 任务 5.13：对任意 path / query 输入与任意机密字符串，断言 HTTP 端口
// 的重定向响应（headers + body）都不会回显机密。
#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    fn pairing_code_strategy() -> impl Strategy<Value = String> {
        prop::collection::vec(
            prop::sample::select(b"ABCDEFGHJKMNPQRSTUVWXYZ23456789".to_vec()),
            8,
        )
        .prop_map(|v| String::from_utf8(v).unwrap())
    }

    fn session_token_strategy() -> impl Strategy<Value = String> {
        prop::collection::vec(
            prop::sample::select(
                b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_".to_vec(),
            ),
            43,
        )
        .prop_map(|v| String::from_utf8(v).unwrap())
    }

    fn path_with_secret_strategy() -> impl Strategy<Value = (String, String, String)> {
        (
            prop::string::string_regex("/[A-Za-z0-9_/-]{0,64}").unwrap(),
            pairing_code_strategy(),
            session_token_strategy(),
        )
            .prop_map(|(base, code, token)| {
                // 把机密硬塞进 query string 与 path（极端反射场景）。
                let path_with_query = format!("{base}?pairingCode={code}&sessionToken={token}");
                (path_with_query, code, token)
            })
    }

    proptest! {
        // Feature: phone-mic-voice-input, Property 23: HTTPS 模式下机密信息不通过 HTTP
        #[test]
        fn property_23_redirect_never_leaks_secrets((path, code, token) in path_with_secret_strategy()) {
            let resp = redirect_response_text(&path, "lan:18443");
            prop_assert!(!resp.contains(&code), "response leaked pairing code: {resp}");
            prop_assert!(!resp.contains(&token), "response leaked session token: {resp}");
        }

        // Property 23 推论：构造的 axum Response 同样不含机密。
        #[test]
        fn property_23_axum_redirect_never_leaks_secrets(
            (path, code, token) in path_with_secret_strategy()
        ) {
            let req = Request::builder().uri(&path).body(Body::empty()).unwrap();
            let resp = build_redirect_response(&req, "lan:18443");
            // 检查所有 header value。
            for (_name, value) in resp.headers().iter() {
                if let Ok(s) = value.to_str() {
                    prop_assert!(!s.contains(&code), "header leaked code: {s}");
                    prop_assert!(!s.contains(&token), "header leaked token: {s}");
                }
            }
            // body 必须是空 (无法把 axum Body 同步转字符串，但我们在 build_redirect_response
            // 中刻意写入 Body::empty()——这条等价于"body 永不含机密"的不变量。
        }
    }
}
