//! Web Server 跨请求共享状态（任务 5.x）。
//!
//! `AppState` 汇集：
//! - [`PairingService`]：配对码 + 限流 + Session 注册表（任务 3.13 / 3.15 / 3.18）；
//! - [`BridgeEventTx`]：投递跨子系统事件（任务 5.6 / 5.14 / 6.2 / 7.11 / 8.5）；
//! - 服务静态资源根（`apps/desktop/src-tauri/resources/web/`，任务 5.8）；
//! - 启动时刻（用于 `/api/health.uptime`，任务 5.7）；
//! - 版本号（默认取 `CARGO_PKG_VERSION`，可由 Tauri 注入覆盖）。

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::Mutex;

use crate::bridge_events::BridgeEventTx;
use crate::pairing_service::PairingService;

/// 跨 axum handler 共享的运行时状态，使用 `tokio::sync::Mutex` 包裹的
/// `PairingService` 以兼容 async handler 中的临界区。
#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    pub pairing: Mutex<PairingService>,
    pub events: BridgeEventTx,
    pub static_root: PathBuf,
    pub version: String,
    pub started_at: Instant,
}

impl AppState {
    /// 构造一个新状态。`pairing` 由调用方先行初始化，便于注入测试用 stub。
    #[must_use]
    pub fn new(
        pairing: PairingService,
        events: BridgeEventTx,
        static_root: PathBuf,
        version: impl Into<String>,
    ) -> Self {
        Self {
            inner: Arc::new(AppStateInner {
                pairing: Mutex::new(pairing),
                events,
                static_root,
                version: version.into(),
                started_at: Instant::now(),
            }),
        }
    }

    /// 借用 [`PairingService`] 锁；调用方需 `await`。
    pub async fn pairing(&self) -> tokio::sync::MutexGuard<'_, PairingService> {
        self.inner.pairing.lock().await
    }

    /// 事件总线发送端的共享克隆。
    #[must_use]
    pub fn events(&self) -> &BridgeEventTx {
        &self.inner.events
    }

    /// 静态资源根目录。
    #[must_use]
    pub fn static_root(&self) -> &std::path::Path {
        &self.inner.static_root
    }

    /// 版本号字符串。
    #[must_use]
    pub fn version(&self) -> &str {
        &self.inner.version
    }

    /// 进程已运行秒数（用于 `/api/health`）。
    #[must_use]
    pub fn uptime_secs(&self) -> u64 {
        self.inner.started_at.elapsed().as_secs()
    }
}
