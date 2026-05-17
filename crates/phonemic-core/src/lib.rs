//! `phonemic-core` —— 桌面端业务逻辑核心。
//!
//! 任务 3.x 在该 crate 内逐步落地纯函数与状态机；
//! 任务 5.x（Web Server）、8.x（ASR Bridge）等模块也将在此实现。
//!
//! 设计来源：`.kiro/specs/phone-mic-voice-input/design.md` §4

#![forbid(unsafe_code)]

// 任务 3.1 端口选择
pub mod port_select;
// 任务 3.3 LAN IPv4 过滤
pub mod lan_filter;
// 任务 3.5 连接 URL 渲染
pub mod url_render;
// 任务 3.7 LAN 状态映射
pub mod lan_view;
// 任务 3.9 / 3.11 Pairing_Code 生成与校验
pub mod pairing_code;
// 任务 3.13 配对失败计数器
pub mod pair_rate_limit;
// 任务 3.15 Session 注册表
pub mod session;
// 任务 3.18 Pairing_Code 重启失效服务
pub mod pairing_service;
// 任务 3.20 滚动日志写入器
pub mod rolling_log;
// 任务 3.22 i18n 字典 + locale 决策
pub mod i18n;
