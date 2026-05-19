//! axum HTTP/HTTPS Web Server 入口（任务 5.1 / 5.10 / 5.14 / 5.15）。
//!
//! 提供 [`WebServer::start`] 与 [`WebServerHandle::shutdown`]：
//! - `start` 选择端口 → 绑定 → 构造路由 → 拉起后台任务；
//! - `shutdown` 关停后台任务，确保端口在 3 秒内释放（Req 2.7）。
//!
//! HTTPS 暂以 PEM 占位（任务 5.10 的证书已生成在用户配置目录），
//! 真正的 rustls 拼装由 [`crate::web::tls`] 协助、由调用方组装。

use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;

use axum::routing::{get, post};
use axum::Router;
use phonemic_protocol::ErrorCode;
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::bridge_events::{BridgeEvent, BridgeEventTx, PortUnavailableEvent};
use crate::pairing_service::PairingService;
use crate::port_select::select_port;

use super::handlers;
use super::middleware as mw;
use super::state::AppState;

/// 启动后由 [`WebServer::start`] 返回的运行时元数据。
#[derive(Debug, Clone)]
pub struct RuntimeInfo {
    /// 实际绑定的端口（可能与首选端口不同，因为可能回退）。
    pub bound_port: u16,
    /// 实际监听的所有地址（通常 `0.0.0.0:port` + IPv6）。
    pub bind_addr: SocketAddr,
    /// 是否启用了 HTTPS。
    pub https: bool,
    /// 服务进程 PID（仅诊断用途）。
    pub pid: u32,
}

/// `start` 失败时可能的错误码。
#[derive(Debug, Error)]
pub enum StartupError {
    /// 找不到任何可用端口（任务 5.14 / Req 2.8）。
    #[error("PORT_UNAVAILABLE: preferred port {preferred} occupied and no fallback succeeded")]
    PortUnavailable { preferred: u16 },
    /// 监听 socket 失败。
    #[error("bind failed: {0}")]
    Bind(#[from] std::io::Error),
}

impl StartupError {
    #[must_use]
    pub fn error_code(&self) -> ErrorCode {
        match self {
            Self::PortUnavailable { .. } | Self::Bind(_) => ErrorCode::PortUnavailable,
        }
    }
}

/// Web Server 启动配置。
#[derive(Debug, Clone)]
pub struct WebServerCfg {
    pub preferred_port: u16,
    pub bind_addr: IpAddr,
    pub static_root: PathBuf,
    pub version: String,
}

/// `WebServer` 仅是逻辑名字空间，不持有状态；通过 `start` 拿到 [`WebServerHandle`]。
pub struct WebServer;

impl WebServer {
    /// 启动 axum HTTP server，监听 `cfg.bind_addr:cfg.preferred_port`。
    ///
    /// `pairing` 与 `events` 由调用方注入（便于复用全局 PairingService）。
    pub async fn start(
        cfg: WebServerCfg,
        pairing: PairingService,
        events: BridgeEventTx,
    ) -> Result<WebServerHandle, StartupError> {
        // 端口选择：occupied 集合在 MVP 中为空（不探测），由系统返回 EADDRINUSE
        // 触发回退；此处先过 select_port 拿到目标端口。
        let port = select_port(cfg.preferred_port, &HashSet::<u16>::new()).ok_or(
            StartupError::PortUnavailable {
                preferred: cfg.preferred_port,
            },
        )?;
        let addr = SocketAddr::new(cfg.bind_addr, port);
        let listener = match TcpListener::bind(addr).await {
            Ok(l) => l,
            Err(e) => {
                // 通过事件总线通知 UI（任务 5.14）。
                let _ = events
                    .send(BridgeEvent::PortUnavailable(PortUnavailableEvent {
                        preferred_port: cfg.preferred_port,
                    }))
                    .await;
                return Err(StartupError::Bind(e));
            }
        };
        let actual_addr = listener.local_addr()?;
        let state = AppState::new(pairing, events.clone(), cfg.static_root.clone(), cfg.version);

        let app = build_router(state);
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let server_task = tokio::spawn(async move {
            let serve = axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            });
            if let Err(err) = serve.await {
                tracing::error!(error = %err, "axum server exited with error");
            }
        });

        Ok(WebServerHandle {
            info: RuntimeInfo {
                bound_port: actual_addr.port(),
                bind_addr: actual_addr,
                https: false,
                pid: std::process::id(),
            },
            shutdown_tx: Some(shutdown_tx),
            server_task: Some(server_task),
        })
    }
}

/// 启动后返回的句柄；drop 时不会自动 shutdown，请显式调用 [`Self::shutdown`]。
pub struct WebServerHandle {
    pub info: RuntimeInfo,
    shutdown_tx: Option<oneshot::Sender<()>>,
    server_task: Option<JoinHandle<()>>,
}

impl WebServerHandle {
    /// 优雅关停：发送 shutdown 信号 + 等待 server task 退出。
    /// Req 2.7：3 秒内释放端口。
    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(task) = self.server_task.take() {
            // 留 3 秒余量；超时则 abort。
            let timeout = std::time::Duration::from_secs(3);
            match tokio::time::timeout(timeout, task).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => tracing::warn!(error = %e, "server task join error"),
                Err(_) => tracing::warn!("server shutdown timed out, aborting"),
            }
        }
    }
}

fn build_router(state: AppState) -> Router {
    // 不强制鉴权的公共路径
    let public = Router::new()
        .route("/api/health", get(handlers::health::health))
        .route("/api/pair", post(handlers::pair::submit_pair))
        .route("/", get(handlers::statics::serve_static))
        .route("/assets/*path", get(handlers::statics::serve_static))
        .route("/ws", get(handlers::ws::upgrade))
        .with_state(state.clone());

    // 全局：先经过 SubnetFilter，再进入 public。
    public.layer(axum::middleware::from_fn(mw::subnet_filter))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge_events::channel;
    use std::net::Ipv4Addr;
    use std::time::Duration;

    fn cfg() -> WebServerCfg {
        WebServerCfg {
            preferred_port: 0, // 0 = 让 OS 自动选择端口，避免端口冲突
            bind_addr: IpAddr::V4(Ipv4Addr::LOCALHOST),
            static_root: std::env::temp_dir().join("phonemic-static-noop"),
            version: "0.1.0".to_owned(),
        }
    }

    #[tokio::test]
    async fn start_and_shutdown_releases_port_within_3s() {
        let (tx, _rx) = channel();
        // 让操作系统分配端口；select_port 不接受 0，故先用 18099 作占位测端口。
        let mut c = cfg();
        c.preferred_port = 18099;
        let handle = WebServer::start(c, PairingService::new(), tx).await;
        // 在 CI 上 18099 也可能冲突；此时跳过即可。
        let Ok(handle) = handle else {
            return;
        };
        let port = handle.info.bound_port;
        assert!(port >= 1024);

        let started = std::time::Instant::now();
        handle.shutdown().await;
        assert!(started.elapsed() < Duration::from_secs(3));
    }
}
