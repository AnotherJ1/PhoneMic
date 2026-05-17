//! 任务 3.3：LAN IPv4 过滤函数。
//!
//! 在不打开任何 socket 的前提下，按照 RFC1918 规则
//! 把网卡列表过滤成可用于桌面端展示的"私有 IPv4 列表"，
//! 供 [`crate::lan_view`] 进一步生成连接 URL 与 banner。
//!
//! - 设计来源：`.kiro/specs/phone-mic-voice-input/design.md` §4.4
//! - 验收准则：`.kiro/specs/phone-mic-voice-input/requirements.md`
//!   Requirements 3.1（在主界面列出 LAN IPv4）、3.2（多网卡时列出全部可用的 LAN IPv4）

use std::net::{IpAddr, Ipv4Addr};

/// 抽象的网卡描述。
///
/// 该结构体刻意不绑定具体的发现机制（`netdev`、`if-watch`、平台 API 等），
/// 由调用方负责把不同来源的网卡信息转换成统一形式，
/// 这样 [`filter_lan_ipv4`] 就成为完全确定性的纯函数，
/// 便于属性测试覆盖（任务 3.4 / Property 2）。
///
/// 设计来源：design.md §4.4。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkInterface {
    /// 接口名（如 `eth0`、`Wi-Fi`），目前仅作展示与诊断用途。
    pub name: String,
    /// 该接口上观察到的所有 IP 地址，IPv4 / IPv6 不限；
    /// [`filter_lan_ipv4`] 会按此向量的顺序遍历。
    pub addrs: Vec<IpAddr>,
}

/// 判断给定 IPv4 是否落在 RFC1918 私有网段内。
///
/// RFC1918 私有 IPv4 范围：
/// - `10.0.0.0/8`
/// - `172.16.0.0/12`
/// - `192.168.0.0/16`
///
/// 其余范围一律返回 `false`，包括：
/// - loopback `127.0.0.0/8`
/// - link-local `169.254.0.0/16`
/// - CGNAT `100.64.0.0/10`
/// - 全部公网 IPv4
///
/// 验证：Requirements 3.2；设计：design.md §4.4。
#[must_use]
pub fn is_rfc1918(ip: Ipv4Addr) -> bool {
    // 直接基于字节表示判断，避免引入额外依赖。
    let [a, b, _, _] = ip.octets();
    // 10.0.0.0/8
    if a == 10 {
        return true;
    }
    // 172.16.0.0/12 —— 第二字节落在 [16, 31]
    if a == 172 && (16..=31).contains(&b) {
        return true;
    }
    // 192.168.0.0/16
    if a == 192 && b == 168 {
        return true;
    }
    false
}

/// 把网卡列表过滤为可在 LAN 中暴露的 RFC1918 IPv4 列表。
///
/// 行为约定：
/// - **过滤**：仅保留满足 [`is_rfc1918`] 的 IPv4；
///   其他 IPv4（loopback、link-local、CGNAT、公网）
///   以及全部 IPv6 一律剔除；
/// - **稳定排序**：按 `ifaces` 给定顺序遍历，
///   每个接口内再按 `addrs` 顺序遍历，
///   保证主界面上多网卡场景下展示顺序确定且与输入一致；
/// - **去重**：相同 IPv4 仅保留首次出现的位置，
///   避免桥接 / Docker / VPN 重复出现造成 UI 噪声。
///
/// 验证：Requirements 3.1, 3.2；设计：design.md §4.4。
#[must_use]
pub fn filter_lan_ipv4(ifaces: &[NetworkInterface]) -> Vec<Ipv4Addr> {
    let mut out: Vec<Ipv4Addr> = Vec::new();
    for iface in ifaces {
        for addr in &iface.addrs {
            // 跳过 IPv6（含 link-local fe80::、ULA、全局单播等所有形式）。
            let IpAddr::V4(v4) = addr else { continue };
            if !is_rfc1918(*v4) {
                continue;
            }
            // 首次出现才入列，保留稳定排序的同时完成去重。
            if !out.contains(v4) {
                out.push(*v4);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv6Addr;

    /// 构造单一接口的便捷帮手，让测试用例更紧凑。
    fn iface(name: &str, addrs: &[IpAddr]) -> NetworkInterface {
        NetworkInterface {
            name: name.to_owned(),
            addrs: addrs.to_vec(),
        }
    }

    fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(a, b, c, d))
    }

    // —— is_rfc1918 ——

    #[test]
    fn is_rfc1918_accepts_each_subrange() {
        // 10/8 边界
        assert!(is_rfc1918(Ipv4Addr::new(10, 0, 0, 0)));
        assert!(is_rfc1918(Ipv4Addr::new(10, 255, 255, 255)));
        // 172.16/12 边界
        assert!(is_rfc1918(Ipv4Addr::new(172, 16, 0, 0)));
        assert!(is_rfc1918(Ipv4Addr::new(172, 31, 255, 255)));
        // 192.168/16 边界
        assert!(is_rfc1918(Ipv4Addr::new(192, 168, 0, 0)));
        assert!(is_rfc1918(Ipv4Addr::new(192, 168, 255, 255)));
    }

    #[test]
    fn is_rfc1918_rejects_172_15_and_172_32() {
        // 172.16/12 之外：低边界外的 172.15.x.x 与高边界外的 172.32.x.x 均不算私有
        assert!(!is_rfc1918(Ipv4Addr::new(172, 15, 0, 1)));
        assert!(!is_rfc1918(Ipv4Addr::new(172, 32, 0, 1)));
    }

    #[test]
    fn is_rfc1918_rejects_loopback_link_local_cgnat_public() {
        // 127.0.0.0/8 loopback
        assert!(!is_rfc1918(Ipv4Addr::new(127, 0, 0, 1)));
        // 169.254.0.0/16 link-local
        assert!(!is_rfc1918(Ipv4Addr::new(169, 254, 1, 1)));
        // 100.64.0.0/10 CGNAT
        assert!(!is_rfc1918(Ipv4Addr::new(100, 64, 0, 1)));
        assert!(!is_rfc1918(Ipv4Addr::new(100, 127, 255, 254)));
        // 公网示例
        assert!(!is_rfc1918(Ipv4Addr::new(8, 8, 8, 8)));
        assert!(!is_rfc1918(Ipv4Addr::new(1, 1, 1, 1)));
        // 192.169.x.x 不属于 192.168/16
        assert!(!is_rfc1918(Ipv4Addr::new(192, 169, 0, 1)));
    }

    // —— filter_lan_ipv4 ——

    #[test]
    fn filter_keeps_only_rfc1918_ipv4() {
        let ifaces = vec![iface(
            "eth0",
            &[
                v4(192, 168, 1, 10),                                      // 保留
                v4(127, 0, 0, 1),                                         // 排除：loopback
                v4(169, 254, 5, 5),                                       // 排除：link-local
                v4(100, 64, 0, 1),                                        // 排除：CGNAT
                v4(8, 8, 8, 8),                                           // 排除：公网
                IpAddr::V6(Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 1)),   // 排除：IPv6 link-local
                IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)), // 排除：IPv6 公网
                v4(10, 0, 0, 5),                                          // 保留
                v4(172, 16, 0, 1),                                        // 保留
            ],
        )];
        let got = filter_lan_ipv4(&ifaces);
        assert_eq!(
            got,
            vec![
                Ipv4Addr::new(192, 168, 1, 10),
                Ipv4Addr::new(10, 0, 0, 5),
                Ipv4Addr::new(172, 16, 0, 1),
            ]
        );
    }

    #[test]
    fn filter_deduplicates_preserving_first_occurrence() {
        // 同一 IP 在多接口出现：仅保留第一次出现位置（按接口顺序、接口内地址顺序）。
        let ifaces = vec![
            iface("br0", &[v4(192, 168, 1, 10), v4(10, 0, 0, 5)]),
            iface("eth0", &[v4(10, 0, 0, 5), v4(192, 168, 1, 10)]), // 全部重复
            iface("wlan0", &[v4(172, 20, 1, 1)]),                   // 新增
        ];
        let got = filter_lan_ipv4(&ifaces);
        assert_eq!(
            got,
            vec![
                Ipv4Addr::new(192, 168, 1, 10),
                Ipv4Addr::new(10, 0, 0, 5),
                Ipv4Addr::new(172, 20, 1, 1),
            ]
        );
    }

    #[test]
    fn filter_preserves_multi_interface_stable_order() {
        // 多接口、顺序敏感：输出顺序应严格等于"按 ifaces 顺序，再按各自 addrs 顺序"。
        let ifaces = vec![
            iface("eth1", &[v4(172, 16, 0, 2), v4(192, 168, 0, 2)]),
            iface("eth0", &[v4(10, 1, 1, 1)]),
            iface("eth2", &[v4(192, 168, 0, 3)]),
        ];
        let got = filter_lan_ipv4(&ifaces);
        assert_eq!(
            got,
            vec![
                Ipv4Addr::new(172, 16, 0, 2),
                Ipv4Addr::new(192, 168, 0, 2),
                Ipv4Addr::new(10, 1, 1, 1),
                Ipv4Addr::new(192, 168, 0, 3),
            ]
        );
    }

    #[test]
    fn filter_handles_empty_inputs() {
        // 完全空输入。
        assert!(filter_lan_ipv4(&[]).is_empty());
        // 接口存在但全是 IPv6 / 公网：返回空向量，配合 lan_view 触发"未检测到局域网连接"。
        let ifaces = vec![iface(
            "eth0",
            &[
                IpAddr::V6(Ipv6Addr::LOCALHOST),
                v4(8, 8, 8, 8),
                v4(127, 0, 0, 1),
            ],
        )];
        assert!(filter_lan_ipv4(&ifaces).is_empty());
    }
}
