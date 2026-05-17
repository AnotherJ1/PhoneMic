// Feature: phone-mic-voice-input, Property 3: 连接 URL 渲染一致性
//
// 设计来源：`.kiro/specs/phone-mic-voice-input/design.md`
//   - §4.1 主界面 / RuntimeInfo 渲染
//   - §7 Property 3：连接 URL 渲染一致性
//   - §9.2 属性测试规范（每条 PBT ≥ 256 cases）
//
// 任务来源：`.kiro/specs/phone-mic-voice-input/tasks.md` 3.6
//
// **Validates: Requirements 3.1**
//
// Property 3 形式化：
//   For any (scheme ∈ {http, https}, ips: Vec<Ipv4Addr>, port: u16),
//     let urls = render_connection_urls(scheme, &ips, port);
//   则 urls.len() == ips.len()，且对每个下标 i 满足
//     urls[i] == format!("{scheme}://{ip}:{port}")
//   且 urls[i] 同时包含 `ips[i]` 与 `port` 的字符串表示
//   （即"每个 IP 与端口都出现在最终展示中"）。

use std::net::Ipv4Addr;

use phonemic_core::url_render::{render_connection_urls, Scheme};
use proptest::prelude::*;

// ---------- 生成器（设计文档 §9.2 的"smart generators"约定） ----------

/// scheme 生成器：仅在 MVP 支持的两种取值之间均匀选择。
fn scheme_strategy() -> impl Strategy<Value = Scheme> {
    prop_oneof![Just(Scheme::Http), Just(Scheme::Https)]
}

/// IPv4 生成器：四个 u8 字节均匀采样后构造 [`Ipv4Addr`]。
///
/// 这里不限制 RFC1918 / loopback 等子集——`render_connection_urls`
/// 是纯渲染函数，不做接口过滤；过滤职责属于任务 3.3 `lan_filter`。
fn ipv4_strategy() -> impl Strategy<Value = Ipv4Addr> {
    (any::<u8>(), any::<u8>(), any::<u8>(), any::<u8>())
        .prop_map(|(a, b, c, d)| Ipv4Addr::new(a, b, c, d))
}

/// IPv4 列表生成器：长度 0..16，覆盖空集与多网卡场景。
fn ips_strategy() -> impl Strategy<Value = Vec<Ipv4Addr>> {
    prop::collection::vec(ipv4_strategy(), 0..16)
}

/// 端口生成器：覆盖整个 `u16` 区间（含 0 与 65535）。
fn port_strategy() -> impl Strategy<Value = u16> {
    any::<u16>()
}

// ---------- proptest 运行参数 ----------

/// 设计 §9.2 要求每个属性测试至少 100 cases；任务规范进一步要求 256 cases。
fn pbt_config() -> ProptestConfig {
    let mut cfg = ProptestConfig::default();
    cfg.cases = 256;
    cfg
}

// ---------- Property 3 ----------

proptest! {
    #![proptest_config(pbt_config())]

    /// Property 3：连接 URL 渲染一致性。
    ///
    /// **Validates: Requirements 3.1**
    #[test]
    fn render_connection_urls_is_consistent(
        scheme in scheme_strategy(),
        ips in ips_strategy(),
        port in port_strategy(),
    ) {
        let urls = render_connection_urls(scheme, &ips, port);

        // (a) 输出长度必须与输入长度一致，且保留输入顺序——上游 `lan_filter`
        //     已完成稳定排序与去重，渲染层不得二次扰动顺序。
        prop_assert_eq!(urls.len(), ips.len());

        let port_str = port.to_string();
        for (i, ip) in ips.iter().enumerate() {
            let rendered = &urls[i];

            // (b) 字面相等：必须严格等于 `<scheme>://<ip>:<port>`。
            let expected = format!("{}://{}:{}", scheme.as_str(), ip, port);
            prop_assert_eq!(rendered, &expected);

            // (c) 「每个 IP 与端口都出现在最终展示中」——Property 3 文本不变量。
            //     这两条断言在 (b) 成立时为推论，但仍显式断言以贴合设计文档原文，
            //     便于回归时定位是哪条不变量被破坏。
            prop_assert!(
                rendered.contains(&ip.to_string()),
                "rendered URL must contain the IP literal"
            );
            prop_assert!(
                rendered.contains(&port_str),
                "rendered URL must contain the port literal"
            );

            // (d) scheme 段必须以协议名 + "://" 起始。
            let scheme_prefix = format!("{}://", scheme.as_str());
            prop_assert!(
                rendered.starts_with(&scheme_prefix),
                "rendered URL must start with the scheme prefix"
            );
        }
    }
}
