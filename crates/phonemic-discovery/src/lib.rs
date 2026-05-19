//! `phonemic-discovery` —— 局域网服务发现与连接信息渲染。
//!
//! 任务 6.x：
//! - 6.1 mDNS 注册（`_phonemic._tcp.local.`）+ 接口变化刷新通告
//! - 6.2 LanLost / LanRestored 事件通过 [`phonemic_core::bridge_events::BridgeEventTx`]
//!   投递到桌面端 UI（与 Web Server 共用同一通道）
//! - 6.3 `qr_encode` 二维码 SVG 渲染 + `qr_decode_for_test` 测试桥
//! - 6.4 / 6.5 见 `qr` / `mdns` 模块的属性与集成测试
//!
//! ## 关于事件通道（6.2 决策）
//!
//! 设计文档允许 Discovery 使用"自有通道"再由总线转发；本实现选择**直接复用
//! Web Server 的 BridgeEvents 通道**，理由：
//! 1. LAN 事件最终消费者只有桌面 UI，与配对 / 注入事件同源；
//! 2. 减少桥接代码，无需在 `phonemic-app` 中维护多组转发逻辑；
//! 3. 测试中用 `BridgeEventTx::raw` 即可拿到底层 mpsc，便于断言。

#![forbid(unsafe_code)]

pub mod mdns;
pub mod qr;

pub use mdns::{Discovery, DiscoveryCfg, DiscoveryError};
pub use qr::{build_pair_url, parse_pair_url, qr_encode, QrError};
