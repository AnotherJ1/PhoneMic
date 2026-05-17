// PhoneMic 桌面端二进制入口
//
// 任务来源：tasks.md 1.2
// 设计来源：design.md §3.1（Tauri）、§4.1（AppController）
//
// 在 Windows 发行构建中禁用控制台窗口；其余平台不受影响。
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

fn main() {
    phonemic_desktop_lib::run();
}
