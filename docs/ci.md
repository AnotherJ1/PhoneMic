# CI / 持续集成

> 设计来源：`design.md` §9.1 测试层级总览、§9.7 覆盖率与门禁
> 任务来源：`tasks.md` 1.4

PhoneMic 在 GitHub Actions 上运行两条相互独立的 CI 流水线，覆盖 Rust 与前端：

| 工作流文件 | 触发 | 说明 |
|------------|------|------|
| `.github/workflows/ci.yml` | `push` / `pull_request` 至 `main` / `master` / `develop`；`workflow_dispatch` | PR 必经门禁，包含 `rust` 与 `frontend` 两组矩阵 job |
| `.github/workflows/release.yml` | 仅 `workflow_dispatch` | 占位；完整实现位于任务 14.3 |

## 矩阵覆盖

两组 job 都按 `ubuntu-latest` / `windows-latest` / `macos-latest` 三平台执行，
对应 Requirement 1.1 中 Windows / macOS / Linux 的等价支持要求。

| Job | 步骤 | 说明 |
|-----|------|------|
| `rust` | `cargo fmt --all -- --check` | 与 `rustfmt.toml` 对齐 |
| `rust` | `cargo clippy --workspace --all-targets -- -D warnings` | 与 `clippy.toml`、根 `Cargo.toml` 中 `[workspace.lints.clippy]` 对齐 |
| `rust` | `cargo test --workspace --all-features --no-fail-fast` | 单元 + 属性测试 |
| `frontend` | `pnpm install --no-frozen-lockfile` | 脚手架阶段尚未提交锁文件，后续严格化为 `--frozen-lockfile` |
| `frontend` | `pnpm -r --if-present lint` | 各 workspace 包按需提供 `lint` 脚本 |
| `frontend` | `pnpm -r --if-present typecheck` | 同上 |
| `frontend` | `pnpm -r --if-present test` | 同上 |
| `frontend` | `pnpm -r --if-present build` | 同上 |

## 工具链版本来源

- Rust：由根 `rust-toolchain.toml` 锁定（当前 1.75.0），`dtolnay/rust-toolchain@stable` 会读取该文件。
- Node：通过 `actions/setup-node@v4` 固定为 20.x（与 `package.json` `engines.node` 对齐）。
- pnpm：通过 `pnpm/action-setup@v4` 固定为 9.12.3（与 `package.json` `packageManager` 对齐）。

## Linux 下的系统依赖

Tauri 在 Linux 上依赖 GTK / WebKit / libsoup 等动态库。即便当前任务（1.4）阶段
`apps/desktop/src-tauri/` 尚未生成，CI 仍会预先安装这些包，避免任务 1.2 启用 Tauri 时
工作流红屏。如未来确认不再需要，可在 `ci.yml` 中收敛此步骤。

涉及包：

```
libgtk-3-dev libsoup-3.0-dev libwebkit2gtk-4.1-dev librsvg2-dev libudev-dev pkg-config
```

## 在本地复现 CI

```bash
# Rust 部分
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-features --no-fail-fast

# 前端部分（先安装 pnpm 9.12.3）
pnpm install --no-frozen-lockfile
pnpm lint
pnpm typecheck
pnpm test
pnpm build
```

> Linux 用户在执行前端 / Tauri 构建前请先安装上文列出的系统依赖。

## 与发布流水线的关系

本 CI 仅负责 **质量门禁**。三平台安装包打包、签名与 GitHub Release
发布由 `.github/workflows/release.yml` 承担，详见任务 14.3。
