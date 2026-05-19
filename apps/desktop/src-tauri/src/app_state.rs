//! 桌面端运行时状态 —— 把 phonemic-core 的多个组件聚合给 Tauri 命令层使用。
//!
//! 任务来源：tasks.md 10.x。
//! 设计来源：design.md §4.1（AppController）。
//!
//! 本结构刻意保持"瘦"：仅持有 [`Mutex`] 包装的少量状态，复杂业务由
//! `phonemic-core` 与 `phonemic-app` 完成。Web Server / Discovery / ASR
//! 在 worker-backend 完成它们的子任务后，会通过 `attach_*` 方法把句柄
//! 注入进来。

use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use phonemic_core::bridge_events::{BridgeEvent, BridgeEventTx, InjectErrorEvent};
use phonemic_core::i18n::{decide_lang, Lang};
use phonemic_core::lan_filter::NetworkInterface;
use phonemic_core::lan_view::compute_lan_view;
use phonemic_core::pairing_service::PairingService;
use phonemic_core::session::{DeviceFingerprint, SessionRegistry};
use phonemic_protocol::{AppConfig, ErrorCode};
use phonemic_injector::{InjectorEventSink, InputInjector, NullSink, VirtualBackend};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::commands::{PairingCodeView, RuntimeInfo};

/// 已配对设备视图（与 worker-mobile-e2e 对齐的 JSON schema）。
#[derive(Debug, Clone, Serialize)]
pub struct SessionView {
    pub device_id: String,
    pub device_label: String,
    pub fingerprint_short: String,
    pub last_used_at: String,
    pub created_at: String,
}

/// Tauri 后端共享状态。
pub struct DesktopState {
    config: Mutex<AppConfig>,
    pairing: Mutex<PairingService>,
    /// 输入注入器（默认是 [`VirtualBackend`]，桌面端构建会替换为平台后端）。
    injector: Arc<dyn InputInjector>,
    /// 注入事件出站接收器（worker-backend 注入 WS sink 之前为 [`NullSink`]）。
    sink: Mutex<Arc<dyn InjectorEventSink>>,
    started_at: Instant,
    /// 当前 Web Server 监听端口；Web Server 启动后由 `attach_runtime` 设置。
    port: Mutex<u16>,
    scheme: Mutex<String>,
    ips: Mutex<Vec<String>>,
    /// `revoke_device` 时记录指纹 → 当前 session 数量，方便 UI 展示"撤销了 N 条"。
    fingerprint_index: Mutex<Vec<DeviceFingerprint>>,
}

impl DesktopState {
    /// 构造一个使用 [`VirtualBackend`] 的桌面状态（默认装配，CI / 测试常用）。
    #[must_use]
    pub fn new_virtual() -> Self {
        let cfg = AppConfig::default();
        let pairing = PairingService::default();
        let injector: Arc<dyn InputInjector> = Arc::new(VirtualBackend::default());
        Self {
            config: Mutex::new(cfg),
            pairing: Mutex::new(pairing),
            injector,
            sink: Mutex::new(Arc::new(NullSink)),
            started_at: Instant::now(),
            port: Mutex::new(0),
            scheme: Mutex::new("http".to_string()),
            ips: Mutex::new(Vec::new()),
            fingerprint_index: Mutex::new(Vec::new()),
        }
    }

    /// 构造时直接传入特定平台后端实例（桌面端 release 路径）。
    #[must_use]
    pub fn new_with_injector(injector: Arc<dyn InputInjector>) -> Self {
        let mut s = Self::new_virtual();
        s.injector = injector;
        s
    }

    /// worker-backend 完成 Web Server 启动后调用，更新端口 / scheme / IP 列表。
    pub fn attach_runtime(&self, scheme: &str, port: u16, ips: Vec<String>) {
        *self.scheme.lock().unwrap() = scheme.to_string();
        *self.port.lock().unwrap() = port;
        *self.ips.lock().unwrap() = ips;
    }

    /// worker-backend 提供 `WsOutbound` 后调用，把出站接收器替换上去。
    pub fn set_injector_sink(&self, sink: Arc<dyn InjectorEventSink>) {
        *self.sink.lock().unwrap() = sink;
    }

    /// 借用注入器（用于消息分发器调用 `inject_text`）。
    #[must_use]
    pub fn injector(&self) -> Arc<dyn InputInjector> {
        Arc::clone(&self.injector)
    }

    /// 借用当前出站接收器（worker-backend 把接收到的注入失败转发给它）。
    #[must_use]
    pub fn sink(&self) -> Arc<dyn InjectorEventSink> {
        Arc::clone(&*self.sink.lock().unwrap())
    }

    /// 当前配置快照。
    #[must_use]
    pub fn config(&self) -> AppConfig {
        self.config.lock().unwrap().clone()
    }

    /// 持久化配置（仅更新内存；任务 10.4 会接入 `save_to_path`）。
    ///
    /// # Errors
    ///
    /// 当 `cfg.validate()` 失败时返回错误。
    pub fn save_config(&self, cfg: AppConfig) -> Result<(), String> {
        cfg.validate().map_err(|e| e.to_string())?;
        // 同步注入相关字段到注入器实例。
        self.injector.set_delay_ms(u32::from(cfg.input.inject_delay_ms));
        self.injector.pause(cfg.input.paused);
        *self.config.lock().unwrap() = cfg;
        Ok(())
    }

    /// 设置注入暂停状态（同时通过托盘菜单 / 设置面板触发）。
    pub fn set_inject_paused(&self, paused: bool) {
        self.injector.pause(paused);
        self.config.lock().unwrap().input.paused = paused;
    }

    /// 设置字符间延迟。
    pub fn set_inject_delay_ms(&self, delay_ms: u16) {
        self.injector.set_delay_ms(u32::from(delay_ms));
        self.config.lock().unwrap().input.inject_delay_ms = delay_ms;
    }

    /// 当前 Pairing_Code 视图。
    #[must_use]
    pub fn pairing_code_view(&self) -> PairingCodeView {
        let p = self.pairing.lock().unwrap();
        let code = p.current_pairing_code().as_str().to_string();
        PairingCodeView {
            qr_svg: render_qr_svg(&code),
            code,
        }
    }

    /// 重新生成 Pairing_Code（保留 sessions）。
    #[must_use]
    pub fn regenerate_pairing_code(&self) -> PairingCodeView {
        let mut p = self.pairing.lock().unwrap();
        let new_code = p.rotate_code();
        let code = new_code.as_str().to_string();
        PairingCodeView {
            qr_svg: render_qr_svg(&code),
            code,
        }
    }

    /// 已配对设备列表。
    #[must_use]
    pub fn list_sessions(&self) -> Vec<SessionView> {
        let p = self.pairing.lock().unwrap();
        p.list_sessions()
            .into_iter()
            .map(|s| SessionView {
                device_id: s.device_id.clone(),
                device_label: s.device_label.clone(),
                fingerprint_short: s.device_id.chars().take(8).collect(),
                last_used_at: rfc3339_from_systemtime(s.last_seen),
                created_at: rfc3339_from_systemtime(s.paired_at),
            })
            .collect()
    }

    /// 吊销单个会话（按 device_id 精确匹配）。
    ///
    /// 由于 SessionRegistry 不直接提供"按 device_id 吊销"，本方法先线性
    /// 扫描 list_sessions，命中后用 `revoke_device(fingerprint)` —— 此处
    /// 我们用 device_id 反向定位 fingerprint 不可行，因此改为在桌面状态
    /// 里维护一份 `fingerprint_index`，由 add_session 路径同步。
    ///
    /// # Errors
    /// 找不到对应设备时返回错误字符串。
    pub fn revoke_session(&self, device_id: &str) -> Result<(), String> {
        let mut p = self.pairing.lock().unwrap();
        let fp_opt: Option<DeviceFingerprint> = {
            let idx = self.fingerprint_index.lock().unwrap();
            idx.iter()
                .find(|fp| {
                    // device_id 是 SHA-256(fp)[..16].to_hex()；此处用 list_sessions 的
                    // 现有 device_id 与传入比对。
                    p.list_sessions()
                        .iter()
                        .any(|s| s.device_id == device_id && fingerprint_matches(fp, &s.device_id))
                })
                .cloned()
        };
        match fp_opt {
            Some(fp) => {
                p.revoke_device(&fp);
                Ok(())
            }
            None => Err(format!("device_id {device_id} not found")),
        }
    }

    /// 吊销全部会话，返回受影响数量。
    pub fn revoke_all_sessions(&self) -> u32 {
        let mut p = self.pairing.lock().unwrap();
        let count = p.list_sessions().len();
        // 通过 sessions_mut 直接重置注册表。
        *p.sessions_mut() = SessionRegistry::new();
        u32::try_from(count).unwrap_or(u32::MAX)
    }

    /// 当前 RuntimeInfo 快照。
    #[must_use]
    pub fn runtime_info(&self) -> RuntimeInfo {
        let cfg = self.config();
        let ips = self.ips.lock().unwrap().clone();
        let port = *self.port.lock().unwrap();
        let scheme = self.scheme.lock().unwrap().clone();
        let urls: Vec<String> = ips
            .iter()
            .map(|ip| format!("{scheme}://{ip}:{port}"))
            .collect();
        let ifaces: Vec<NetworkInterface> = ips
            .iter()
            .filter_map(|ip| {
                ip.parse::<std::net::Ipv4Addr>().ok().map(|v4| NetworkInterface {
                    name: "iface".to_string(),
                    addrs: vec![std::net::IpAddr::V4(v4)],
                })
            })
            .collect();
        let lang = match cfg.ui.language {
            phonemic_protocol::config::UiLanguage::ZhCN => Lang::ZhCN,
            phonemic_protocol::config::UiLanguage::EnUS => Lang::EnUS,
            phonemic_protocol::config::UiLanguage::Auto => decide_lang(
                sys_locale::get_locale().unwrap_or_else(|| "en-US".to_string()).as_str(),
            ),
        };
        let view = compute_lan_view(&ifaces, lang);
        RuntimeInfo {
            scheme,
            port,
            ips,
            urls,
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_secs: self.started_at.elapsed().as_secs(),
            lan_disabled: view.scan_disabled,
            banner: view.banner.clone(),
            paused: cfg.input.paused,
            inject_delay_ms: cfg.input.inject_delay_ms,
        }
    }
}

/// 桥接：worker-backend 在配对成功路径上 push fingerprint 进 `state.fingerprint_index`，
/// 这样 `revoke_session(device_id)` 才能反查到原始 fingerprint。
impl DesktopState {
    pub fn record_pairing(&self, fp: DeviceFingerprint) {
        let mut idx = self.fingerprint_index.lock().unwrap();
        if !idx.contains(&fp) {
            idx.push(fp);
        }
    }
}

fn fingerprint_matches(fp: &DeviceFingerprint, expected_device_id: &str) -> bool {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(fp.0.as_bytes());
    let actual = hex::encode(&digest[..16]);
    actual == expected_device_id
}

fn render_qr_svg(text: &str) -> String {
    // 占位 SVG：worker-backend 完成 discovery::qr 后由 phonemic-discovery 提供真实二维码。
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 200 200\">\
         <rect width=\"200\" height=\"200\" fill=\"#fff\"/>\
         <text x=\"100\" y=\"100\" font-size=\"14\" text-anchor=\"middle\">{}</text>\
         </svg>",
        html_escape(text)
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn rfc3339_from_systemtime(t: SystemTime) -> String {
    let dur = t.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = i64::try_from(dur.as_secs()).unwrap_or(i64::MAX);
    let millis = dur.subsec_millis();
    epoch_millis_to_rfc3339(secs, millis)
}

fn epoch_millis_to_rfc3339(secs: i64, millis: u32) -> String {
    let days = secs.div_euclid(86_400);
    let mut sid = secs.rem_euclid(86_400);
    let hour = sid / 3600;
    sid %= 3600;
    let minute = sid / 60;
    let second = sid % 60;
    let (y, mo, d) = civil_from_days(days);
    format!("{y:04}-{mo:02}-{d:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe.wrapping_sub(doe / 1_460) + doe / 36_524 - doe / 146_096) / 365;
    let y = (yoe as i32) + (era as i32) * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// 帮 main lib 在启动序列中向前端发事件。
pub fn emit_startup_stage(app: &AppHandle, stage: &str, message: &str, ready: bool) {
    let _ = app.emit(
        "phonemic://startup-stage",
        serde_json::json!({ "stage": stage, "message": message, "ready": ready }),
    );
}

/// `InjectorEventSink` 实现：把注入失败 / 成功事件转换为 [`BridgeEvent::InjectError`]
/// 投递到 worker-backend 提供的 [`BridgeEventTx`]，由后者写到 WS 出站层。
///
/// 任务 7.11 / 13.3 的最后一公里：把 `phonemic-injector` 的错误码桥接到
/// `phonemic-core::bridge_events`，从而让 ASR 与 Injector 共享同一个 sink 抽象。
pub struct BridgeEventSink {
    tx: BridgeEventTx,
}

impl BridgeEventSink {
    /// 用 worker-backend 提供的 [`BridgeEventTx`] 构造 sink。
    #[must_use]
    pub fn new(tx: BridgeEventTx) -> Self {
        Self { tx }
    }
}

impl InjectorEventSink for BridgeEventSink {
    fn on_inject_error(&self, code: &str, message: &str, request_id: Option<&str>) {
        let parsed = code.parse::<ErrorCode>().unwrap_or(ErrorCode::InjectBackendError);
        let evt = BridgeEvent::InjectError(InjectErrorEvent {
            submit_id: request_id.unwrap_or("").to_string(),
            code: parsed,
            message: message.to_string(),
        });
        // 同步路径不能等：用 try_send，丢弃满 / 关闭情况由 tracing 记录。
        if let Err(e) = self.tx.try_send(evt) {
            tracing::warn!(error = ?e, "BridgeEventSink: inject.error try_send failed");
        }
    }

    fn on_inject_ack(&self, _request_id: Option<&str>, _chars: usize) {
        // BridgeEvents 当前不携带 inject.ack；ack 由 worker-backend 的
        // WsOutbound 直接落到 `inject.ack` ServerMessage。此处保持空实现。
    }
}

/// 任务 13.3：把 [`phonemic_core::bridge_events::BridgeEvent`] 转发到 Tauri
/// 前端事件，让桌面 UI 通知中心 / mobile views 能收到统一通道的错误 / 状态变化。
///
/// 调用方在 Tauri `setup` 中 `spawn` 这个函数，把 worker-backend 创建的
/// `BridgeEventRx` 移交进来即可：
/// ```ignore
/// tauri::async_runtime::spawn(forward_bridge_events(app_handle, rx));
/// ```
pub async fn forward_bridge_events(
    app: AppHandle,
    mut rx: phonemic_core::bridge_events::BridgeEventRx,
) {
    while let Some(evt) = rx.recv().await {
        let (channel, payload) = match evt {
            BridgeEvent::DevicePaired(e) => (
                "phonemic://session-changed",
                serde_json::json!({
                    "kind": "added",
                    "device_id": e.device_id,
                    "device_label": e.device_label,
                }),
            ),
            BridgeEvent::DeviceRevoked(e) => (
                "phonemic://session-changed",
                serde_json::json!({ "kind": "revoked", "device_ids": e.device_ids }),
            ),
            BridgeEvent::InjectError(e) => (
                "phonemic://inject-error",
                serde_json::json!({
                    "code": e.code.as_str(),
                    "message": e.message,
                    "request_id": e.submit_id,
                }),
            ),
            BridgeEvent::AsrTimeout(e) => (
                "phonemic://asr-error",
                serde_json::json!({ "code": "ASR_TIMEOUT", "segment_id": e.segment_id }),
            ),
            BridgeEvent::PortUnavailable(e) => (
                "phonemic://port-unavailable",
                serde_json::json!({ "preferred_port": e.preferred_port }),
            ),
            BridgeEvent::LanLost => (
                "phonemic://lan-changed",
                serde_json::json!({ "disabled": true, "ips": [] }),
            ),
            BridgeEvent::LanRestored => (
                "phonemic://lan-changed",
                serde_json::json!({ "disabled": false }),
            ),
        };
        let _ = app.emit(channel, payload);
    }
}
