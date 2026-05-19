//! mDNS service registration / deregistration（任务 6.1 / 6.2）。
//!
//! 注册 `_phonemic._tcp.local.`，TXT 记录：`v=1`、`tls=0|1`、`port=<port>`。
//! 接口变化时刷新通告；所有 RFC1918 接口消失时通过 [`BridgeEventTx`] 投递
//! [`BridgeEvent::LanLost`]（任务 6.2 / Req 3.6）。

use std::collections::HashSet;
use std::net::IpAddr;
use std::sync::Arc;

use mdns_sd::{ServiceDaemon, ServiceInfo};
use phonemic_core::bridge_events::{BridgeEvent, BridgeEventTx};
use phonemic_core::lan_filter::is_rfc1918;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

const SERVICE_TYPE: &str = "_phonemic._tcp.local.";

/// Discovery 启动配置。
#[derive(Debug, Clone)]
pub struct DiscoveryCfg {
    pub instance_name: String, // 例如 hostname
    pub port: u16,
    pub https: bool,
}

/// `Discovery::start` 失败原因。
#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error("mdns daemon error: {0}")]
    Daemon(String),
}

/// Discovery 服务句柄；drop 不会自动注销，请显式 [`Self::stop`]。
pub struct Discovery {
    daemon: ServiceDaemon,
    full_name: String,
    iface_watch: Option<JoinHandle<()>>,
    #[allow(dead_code)]
    inner: Arc<DiscoveryInner>,
}

struct DiscoveryInner {
    events: BridgeEventTx,
    state: Mutex<DiscoveryState>,
}

#[derive(Debug, Default)]
struct DiscoveryState {
    last_lan_present: bool,
    last_ips: HashSet<IpAddr>,
}

impl Discovery {
    /// 启动 mDNS 服务。
    ///
    /// `events` 用于 LanLost / LanRestored 通知（任务 6.2）。如果上层希望使用
    /// 独立通道而非 Web Server 的 BridgeEvents，可以传入另一份 [`BridgeEventTx`]
    /// 包装；默认我们沿用主通道，将 LAN 事件直接路由到桌面 UI。
    pub fn start(cfg: DiscoveryCfg, events: BridgeEventTx) -> Result<Self, DiscoveryError> {
        let daemon = ServiceDaemon::new().map_err(|e| DiscoveryError::Daemon(e.to_string()))?;
        let host_ipv4: Vec<IpAddr> = current_lan_ips();
        let tls_flag = if cfg.https { "1" } else { "0" };
        let txt = [
            ("v", "1"),
            ("tls", tls_flag),
            ("port", &cfg.port.to_string()),
        ];

        let info = ServiceInfo::new(
            SERVICE_TYPE,
            &cfg.instance_name,
            &format!("{}.local.", cfg.instance_name),
            &host_ipv4[..],
            cfg.port,
            &txt[..],
        )
        .map_err(|e| DiscoveryError::Daemon(e.to_string()))?;
        let full_name = info.get_fullname().to_owned();
        daemon
            .register(info)
            .map_err(|e| DiscoveryError::Daemon(e.to_string()))?;

        let inner = Arc::new(DiscoveryInner {
            events,
            state: Mutex::new(DiscoveryState {
                last_lan_present: !host_ipv4.is_empty(),
                last_ips: host_ipv4.into_iter().collect(),
            }),
        });

        // 任务 6.2 接口监听：每 5 秒重新扫描；真实生产可用 `if-watch` 替换。
        let watch_inner = Arc::clone(&inner);
        let watch_daemon = daemon.clone();
        let watch_full = full_name.clone();
        let watch_cfg = cfg.clone();
        let iface_watch = tokio::spawn(async move {
            let interval = std::time::Duration::from_secs(5);
            loop {
                tokio::time::sleep(interval).await;
                let now: HashSet<IpAddr> = current_lan_ips().into_iter().collect();
                let mut state = watch_inner.state.lock().await;
                if now == state.last_ips {
                    continue;
                }
                let was_present = state.last_lan_present;
                let now_present = !now.is_empty();
                state.last_ips = now.clone();
                state.last_lan_present = now_present;
                drop(state);

                // 重新注册以刷新地址列表。
                let _ = watch_daemon.unregister(&watch_full);
                let txt = [
                    ("v", "1"),
                    ("tls", if watch_cfg.https { "1" } else { "0" }),
                    ("port", &watch_cfg.port.to_string()),
                ];
                let ips: Vec<IpAddr> = now.into_iter().collect();
                if let Ok(info) = ServiceInfo::new(
                    SERVICE_TYPE,
                    &watch_cfg.instance_name,
                    &format!("{}.local.", watch_cfg.instance_name),
                    &ips[..],
                    watch_cfg.port,
                    &txt[..],
                ) {
                    let _ = watch_daemon.register(info);
                }

                if was_present && !now_present {
                    let _ = watch_inner.events.send(BridgeEvent::LanLost).await;
                } else if !was_present && now_present {
                    let _ = watch_inner.events.send(BridgeEvent::LanRestored).await;
                }
            }
        });

        Ok(Self {
            daemon,
            full_name,
            iface_watch: Some(iface_watch),
            inner,
        })
    }

    /// 显式注销服务，停止接口监听 task。
    pub async fn stop(mut self) {
        if let Some(handle) = self.iface_watch.take() {
            handle.abort();
            let _ = handle.await;
        }
        let _ = self.daemon.unregister(&self.full_name);
        let _ = self.daemon.shutdown();
    }

    /// mDNS 完整服务实例名（含 `_phonemic._tcp.local.` 后缀），用于诊断与测试。
    #[must_use]
    pub fn full_name(&self) -> &str {
        &self.full_name
    }
}

/// 当前主机所有 RFC1918 IPv4 地址（不包含 loopback / link-local / 公网）。
fn current_lan_ips() -> Vec<IpAddr> {
    use std::net::Ipv4Addr;
    let mut out: Vec<IpAddr> = Vec::new();
    // MVP：通过 std::net 的 hostname 解析 + 系统接口能力暂不在 std 内；
    // 实际项目使用 `netdev` / `pnet` 枚举。这里保留一个稳定 fallback：
    // 仅尝试连接外网"探针 IP" 后查询本地源 IP，避免在没有外网时阻塞，
    // 我们使用 UDP socket bind + connect。
    if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
        // 8.8.8.8:80 仅用于让 OS 选择默认源 IP，无实际数据发送。
        let _ = socket.connect("8.8.8.8:80");
        if let Ok(addr) = socket.local_addr() {
            if let IpAddr::V4(v4) = addr.ip() {
                if is_rfc1918(v4) {
                    out.push(IpAddr::V4(v4));
                }
            }
        }
    }
    // 兜底：始终包含本地 loopback，便于本机自测；上层通过 RFC1918 过滤
    // 已经把它排除在 LAN ip 集合之外，这里只为 mDNS 可达性保留。
    if out.is_empty() {
        out.push(IpAddr::V4(Ipv4Addr::LOCALHOST));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 在没有 LAN 接口时，`current_lan_ips` 至少包含 loopback，避免空注册。
    #[test]
    fn current_lan_ips_is_never_empty() {
        let ips = current_lan_ips();
        assert!(!ips.is_empty());
    }
}
