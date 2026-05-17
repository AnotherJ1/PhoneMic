// Feature: phone-mic-voice-input, Property 2: LAN IPv4 列表过滤
//
// 设计来源：`.kiro/specs/phone-mic-voice-input/design.md`
//   - §4.4 LAN 状态视图 / `filter_lan_ipv4`
//   - §7 Property 2
//   - §9.2 属性测试规范（共享生成器 `lan_iface_set()`）
//
// 任务来源：`.kiro/specs/phone-mic-voice-input/tasks.md` 3.4
//
// **Validates: Requirements 3.2**
//
// 性质陈述：
//   对任意 `Vec<NetworkInterface>` —— 其中地址可任意混合 IPv4 / IPv6、loopback、
//   link-local、CGNAT、公网与 RFC1918，且允许跨接口或接口内重复 ——
//   `filter_lan_ipv4(&ifaces)` 满足：
//     1) 输出仅含落在 RFC1918 私有网段的 IPv4（`is_rfc1918` 全部成立）；
//     2) 输出无重复；
//     3) 输出顺序与"按 ifaces 顺序、再按各自 addrs 顺序、保留首次出现"严格一致。

use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use phonemic_core::lan_filter::{filter_lan_ipv4, is_rfc1918, NetworkInterface};
use proptest::prelude::*;

// ---------- 共享生成器 ----------

/// 单条 IP 地址生成器：在 §9.2 要求覆盖的多种类别中均匀混合。
///
/// IPv4 类别：
///   - RFC1918 三段（10/8、172.16/12、192.168/16）
///   - CGNAT 100.64/10（应被过滤）
///   - loopback 127/8（应被过滤）
///   - link-local 169.254/16（应被过滤）
///   - 公网示例（应被过滤）
///
/// IPv6 类别（全部应被过滤）：
///   - loopback ::1
///   - link-local fe80::/10
///   - ULA fc00::/7
///   - 全局单播 2000::/3
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
        // 公网示例：8.x.x.x、1.x.x.x（避开私有网段）
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
/// 名称仅用于 `NetworkInterface::name`，不参与过滤逻辑，因此使用紧凑字符串集即可。
fn iface_strategy() -> impl Strategy<Value = NetworkInterface> {
    let name = "[a-z]{1,4}[0-9]?";
    let addrs = prop::collection::vec(any_addr_strategy(), 0..10);
    (name, addrs).prop_map(|(name, addrs)| NetworkInterface { name, addrs })
}

/// 共享生成器 `lan_iface_set()`：0..6 个接口的列表。
fn ifaces_strategy() -> impl Strategy<Value = Vec<NetworkInterface>> {
    prop::collection::vec(iface_strategy(), 0..6)
}

/// 引用实现：按 ifaces 顺序、再按各自 addrs 顺序，保留首次出现的 RFC1918 IPv4。
///
/// 与 `filter_lan_ipv4` 实现相互独立 —— 这里使用 `Vec::contains` 的等价形式，
/// 通过 `HashSet` 去重 + 顺序记录，避免照搬被测函数的实现细节。
fn expected_filter(ifaces: &[NetworkInterface]) -> Vec<Ipv4Addr> {
    let mut seen: HashSet<Ipv4Addr> = HashSet::new();
    let mut out: Vec<Ipv4Addr> = Vec::new();
    for iface in ifaces {
        for addr in &iface.addrs {
            if let IpAddr::V4(v4) = addr {
                if is_rfc1918(*v4) && seen.insert(*v4) {
                    out.push(*v4);
                }
            }
        }
    }
    out
}

// ---------- proptest 运行参数 ----------

/// §9.2 要求每个 PBT ≥ 100 cases；与同 spec 下 Property 11 对齐使用 256。
fn pbt_config() -> ProptestConfig {
    let mut cfg = ProptestConfig::default();
    cfg.cases = 256;
    cfg
}

// ---------- 属性测试 ----------

proptest! {
    #![proptest_config(pbt_config())]

    /// Property 2：LAN IPv4 列表过滤。
    ///
    /// **Validates: Requirements 3.2**
    #[test]
    fn filter_lan_ipv4_keeps_only_rfc1918_dedup_and_stable(
        ifaces in ifaces_strategy(),
    ) {
        let out = filter_lan_ipv4(&ifaces);

        // (1) 输出每一项都是 RFC1918。
        for ip in &out {
            prop_assert!(
                is_rfc1918(*ip),
                "non-RFC1918 leaked: {ip}",
            );
        }

        // (2) 无重复。
        let unique: HashSet<&Ipv4Addr> = out.iter().collect();
        prop_assert_eq!(
            unique.len(),
            out.len(),
            "duplicate detected in output: {:?}",
            out
        );

        // (3) 稳定首次出现顺序：与独立计算的引用实现完全相同。
        let expected = expected_filter(&ifaces);
        prop_assert_eq!(out, expected);
    }
}
