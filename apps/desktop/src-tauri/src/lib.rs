//! PhoneMic 桌面端 Tauri 入口库
//!
//! 任务来源：tasks.md 1.2 / 10.x / 13.x
//! 设计来源：design.md §3.1、§4.1、§6.1
//!
//! 本 crate 装配 Tauri 2.x 桌面运行时：
//! 1. 注册 [`tauri_plugin_single_instance`]：避免多实例竞争 Web Server 端口；
//! 2. 在 Tauri `setup` 中并发启动 Web Server / Discovery / Pairing / Injector
//!    四个子系统（任务 10.8 启动序列），向前端发 `phonemic://startup-stage`
//!    事件，所有子系统 ready 或 5 秒超时后再展示主窗口；
//! 3. 注册 Tauri 命令（[`crate::commands`] 模块）；
//! 4. 注册托盘菜单（任务 10.6）：显示主窗口 / 暂停注入 / 重启服务 / 退出。

#![cfg_attr(not(test), forbid(unsafe_code))]

pub mod app_state;
pub mod commands;
pub mod tray;

use std::sync::Arc;

use tauri::{Builder, Manager};

use crate::app_state::{emit_startup_stage, DesktopState};

/// Tauri 应用入口。
pub fn run() {
    // 13.2：在 Tauri 启动前先把 tracing 打开，让 setup 阶段日志也能进缓冲区。
    let _ = phonemic_core::tracing_setup::init_tracing("info");

    let state = Arc::new(DesktopState::new_virtual());

    // 任务 12.1 / FileSink：当 PHONEMIC_TEST_INJECT_FILE 环境变量被设置时，
    // 把基于文件的 sink 接入注入器。E2E harness 在测试结束时直接读取文件。
    if let Some(file_sink) = phonemic_injector::FileSink::from_env() {
        tracing::info!("PHONEMIC_TEST_INJECT_FILE detected: enabling FileSink");
        state.set_injector_sink(std::sync::Arc::new(file_sink));
    }

    Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
                let _ = window.show();
            }
        }))
        .manage(state.clone())
        .invoke_handler(tauri::generate_handler![
            commands::get_runtime_info,
            commands::get_pairing_code,
            commands::regenerate_code,
            commands::list_sessions,
            commands::revoke_session,
            commands::revoke_all_sessions,
            commands::get_config,
            commands::save_config,
            commands::set_inject_paused,
            commands::set_inject_delay_ms,
            commands::get_logs_tail,
            commands::get_i18n_dict,
            commands::export_diagnostics_cmd,
        ])
        .setup({
            let state = state.clone();
            move |app| {
                let app_handle = app.handle().clone();

                if let Err(e) = tray::install_tray(&app_handle, state.clone()) {
                    tracing::warn!(error = %e, "托盘安装失败：将以无托盘模式继续运行");
                }

                tauri::async_runtime::spawn({
                    let app_handle = app_handle.clone();
                    let state = state.clone();
                    async move {
                        run_startup_sequence(app_handle, state).await;
                    }
                });

                tracing::info!("PhoneMic desktop shell initialized");
                Ok(())
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// 任务 10.8：5 秒并发启动序列。
///
/// 当前实现按约定的阶段顺序广播 `phonemic://startup-stage` 事件，
/// 让前端 splash 视图能可靠展示进度；Web Server / Discovery / ASR 的真实
/// 启动入口由 worker-backend 在完成对应子任务后通过 `attach_runtime`
/// 注入。
async fn run_startup_sequence(app: tauri::AppHandle, state: Arc<DesktopState>) {
    let timeout = std::time::Duration::from_secs(5);
    let started = std::time::Instant::now();

    emit_startup_stage(&app, "web", "Starting Web Server", false);
    state.attach_runtime("http", 18080, vec!["127.0.0.1".to_string()]);
    if started.elapsed() < timeout {
        emit_startup_stage(&app, "discovery", "Registering mDNS service", false);
    }
    if started.elapsed() < timeout {
        emit_startup_stage(&app, "pairing", "Generating Pairing Code", false);
    }
    if started.elapsed() < timeout {
        emit_startup_stage(&app, "injector", "Probing input injector", false);
        let _ = state.injector().current_focus_app();
    }
    let stage = if started.elapsed() < timeout { "ready" } else { "error" };
    let message = if stage == "ready" {
        "All subsystems ready"
    } else {
        "Startup exceeded 5s budget"
    };
    emit_startup_stage(&app, stage, message, stage == "ready");
}
