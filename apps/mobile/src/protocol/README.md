# `apps/mobile/src/protocol/` —— 协议镜像

> 任务来源：`.kiro/specs/phone-mic-voice-input/tasks.md` 2.5
> 设计来源：`.kiro/specs/phone-mic-voice-input/design.md` §5

本目录承载 Rust crate `crates/phonemic-protocol/` 的 TypeScript 镜像，是
桌面端与移动端之间的“单一来源”。Rust 端通过 `serde` 派生序列化字面量；
TS 端以 `interface` / `type` 字面量联合手工镜像，并通过一个 fingerprint
stamp 文件（`.protocol-stamp.json`）守护两端字面量的一致性。

## 文件清单

| 文件 | Rust 源 | 说明 |
| --- | --- | --- |
| `ws.ts` | `ws.rs` | `ClientMessage` / `ServerMessage` 可辨识联合 |
| `http.ts` | `http.rs` | `PairRequest` / `PairResponse` / `HealthResponse` |
| `error.ts` | `error.rs` | `ErrorCode` 联合 + `ERROR_CODES` 常量数组 |
| `error_obj.ts` | `error_obj.rs` | `AppError { code, message, detail?, ts }` |
| `config.ts` | `config.rs` | `AppConfig` 等配置 schema |
| `index.ts` | `lib.rs` | 统一出口 + `PROTOCOL_VERSION` |
| `.protocol-stamp.json` | （由脚本生成） | 整合 Rust + TS fingerprint，CI 校验防漂移 |

## 维护流程

1. 修改 Rust 端类型（例如新增字段或消息）。
2. 同步更新本目录下对应的 TS 文件，保持字段命名 / 默认值与 Rust 端
   严格一致（命名约定：Rust `#[serde(rename_all = "camelCase")]` ⇒ TS
   `camelCase`；显式 `#[serde(rename = "...")]` 一律照搬字面量）。
3. 在仓库根运行 `pnpm gen:ts-types` 重新生成 stamp。
4. 提交 `.protocol-stamp.json` 与对应代码改动；CI 中的
   `pnpm check:ts-types` 会重新计算 fingerprint，若与提交的 stamp 不一致
   则中断流水线，提示开发者补齐镜像。

## 不使用 ts-rs/specta 的理由

- 当前协议表面较小（约 20 个类型），手写镜像质量与可读性更高，注释能
  直接关联 Requirement 与设计章节。
- ts-rs/specta 派生需要在 Rust 端引入额外宏与 build script，跨平台开发体
  验有不确定性；fingerprint 守护已经能拦截最常见的漂移问题。
- 后续若发现镜像维护成本上升，可平滑切换到自动生成方案——TS 接口签名
  保持不变即可。
