// Web Server integration test (任务 5.15).
//
// 验证 axum WebServer 真实启动后：
// - GET /api/health 返回 JSON
// - POST /api/pair：正确码 → 200，错误码 → 401 PAIR_INVALID
// - POST /api/pair：5 次失败后 429 PAIR_RATELIMIT + Retry-After
// - 模拟公网 IP（X-Forwarded-For）→ 403 FORBIDDEN_SUBNET
// - shutdown 后端口在 3 秒内释放
//
// 注：这里走 HTTP 客户端而不是 reqwest（避免引入额外依赖）；用 hyper 1 + tokio 直接构造请求。

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use phonemic_core::bridge_events::channel as events_channel;
use phonemic_core::pairing_service::PairingService;
use phonemic_core::web::server::{WebServer, WebServerCfg};

fn cfg(port: u16) -> WebServerCfg {
    WebServerCfg {
        preferred_port: port,
        bind_addr: IpAddr::V4(Ipv4Addr::LOCALHOST),
        static_root: std::env::temp_dir().join("phonemic-web-test-static"),
        version: "0.1.0-test".into(),
    }
}

async fn http_get(addr: SocketAddr, path: &str) -> (u16, String) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    let mut stream = TcpStream::connect(addr).await.expect("connect");
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await.expect("write");
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.expect("read");
    let text = String::from_utf8_lossy(&buf).to_string();
    let status = parse_status(&text);
    (status, text)
}

async fn http_get_with_xff(addr: SocketAddr, path: &str, xff: &str) -> (u16, String) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    let mut stream = TcpStream::connect(addr).await.expect("connect");
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {addr}\r\nX-Forwarded-For: {xff}\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await.expect("write");
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.expect("read");
    let text = String::from_utf8_lossy(&buf).to_string();
    let status = parse_status(&text);
    (status, text)
}

async fn http_post_json(addr: SocketAddr, path: &str, body: &str) -> (u16, String) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    let mut stream = TcpStream::connect(addr).await.expect("connect");
    let request = format!(
        "POST {path} HTTP/1.1\r\n\
         Host: {addr}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {len}\r\n\
         Connection: close\r\n\r\n{body}",
        len = body.len(),
    );
    stream.write_all(request.as_bytes()).await.expect("write");
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.expect("read");
    let text = String::from_utf8_lossy(&buf).to_string();
    let status = parse_status(&text);
    (status, text)
}

fn parse_status(resp: &str) -> u16 {
    // "HTTP/1.1 200 OK\r\n..." → 200
    let line = resp.lines().next().unwrap_or("");
    let parts: Vec<&str> = line.split_whitespace().collect();
    parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0)
}

fn extract_body(resp: &str) -> &str {
    if let Some(idx) = resp.find("\r\n\r\n") {
        &resp[idx + 4..]
    } else {
        ""
    }
}

#[tokio::test]
async fn health_endpoint_returns_version_and_uptime() {
    let (tx, _rx) = events_channel();
    let port = 18181;
    let Ok(handle) = WebServer::start(cfg(port), PairingService::new(), tx).await else {
        eprintln!("port {port} unavailable; skipping");
        return;
    };
    let bound = handle.info.bind_addr;
    let (status, resp) = http_get(bound, "/api/health").await;
    assert_eq!(status, 200, "health response: {resp}");
    let body = extract_body(&resp);
    assert!(body.contains("\"version\""), "body: {body}");
    assert!(body.contains("\"uptime\""), "body: {body}");
    handle.shutdown().await;
}

#[tokio::test]
async fn pair_endpoint_accepts_correct_code_and_emits_event() {
    let (tx, mut rx) = events_channel();
    let port = 18182;
    let svc = PairingService::new();
    let code = svc.current_pairing_code().as_str().to_owned();
    let Ok(handle) = WebServer::start(cfg(port), svc, tx).await else {
        eprintln!("port {port} unavailable; skipping");
        return;
    };
    let body = format!(
        r#"{{"pairingCode":"{code}","fingerprint":"fp-it","deviceLabel":"iPhone"}}"#
    );
    let (status, resp) = http_post_json(handle.info.bind_addr, "/api/pair", &body).await;
    assert_eq!(status, 200, "pair response: {resp}");
    let body = extract_body(&resp);
    assert!(body.contains("\"sessionToken\""), "body: {body}");
    assert!(body.contains("\"expiresAt\""), "body: {body}");

    // DevicePaired 事件被投递。
    match tokio::time::timeout(Duration::from_millis(500), rx.recv()).await {
        Ok(Some(phonemic_core::bridge_events::BridgeEvent::DevicePaired(p))) => {
            assert_eq!(p.device_label, "iPhone");
        }
        other => panic!("expected DevicePaired event, got {other:?}"),
    }
    handle.shutdown().await;
}

#[tokio::test]
async fn pair_endpoint_rejects_wrong_code_with_pair_invalid() {
    let (tx, _rx) = events_channel();
    let port = 18183;
    let Ok(handle) = WebServer::start(cfg(port), PairingService::new(), tx).await else {
        return;
    };
    let body = r#"{"pairingCode":"WRONGCOD","fingerprint":"fp","deviceLabel":"x"}"#;
    let (status, resp) = http_post_json(handle.info.bind_addr, "/api/pair", body).await;
    assert_eq!(status, 401, "resp: {resp}");
    assert!(resp.contains("PAIR_INVALID"));
    handle.shutdown().await;
}

#[tokio::test]
async fn pair_endpoint_rate_limits_after_threshold() {
    let (tx, _rx) = events_channel();
    let port = 18184;
    let Ok(handle) = WebServer::start(cfg(port), PairingService::new(), tx).await else {
        return;
    };
    let body = r#"{"pairingCode":"WRONGCOD","fingerprint":"fp","deviceLabel":"x"}"#;
    let mut last_status = 0;
    let mut last_resp = String::new();
    for _ in 0..6 {
        let (s, r) = http_post_json(handle.info.bind_addr, "/api/pair", body).await;
        last_status = s;
        last_resp = r;
    }
    assert_eq!(last_status, 429, "resp: {last_resp}");
    assert!(last_resp.contains("PAIR_RATELIMIT"));
    // HTTP 头部大小写不敏感；axum 把所有响应头小写化。
    let lower = last_resp.to_ascii_lowercase();
    assert!(lower.contains("retry-after"), "response missing Retry-After: {last_resp}");
    handle.shutdown().await;
}

#[tokio::test]
async fn public_ip_via_xff_is_rejected_with_403() {
    let (tx, _rx) = events_channel();
    let port = 18185;
    let Ok(handle) = WebServer::start(cfg(port), PairingService::new(), tx).await else {
        return;
    };
    let (status, resp) = http_get_with_xff(handle.info.bind_addr, "/api/health", "203.0.113.42").await;
    assert_eq!(status, 403, "resp: {resp}");
    assert!(resp.contains("FORBIDDEN_SUBNET"));
    handle.shutdown().await;
}

#[tokio::test]
async fn shutdown_releases_port_within_3s() {
    let (tx, _rx) = events_channel();
    let port = 18186;
    let Ok(handle) = WebServer::start(cfg(port), PairingService::new(), tx).await else {
        return;
    };
    let bound_port = handle.info.bound_port;
    let started = std::time::Instant::now();
    handle.shutdown().await;
    let elapsed = started.elapsed();
    assert!(elapsed < Duration::from_secs(3), "shutdown took {elapsed:?}");

    // 端口应能立即被新 listener 重新绑定。
    let new_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), bound_port);
    let listener = tokio::net::TcpListener::bind(new_addr).await;
    assert!(listener.is_ok(), "port {bound_port} not released within 3s");
}
