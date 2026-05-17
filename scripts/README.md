# scripts/

仓库级脚本目录。

## 当前脚本

| 脚本 | 任务来源 | 说明 |
| --- | --- | --- |
| `gen-ts-types.mjs` | tasks.md 2.5 | 计算 Rust 协议源 + TS 镜像的 fingerprint，并写入 `apps/mobile/src/protocol/.protocol-stamp.json`。`--check` 模式仅校验，CI 使用。 |
| `lib/protocol-fingerprint.mjs` | tasks.md 2.5 | `gen-ts-types.mjs` 的纯函数库；列出参与 fingerprint 的源文件、提供确定性 SHA-256 计算工具。 |

## 常用命令

```bash
# 生成 / 刷新 stamp（开发者本地，protocol 改动后运行）
pnpm gen:ts-types

# 只校验、不写入（CI 使用）
pnpm check:ts-types
```

## 后续将加入

- 平台打包 / 签名脚本（任务 13.x）
- 性能基准触发脚本（按需）
