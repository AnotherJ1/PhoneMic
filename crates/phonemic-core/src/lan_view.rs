//! 任务 3.7：LAN 状态映射函数 `compute_lan_view`。
//!
//! 把网卡集合 + 当前 UI 语言映射成主界面"可扫码 / 不可扫码"两态视图：
//! - 当 [`crate::lan_filter::filter_lan_ipv4`] 过滤后为空集时，
//!   主界面禁用扫码区域并展示"未检测到局域网连接"提示
//!   （文案由 [`crate::i18n`] 按 locale 替换，对应字典 key
//!   `app.banner.no_lan`）；
//! - 否则下发可用 IPv4 列表，由上层渲染连接 URL 与 QR_Code。
//!
//! 该函数刻意保持纯函数形式：输入只来自 [`NetworkInterface`] 切片与
//! [`Lang`]，不读时间、不读环境变量、不打开 socket，便于在
//! `Discovery_Service` 上报接口变化时被无副作用地多次重算
//! （参见 design.md §4.4）。同样满足 design.md §7 Property 5 的
//! 确定性前提：相同输入恒得相同输出。
//!
//! - 设计来源：`.kiro/specs/phone-mic-voice-input/design.md` §4.4、§7 Property 5
//! - 验收准则：requirements.md
//!   Requirements 3.5（接口变化时主界面 5 秒内刷新）、
//!   Requirements 3.6（无 LAN 时显示"未检测到局域网连接"并禁用扫码区域）

use std::net::Ipv4Addr;

use crate::i18n::{Lang, t};
use crate::lan_filter::{NetworkInterface, filter_lan_ipv4};

/// 字典缺失时的英文兜底文案。
///
/// 仅在内嵌字典损坏 / 被裁剪到不含 `app.banner.no_lan` 这种异常路径下使用，
/// 正常构建中字典完整性由任务 3.24 的测试保证；保留该常量是为了让
/// `compute_lan_view` 在最坏情况下仍能对 UI 给出可读字符串而不返回 `None`。
const NO_LAN_BANNER_FALLBACK: &str = "No LAN detected";

/// 主界面 LAN 状态视图。
///
/// 字段对应 design.md §4.4 中描述的"扫码区域 + IP 列表 + 提示语"三元组：
/// - `scan_disabled`：UI 是否需要禁用扫码 / 配对入口（Requirements 3.6）；
/// - `ips`：当前可用的 RFC1918 IPv4 列表，顺序与 `filter_lan_ipv4` 一致；
/// - `banner`：可选的提示文案，仅在无可用 LAN 时为 `Some`。
///
/// `scan_disabled` 与 `banner.is_some()` 在本函数下严格等价，
/// 保留两个字段是为了让 UI 层无需重复判空，
/// 也方便未来扩展为多种 banner（如"网卡正在初始化…"）时不破坏调用方。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanView {
    /// 是否禁用主界面的扫码区域（Requirements 3.6）。
    pub scan_disabled: bool,
    /// 可暴露给手机端的 RFC1918 IPv4 列表，可能为空。
    pub ips: Vec<Ipv4Addr>,
    /// 主界面顶部 banner 文案，仅当 `ips` 为空时携带。
    pub banner: Option<String>,
}

/// 根据网卡集合与当前 UI 语言计算主界面的 LAN 状态视图。
///
/// 行为约定：
/// - 先调用 [`filter_lan_ipv4`] 完成过滤 / 去重 / 稳定排序；
/// - 若结果为空：返回 `scan_disabled = true`、`ips = []`、
///   `banner = Some(<i18n("app.banner.no_lan")>)`，对应 Requirements 3.6；
///   字典缺失时回退到英文常量 `"No LAN detected"`，避免 banner 消失导致
///   UI 误判为"扫码可用"；
/// - 否则返回 `scan_disabled = false`、`ips = <filtered>`、`banner = None`，
///   交由上层按 Requirements 3.5 在接口变化时刷新 URL 与 QR_Code。
///
/// 该函数对同一 `(ifaces, lang)` 输入是确定性的，可被属性测试反复调用
/// （参见 design.md §7 Property 5）。
///
/// 验证：Requirements 3.5, 3.6；设计：design.md §4.4、§7 Property 5。
#[must_use]
pub fn compute_lan_view(ifaces: &[NetworkInterface], lang: Lang) -> LanView {
    let ips = filter_lan_ipv4(ifaces);
    if ips.is_empty() {
        let banner = t(lang, "app.banner.no_lan")
            .unwrap_or(NO_LAN_BANNER_FALLBACK)
            .to_owned();
        LanView {
            scan_disabled: true,
            ips: Vec::new(),
            banner: Some(banner),
        }
    } else {
        LanView {
            scan_disabled: false,
            ips,
            banner: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv6Addr};

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

    /// 取得字典中 `app.banner.no_lan` 的真实文案，用于测试断言；
    /// 与生产代码同一来源，保证字典更新时测试自动跟随。
    fn expected_banner(lang: Lang) -> &'static str {
        t(lang, "app.banner.no_lan").expect("字典必须包含 app.banner.no_lan")
    }

    /// 空网卡集合：禁用扫码 + 展示 zh-CN 文案。
    /// Requirements 3.6。
    #[test]
    fn empty_interfaces_disable_scan_with_zh_cn_banner() {
        let view = compute_lan_view(&[], Lang::ZhCN);
        assert!(view.scan_disabled);
        assert!(view.ips.is_empty());
        assert_eq!(view.banner.as_deref(), Some(expected_banner(Lang::ZhCN)));
        // 额外校验取到的是中文字典，避免回退到英文兜底而无人察觉。
        assert!(view.banner.as_deref().unwrap().contains("局域网"));
    }

    /// 空网卡集合 + en-US：禁用扫码 + 展示英文文案。
    /// Requirements 3.6 + R8.1（双语字典覆盖）。
    #[test]
    fn empty_interfaces_disable_scan_with_en_us_banner() {
        let view = compute_lan_view(&[], Lang::EnUS);
        assert!(view.scan_disabled);
        assert!(view.ips.is_empty());
        assert_eq!(view.banner.as_deref(), Some(expected_banner(Lang::EnUS)));
        // 英文文案应以 "No LAN" 起头，区别于中文字典。
        assert!(view.banner.as_deref().unwrap().contains("LAN"));
    }

    /// 接口存在但只有 IPv6：等价于"无可用 LAN"，banner 仍按语言渲染。
    /// Requirements 3.6。
    #[test]
    fn ipv6_only_interfaces_disable_scan_with_banner() {
        let ifaces = vec![iface(
            "eth0",
            &[
                IpAddr::V6(Ipv6Addr::LOCALHOST),
                IpAddr::V6(Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 1)),
                IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)),
            ],
        )];
        let view = compute_lan_view(&ifaces, Lang::ZhCN);
        assert!(view.scan_disabled);
        assert!(view.ips.is_empty());
        assert_eq!(view.banner.as_deref(), Some(expected_banner(Lang::ZhCN)));
    }

    /// 全部为公网 / 非私有 IPv4：同样视为"无 LAN"。
    /// Requirements 3.6。
    #[test]
    fn all_public_ipv4_disable_scan_with_banner() {
        let ifaces = vec![iface(
            "wan0",
            &[v4(8, 8, 8, 8), v4(1, 1, 1, 1), v4(100, 64, 0, 1)],
        )];
        let view = compute_lan_view(&ifaces, Lang::EnUS);
        assert!(view.scan_disabled);
        assert!(view.ips.is_empty());
        assert_eq!(view.banner.as_deref(), Some(expected_banner(Lang::EnUS)));
    }

    /// 仅有 loopback 也应判为"无 LAN"。
    /// Requirements 3.6。
    #[test]
    fn loopback_only_disable_scan_with_banner() {
        let ifaces = vec![iface("lo", &[v4(127, 0, 0, 1)])];
        let view = compute_lan_view(&ifaces, Lang::ZhCN);
        assert!(view.scan_disabled);
        assert!(view.ips.is_empty());
        assert_eq!(view.banner.as_deref(), Some(expected_banner(Lang::ZhCN)));
    }

    /// 公网与 RFC1918 混合：保留私有 IPv4，banner 为 None，扫码可用。
    /// 顺序应严格等于 `filter_lan_ipv4` 的输出顺序（多接口稳定排序）。
    /// Requirements 3.5（接口变化后视图反映过滤结果）。
    #[test]
    fn mixed_with_rfc1918_enables_scan_without_banner() {
        let ifaces = vec![
            iface(
                "eth0",
                &[
                    v4(8, 8, 8, 8),      // 排除：公网
                    v4(192, 168, 1, 10), // 保留
                    v4(127, 0, 0, 1),    // 排除：loopback
                ],
            ),
            iface("wlan0", &[v4(10, 0, 0, 5), v4(169, 254, 1, 1)]), // 保留 + 排除 link-local
            iface("eth1", &[v4(172, 16, 0, 2)]),                    // 保留
        ];
        let view = compute_lan_view(&ifaces, Lang::EnUS);
        assert!(!view.scan_disabled);
        assert_eq!(view.banner, None);
        // 顺序与 filter_lan_ipv4 保持一致：按 ifaces 顺序，再按 addrs 顺序。
        assert_eq!(
            view.ips,
            filter_lan_ipv4(&ifaces),
            "lan_view 必须复用 filter_lan_ipv4 的稳定排序"
        );
        assert_eq!(
            view.ips,
            vec![
                Ipv4Addr::new(192, 168, 1, 10),
                Ipv4Addr::new(10, 0, 0, 5),
                Ipv4Addr::new(172, 16, 0, 2),
            ]
        );
    }

    /// 非空 LAN 时 banner 始终为 None，无论 UI 语言。
    /// Requirements 3.5、3.6。
    #[test]
    fn non_empty_lan_has_no_banner_in_any_language() {
        let ifaces = vec![iface("eth0", &[v4(192, 168, 0, 2)])];
        for lang in [Lang::ZhCN, Lang::EnUS] {
            let view = compute_lan_view(&ifaces, lang);
            assert!(!view.scan_disabled);
            assert_eq!(view.banner, None);
            assert_eq!(view.ips, vec![Ipv4Addr::new(192, 168, 0, 2)]);
        }
    }

    /// `scan_disabled` 与 `banner.is_some()` 在所有用例下保持等价。
    /// 这里覆盖正反两种情形，作为字段一致性的回归保护。
    #[test]
    fn scan_disabled_matches_banner_presence() {
        let empty = compute_lan_view(&[], Lang::ZhCN);
        assert_eq!(empty.scan_disabled, empty.banner.is_some());

        let lan = compute_lan_view(&[iface("eth0", &[v4(192, 168, 0, 2)])], Lang::EnUS);
        assert_eq!(lan.scan_disabled, lan.banner.is_some());
    }

    /// 同一输入的多次调用结果完全一致（Property 5 的确定性前提）。
    #[test]
    fn compute_lan_view_is_deterministic() {
        let ifaces = vec![
            iface("eth0", &[v4(192, 168, 1, 10), v4(8, 8, 8, 8)]),
            iface("wlan0", &[v4(10, 0, 0, 5)]),
        ];
        for lang in [Lang::ZhCN, Lang::EnUS] {
            let a = compute_lan_view(&ifaces, lang);
            let b = compute_lan_view(&ifaces, lang);
            assert_eq!(a, b);
        }
    }
}
