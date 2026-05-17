//! PhoneMic 桌面端 Tauri 入口库
//!
//! 任务来源：`.kiro/specs/phone-mic-voice-input/tasks.md` 1.2
//! 设计来源：`.kiro/specs/phone-mic-voice-input/design.md` §3.1、§4.1
//!
//! 本 crate 仅承担 Tauri 2.x 运行时的最小装配工作：
//! 1. 注册 [`tauri_plugin_single_instance`]，确保同一台机器只运行一个 Web Server 实例
//!    （需求 R2.x：仅一个端口监听）。
//! 2. 启用托盘（tray-icon）能力，托盘菜单的具体内容会在任务 10.x 中由 `phonemic-app` 注入。
//! 3. 调用 [`phonemic_app`] 中的 `AppController` 完成 Web Server / Pairing / Discovery
//!    等业务子模块的初始化（任务 10.x 实现）。
//!
//! 当前阶段（任务 1.2）只搭出"能编译、能启动空窗口"的骨架，后续 Wave 会逐步补全。
//!
//! ## 单实例策略
//! 当用户重复双击启动时，[`tauri_plugin_single_instance`] 会把第二个进程的命令行参数
//! 转发给已有进程，触发主窗口聚焦逻辑（在 closure 内实现）。

#![forbid(unsafe_code)]

use tauri::{Builder, Manager};

/// Tauri 应用入口。
///
/// 该函数被 [`crate::main`] 与未来的移动端入口共同使用。
/// 任何 panic 都会被 Tauri runtime 接管并展示给用户。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 初始化结构化日志；级别由 `RUST_LOG` 控制（design §8.4）
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .try_init();

    Builder::default()
        // 单实例插件：避免在同一台机器上启动多个 Web Server，确保端口选择算法
        // （任务 3.1）的不变量在用户层面成立。
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // 已有实例被再次启动时，把主窗口拉到前台
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
                let _ = window.show();
            }
        }))
        .setup(|_app| {
            // 任务 10.x 将在此处装配 AppController：
            //   - Web Server 启动 / 端口选择
            //   - Discovery_Service 注册 mDNS
            //   - Pairing_Service 生成 Pairing_Code
            //   - 托盘菜单与事件回调
            // 当前 1.2 仅做最小骨架，确保 `cargo tauri dev` 能拉起空窗口。
            tracing::info!("PhoneMic desktop shell initialized (skeleton only)");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
