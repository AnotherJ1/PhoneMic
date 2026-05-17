// Tauri 构建脚本
//
// 任务来源：tasks.md 1.2
// 设计来源：design.md §3.1
//
// `tauri-build` 会读取相邻的 `tauri.conf.json`，根据其内容：
//   - 在 Windows 上嵌入应用图标 / manifest
//   - 在 macOS 上生成 Info.plist 占位
//   - 校验前端构建产物路径（frontendDist）
// 该文件无需手工修改。

fn main() {
    tauri_build::build();
}
