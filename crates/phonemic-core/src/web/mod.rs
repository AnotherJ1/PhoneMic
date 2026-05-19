//! Web Server 子树（任务 5.x）。
//!
//! - 任务来源：`.kiro/specs/phone-mic-voice-input/tasks.md` 5.1–5.15
//! - 设计来源：`.kiro/specs/phone-mic-voice-input/design.md` §4.2 / §6.2
//!
//! 本模块负责：
//! - 启动 / 关闭 axum HTTP/HTTPS 服务（[`server`]）；
//! - 挂载 SubnetFilter / RateLimit / Auth 中间件（[`middleware`]）；
//! - 暴露 `/api/pair`（[`handlers::pair`]）、`/api/health`（[`handlers::health`]）、
//!   `/`、`/assets/*`（[`handlers::statics`]）与 `/ws`（[`handlers::ws`]）；
//! - 路由 WebSocket 消息至 dispatcher（[`dispatcher`]）；
//! - HTTPS 证书生成与持久化（[`tls`]）。
//!
//! 共享应用状态聚合在 [`AppState`]，以 `Arc<Mutex<...>>` 形式跨请求共享。

pub mod dispatcher;
pub mod errors;
pub mod handlers;
pub mod middleware;
pub mod redirect;
pub mod server;
pub mod state;
pub mod tls;

pub use dispatcher::{DispatcherOutcome, MessageDispatcher};
pub use server::{RuntimeInfo, StartupError, WebServer, WebServerHandle};
pub use state::AppState;
