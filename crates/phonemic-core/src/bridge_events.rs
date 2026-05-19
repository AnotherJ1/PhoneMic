//! 桌面端跨子系统事件总线（任务 5.x / 6.x / 7.x / 8.x）。
//!
//! - 任务来源：`.kiro/specs/phone-mic-voice-input/tasks.md` 5.6 / 5.14 / 6.2 / 7.11 / 8.5
//! - 设计来源：`.kiro/specs/phone-mic-voice-input/design.md` §4.1 / §6.1 / §8.2
//!
//! Web Server / Discovery / Input_Injector / ASR Bridge 这些子系统不直接互相
//! 调用，而是通过本模块暴露的 [`BridgeEvents`] mpsc 通道把"用户可见"的事件
//! 转发到桌面端 UI（Tauri 侧）。事件类型集中定义在 [`BridgeEvent`] 枚举中，
//! 任何新增事件都必须在此先扩展枚举再被消费方处理。
//!
//! ## 设计要点
//!
//! - 通道按"事件无界数量"考虑容量：默认 256，足以承载几次 LAN 抖动 +
//!   数十次 inject error。生产环境若 UI 处理速度落后，会优雅地从队尾
//!   开始等待 sender，但永不丢消息（这是 Property 33 的前置条件）。
//! - [`BridgeEvent`] **不**直接序列化到客户端：错误码 / 文案对应关系由
//!   `phonemic-protocol::AppError` 与 `phonemic-app` 的 i18n 层独立完成。
//!   本通道只在桌面端进程内传递结构化事件。
//! - [`BridgeEvent::DevicePaired`] 等"机密敏感"事件携带的字段经过裁剪：
//!   永远不携带 `Session_Token` 原文（仅长度 + 摘要前缀），以满足 Req 9.7。

use std::net::IpAddr;
use std::time::SystemTime;

use phonemic_protocol::ErrorCode;
use tokio::sync::mpsc;

/// `BridgeEvents` 通道默认容量。
///
/// 256 ≫ 任意一次 LAN 抖动 / 注入失败连珠产生的事件数量；同时不至于在
/// 异常情况下无限堆积。如需更大缓冲，可由调用方使用 [`channel_with_capacity`]。
pub const DEFAULT_CHANNEL_CAPACITY: usize = 256;

/// 桌面端跨子系统事件。
///
/// 任何新增事件都应当：
/// 1. 在此枚举中定义结构化字段（避免 `String` 充当多意 payload）；
/// 2. 在 design.md §8.2 错误处理矩阵 / §6.1 事件流图中同步登记；
/// 3. 在桌面端 UI（任务 10.x）补一条对应的展示分支。
#[derive(Debug, Clone, PartialEq)]
pub enum BridgeEvent {
    /// 一台新设备完成配对（`POST /api/pair` 成功）。
    ///
    /// 由任务 5.6 在 `submit_pair` 成功后投递；桌面 UI 据此刷新"已配对
    /// 设备列表"面板（任务 10.3）。
    DevicePaired(DevicePairedEvent),
    /// 一个或多个 Session_Token 被显式吊销。
    ///
    /// 由任务 10.3 / 10.5 在用户点击"撤销"按钮时投递。
    DeviceRevoked(DeviceRevokedEvent),
    /// 键盘注入路径出现失败，向 Mobile 与桌面端 UI 同步通知。
    ///
    /// 由任务 7.11 / 7.12 投递；同时附带 `code` 字段方便桌面端日志层
    /// 直接做匹配。
    InjectError(InjectErrorEvent),
    /// ASR 引擎 10 秒看门狗触发（任务 8.5）。
    AsrTimeout(AsrTimeoutEvent),
    /// Web Server 启动失败：所选首选端口不可用（任务 5.14 / Req 2.8）。
    PortUnavailable(PortUnavailableEvent),
    /// 当前进程检测到所有 RFC1918 网卡均消失（任务 6.2 / Req 3.6）。
    LanLost,
    /// LAN 接口恢复（与 [`BridgeEvent::LanLost`] 配对）。
    LanRestored,
}

/// `BridgeEvent::DevicePaired` 的载荷。
///
/// 不携带 Session_Token 明文：UI 仅需要展示设备元数据，token 留在
/// `SessionRegistry` 内部。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevicePairedEvent {
    /// 设备指纹推导出的稳定 ID（`hex(SHA-256(fp))[..16]`）；与
    /// `Session::device_id` 完全一致，便于 UI 通过 ID 索引到 session。
    pub device_id: String,
    /// 用户可读设备标签（`PairRequest::device_label` 透传）。
    pub device_label: String,
    /// 配对来源 IP；用于日志 / 审计页（不展示给最终用户）。
    pub peer_ip: IpAddr,
    /// 配对成功时间戳（墙钟）。
    pub paired_at: SystemTime,
}

/// `BridgeEvent::DeviceRevoked` 的载荷。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceRevokedEvent {
    /// 受影响设备 ID 集合；批量吊销时一次发送多条 ID。
    pub device_ids: Vec<String>,
    /// 触发时刻（墙钟）。
    pub revoked_at: SystemTime,
}

/// `BridgeEvent::InjectError` 的载荷。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InjectErrorEvent {
    /// 关联的 `text.submit.id`（与 WS 消息一一对应）。
    pub submit_id: String,
    /// 错误码（强类型，便于桌面端做精确分支）。
    pub code: ErrorCode,
    /// 用户可见的简短描述（不含明文文本，遵守 Req 9.7）。
    pub message: String,
}

/// `BridgeEvent::AsrTimeout` 的载荷。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsrTimeoutEvent {
    /// 触发超时的"录音段"标识，由 ASR 桥在 `feed` 启动时生成。
    pub segment_id: String,
}

/// `BridgeEvent::PortUnavailable` 的载荷。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortUnavailableEvent {
    /// 配置中给出的首选端口；UI 用于"重试 / 修改端口"提示。
    pub preferred_port: u16,
}

/// `BridgeEvents` 通道发送端。
///
/// 内部仅是 [`tokio::sync::mpsc::Sender`] 的薄包装；之所以 newtype 化，
/// 是为了避免 Web Server / Discovery 等子系统拿到一个"无名 mpsc"的
/// 弱类型句柄，对外只暴露语义化方法。
#[derive(Clone, Debug)]
pub struct BridgeEventTx {
    inner: mpsc::Sender<BridgeEvent>,
}

impl BridgeEventTx {
    /// 异步发送事件；当通道容量耗尽时会等待对端消费。
    pub async fn send(&self, evt: BridgeEvent) -> Result<(), BridgeSendError> {
        self.inner.send(evt).await.map_err(|e| BridgeSendError(e.0))
    }

    /// 非阻塞发送：通道已满 / 已关闭时立即返回错误。
    ///
    /// 仅用于"绝不能阻塞调用者"的路径（例如 sync drop / panic hook）；
    /// 业务路径请优先使用 [`BridgeEventTx::send`]。
    pub fn try_send(&self, evt: BridgeEvent) -> Result<(), BridgeTrySendError> {
        self.inner
            .try_send(evt)
            .map_err(|e| match e {
                mpsc::error::TrySendError::Full(v) => BridgeTrySendError::Full(v),
                mpsc::error::TrySendError::Closed(v) => BridgeTrySendError::Closed(v),
            })
    }

    /// 提供底层 [`mpsc::Sender`] 的克隆，便于把通道注入到不便引用 newtype
    /// 的旧代码（如 third-party `tracing::Layer`）。新代码请使用 newtype。
    #[must_use]
    pub fn raw(&self) -> mpsc::Sender<BridgeEvent> {
        self.inner.clone()
    }
}

/// `BridgeEvents` 通道接收端。
///
/// 与 [`BridgeEventTx`] 对应；上层通常只持有一份 `BridgeEventRx`，由
/// Tauri 主线程在 `setup` 阶段消费。
#[derive(Debug)]
pub struct BridgeEventRx {
    inner: mpsc::Receiver<BridgeEvent>,
}

impl BridgeEventRx {
    /// 接收下一个事件；通道关闭时返回 `None`。
    pub async fn recv(&mut self) -> Option<BridgeEvent> {
        self.inner.recv().await
    }

    /// 关闭通道：调用后任何 sender 的 `send` / `try_send` 都会失败。
    pub fn close(&mut self) {
        self.inner.close();
    }
}

/// `send` 失败：对端 receiver 已 drop 或 channel 已关闭。
///
/// 携带未投递的事件，便于调用方记录或重试。
#[derive(Debug, thiserror::Error)]
#[error("BridgeEvents channel closed; event dropped")]
pub struct BridgeSendError(pub BridgeEvent);

/// `try_send` 的非阻塞版本错误码。
#[derive(Debug, thiserror::Error)]
pub enum BridgeTrySendError {
    /// 通道已满：调用方需要回退到异步 [`BridgeEventTx::send`]。
    #[error("BridgeEvents channel full")]
    Full(BridgeEvent),
    /// 通道已关闭。
    #[error("BridgeEvents channel closed")]
    Closed(BridgeEvent),
}

/// 创建一组 `BridgeEvents` 收发端。
#[must_use]
pub fn channel() -> (BridgeEventTx, BridgeEventRx) {
    channel_with_capacity(DEFAULT_CHANNEL_CAPACITY)
}

/// 指定容量版本，便于测试 / 特殊场景。
#[must_use]
pub fn channel_with_capacity(capacity: usize) -> (BridgeEventTx, BridgeEventRx) {
    let (tx, rx) = mpsc::channel(capacity.max(1));
    (BridgeEventTx { inner: tx }, BridgeEventRx { inner: rx })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;
    use std::time::Duration;

    fn sample_paired_event() -> BridgeEvent {
        BridgeEvent::DevicePaired(DevicePairedEvent {
            device_id: "abcd".repeat(8), // 32 hex chars
            device_label: "iPhone".to_owned(),
            peer_ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)),
            paired_at: SystemTime::UNIX_EPOCH + Duration::from_secs(100),
        })
    }

    #[tokio::test]
    async fn channel_round_trips_events_in_order() {
        let (tx, mut rx) = channel();
        let a = sample_paired_event();
        let b = BridgeEvent::LanLost;
        let c = BridgeEvent::LanRestored;

        tx.send(a.clone()).await.unwrap();
        tx.send(b.clone()).await.unwrap();
        tx.send(c.clone()).await.unwrap();

        assert_eq!(rx.recv().await, Some(a));
        assert_eq!(rx.recv().await, Some(b));
        assert_eq!(rx.recv().await, Some(c));
    }

    #[tokio::test]
    async fn channel_closed_after_rx_drop() {
        let (tx, rx) = channel_with_capacity(1);
        drop(rx);
        let err = tx.send(BridgeEvent::LanLost).await.unwrap_err();
        // 错误中应保留原始事件以便上层重试。
        assert!(matches!(err.0, BridgeEvent::LanLost));
    }

    #[tokio::test]
    async fn try_send_full_returns_event() {
        let (tx, _rx) = channel_with_capacity(1);
        tx.send(BridgeEvent::LanLost).await.unwrap();
        let err = tx.try_send(BridgeEvent::LanRestored).unwrap_err();
        assert!(matches!(err, BridgeTrySendError::Full(BridgeEvent::LanRestored)));
    }

    #[test]
    fn inject_error_event_is_clone_and_eq() {
        let evt = InjectErrorEvent {
            submit_id: "m-1".into(),
            code: ErrorCode::InjectNoFocusTarget,
            message: "no focus".into(),
        };
        let cloned = evt.clone();
        assert_eq!(evt, cloned);
    }
}
