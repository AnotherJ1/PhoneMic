# PhoneMic

让你的手机变成电脑的"无线语音麦克风"。

> 详细需求见 [`.kiro/specs/phone-mic-voice-input/requirements.md`](.kiro/specs/phone-mic-voice-input/requirements.md)
> 详细设计见 [`.kiro/specs/phone-mic-voice-input/design.md`](.kiro/specs/phone-mic-voice-input/design.md)
> 实施计划见 [`.kiro/specs/phone-mic-voice-input/tasks.md`](.kiro/specs/phone-mic-voice-input/tasks.md)

## 仓库布局

```
.
├─ apps/
│  ├─ desktop/        # Tauri 桌面端（任务 1.2 通过 `cargo tauri init` 生成 src-tauri）
│  └─ mobile/         # Vue 3 + Vite + UnoCSS 移动端（任务 1.3 初始化）
├─ crates/
│  ├─ phonemic-app/        # Tauri 进程入口装配层
│  ├─ phonemic-core/       # 业务逻辑（端口选择、Pairing、Web Server、状态机）
│  ├─ phonemic-protocol/   # 桌面 ↔ 移动共享协议与类型
│  ├─ phonemic-injector/   # 跨平台键盘注入抽象
│  ├─ phonemic-discovery/  # mDNS 发现与连接信息广播
│  └─ phonemic-asr/        # Server_ASR 引擎桥接（whisper.cpp）
├─ docs/              # 架构 / 协议 / 安全 / 发布等长文档
├─ scripts/           # 仓库级脚本（如协议类型生成、打包）
├─ Cargo.toml         # workspace 清单
├─ rust-toolchain.toml
├─ rustfmt.toml
└─ clippy.toml
```

## 开发前置

- Rust 工具链：由 `rust-toolchain.toml` 锁定为 `1.75.0`。
- 包管理器（前端）：将在任务 1.4 引入 pnpm workspaces。

## 常用命令

```bash
# 全工作区编译检查
cargo check --workspace

# 格式化与 Lint
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings

# 测试
cargo test --workspace
```
