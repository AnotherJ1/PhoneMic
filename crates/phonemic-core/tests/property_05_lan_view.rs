// Feature: phone-mic-voice-input, Property 5: LAN 状态映射
//
// 设计来源：`.kiro/specs/phone-mic-voice-input/design.md`
//   - §4.4 LAN 状态视图 / `compute_lan_view`
//   - §7 Property 5
//   - §9.2 属性测试规范（共享生成器 `lan_iface_set()`）
//
// 任务来源：`.kiro/specs/phone-mic-voice-input/tasks.md` 3.8
//
// **Validates: Requirements 3.5, 3.6**
//
// 性质陈述：
//   对任意 `Vec<NetworkInterface>`（地址可任意混合 IPv4 / IPv6、loopback、
//   link-local、CGNAT、公网与 RFC1918，允许跨接口 / 接口内重复）以及
//   任意 `Lang lang ∈ { Lang::ZhCN, Lang::EnUS }`，
//   `compute_lan_view(&ifaces, lang)` 满足：
//
//   1) 若 `filter_lan_ipv4(&ifaces)` 为空：
//      - `view.scan_disabled == true`；
//      - `view.ips.is_empty()`；
//      - `view.banner.is_some()`，且 `view.banner.unwrap()`
//        等于 `phonemic_core::i18n::t(lang, "app.banner.no_lan").unwrap()`。
//   2) 否则：
//      - `view.scan_disabled == false`；
//      - `view.banner.is_none()`；
//      - `view.ips == filter_lan_ipv4(&ifaces)`（顺序保留）。
//
// 备注：address-mix 生成器从 Property 02 重新内联实现而非跨测试文件导入，
// 这样每个属性测试都是自包含的二进制目标，便于单独运行与回归隔离。

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use phonemic_core::i18n::{Lang, t};
use phonemic_core::lan_filter::{NetworkInterface, filter_lan_ipv4};
use phonemic_core::lan_view::compute_lan_view;
use proptest::prelude::*;

// ---------- 共享生成器（与 property_02 等价，inline 重写以保持自包含） ----------

/// 单条 IP 地址生成器：在 §9.2 要求覆盖的多种类别中均匀混合。
///
/// IPv4 类别：RFC1918 三段、CGNAT、loopback、link-local、公网；
/// IPv6 类别（全部应被过滤）：loopback、link-local、ULA、全局单播。
fn any_addr_strategy() -> impl Strategy<Value = IpAddr> {
    prop_oneof![
        // RFC1918：10.0.0.0/8
        3 => (any::<u8>(), any::<u8>(), any::<u8>())
            .prop_map(|(b, c, d)| IpAddr::V4(Ipv4Addr::new(10, b, c, d))),
        // RFC1918：172.16.0.0/12（第二字节固定在 [16, 31]）
        3 => (16u8..=31, any::<u8>(), any::<u8>())
            .prop_map(|(b, c, d)| IpAddr::V4(Ipv4Addr::new(172, b, c, d))),
        // RFC1918：192.168.0.0/16
        3 => (any::<u8>(), any::<u8>())
            .prop_map(|(c, d)| IpAddr::V4(Ipv4Addr::new(192, 168, c, d))),
        // CGNAT：100.64.0.0/10（第二字节 [64, 127]）
        2 => (64u8..=127, any::<u8>(), any::<u8>())
            .prop_map(|(b, c, d)| IpAddr::V4(Ipv4Addr::new(100, b, c, d))),
        // loopback：127.0.0.0/8
        2 => (any::<u8>(), any::<u8>(), any::<u8>())
            .prop_map(|(b, c, d)| IpAddr::V4(Ipv4Addr::new(127, b, c, d))),
        // link-local：169.254.0.0/16
        2 => (any::<u8>(), any::<u8>())
            .prop_map(|(c, d)| IpAddr::V4(Ipv4Addr::new(169, 254, c, d))),
        // 公网示例：1.x.x.x、8.x.x.x（避开私有网段）
        2 => (prop_oneof![Just(1u8), Just(8u8)], any::<u8>(), any::<u8>(), any::<u8>())
            .prop_map(|(a, b, c, d)| IpAddr::V4(Ipv4Addr::new(a, b, c, d))),
        // IPv6 loopback
        1 => Just(IpAddr::V6(Ipv6Addr::LOCALHOST)),
        // IPv6 link-local fe80::/10
        2 => (any::<u16>(), any::<u16>(), any::<u16>(), any::<u16>())
            .prop_map(|(e, f, g, h)| IpAddr::V6(Ipv6Addr::new(0xfe80, 0, 0, 0, e, f, g, h))),
        // IPv6 ULA fc00::/7
        2 => (any::<u16>(), any::<u16>(), any::<u16>(), any::<u16>())
            .prop_map(|(e, f, g, h)| IpAddr::V6(Ipv6Addr::new(0xfd00, 0, 0, 0, e, f, g, h))),
        // IPv6 全局单播 2001:db8::/32
        2 => (any::<u16>(), any::<u16>(), any::<u16>(), any::<u16>())
            .prop_map(|(e, f, g, h)| IpAddr::V6(Ipv6Addr::new(0x2001, 0x0db8, 0, 0, e, f, g, h))),
    ]
}

/// 单个网卡生成器：随机名 + 0..10 条地址。
///
/// 名称仅用于 `NetworkInterface::name`，不参与过滤 / 视图计算逻辑，
/// 因此使用紧凑字符串集即可。
fn iface_strategy() -> impl Strategy<Value = NetworkInterface> {
    let name = "[a-z]{1,4}[0-9]?";
    let addrs = prop::collection::vec(any_addr_strategy(), 0..10);
    (name, addrs).prop_map(|(name, addrs)| NetworkInterface { name, addrs })
}

/// 共享生成器 `lan_iface_set()`：0..6 个接口的列表。
fn ifaces_strategy() -> impl Strategy<Value = Vec<NetworkInterface>> {
    prop::collection::vec(iface_strategy(), 0..6)
}

/// `Lang` 生成器：仅支持 zh-CN / en-US 两种取值（design.md §4.1、R8.1）。
fn lang_strategy() -> impl Strategy<Value = Lang> {
    prop_oneof![Just(Lang::ZhCN), Just(Lang::EnUS)]
}

// ---------- proptest 运行参数 ----------

/// §9.2 要求每个 PBT ≥ 100 cases；与同 spec 下 Property 02 / 11 对齐使用 256。
fn pbt_config() -> ProptestConfig {
    let mut cfg = ProptestConfig::default();
    cfg.cases = 256;
    cfg
}

// ---------- 属性测试 ----------

proptest! {
    #![proptest_config(pbt_config())]

    /// Property 5：LAN 状态映射。
    ///
    /// **Validates: Requirements 3.5, 3.6**
    #[test]
    fn compute_lan_view_maps_filter_to_scan_state_and_banner(
        ifaces in ifaces_strategy(),
        lang in lang_strategy(),
    ) {
        let view = compute_lan_view(&ifaces, lang);
        let expected_ips = filter_lan_ipv4(&ifaces);

        if expected_ips.is_empty() {
            // 空集分支：扫码禁用 + 显示 i18n banner（Requirements 3.6）。
            prop_assert!(
                view.scan_disabled,
                "empty LAN must disable scan, lang={:?}",
                lang.as_str()
            );
            prop_assert!(
                view.ips.is_empty(),
                "empty LAN must yield empty ips, got {:?}",
                view.ips
            );
            prop_assert!(
                view.banner.is_some(),
                "empty LAN must carry a banner, lang={:?}",
                lang.as_str()
            );

            let expected_banner = t(lang, "app.banner.no_lan")
                .expect("字典必须包含 app.banner.no_lan");
            prop_assert_eq!(
                view.banner.as_deref(),
                Some(expected_banner),
                "banner must come from i18n dict for lang={:?}",
                lang.as_str()
            );
        } else {
            // 非空分支：扫码可用 + 顺序与 filter_lan_ipv4 一致（Requirements 3.5）。
            prop_assert!(
                !view.scan_disabled,
                "non-empty LAN must NOT disable scan, lang={:?}",
                lang.as_str()
            );
            prop_assert_eq!(
                view.banner.as_ref(),
                None,
                "non-empty LAN must NOT carry a banner"
            );
            prop_assert_eq!(
                &view.ips,
                &expected_ips,
                "ips must equal filter_lan_ipv4 output (order preserved)"
            );
        }
    }
}
