# `apps/desktop` — PhoneMic 桌面端 Tauri 2.x 工程

> 任务来源：`.kiro/specs/phone-mic-voice-input/tasks.md` 1.2
> 设计来源：`.kiro/specs/phone-mic-voice-input/design.md` §3.1、§3.4、§4.1

本目录承载 PhoneMic 桌面端的 Tauri 2.x 入口工程。Tauri 同时打包前端静态资源（来自
`apps/mobile`，任务 1.3 实现）与 Rust 业务后端（`crates/phonemic-*`）。

## 目录结构

```
apps/desktop/
├── README.md           ← 本文档
└── src-tauri/
    ├── Cargo.toml      ← Tauri 二进制 + 库 crate（被纳入 workspace）
    ├── build.rs        ← 调用 tauri-build 生成平台元数据
    ├── tauri.conf.json ← productName / identifier / 窗口 / bundle 配置
    ├── capabilities/
    │   └── default.json← Tauri 2.x 能力（permissions）声明
    ├── icons/
    │   └── README.md   ← 图标资源说明（占位，待视觉设计补齐）
    └── src/
        ├── main.rs     ← 二进制入口，仅调用 lib::run()
        └── lib.rs      ← Tauri runtime 装配（托盘 + 单实例插件）
```

## 任务 1.2 已完成事项

- [x] 在 `apps/desktop/src-tauri/` 内手工创建 Tauri 2.x 等价骨架（环境无 `cargo tauri` CLI，
      已按官方 `tauri init` 默认布局复制；后续若 CLI 可用，可直接 `cargo tauri info` 验证）。
- [x] `tauri.conf.json` 配置：
  - `productName` = `PhoneMic`
  - `identifier`  = `io.phonemic.desktop`
  - 主窗口 `title` = `PhoneMic`，最小尺寸 720×480
  - `bundle.targets` 覆盖 `deb` / `appimage` / `msi` / `dmg`
  - 最低 OS 版本：
    - Windows 10 — 通过 WebView2 `downloadBootstrapper` 策略保障，WiX `upgradeCode`
      已生成（需求 R1.1）
    - macOS 11 — `bundle.macOS.minimumSystemVersion: "11.0"`（需求 R1.1）
    - Ubuntu 20.04 — Tauri 2 schema 未直接暴露 `minOSVersion`，改为通过
      `bundle.linux.deb.depends` 绑定 `libwebkit2gtk-4.1-0` 等运行库，间接保证
      glibc / GTK 版本满足（需求 R1.1）
- [x] `src-tauri/Cargo.toml` 通过 workspace path 依赖引用 `phonemic-app` 与
      `phonemic-protocol`；`phonemic-app` 内部已聚合 core / injector / discovery / asr。
- [x] 启用托盘：`tauri = { features = ["tray-icon"] }`，`tauri.conf.json` 配置
      `app.trayIcon`。托盘菜单内容由任务 10.x 在 `phonemic-app` 中注入。
- [x] 启用单实例插件：`tauri-plugin-single-instance`，在 `lib.rs::run()` 中注册
      callback，第二个进程启动时把已存在的主窗口聚焦到前台（满足 R2.x"仅一个
      Web Server 监听端口"前置约束）。
- [x] 工作区根 `Cargo.toml` 已把 `apps/desktop/src-tauri` 加入 `members`，
      并在 `[workspace.dependencies]` 集中声明 `tauri`、`tauri-build`、
      `tauri-plugin-single-instance` 的版本。

## 任务 1.2 留作后续完成的事项

| 项 | 何时完成 | 说明 |
| --- | --- | --- |
| `icons/icon.{png,ico,icns}` | 视觉设计就绪 | 占位 README 已说明 `cargo tauri icon` 用法 |
| `resources/web/` 静态产物 | 任务 1.3 | 移动端 Vite 构建直接输出至此（已在 `.gitignore` 忽略） |
| 托盘菜单与 `AppController` | 任务 10.x | 当前 `setup` 仅打印日志 |
| 自定义 Tauri 命令 / IPC | 任务 5.x – 10.x | `default.json` 仅放开 core 默认权限 |
| `cargo tauri info` / `dev` 验证 | CI / 本地开发机 | 需安装 `tauri-cli`（`cargo install tauri-cli --version "^2"`） |

## 本地启动指引（一旦工具链就位）

```sh
# 安装 Tauri 2.x CLI（首次）
cargo install tauri-cli --version "^2.0.0"

# 在仓库根目录运行
cargo tauri dev   # 启动 Vite + Tauri 联调（依赖任务 1.3 的 apps/mobile）
cargo tauri build # 打包当前平台目标
```

> 当前环境未提供 `cargo` 与 `tauri-cli`，因此该 README 描述的命令在 CI（任务 1.4）
> 与开发者本机执行；任务 1.2 仅交付源代码骨架。
