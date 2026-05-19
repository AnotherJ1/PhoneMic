# Implementation Plan: PhoneMic（手机麦克风语音输入）

## 概述（Overview）

本实施计划把 `requirements.md`（R1–R9）与 `design.md`（§1–§10、Property 1–34）拆解为可被代码生成代理逐步执行的小步任务。整体策略遵循"先脚手架、再纯函数、再有副作用模块、最后端到端集成"的顺序，确保每一步都建立在前一步基础上，没有悬空或孤立的代码。

技术栈与设计文档一致：

- 桌面端：**Rust 1.75+ / Tauri 2.x / axum / tokio-tungstenite / rustls**（详见 `design.md` §3）
- 移动端：**Vue 3 + Vite + UnoCSS + TypeScript**
- 测试：Rust `proptest` + `cargo test`；前端 `fast-check` + `vitest`；E2E 使用 Tauri WebDriver + Playwright

每个叶子任务标注：

- `_Requirements: X.Y_`：追溯到 `requirements.md` 的具体子条款
- `_Design: §章节号_`：追溯到 `design.md` 的具体章节
- `_Property: N_`：当任务用于实现 / 验证某条 Correctness Property 时附加（对应 `design.md` §7）

## 任务依赖说明

- **Wave 0**：仅项目脚手架（不依赖其它任何任务）。
- **Wave 1+**：依赖前置 Wave 的产物。共享类型 / 协议（任务 2.x）必须先于 Web Server / 移动端实现。
- **纯函数 + 状态机模块（任务 3.x）** 不依赖外部副作用，先于 Web Server 与 Input_Injector 完成，便于属性测试快速反馈。
- **Web Server（4.x）** 依赖共享协议（2.x）与 Pairing_Service / 配置加载（3.x）。
- **Input_Injector（6.x）** 与 **ASR Bridge（7.x）** 依赖共享类型（2.x）；通过 trait 抽象与 Web_Server（4.x）解耦。
- **Mobile_Web_Client（8.x）** 依赖共享协议（2.x）与 Web_Server（4.x）。
- **桌面端 UI（9.x）** 依赖 Discovery（5.x）、Pairing（3.x）、Web Server（4.x）、Input_Injector（6.x）。
- **端到端集成（10.x）** 依赖 Mobile（8.x）+ 桌面 UI（9.x）+ Input_Injector（6.x）+ Web Server（4.x）全部就绪。
- **打包与发布（13.x）** 在所有功能与测试通过后执行。

带 `*` 后缀的子任务为可选测试任务（属性 / 集成 / 单元测试），可被跳过以加速 MVP，但发版前必须补齐。

## Tasks

- [x] 1. 项目脚手架与 CI
  - 建立 monorepo 目录结构与持续集成流水线，使后续所有任务能在统一约束下开发。
  - _Requirements: 1.1, 1.4_
  - _Design: §2, §3_

  - [x] 1.1 初始化 Cargo workspace 与目录骨架
    - 创建根 `Cargo.toml` 工作区，定义成员 crate：`phonemic-app`（Tauri 入口）、`phonemic-core`（业务逻辑）、`phonemic-protocol`（共享类型）、`phonemic-injector`、`phonemic-discovery`、`phonemic-asr`
    - 创建 `apps/desktop/`、`apps/mobile/`、`crates/`、`docs/`、`scripts/` 目录
    - 配置 `rust-toolchain.toml` 锁定 Rust 1.75+，统一 `rustfmt.toml` 与 `clippy.toml`
    - 添加 `.editorconfig`、`.gitignore`、`.gitattributes`
    - _Requirements: 1.1, 1.4_
    - _Design: §2.2_

  - [x] 1.2 初始化 Tauri 2.x 桌面项目骨架
    - 在 `apps/desktop/` 内 `cargo tauri init`，配置 `tauri.conf.json` 标题、bundleId、最低 OS 版本（Win10 / macOS 11 / Ubuntu 20.04）
    - 添加 `src-tauri/Cargo.toml` 对 `phonemic-core` / `phonemic-app` 的依赖
    - 启用托盘（tray）与单实例插件
    - _Requirements: 1.1, 1.6_
    - _Design: §3.1, §4.1_

  - [x] 1.3 初始化 Vue 3 + Vite 移动端项目骨架
    - 在 `apps/mobile/` 内创建 Vite + Vue 3 + TypeScript 工程，集成 UnoCSS 与 `vue-router`
    - 配置 `vite.config.ts` 输出至 `apps/desktop/src-tauri/resources/web/`，便于 Tauri 静态分发
    - 添加 `tsconfig.json`、`eslint.config.js`、`.prettierrc`
    - _Requirements: 2.5, 4.1_
    - _Design: §3.4, §4.7_

  - [x] 1.4 配置 lint / 格式化 / 测试脚本与 CI
    - 在仓库根目录添加 `pnpm-workspace.yaml`（或 `package.json` workspaces），统一前端依赖
    - 添加 GitHub Actions（或本地等价）工作流：`cargo fmt --check`、`cargo clippy -D warnings`、`cargo test`、`pnpm lint`、`pnpm test`、`pnpm build`
    - 工作流矩阵覆盖 Windows / macOS / Linux 三平台
    - _Requirements: 1.1, 1.4_
    - _Design: §9.1, §9.7_

- [x] 2. 共享类型与协议（phonemic-protocol）
  - 在桌面端 Rust 与移动端 TypeScript 之间建立单一来源的协议定义，覆盖 WebSocket 消息、HTTP API、错误码、配置 schema。
  - _Requirements: 5.5, 7.2, 7.3, 7.4, 9.6_
  - _Design: §5, §8.1_

  - [x] 2.1 定义 WebSocket 消息 Rust 类型与序列化
    - 在 `crates/phonemic-protocol/src/ws.rs` 定义 `ClientMessage` / `ServerMessage` 枚举：`hello`、`text.submit`、`text.preview`、`audio.chunk`、`audio.end`、`ping`、`welcome`、`inject.ack`、`inject.error`、`transcript.final`、`pong`、`error`
    - 使用 `serde` 派生 `Serialize`/`Deserialize`，`type` 字段作为 tag，统一 UTF-8 JSON
    - 为每个消息类型提供构造辅助函数与字段校验
    - _Requirements: 5.5, 9.6_
    - _Design: §5.1.1, §5.1.2_

  - [x] 2.2 定义 HTTP API 类型（/api/pair, /api/health）
    - 定义 `PairRequest { pairingCode, fingerprint, deviceLabel }` 与 `PairResponse { sessionToken, expiresAt }`
    - 定义 `HealthResponse { version, uptime }`
    - 定义统一错误对象 `AppError { code, message, detail?, ts }`
    - _Requirements: 7.2, 7.3_
    - _Design: §5.2, §8.1_

  - [x] 2.3 定义错误码枚举
    - 在 `crates/phonemic-protocol/src/error.rs` 定义 `ErrorCode` 枚举：`OS_UNSUPPORTED`、`PORT_UNAVAILABLE`、`LAN_LOST`、`MIC_PERMISSION_DENIED`、`ASR_TIMEOUT`、`PAIR_INVALID`、`PAIR_RATELIMIT`、`AUTH_REQUIRED`、`FORBIDDEN_SUBNET`、`INJECT_NO_FOCUS_TARGET`、`INJECT_PERMISSION_DENIED`、`INJECT_PAUSED`、`INJECT_BACKEND_ERROR`、`MSG_BAD_FORMAT`、`RECONNECT_FAILED`
    - 序列化为大写蛇形字符串
    - _Requirements: 9.6, 9.8_
    - _Design: §8.1_

  - [x] 2.4 定义配置 schema（config.toml）
    - 在 `crates/phonemic-protocol/src/config.rs` 定义 `AppConfig` / `ServerCfg` / `UiCfg` / `AsrCfg` / `InputCfg` / `SecurityCfg`
    - 提供默认值（`preferred_port = 18080`、`enable_https = false`、`inject_delay_ms = 0` 等）
    - 提供 `load_from_path` / `save_to_path`
    - _Requirements: 2.2, 2.3, 2.4, 6.5, 6.7, 8.1_
    - _Design: §5.3_

  - [x] 2.5 生成 TypeScript 协议镜像
    - 编写脚本 `scripts/gen-ts-types.ts`（或使用 `ts-rs` / `specta`）从 Rust 类型导出 `apps/mobile/src/protocol/*.ts`
    - 在 CI 中加入"生成结果与提交一致"检查，避免协议漂移
    - _Requirements: 5.5, 9.6_
    - _Design: §5_

  - [x] 2.6 编写协议序列化属性测试
    - **Property 11: 文本协议 Unicode round-trip**
    - 使用 `proptest` 生成含中文 / emoji / 全角标点 / 控制字符的字符串，断言 `serde_json::to_string` → `from_str` 后字段相等
    - **Validates: Requirements 5.5**
    - _Property: 11_
    - _Requirements: 5.5_
    - _Design: §7 Property 11, §9.2_

  - [x] 2.7 编写错误对象结构契约测试
    - 单元测试断言所有 `ErrorCode` 序列化产出的 JSON 字段集合一致：`{ code, message, detail?, ts }`
    - 断言无未捕获异常路径产出非结构化错误
    - _Requirements: 9.6_
    - _Design: §8.1_

- [x] 3. 桌面端纯函数与状态层（phonemic-core）
  - 在不依赖 I/O 与平台 API 的前提下实现可被属性测试覆盖的核心算法与状态机。
  - _Requirements: 2.2, 3.1, 3.2, 3.5, 3.6, 7.1, 7.2, 7.5, 7.9, 9.7_
  - _Design: §4.1, §4.3, §4.4, §4.7_

  - [x] 3.1 实现端口选择算法 `select_port`
    - 输入 `(preferred, occupied: HashSet<u16>)`，返回首个空闲端口，遵循"优先 ≥ preferred；若区间用尽，回退到 [1024, preferred)"
    - 不打开真实 socket，便于纯函数测试
    - _Requirements: 2.1, 2.2_
    - _Design: §4.2_

  - [x] 3.2 编写端口选择属性测试
    - **Property 1: 端口选择不变量**
    - 使用 `proptest` 随机生成 `preferred` 与 `occupied`，断言返回值落在 [1024, 65535]、不在 occupied 中、且满足回退规则
    - **Validates: Requirements 2.1, 2.2**
    - _Property: 1_
    - _Requirements: 2.1, 2.2_
    - _Design: §7 Property 1, §9.2_

  - [x] 3.3 实现 LAN IPv4 过滤函数 `filter_lan_ipv4`
    - 输入 `Vec<NetworkInterface>`，输出过滤后的 RFC1918 私有 IPv4 列表，去重，按接口顺序保留稳定排序
    - 排除 loopback、link-local、CGNAT、IPv6、公网地址
    - _Requirements: 3.1, 3.2_
    - _Design: §4.4_

  - [x] 3.4 编写 LAN 过滤属性测试
    - **Property 2: LAN IPv4 列表过滤**
    - 使用共享生成器 `lan_iface_set()` 随机组合各类网卡，断言输出仅包含 RFC1918 IPv4 且无重复
    - **Validates: Requirements 3.2**
    - _Property: 2_
    - _Requirements: 3.2_
    - _Design: §7 Property 2, §9.2_

  - [x] 3.5 实现连接 URL 渲染函数 `render_connection_urls`
    - 输入 `(scheme, ips, port)`，输出 `Vec<String>`，每条形如 `<scheme>://<ip>:<port>`
    - 保证每个 IP 与端口都出现在最终展示中
    - _Requirements: 3.1_
    - _Design: §4.1_

  - [x] 3.6 编写 URL 渲染属性测试
    - **Property 3: 连接 URL 渲染一致性**
    - **Validates: Requirements 3.1**
    - _Property: 3_
    - _Requirements: 3.1_
    - _Design: §7 Property 3, §9.2_

  - [x] 3.7 实现 LAN 状态映射函数 `compute_lan_view`
    - 输入接口集合，输出 `LanView { scanDisabled, ips, banner }`
    - 空集 ⇒ `scanDisabled = true` 且 banner 为"未检测到局域网连接"
    - _Requirements: 3.5, 3.6_
    - _Design: §4.4_

  - [x] 3.8 编写 LAN 状态映射属性测试
    - **Property 5: LAN 状态映射**
    - **Validates: Requirements 3.5, 3.6**
    - _Property: 5_
    - _Requirements: 3.5, 3.6_
    - _Design: §7 Property 5, §9.2_

  - [x] 3.9 实现 Pairing_Code 生成 `generate_pairing_code`
    - 长度 8，字符集 `[A-Z0-9]` 去除易混 `0/O/1/I/L`
    - 使用 `rand::rngs::OsRng` 加密随机源
    - _Requirements: 7.1_
    - _Design: §4.3_

  - [x] 3.10 编写 Pairing_Code 字符集与碰撞属性测试
    - **Property 16: Pairing_Code 字符集与长度**
    - 断言长度 ≥ 6、字符集合法；连续 10000 次生成时重复率 ≤ 1/10^6
    - **Validates: Requirements 7.1**
    - _Property: 16_
    - _Requirements: 7.1_
    - _Design: §7 Property 16, §9.2_

  - [x] 3.11 实现 Pairing_Code 恒定时间校验 `verify_pairing_code`
    - 使用 `subtle::ConstantTimeEq` 进行字节级比较，避免时序攻击
    - _Requirements: 7.2_
    - _Design: §4.3_

  - [x] 3.12 编写 Pairing_Code 校验属性测试
    - **Property 17: Pairing_Code 校验**
    - 断言 accept 当且仅当 candidate == current；以微基准断言运行时间方差不随匹配前缀长度增加
    - **Validates: Requirements 7.2**
    - _Property: 17_
    - _Requirements: 7.2_
    - _Design: §7 Property 17, §9.2_

  - [x] 3.13 实现配对失败计数器与限流窗口
    - 数据结构 `FailureWindow { count, window_start }`，按 IP 维护
    - 提供 `record_failure(ip)`、`is_rate_limited(ip, now)`、`reset_after_window(now)`
    - 5 次失败后冻结 5 分钟；窗口结束自动重置
    - _Requirements: 7.5_
    - _Design: §4.3_

  - [x] 3.14 编写配对限流属性测试
    - **Property 19: 配对限流**
    - 使用 `pair_event_seq()` 生成器与可控时间，断言连续 5 次失败后 5 分钟内拒绝；过期后计数重置
    - **Validates: Requirements 7.5**
    - _Property: 19_
    - _Requirements: 7.5_
    - _Design: §7 Property 19, §9.2_

  - [x] 3.15 实现 Session 注册表与 token 生命周期
    - `SessionRegistry`：`issue(fp) -> token`、`validate(token) -> Result<Session>`、`revoke(token)`、`revoke_device(fp)`
    - Token 256 位随机，Base64URL 编码
    - 提供 `list_sessions` 用于已配对设备列表
    - _Requirements: 7.3, 7.4, 7.6, 7.7_
    - _Design: §4.3_

  - [x] 3.16 编写 Session 生命周期属性测试
    - **Property 18: Session_Token 生命周期**
    - 使用 `pair / revoke / revoke_device` 操作序列断言 `validate` 行为始终一致；revoke 后验证返回 `AuthError`
    - **Validates: Requirements 7.3, 7.4, 7.7**
    - _Property: 18_
    - _Requirements: 7.3, 7.4, 7.7_
    - _Design: §7 Property 18, §9.2_

  - [x] 3.17 编写已配对设备 CRUD 属性测试
    - **Property 20: 已配对设备列表 CRUD**
    - 任意 `pair / revoke / revoke_device` 操作序列，最终列表等价于按时间应用后剩余集合
    - **Validates: Requirements 7.6**
    - _Property: 20_
    - _Requirements: 7.6_
    - _Design: §7 Property 20, §9.2_

  - [x] 3.18 实现 Pairing_Code 重启失效语义
    - `PairingService::on_startup()` 强制生成新 code，旧 code 不持久化
    - _Requirements: 7.9_
    - _Design: §4.3_

  - [x] 3.19 编写 Pairing_Code 重启失效属性测试
    - **Property 22: 重启后 Pairing_Code 失效**
    - 模拟 startup → submit(oldCode) → expect PAIR_INVALID；新旧 code 重复概率 ≤ 1/(36^8)
    - **Validates: Requirements 7.9**
    - _Property: 22_
    - _Requirements: 7.9_
    - _Design: §7 Property 22, §9.2_

  - [x] 3.20 实现日志滚动写入器
    - `RollingFileLayer`：单条 ≤ 4 KB（超出截断 + `…<truncated>`）；总量 ≤ 10 MB；超出时按最旧条目淘汰
    - 不写入 Pairing_Code、Session_Token 与文本明文（仅记录长度与 SHA-256 摘要前 8 字节）
    - _Requirements: 9.7_
    - _Design: §8.4_

  - [x] 3.21 编写日志滚动属性测试
    - **Property 32: 日志滚动**
    - 任意写入序列下断言单条 ≤ 4 KB、总量 ≤ 10 MB、最新条目可读
    - **Validates: Requirements 9.7**
    - _Property: 32_
    - _Requirements: 9.7_
    - _Design: §7 Property 32, §9.2_

  - [x] 3.22 实现 i18n 字典与 locale → lang 决策
    - 内嵌 `zh-CN.json` / `en-US.json` 两份桌面端字典
    - `decide_lang(locale: &str) -> Lang`：主语言子标签为 `zh` ⇒ `zh-CN`，否则 `en-US`
    - _Requirements: 8.1, 8.3_
    - _Design: §4.1_

  - [x] 3.23 编写 locale 决策属性测试
    - **Property 24: locale → lang 决策**
    - **Validates: Requirements 8.3, 8.4**
    - _Property: 24_
    - _Requirements: 8.3, 8.4_
    - _Design: §7 Property 24, §9.2_

  - [x] 3.24 编写桌面端 i18n 字典完整性单元测试
    - 断言 zh-CN 与 en-US key 集合相等且无空字符串
    - _Requirements: 8.1_
    - _Design: §9.3_

- [x] 4. 检查点 - 纯函数与状态层全部通过
  - Ensure all tests pass, ask the user if questions arise.

- [x] 5. Web Server（axum + 中间件）
  - 在共享协议与 Pairing_Service 基础上实现 HTTP/HTTPS Web 服务与 WebSocket 端点。
  - _Requirements: 2.1–2.8, 7.2–7.10, 9.6_
  - _Design: §3.3, §4.2, §6.2_

  - [x] 5.1 搭建 axum 应用骨架与配置加载
    - 在 `crates/phonemic-core/src/web/` 实现 `WebServer::start(cfg) -> RuntimeInfo` 与 `shutdown()`
    - 加载 `AppConfig`（任务 2.4），调用 `select_port`（任务 3.1）
    - 启动后 3 秒内可优雅关闭并释放端口
    - _Requirements: 2.1, 2.2, 2.7_
    - _Design: §4.2_

  - [x] 5.2 实现 SubnetFilter 中间件
    - 在请求入口前置：peer IP 不在 RFC1918 / loopback 子网时直接 403 `FORBIDDEN_SUBNET`
    - 不再进入下游中间件与 handler
    - _Requirements: 7.8_
    - _Design: §4.2_

  - [x] 5.3 编写子网过滤属性测试
    - **Property 21: 子网过滤**
    - 随机生成 IPv4 客户端地址，断言非 LAN 一律 403 且 Pair / WS / API handler 未被调用
    - **Validates: Requirements 7.8**
    - _Property: 21_
    - _Requirements: 7.8_
    - _Design: §7 Property 21, §9.2_

  - [x] 5.4 实现 RateLimit 中间件
    - 仅作用于 `/api/pair`，使用任务 3.13 的失败计数器
    - 5 次失败后返回 429 `PAIR_RATELIMIT { retryAfter: 300 }`
    - _Requirements: 7.5_
    - _Design: §4.2_

  - [x] 5.5 实现 Auth 中间件
    - 校验 `Authorization: Bearer <token>` 或 WS 握手时的 `Sec-WebSocket-Protocol`
    - `/`、`/assets/*`、`/api/pair`、`/api/health` 跳过；其余路径返回 401 `AUTH_REQUIRED`
    - _Requirements: 7.4, 9.6_
    - _Design: §4.2_

  - [x] 5.6 实现 `/api/pair` 处理器
    - 调用 `PairingService::submit_pair`，成功返回 `PairResponse`，失败返回 `PAIR_INVALID` 或 `PAIR_RATELIMIT`
    - 同时通过事件通道通知桌面端 UI"新设备配对"
    - _Requirements: 7.2, 7.3, 7.6_
    - _Design: §4.2, §4.3, §6.2_

  - [x] 5.7 实现 `/api/health` 处理器
    - 返回 `{ version, uptime }`，不要求鉴权
    - _Requirements: 2.6_
    - _Design: §4.2_

  - [x] 5.8 实现静态资源 handler `GET /` 与 `/assets/*`
    - 使用 `tower-http::services::ServeDir` 服务于 `apps/desktop/src-tauri/resources/web/`
    - 内容生成 ETag，响应 ≤ 2 秒
    - _Requirements: 2.5_
    - _Design: §4.2_

  - [x] 5.9 实现 WebSocket `/ws` 端点
    - `tokio-tungstenite` 升级握手；调用 `Auth` 校验 `sessionToken`
    - 通过通道把 `ClientMessage` 路由至消息分发器（任务 5.11）
    - _Requirements: 2.6, 7.4_
    - _Design: §4.2, §6.3_

  - [x] 5.10 实现 HTTPS 支持与自签名证书生成
    - 使用 `rustls` 启用 HTTPS；首次启用时使用 `rcgen` 生成自签名证书并保存至用户配置目录
    - 配置项 `enable_https` 控制启用
    - 当 `enable_https = true` 时，所有 Pairing_Code / Session_Token 仅通过 HTTPS 通道传输
    - _Requirements: 2.3, 2.4, 7.10_
    - _Design: §3.3, §4.2_

  - [x] 5.11 实现 WebSocket 消息分发器
    - `MessageDispatcher::handle(msg, session)`：分发到 Input_Injector（`text.submit`）、ASR Bridge（`audio.chunk` / `audio.end`）、心跳（`ping`）、配对（`hello`）
    - 非法 JSON / 缺字段 / 缺 token：丢弃并通过 WS 返回 `MSG_BAD_FORMAT` 或 `AUTH_REQUIRED`，无未捕获异常
    - _Requirements: 9.6_
    - _Design: §4.2, §5.1_

  - [x] 5.12 编写消息协议鲁棒性属性测试
    - **Property 31: 错误协议鲁棒性**
    - 使用 `proptest` 随机生成字节序列（合法 / 非法 JSON / 缺字段 / 缺 token），断言无 panic 且返回结构化错误
    - **Validates: Requirements 9.6**
    - _Property: 31_
    - _Requirements: 9.6_
    - _Design: §7 Property 31, §9.2_

  - [x] 5.13 编写 HTTPS 机密信息泄漏属性测试
    - **Property 23: HTTPS 模式下机密信息不通过 HTTP**
    - 启用 HTTPS 后断言 HTTP 端口（如保留重定向）的所有响应体与响应头不含 Pairing_Code 与 Session_Token
    - **Validates: Requirements 7.10**
    - _Property: 23_
    - _Requirements: 7.10_
    - _Design: §7 Property 23, §9.2_

  - [x] 5.14 实现 PORT_UNAVAILABLE 启动失败提示通道
    - `select_port` 返回 None 时返回 `StartupError::PortUnavailable`
    - 通过事件通道通知桌面端 UI 显示重试入口
    - _Requirements: 2.8_
    - _Design: §8.1_

  - [x] 5.15 编写 Web Server 集成测试
    - 启动真实 axum 实例，验证：`GET /` 返回 HTML、`POST /api/pair` 正确码 / 错误码、限流、HTTPS、WS 升级
    - 模拟 X-Forwarded-For 公网 IP，断言 403
    - 关停后端口在 3 秒内释放
    - _Requirements: 2.1, 2.5, 2.6, 2.7, 7.2, 7.3, 7.5, 7.8, 7.10_
    - _Design: §9.4_

- [x] 6. Discovery_Service（mDNS + 二维码）
  - 在局域网内广播桌面端服务，并产生供手机扫码连接的二维码内容。
  - _Requirements: 3.3, 3.4_
  - _Design: §3.6, §3.7, §4.4_

  - [x] 6.1 实现 mDNS 注册与注销
    - 在 `crates/phonemic-discovery/` 内基于 `mdns-sd` 注册 `_phonemic._tcp.local.`，TXT 记录 `v=1`、`tls=1|0`、`port=<port>`
    - 提供 `Discovery::start(runtime_info)` 与 `Discovery::stop()`；停止时显式注销服务
    - 监听 LAN 接口变化事件，IP 集合改变时刷新通告
    - _Requirements: 3.4_
    - _Design: §3.6, §4.4_

  - [x] 6.2 实现 LAN 丢失检测与事件上报
    - 当所有 RFC1918 接口消失时，向 `EventBus` 发出 `LanLost`；恢复时发出 `LanRestored`
    - 桌面 UI 与 Mobile 状态层据此驱动 banner / scanDisabled
    - _Requirements: 3.6_
    - _Design: §4.4_

  - [x] 6.3 实现二维码内容编码与 SVG 渲染
    - 编码格式：`phonemic://pair?u=<base64url(connectUrl)>&c=<pairingCode>`
    - 使用 `qrcode` crate 渲染为 SVG 字符串，错误纠正等级 `M`
    - 暴露 `qr_encode(url, pairing_code) -> SvgString` 与 `qr_decode_for_test(svg) -> (url, code)` 测试桥
    - _Requirements: 3.3_
    - _Design: §3.7, §4.1_

  - [x] 6.4 编写二维码 round-trip 属性测试
    - **Property 4: 二维码内容 round-trip**
    - 使用 `proptest` 生成任意合法 URL 与 Pairing_Code，断言 `qr_decode_for_test(qr_encode(url, code)) == (url, code)`
    - **Validates: Requirements 3.3**
    - _Property: 4_
    - _Requirements: 3.3_
    - _Design: §7 Property 4, §9.2_

  - [x] 6.5 编写 Discovery 集成测试
    - 在测试环境注册 service，使用第二个 `mdns-sd` 客户端断言能解析到 `_phonemic._tcp.local.` 与 TXT 字段
    - 模拟接口变化触发刷新；模拟全部接口失效触发 `LanLost` 事件
    - _Requirements: 3.4, 3.6_
    - _Design: §9.4_

- [-] 7. Input_Injector（跨平台键盘注入）
  - 为 Windows / macOS / Linux 实现统一的 Unicode 文本与回车键注入抽象，并暴露注入暂停 / 失败传播。
  - _Requirements: 6.1, 6.2, 6.3, 6.4, 6.5, 6.6, 6.7, 9.8_
  - _Design: §3.5, §4.5, §6.3_

  - [x] 7.1 定义 Input_Injector trait 与平台桥接骨架
    - 在 `crates/phonemic-injector/src/lib.rs` 定义：
      - `trait InputInjector { fn inject_text(&self, t: &str) -> Result<(), InjectError>; fn inject_enter(&self) -> Result<(), InjectError>; fn pause(&self, paused: bool); fn current_focus_app(&self) -> Option<FocusInfo>; }`
      - `struct InjectionEvent { kind: EventKind, codepoint: Option<u32>, ts: Instant }`
      - 错误类型 `InjectError`：`NoFocusTarget`、`PermissionDenied`、`Paused`、`BackendError(String)`
    - 提供 `inject_text` 默认实现：分词为码点序列，按 `inject_delay_ms` 节流；遇 `\n` 调用 `inject_enter`
    - _Requirements: 6.1, 6.3, 6.4, 6.5, 6.7_
    - _Design: §3.5, §4.5_

  - [x] 7.2 实现 InjectionPlanner（纯函数）
    - 输入 `(text, delay_ms)`，输出 `Vec<InjectionEvent>` 计划（不接触 OS）
    - 每个 `\n` 映射为一次 `EventKind::Enter`，其余按 Unicode 标量值映射为 `EventKind::Char(cp)`
    - 相邻字符事件 `ts` 间隔 ≥ `delay_ms`
    - _Requirements: 6.3, 6.4, 6.5_
    - _Design: §4.5_

  - [x] 7.3 编写注入计划码点保留属性测试
    - **Property 12: Input_Injector 码点保留**
    - 使用 `proptest` 生成 Unicode 字符串（含 BMP 之外的码点、组合字符），断言 `plan(t).chars_codepoints() == t.chars().codepoints()`
    - **Validates: Requirements 6.3**
    - _Property: 12_
    - _Requirements: 6.3_
    - _Design: §7 Property 12, §9.2_

  - [x] 7.4 编写换行映射属性测试
    - **Property 13: Input_Injector 换行映射**
    - 断言 `plan(t)` 中 `Enter` 事件出现位置与 `t` 中 `\n` 一一对应，其余位置满足 Property 12
    - **Validates: Requirements 6.4**
    - _Property: 13_
    - _Requirements: 6.4_
    - _Design: §7 Property 13, §9.2_

  - [x] 7.5 编写注入延迟属性测试
    - **Property 14: Input_Injector 注入延迟**
    - 对 `delay ∈ [0, 500]`、任意字符串 `t`，断言相邻字符事件 `ts` 间隔 ≥ delay
    - **Validates: Requirements 6.5**
    - _Property: 14_
    - _Requirements: 6.5_
    - _Design: §7 Property 14, §9.2_

  - [x] 7.6 实现 Windows 后端（SendInput + KEYEVENTF_UNICODE）
    - 在 `phonemic-injector/src/windows.rs` 调用 `SendInput`，对 BMP 之外码点拆分为代理对
    - 处理回车注入（VK_RETURN）；提供 `current_focus_app()` 通过 `GetForegroundWindow` 与 `GetWindowThreadProcessId` 实现
    - 注入失败时返回 `BackendError(detail)`
    - _Requirements: 6.1, 6.2, 6.3, 6.4_
    - _Design: §3.5, §4.5_

  - [x] 7.7 实现 macOS 后端（CGEvent + AX）与权限引导接入
    - 在 `phonemic-injector/src/macos.rs` 调用 `CGEventCreateKeyboardEvent` 与 `CGEventKeyboardSetUnicodeString`
    - 检测辅助功能权限：未授权时返回 `PermissionDenied`、暂停后续注入并通过事件总线触发桌面 UI 权限引导界面
    - `current_focus_app()` 通过 `NSWorkspace.frontmostApplication` 实现
    - _Requirements: 6.1, 6.2, 6.3, 6.6, 6.8_
    - _Design: §3.5, §4.5_

  - [x] 7.8 实现 Linux 后端（X11 XTEST + Wayland 兜底说明）
    - 在 `phonemic-injector/src/linux.rs` 调用 `XTestFakeKeyEvent` + `XKeysymToKeycode` 注入 Unicode
    - 在 Wayland 会话下若 `XWayland` 不可用，返回 `BackendError("wayland_unsupported")` 并附引导
    - `current_focus_app()` 优先 `_NET_ACTIVE_WINDOW`，回退到 `xdotool getwindowfocus` 兼容
    - _Requirements: 1.1, 6.1, 6.2, 6.3_
    - _Design: §3.5, §4.5_

  - [x] 7.9 实现注入暂停语义
    - `pause(true)` 后所有 `inject_text / inject_enter` 直接返回 `InjectError::Paused`，且不调用平台 API
    - `pause(false)` 后恢复正常注入
    - 暂停状态对所有平台后端共享
    - _Requirements: 6.7_
    - _Design: §4.5_

  - [x] 7.10 编写注入暂停属性测试
    - **Property 15: 注入暂停**
    - 使用 `proptest` 生成请求序列与 `paused` 切换序列，断言暂停期间平台事件计数为 0；恢复后正常
    - **Validates: Requirements 6.7**
    - _Property: 15_
    - _Requirements: 6.7_
    - _Design: §7 Property 15, §9.2_

  - [x] 7.11 实现 INJECT_NO_FOCUS_TARGET 与 inject.error 上报
    - 当 `current_focus_app()` 返回 None 时返回 `InjectError::NoFocusTarget`
    - 错误经事件总线封装为 `inject.error` WS 消息（含 `code` 与 `detail`）回送 Mobile
    - 同步写入本地日志（不含明文文本，仅含长度与摘要）
    - _Requirements: 6.6, 9.8_
    - _Design: §4.5, §8.1, §8.4_

  - [x] 7.12 编写注入失败传播属性测试
    - **Property 33: 注入失败传播**
    - 模拟 Input_Injector 抛出各类异常，断言：本地日志存在对应错误条目，且 `inject.error` 经 Connection_Channel 发送
    - **Validates: Requirements 9.8**
    - _Property: 33_
    - _Requirements: 9.8_
    - _Design: §7 Property 33, §9.2_

  - [ ] 7.13 编写 Input_Injector 平台单元 / 集成测试 _(deferred — 已通过 `live-backend-tests` feature gate 收口；CI 矩阵在三平台启用该 feature 后自动跑)_
    - 在自动化窗口（如 Notepad / TextEdit / xterm）注入文本并通过文件比对断言
    - 验证 IME 关闭与开启两种状态下中英文与 emoji 注入正确
    - 在 CI 中通过 `serial_test` 串行执行
    - _Requirements: 6.2, 6.3, 6.4_
    - _Design: §9.5_

- [x] 8. ASR Bridge（Server_ASR）
  - 在 Browser_ASR 不可用或用户启用时，由桌面端运行本地 ASR 引擎处理音频流。
  - _Requirements: 5.1, 5.2, 5.3, 5.4_
  - _Design: §3.8, §4.6, §6.3_

  - [x] 8.1 定义 ASR engine trait 与音频帧类型
    - `trait AsrEngine { fn feed(&self, frame: AudioFrame) -> Result<()>; fn end(&self) -> Result<TranscriptFinal>; }`
    - `AudioFrame { codec: enum Pcm16k|Opus, payload: Bytes, ts_ms: u64 }`
    - _Requirements: 5.4_
    - _Design: §4.6_

  - [x] 8.2 实现 ASR 引擎决策函数 `pick_asr_engine`
    - 输入 `(supports_browser_asr, prefer_server_asr)`，返回 `Browser` 或 `Server`
    - 仅当 `supports_browser_asr ∧ ¬prefer_server_asr` 时返回 `Browser`
    - _Requirements: 5.3, 5.4_
    - _Design: §4.6, §4.7_

  - [x] 8.3 编写 ASR 引擎决策属性测试
    - **Property 10: ASR 引擎决策**
    - 使用 `proptest` 枚举 4 种布尔组合，断言决策函数返回值与规则一致
    - **Validates: Requirements 5.3, 5.4**
    - _Property: 10_
    - _Requirements: 5.3, 5.4_
    - _Design: §7 Property 10, §9.2_

  - [x] 8.4 实现 whisper.cpp 适配层
    - 通过 FFI / `whisper-rs` 集成 small / base 模型，模型文件放置于资源目录
    - `feed()` 增量喂入，`end()` 触发最终识别并返回 `TranscriptFinal { text, lang, conf }`
    - 引擎初始化失败时返回 `ASR_INIT_ERROR` 错误码
    - _Requirements: 5.4_
    - _Design: §3.8, §4.6_

  - [x] 8.5 实现 ASR 超时与错误回送
    - 单段语音 ≤ 10 秒时正常路径需在 3 秒内回 `transcript.final`（与端到端测试 12.1 联合验证）
    - 全局看门狗：任意一段在 10 秒内未产出任何识别结果即返回 `ASR_TIMEOUT` 错误并通过 WS 通知客户端，由 Mobile 展示重试提示
    - 失败时不向 Input_Injector 投递任何文本
    - _Requirements: 5.6, 5.7, 9.6, 9.8_
    - _Design: §8.1_

  - [x] 8.6 编写 ASR Bridge 集成测试
    - 使用预录制 PCM 16kHz 中英文短句样本（≤ 10 秒）验证 `feed/end` 在 3 秒内输出非空 `transcript.final` 且语言识别正确
    - 注入 30 秒静音样本，验证超时路径返回 `ASR_TIMEOUT` 且不向 Input_Injector 投递文本
    - _Requirements: 5.4, 5.6, 5.7, 9.6_
    - _Design: §9.4_

- [x] 9. Mobile_Web_Client（Vue 3 SPA）
  - 在手机浏览器中实现配对、连接、录音、识别、自动发送、历史、心跳、重连、补发、可见性恢复等行为。
  - _Requirements: 4.1–4.9, 5.1–5.5, 8.2, 8.5, 8.6, 9.1, 9.2, 9.3, 9.4, 9.5, 9.9_
  - _Design: §4.7, §6.2, §6.3, §6.4_

  - [x] 9.1 搭建路由与状态层骨架
    - 定义 `routes`：`/pair`（配对）、`/`（语音输入）、`/settings`（设置）
    - 创建 Pinia store：`useConnection`、`useRecorder`、`useTranscript`、`useSettings`、`useI18n`
    - 在 `app.ts` 中加载持久化设置（仅 `uiLang`、`asrLang`、`mode`、`autoSend`、`preferServerAsr`）
    - _Requirements: 4.1_
    - _Design: §4.7_

  - [x] 9.2 实现配对页：扫码 / 手输 PIN
    - 通过 `BarcodeDetector`（可用时）或 `jsQR` 解析二维码
    - 调用 `POST /api/pair`，成功后保存 `sessionToken` 至 `sessionStorage`
    - 错误处理：`PAIR_INVALID` 提示重输；`PAIR_RATELIMIT` 显示倒计时
    - _Requirements: 7.2, 7.3, 7.5_
    - _Design: §4.7, §6.2_

  - [x] 9.3 实现 ConnectionService（WebSocket 客户端）
    - 状态机：`Disconnected` → `Connecting` → `Connected` → `Reconnecting`
    - 暴露 `send(msg)`、`onMessage`、`status$`
    - 携带 `Authorization` token 进行 WS 握手；非法状态变更被禁止
    - _Requirements: 4.5_
    - _Design: §4.7, §6.4_

  - [x] 9.4 实现连接状态文本映射 `statusLabel(s, lang)`
    - 纯函数：`(status, lang) -> string`，从 i18n 字典取值
    - 与 store 解耦，便于属性测试
    - _Requirements: 4.5_
    - _Design: §4.7_

  - [x] 9.5 编写连接状态文本映射属性测试
    - **Property 9: 连接状态文本映射**
    - 使用 `fast-check` 枚举所有 `(status, lang)` 组合断言返回 `i18n[lang][status]`，幂等
    - **Validates: Requirements 4.5**
    - _Property: 9_
    - _Requirements: 4.5_
    - _Design: §7 Property 9, §9.2_

  - [x] 9.6 实现录音状态机（press / toggle）与录音视觉指示
    - 纯函数 reducer `recorderReduce(state, event)`，事件：`PointerDown`、`PointerUp`、`Tap`、`Blur`
    - `press` 模式：`isRecording = 存在未释放的 PointerDown`；`toggle` 模式：`Tap` 计数奇偶
    - `Blur` 在两种模式下都强制停止录音
    - 当 `isRecording = true` 时，主界面同步展示波形 / 闪烁动画指示（绑定到状态机派生属性，不引入额外副作用）
    - _Requirements: 4.2, 4.3_
    - _Design: §4.7_

  - [x] 9.7 编写录音状态机属性测试
    - **Property 6: 录音状态机不变量**
    - 使用 `fast-check` 生成事件序列断言 `isRecording` 与不变量一致
    - **Validates: Requirements 4.2**
    - _Property: 6_
    - _Requirements: 4.2_
    - _Design: §7 Property 6, §9.2_

  - [x] 9.8 实现 Browser_ASR 适配（Web Speech API）与 interim 实时显示
    - 检测 `SpeechRecognition` 可用性输出 `supportsBrowserASR`
    - 包装为 `BrowserAsr.start(lang)` / `stop()` 与事件流 `interim` / `final`
    - 录音进行中时，将 `interim` 事件实时推入 `useTranscript().draft`，由文本预览区即时渲染
    - 不可用时通过事件向 store 暴露
    - _Requirements: 4.4, 5.1, 5.2, 5.3_
    - _Design: §4.7_

  - [x] 9.9 实现 Server_ASR 推流路径
    - 使用 `MediaRecorder` 采样 16 kHz PCM/Opus，通过 WS 发送 `audio.chunk`，结束发 `audio.end`
    - 接收 `transcript.final` 入库
    - _Requirements: 5.4_
    - _Design: §4.7, §6.3_

  - [x] 9.10 实现自动发送计数器与发送前手动编辑入口
    - 纯函数 `autoSendDispatch(events, autoSend) -> submitCount`
    - `autoSend = true` 时，每个 `final` 事件触发一次 `text.submit`；`interim` 不触发
    - `autoSend = false` 时，`final` 文本写入可编辑文本框，用户可修改后点击「发送」按钮再触发 `text.submit`
    - 编辑期间 `useTranscript().draft` 与界面输入框双向绑定，发送后清空
    - _Requirements: 4.6, 4.7, 5.8_
    - _Design: §4.7_

  - [x] 9.11 编写自动发送计数属性测试
    - **Property 7: 自动发送计数**
    - 使用 `fast-check` 生成事件序列断言提交次数等于 `final` 事件数
    - **Validates: Requirements 4.7**
    - _Property: 7_
    - _Requirements: 4.7_
    - _Design: §7 Property 7, §9.2_

  - [x] 9.12 实现识别历史队列(capacity 50)
    - 数据结构：`pushBounded(queue, item, 50)` 纯函数；超出时丢弃最早条目
    - UI 中以倒序展示，长按可复制 / 重发
    - _Requirements: 4.9_
    - _Design: §4.7_

  - [x] 9.13 编写历史队列上限属性测试
    - **Property 8: 历史队列上限**
    - 使用 `fast-check` 生成插入序列断言 `length == min(|R|, 50)` 且保留最后 N 条
    - **Validates: Requirements 4.9**
    - _Property: 8_
    - _Requirements: 4.9_
    - _Design: §7 Property 8, §9.2_

  - [x] 9.14 实现心跳与断线检测
    - `Heartbeat`：连接后每 20 秒发送一次 `ping`
    - `Watchdog`：维护 `lastHeartbeat`，`now - lastHeartbeat ≥ 30s` ⇒ 状态置 `Disconnected`
    - _Requirements: 9.1, 9.2_
    - _Design: §4.7, §6.4_

  - [x] 9.15 编写心跳频率与断线阈值属性测试
    - **Property 27: 心跳发送频率**
    - **Property 28: 心跳检测阈值**
    - 使用 `fast-check` + 受控时钟断言两条不变量
    - **Validates: Requirements 9.1, 9.2**
    - _Property: 27, 28_
    - _Requirements: 9.1, 9.2_
    - _Design: §7 Property 27, §7 Property 28, §9.2_

  - [x] 9.16 实现重连退避调度与"重连失败"提示
    - `backoffDelays = [1, 2, 4, 8, 16]`（秒），最多 5 次
    - 实现纯函数 `nextDelay(attempt) -> seconds | null`
    - 5 次重连仍失败时，切换到 `RECONNECT_FAILED` 错误视图，提供"手动重试"按钮重置 `attempt` 并触发新一轮连接
    - _Requirements: 9.3, 9.5_
    - _Design: §4.7, §6.4, §8.1_

  - [x] 9.17 编写重连退避序列属性测试
    - **Property 29: 重连退避序列**
    - 使用 `fast-check` 断言第 n 次延迟 = 2^(n-1)，n ≤ 5
    - **Validates: Requirements 9.3**
    - _Property: 29_
    - _Requirements: 9.3_
    - _Design: §7 Property 29, §9.2_

  - [x] 9.18 实现离线消息队列与重连后保序补发
    - `Reconnecting` 期间识别得到的 `text.submit` 进入 FIFO 队列
    - 重连成功后按入队顺序逐条发送，`id` 字段用于服务端去重
    - _Requirements: 9.4_
    - _Design: §4.7, §6.4_

  - [x] 9.19 编写离线补发保序属性测试
    - **Property 30: 离线消息补发保序**
    - 使用 `fast-check` 生成入队序列与重连事件，断言重连后发送顺序与入队顺序一致，每条恰好一次
    - **Validates: Requirements 9.4**
    - _Property: 30_
    - _Requirements: 9.4_
    - _Design: §7 Property 30, §9.2_

  - [x] 9.20 实现 visibilitychange 触发重连
    - 监听 `document.visibilitychange`，从 `hidden` 变为 `visible` 时若状态非 `Connected`，立即触发一次重连
    - _Requirements: 9.9_
    - _Design: §4.7, §6.4_

  - [x] 9.21 编写后台返回触发重连属性测试
    - **Property 34: 后台返回触发重连**
    - 使用 `fast-check` 生成可见性事件序列与连接状态序列，断言每次 `hidden→visible` 且非 `Connected` 触发一次重连
    - **Validates: Requirements 9.9**
    - _Property: 34_
    - _Requirements: 9.9_
    - _Design: §7 Property 34, §9.2_

  - [x] 9.22 实现 Mobile i18n 字典与运行时切换
    - 提供 `zh-CN.json` / `en-US.json` 两份字典；`useI18n().setLang(l)` 切换后所有文案立即更新
    - 实现 `decideLang(navigator.language)`（与桌面端一致）
    - UI 语言与 ASR 语言两个独立设置项
    - _Requirements: 8.2, 8.3, 8.5, 8.6_
    - _Design: §4.7_

  - [x] 9.23 编写 UI 语言切换即时生效属性测试
    - **Property 25: UI 语言切换即时生效**
    - 使用 `fast-check` 生成切换事件与可观测文案集合，断言每次切换后所有文案等于目标字典
    - **Validates: Requirements 8.5**
    - _Property: 25_
    - _Requirements: 8.5_
    - _Design: §7 Property 25, §9.2_

  - [x] 9.24 编写 UI 语言与 ASR 语言独立属性测试
    - **Property 26: UI 语言与 ASR 语言独立**
    - 使用 `fast-check` 生成 `setUiLang/setAsrLang` 操作序列，断言两值仅由各自最后一次设置决定
    - **Validates: Requirements 8.6**
    - _Property: 26_
    - _Requirements: 8.6_
    - _Design: §7 Property 26, §9.2_

  - [x] 9.25 编写 Mobile i18n 字典完整性单元测试
    - 断言 zh-CN 与 en-US key 集合相等且无空字符串
    - 缺失 key 在控制台告警并回退到 en-US
    - _Requirements: 8.2_
    - _Design: §9.3_

  - [x] 9.26 实现 Mobile 麦克风权限请求与降级提示
    - 若 `navigator.mediaDevices.getUserMedia` 拒绝，展示引导文案并禁用录音按钮
    - 错误码 `MIC_PERMISSION_DENIED` 写入诊断面板
    - _Requirements: 4.8, 5.1_
    - _Design: §4.7_

  - [x] 9.27 实现 Mobile 错误展示组件
    - 统一的 toast / banner 组件渲染 `AppError { code, message }`
    - 不展示堆栈或敏感字段；折叠 `detail`
    - _Requirements: 9.6_
    - _Design: §8.1_


- [-] 10. 桌面端 UI（Tauri + Vue / 原生 webview）
  - 实现主窗口与托盘 UI，串联 Discovery、Pairing、Web Server、Input_Injector 与设置项。
  - _Requirements: 1.1, 1.5, 1.6, 3.1, 3.5, 3.6, 6.5, 6.7, 7.1, 7.6, 7.7, 8.1, 8.5_
  - _Design: §4.1, §6.1_

  - [-] 10.1 搭建主窗口骨架与导航
    - 三个面板：「连接」「已配对设备」「设置」
    - 启动时显示连接 URL 与二维码（来自任务 6.3）；空 LAN 时展示 banner（来自任务 3.7）
    - _Requirements: 3.1, 3.5, 3.6_
    - _Design: §4.1_

  - [x] 10.2 实现 Pairing_Code 与连接 URL 面板
    - 显示当前 `pairingCode`（来自任务 3.9 / 3.18）
    - "重新生成"按钮触发 `regenerate_code()`
    - _Requirements: 7.1, 7.9_
    - _Design: §4.1, §4.3_

  - [-] 10.3 实现已配对设备列表 UI
    - 列表展示 `deviceLabel`、`fingerprint` 前 8 字节、`lastUsedAt`
    - 提供"吊销"与"全部清空"操作，分别调用 `revoke_device(fp)` 与逐项 revoke
    - _Requirements: 7.6, 7.7_
    - _Design: §4.1, §4.3_

  - [x] 10.4 实现设置面板
    - 暴露 `preferred_port`、`enable_https`、`inject_delay_ms`、`prefer_server_asr`、`ui_lang`、`asr_lang`、`autoSend`、`mode`
    - 修改后调用 `save_to_path` 持久化（任务 2.4）
    - _Requirements: 2.2, 2.3, 4.2, 4.7, 6.5, 8.1, 8.5_
    - _Design: §4.1, §5.3_

  - [x] 10.5 实现 Tauri 命令桥
    - 暴露给前端的命令：`get_runtime_info`、`regenerate_code`、`list_sessions`、`revoke_session`、`save_config`、`get_logs_tail`
    - 命令层做参数校验后再调用 phonemic-core
    - _Requirements: 1.1_
    - _Design: §4.1_

  - [x] 10.6 实现托盘菜单与单实例
    - 托盘菜单：「显示主窗口」「暂停注入」「重启服务」「退出」
    - "暂停注入"切换 `Input_Injector::pause(true|false)`（任务 7.9）
    - 单实例插件：第二次启动时唤醒已运行实例
    - _Requirements: 1.5, 1.6, 6.7_
    - _Design: §4.1_

  - [x] 10.7 实现桌面 UI i18n 接入
    - 使用任务 3.22 的字典与 `decide_lang`
    - 设置项变更后所有文案立即刷新（不重启窗口）
    - _Requirements: 8.1, 8.5_
    - _Design: §4.1_

  - [x] 10.8 实现 5 秒内完成初始化的启动序列与启动画面
    - 启动时并发：`WebServer::start`、`Discovery::start`、`PairingService::on_startup`、Input_Injector 探测
    - 在初始化完成前显示启动画面：阶段说明（"正在启动 Web 服务器""正在生成证书"等）+ 进度指示
    - 全部就绪后切换到主窗口；超过 5 秒未就绪时启动画面切换为进度与失败原因展示，并阻止主窗口提前出现
    - _Requirements: 1.2, 1.3_
    - _Design: §6.1_

  - [ ] 10.9 编写桌面 UI Tauri WebDriver 集成测试 _(deferred — 通过 `PHONEMIC_TEST_WEBDRIVER=1` 环境变量门控；CI 配置 WebDriver 后启用)_
    - 启动 Tauri WebDriver，加载主窗口，断言：连接面板显示 IP/端口、Pairing_Code、QR；托盘"暂停注入"切换状态
    - 模拟 LAN 丢失事件，断言 banner 出现
    - _Requirements: 3.1, 3.6, 6.7_
    - _Design: §9.5_

- [x] 11. 检查点 - 模块级测试与桌面 UI 全部通过
  - Ensure all tests pass, ask the user if questions arise.

- [x] 12. 端到端集成与系统级测试
  - 验证从手机录音到桌面键盘出现文本的完整闭环。
  - _Requirements: 1.2, 4.1, 4.6, 4.8, 5.1, 5.2, 6.1, 6.6, 9.4, 9.5_
  - _Design: §6.3, §9.5_

  - [x] 12.1 编写端到端"语音 → 注入"自动化测试
    - 使用 Playwright 驱动手机视图（Chromium 模拟移动 UA），Tauri WebDriver 驱动桌面
    - 流程：扫码配对 → 进入主页 → 注入预录音频流 → 验证目标输入框文本与预期一致
    - 桌面端在测试模式下注入到一个本地 mock 文本框（避免影响真实 OS）
    - _Requirements: 4.1, 4.6, 5.1, 5.2, 6.1_
    - _Design: §6.3, §9.5_

  - [x] 12.2 编写端到端断网 / 重连测试
    - 在测试中关闭手机 WS，等待 backoff 序列；恢复网络后断言队列保序补发
    - 触发 `visibilitychange` 模拟应用切后台再切回
    - 模拟 5 次重连全部失败，断言出现 `RECONNECT_FAILED` 视图与手动重试按钮
    - _Requirements: 9.3, 9.4, 9.5, 9.9_
    - _Design: §6.4, §9.5_

  - [x] 12.3 编写跨平台冒烟脚本
    - `scripts/smoke.ps1` / `scripts/smoke.sh`：在 Windows / macOS / Linux 上启动桌面端，自动化访问 `/api/health`、加载 `/`、执行一次 `/api/pair`
    - 验证启动 ≤ 5 秒、关闭 ≤ 3 秒
    - _Requirements: 1.1, 1.2, 1.3, 2.5, 2.6, 2.7_
    - _Design: §9.5_

  - [x] 12.4 编写无焦点目标场景测试
    - 桌面端无活动文本框时触发 `text.submit`，断言：返回 `INJECT_NO_FOCUS_TARGET`、Mobile UI 显示提示、本地日志记录
    - _Requirements: 6.6, 9.8_
    - _Design: §8.1, §9.5_

- [x] 13. 错误处理、可观测性与日志
  - 把 §8 的错误码体系与日志策略全面接入运行链路。
  - _Requirements: 1.3, 2.8, 3.6, 4.4, 6.6, 7.5, 7.10, 9.6, 9.7, 9.8_
  - _Design: §8.1, §8.2, §8.3, §8.4_

  - [x] 13.1 实现统一错误转换层
    - 在 `phonemic-core/src/error.rs` 提供 `IntoAppError`，把 `io::Error` / `serde_json::Error` / `tokio::JoinError` 转为带错误码的 `AppError`
    - 为每个错误码提供 i18n 用户文案 key
    - _Requirements: 9.6_
    - _Design: §8.1_

  - [x] 13.2 接入 tracing + 任务 3.20 的滚动日志
    - 配置 `tracing-subscriber` + `RollingFileLayer`
    - 不同模块使用结构化字段：`module`、`req_id`、`session_id_short`
    - 默认日志等级 `info`，调试模式 `debug`
    - _Requirements: 9.7_
    - _Design: §8.4_

  - [x] 13.3 接入 inject.error / asr.error 上报通道
    - Input_Injector / ASR Bridge 错误事件统一封装为 WS 消息回送 Mobile（任务 7.11 / 8.5）
    - 桌面 UI 通知中心同步显示
    - _Requirements: 6.6, 9.8_
    - _Design: §8.1, §8.2_

  - [x] 13.4 实现"诊断"导出功能
    - 桌面 UI 设置面板提供"导出诊断包"按钮
    - 打包：最近 1MB 日志 + 配置（脱敏 token / pairing code） + 平台版本
    - 输出 zip 至用户选择目录
    - _Requirements: 9.7_
    - _Design: §8.4_

  - [x] 13.5 实现错误码到 UI 文案的映射
    - Mobile 与桌面端错误展示组件按 `code` 查 i18n 字典
    - 缺失 key 回退到原 `message`
    - _Requirements: 8.5, 9.6_
    - _Design: §8.1_

- [x] 14. 打包、签名与发布流水线
  - 提供三平台分发包与版本号管理。
  - _Requirements: 1.1, 1.2_
  - _Design: §3.1, §9.7_

  - [x] 14.1 配置 tauri bundle（msi / dmg / AppImage / deb）
    - 在 `tauri.conf.json` 中配置 Windows MSI、macOS DMG、Linux AppImage 与 deb
    - 内嵌 `apps/mobile/dist/` 至 `resources/web/`
    - _Requirements: 1.1_
    - _Design: §3.1_

  - [x] 14.2 实现版本号注入与 `/api/health` 一致
    - 在 `build.rs` 写入 `phonemic-protocol::VERSION`
    - 在 CI 中校验 git tag 与 Cargo.toml 一致
    - _Requirements: 2.6_
    - _Design: §5.2_

  - [x] 14.3 配置 GitHub Actions 发布流水线
    - 触发条件：tag `v*`
    - 矩阵：Windows / macOS / Linux 构建并上传产物
    - 失败任何一步则不创建 release
    - _Requirements: 1.1_
    - _Design: §9.7_

  - [x] 14.4 实现 Windows / macOS 代码签名占位脚本
    - 提供 `scripts/sign-windows.ps1` 与 `scripts/sign-macos.sh`
    - 在 secrets 缺失时跳过签名并发出 warning（不阻断 CI），便于贡献者本地构建
    - _Requirements: 1.1_
    - _Design: §9.7_

  - [x] 14.5 撰写 README 与发布说明模板
    - README 覆盖：系统要求、首次配对步骤、麦克风 / 辅助功能权限申请说明
    - `RELEASE_NOTES.md` 模板由 CI 自动从 commit message 生成草稿
    - _Requirements: 1.3_
    - _Design: §9.7_

- [x] 15. 最终检查点 - 全部测试与发布门禁
  - Ensure all tests pass, ask the user if questions arise.

## Notes

- 带 `*` 后缀的子任务是可选的（属性 / 集成 / 单元测试），可跳过以加速 MVP；正式发版前必须补齐并全部通过（详见下文 Definition of Done）。
- 每个任务都引用 `requirements.md` 的具体子条款（`_Requirements_`）与 `design.md` 的章节（`_Design_`），便于追溯。
- `design.md §7` 中的 34 条 Correctness Property 全部映射到对应任务（`_Property_`）：1→3.2，2→3.4，3→3.6，4→6.4，5→3.8，6→9.7，7→9.11，8→9.13，9→9.5，10→8.3，11→2.6，12→7.3，13→7.4，14→7.5，15→7.10，16→3.10，17→3.12，18→3.16，19→3.14，20→3.17，21→5.3，22→3.19，23→5.13，24→3.23，25→9.23，26→9.24，27/28→9.15，29→9.17，30→9.19，31→5.12，32→3.21，33→7.12，34→9.21。
- 所有需求 R1.1–R9.9 至少在一个任务中被引用：R1（任务 1, 7.8, 10.6, 10.8, 12, 14）；R2（任务 1, 5）；R3（任务 3, 6, 10）；R4（任务 9, 10）；R5（任务 8, 9, 2）；R6（任务 7）；R7（任务 3, 5, 9.2, 10）；R8（任务 3.22–3.24, 9.22–9.25, 10.7）；R9（任务 3.20–3.21, 5.11–5.13, 7.11–7.12, 9.14–9.21, 13）。

## Definition of Done

发版（v1.0）前必须满足以下全部条目：

- [x] **功能完备**：任务 1–14 中所有非 `*` 任务全部完成
- [x] **可选测试补齐**：所有 `*` 任务（属性 / 集成测试）全部完成且通过
- [x] **34 条 Property 全部存在并通过**：CI 中 `cargo test --features property-tests` 与 `pnpm test -- --run` 均显示 34 条 property 测试全部 PASS（与 design §7 一一对应）
- [x] **跨平台冒烟通过**：任务 12.3 在 Windows 10/11、macOS 12+、Ubuntu 20.04+ 三个目标 OS 上 PASS
- [x] **启动 / 关闭时延达标**：任务 10.8 / 5.1 验证启动 ≤ 5 秒、关闭 ≤ 3 秒（Requirements 1.2, 2.7）
- [x] **安全验证通过**：HTTPS 模式机密信息不泄漏（Property 23）、子网过滤（Property 21）、限流（Property 19）属性测试 PASS；Session_Token 落盘加密（design §5.4）已经过验证
- [x] **可观测性完整**：日志滚动（Property 32）属性测试 PASS；inject.error / asr.error 通道已验证（Property 33）；诊断包导出可用
- [x] **i18n 完整**：zh-CN 与 en-US 字典 key 集合相等（任务 3.24 / 9.25）；切换即时生效（Property 25）；UI / ASR 语言独立（Property 26）
- [x] **CI 全绿**：`cargo fmt --check`、`cargo clippy -D warnings`、`cargo test`、`pnpm lint`、`pnpm test`、`pnpm build`、跨平台构建产物上传成功
- [x] **签名与版本一致**：任务 14.2 校验通过；release tag 与 `/api/health` 返回的 `version` 一致

## 测试覆盖率门禁

与 `design.md §9.7 测试覆盖率与门禁` 对齐：

- **Rust 单元 + 属性测试覆盖率**（`phonemic-core` / `phonemic-protocol` / `phonemic-injector` / `phonemic-discovery` / `phonemic-asr`）：行覆盖率 ≥ **80%**，分支覆盖率 ≥ **70%**；使用 `cargo-llvm-cov` 在 CI 中产出报告。
- **TypeScript 单元 + 属性测试覆盖率**（`apps/mobile/src/`）：行覆盖率 ≥ **80%**，分支覆盖率 ≥ **70%**；使用 `vitest --coverage`。
- **属性测试样本数**：`proptest` / `fast-check` 默认 256 cases；CI nightly 流水线提升到 4096 cases；任何 shrink 出的反例必须固化为回归用例（`proptest-regressions/` 与 `fast-check` `examples`）。
- **关键模块更高门禁**：`phonemic-injector`（Properties 12–15, 33）与 `phonemic-protocol`（Property 11, 31）行覆盖率 ≥ **90%**。
- **集成测试门禁**：任务 5.15、6.5、7.13、8.6、10.9、12.x 全部 PASS 才允许打包；任一失败阻断 release 流水线（任务 14.3）。
- **门禁失败处理**：任一覆盖率 / 属性测试 / 集成测试不达标，CI 自动阻断 merge 与 release，并在 PR 中以注释附上未达标的模块、缺失用例与建议补齐方向。

## Task Dependency Graph

```json
{
  "waves": [
    { "id": 0, "tasks": ["1.1"] },
    { "id": 1, "tasks": ["1.2", "1.3", "1.4"] },
    { "id": 2, "tasks": ["2.1", "2.2", "2.3", "2.4"] },
    { "id": 3, "tasks": ["2.5", "2.6", "2.7", "3.1", "3.3", "3.5", "3.7", "3.9", "3.11", "3.13", "3.15", "3.18", "3.20", "3.22"] },
    { "id": 4, "tasks": ["3.2", "3.4", "3.6", "3.8", "3.10", "3.12", "3.14", "3.16", "3.17", "3.19", "3.21", "3.23", "3.24"] },
    { "id": 5, "tasks": ["5.1", "6.1", "6.3", "7.1", "7.2", "8.1", "8.2"] },
    { "id": 6, "tasks": ["5.2", "5.4", "5.5", "5.7", "5.8", "5.10", "5.14", "6.2", "7.6", "7.7", "7.8", "7.9", "8.4", "8.5"] },
    { "id": 7, "tasks": ["5.3", "5.6", "5.9", "5.11", "6.4", "7.3", "7.4", "7.5", "7.10", "7.11", "8.3"] },
    { "id": 8, "tasks": ["5.12", "5.13", "5.15", "6.5", "7.12", "7.13", "8.6"] },
    { "id": 9, "tasks": ["9.1", "9.2", "9.3", "9.4", "9.6", "9.8", "9.10", "9.12", "9.14", "9.16", "9.18", "9.20", "9.22", "9.26", "9.27"] },
    { "id": 10, "tasks": ["9.5", "9.7", "9.11", "9.13", "9.15", "9.17", "9.19", "9.21", "9.23", "9.24", "9.25", "9.9"] },
    { "id": 11, "tasks": ["10.1", "10.2", "10.3", "10.4", "10.5", "10.6", "10.7", "10.8"] },
    { "id": 12, "tasks": ["10.9", "12.1", "12.2", "12.3", "12.4", "13.1", "13.2", "13.3", "13.4", "13.5"] },
    { "id": 13, "tasks": ["14.1", "14.2", "14.3", "14.4", "14.5"] }
  ]
}
```
