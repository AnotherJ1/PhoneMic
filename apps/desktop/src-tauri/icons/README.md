# PhoneMic 图标资源

任务来源：`.kiro/specs/phone-mic-voice-input/tasks.md` 1.2

`tauri.conf.json` 当前引用以下图标，将在视觉设计就绪后补齐：

| 文件 | 用途 | 备注 |
| ---- | ---- | ---- |
| `icon.png` | 通用 / 托盘图标（Linux + macOS template） | 至少 512×512，PNG with alpha |
| `icon.ico` | Windows 安装包与窗口图标 | 多分辨率 ICO（16/32/48/64/256） |
| `icon.icns` | macOS .app bundle 图标 | 由 `tauri icon` 自动生成 |
| `Square150x150Logo.png` 等 MSI tile | Windows 应用商店瓷贴 | 可选 |

### 临时占位策略

任务 1.2 仅产出工程骨架，未生成最终图标。`cargo tauri build` 会在缺少这些
文件时报错；首次本地打包前请执行：

```sh
# 准备一张 1024x1024 的 PNG 作为源
cargo tauri icon path/to/source.png
```

CLI 会按 `tauri.conf.json` 的 `bundle.icon` 字段在本目录下生成全套图标。

> 占位 PNG 不入库，避免误打包到正式发布产物。
