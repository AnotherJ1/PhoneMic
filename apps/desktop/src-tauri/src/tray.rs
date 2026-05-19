//! 托盘菜单（任务 10.6）。
//!
//! 设计来源：design.md §4.1 与 design §6.1。
//!
//! 菜单项：
//! - 「显示主窗口」 → unminimize + show + focus 主窗口；
//! - 「暂停注入」/「恢复注入」 → 切换 [`InputInjector::pause`] 并刷新 label；
//! - 「重启服务」 → 通过事件让 Web Server 重启（先广播 `phonemic://restart-requested`）；
//! - 「退出」 → 清理后调用 `app.exit(0)`。

use std::sync::Arc;

use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager};

use crate::app_state::DesktopState;

const ID_SHOW: &str = "phonemic.tray.show";
const ID_TOGGLE_PAUSE: &str = "phonemic.tray.toggle_pause";
const ID_RESTART: &str = "phonemic.tray.restart";
const ID_QUIT: &str = "phonemic.tray.quit";

/// 在 [`AppHandle`] 上安装托盘图标与菜单。
///
/// # Errors
/// 当托盘图标 / 菜单构造失败时返回错误。
pub fn install_tray(app: &AppHandle, state: Arc<DesktopState>) -> tauri::Result<()> {
    let pause_label = if state.config().input.paused {
        "Resume injection"
    } else {
        "Pause injection"
    };

    let show_item = MenuItemBuilder::with_id(ID_SHOW, "Show main window").build(app)?;
    let pause_item = MenuItemBuilder::with_id(ID_TOGGLE_PAUSE, pause_label).build(app)?;
    let restart_item = MenuItemBuilder::with_id(ID_RESTART, "Restart service").build(app)?;
    let quit_item = MenuItemBuilder::with_id(ID_QUIT, "Quit").build(app)?;

    let menu = MenuBuilder::new(app)
        .items(&[&show_item, &pause_item, &restart_item, &quit_item])
        .build()?;

    let app_clone = app.clone();
    let _icon = TrayIconBuilder::with_id("phonemic-tray")
        .menu(&menu)
        .on_menu_event(move |app, event| {
            handle_menu_event(app, &state, event.id().as_ref());
            // 重新计算暂停标签（仅在切换时刷新）。
            if event.id().as_ref() == ID_TOGGLE_PAUSE {
                refresh_pause_label(&app_clone, &state);
            }
        })
        .tooltip("PhoneMic")
        .build(app)?;

    Ok(())
}

fn handle_menu_event(app: &AppHandle, state: &Arc<DesktopState>, id: &str) {
    match id {
        ID_SHOW => {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }
        ID_TOGGLE_PAUSE => {
            let new_paused = !state.config().input.paused;
            state.set_inject_paused(new_paused);
            let _ = app.emit(
                "phonemic://inject-paused-changed",
                serde_json::json!({ "paused": new_paused }),
            );
        }
        ID_RESTART => {
            let _ = app.emit("phonemic://restart-requested", serde_json::json!({}));
        }
        ID_QUIT => {
            app.exit(0);
        }
        _ => {}
    }
}

fn refresh_pause_label(_app: &AppHandle, _state: &Arc<DesktopState>) {
    // tauri 2.x 的 MenuItem.set_text 在多平台上行为各异；当前 MVP 通过
    // `phonemic://inject-paused-changed` 事件让前端 / 下次菜单展开自行
    // 决定文案。真实 label 切换将在任务 10.6 集成测试中补齐。
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 编译期验证：菜单项 ID 常量保持稳定。
    #[test]
    fn ids_are_stable() {
        assert_eq!(ID_SHOW, "phonemic.tray.show");
        assert_eq!(ID_TOGGLE_PAUSE, "phonemic.tray.toggle_pause");
        assert_eq!(ID_RESTART, "phonemic.tray.restart");
        assert_eq!(ID_QUIT, "phonemic.tray.quit");
    }
}

#[allow(dead_code)]
fn _link_menu_builder() {}
