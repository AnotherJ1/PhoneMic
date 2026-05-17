//! 任务 3.5：连接 URL 渲染（`render_connection_urls`）。
//!
//! - 需求来源：`requirements.md` R3.1 —— Web_Server 启动后桌面端需显示包含
//!   "LAN IPv4 地址 + 监听端口"的连接 URL。
//! - 设计来源：`design.md` §4.1（主界面 / RuntimeInfo 渲染） + Property 3。
//!
//! 模块只负责"把 (scheme, ips, port) 渲染为 `Vec<String>`"这一纯函数职责，
//! 不接触任何 I/O 或平台 API，便于属性测试覆盖（任务 3.6 / Property 3）。

use std::fmt;
use std::net::Ipv4Addr;

/// 连接所使用的协议方案。
///
/// MVP 仅支持 `http` 与 `https` 两种取值；序列化字符串与 URL 中的 scheme 段一致。
/// 设计来源：`design.md` §3.3 / §4.2（HTTP/HTTPS 双模启动）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scheme {
    /// 明文 HTTP。
    Http,
    /// 启用自签名 TLS 的 HTTPS。
    Https,
}

impl Scheme {
    /// 返回 URL 中使用的小写 scheme 字符串。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
        }
    }
}

impl fmt::Display for Scheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 把 LAN IPv4 列表渲染为可被手机访问的连接 URL 列表。
///
/// - 输入：协议方案、IPv4 列表、监听端口。
/// - 输出：与输入 `ips` 顺序、长度一一对应的 `Vec<String>`，每条形如
///   `<scheme>://<ip>:<port>`。
/// - 不会做去重或排序——上游应在 [`crate::lan_filter`]（任务 3.3）中完成稳定排序与去重。
///
/// 满足 Property 3：每个 IP 与端口都会出现在最终展示中。
///
/// **Validates: Requirements 3.1**
///
/// 设计来源：`design.md` §4.1、§7 Property 3。
#[must_use]
pub fn render_connection_urls(scheme: Scheme, ips: &[Ipv4Addr], port: u16) -> Vec<String> {
    ips.iter()
        .map(|ip| format!("{}://{}:{}", scheme.as_str(), ip, port))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheme_as_str_returns_lowercase_protocol() {
        assert_eq!(Scheme::Http.as_str(), "http");
        assert_eq!(Scheme::Https.as_str(), "https");
    }

    #[test]
    fn scheme_display_matches_as_str() {
        assert_eq!(format!("{}", Scheme::Http), "http");
        assert_eq!(format!("{}", Scheme::Https), "https");
    }

    #[test]
    fn render_empty_input_yields_empty_output() {
        let out = render_connection_urls(Scheme::Http, &[], 18080);
        assert!(out.is_empty());
    }

    #[test]
    fn render_single_ip_http() {
        let ips = [Ipv4Addr::new(192, 168, 1, 10)];
        let out = render_connection_urls(Scheme::Http, &ips, 18080);
        assert_eq!(out, vec!["http://192.168.1.10:18080".to_string()]);
    }

    #[test]
    fn render_single_ip_https() {
        let ips = [Ipv4Addr::new(10, 0, 0, 5)];
        let out = render_connection_urls(Scheme::Https, &ips, 8443);
        assert_eq!(out, vec!["https://10.0.0.5:8443".to_string()]);
    }

    #[test]
    fn render_multiple_ips_preserves_input_order() {
        let ips = [
            Ipv4Addr::new(192, 168, 1, 2),
            Ipv4Addr::new(10, 0, 0, 3),
            Ipv4Addr::new(172, 16, 0, 4),
        ];
        let out = render_connection_urls(Scheme::Http, &ips, 18080);
        assert_eq!(
            out,
            vec![
                "http://192.168.1.2:18080".to_string(),
                "http://10.0.0.3:18080".to_string(),
                "http://172.16.0.4:18080".to_string(),
            ]
        );
        // 每个 IP 与端口都必须出现在最终展示中（Property 3 关键不变量）。
        for (rendered, ip) in out.iter().zip(ips.iter()) {
            assert!(rendered.contains(&ip.to_string()));
            assert!(rendered.contains("18080"));
        }
    }

    #[test]
    fn render_zero_ipv4_address() {
        let ips = [Ipv4Addr::UNSPECIFIED]; // 0.0.0.0
        let out = render_connection_urls(Scheme::Http, &ips, 1024);
        assert_eq!(out, vec!["http://0.0.0.0:1024".to_string()]);
    }

    #[test]
    fn render_max_port_65535() {
        let ips = [Ipv4Addr::new(192, 168, 0, 1)];
        let out = render_connection_urls(Scheme::Https, &ips, u16::MAX);
        assert_eq!(out, vec!["https://192.168.0.1:65535".to_string()]);
    }

    #[test]
    fn render_min_port_zero_still_renders() {
        // 端口 0 在生产路径不会出现，但渲染函数不应越权拒绝；
        // 端口合法性由 `port_select`（任务 3.1）负责。
        let ips = [Ipv4Addr::LOCALHOST];
        let out = render_connection_urls(Scheme::Http, &ips, 0);
        assert_eq!(out, vec!["http://127.0.0.1:0".to_string()]);
    }

    #[test]
    fn render_output_length_matches_input_length() {
        let ips = [
            Ipv4Addr::new(192, 168, 1, 1),
            Ipv4Addr::new(192, 168, 1, 2),
            Ipv4Addr::new(192, 168, 1, 3),
            Ipv4Addr::new(192, 168, 1, 4),
        ];
        let out = render_connection_urls(Scheme::Http, &ips, 18080);
        assert_eq!(out.len(), ips.len());
    }
}
