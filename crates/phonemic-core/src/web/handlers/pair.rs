//! `POST /api/pair` 实现（任务 5.6）。
//!
//! 校验 + 限流 + 颁发 Session_Token 三步走，每一步失败都返回结构化
//! AppError；成功则同步通过 BridgeEvents 投递 `DevicePaired` 事件。

use std::net::SocketAddr;
use std::time::{Instant, SystemTime};

use axum::extract::{ConnectInfo, State};
use axum::http::HeaderMap;
use axum::Json;
use phonemic_protocol::http::{PairRequest, PairResponse};

use crate::bridge_events::{BridgeEvent, DevicePairedEvent};
use crate::pair_rate_limit::FAILURE_WINDOW;
use crate::pairing_service::PairError;
use crate::session::DeviceFingerprint;
use crate::web::errors::ApiError;
use crate::web::state::AppState;

pub async fn submit_pair(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<PairRequest>,
) -> Result<Json<PairResponse>, ApiError> {
    let peer = peer_ip(addr, &headers);
    let mut svc = state.pairing().await;
    let now = SystemTime::now();
    let mono = Instant::now();

    let token = svc
        .submit_pair(
            &req.pairing_code,
            DeviceFingerprint::from(req.fingerprint),
            req.device_label.clone(),
            peer,
            now,
            mono,
        )
        .map_err(|e| match e {
            PairError::Invalid => ApiError::pair_invalid(),
            PairError::RateLimited => ApiError::pair_rate_limit(FAILURE_WINDOW.as_secs()),
        })?;

    // 投递事件：注意 device_id 通过 fingerprint 衍生，此处需要从 session
    // 注册表反查最新颁发的 session。
    let session = svc
        .sessions()
        .validate(&token)
        .expect("just-issued token must validate");
    drop(svc);

    let event = BridgeEvent::DevicePaired(DevicePairedEvent {
        device_id: session.device_id.clone(),
        device_label: req.device_label,
        peer_ip: peer,
        paired_at: session.paired_at,
    });
    // 投递失败仅记录，不改变 HTTP 响应（设备已经成功配对）。
    if let Err(err) = state.events().send(event).await {
        tracing::warn!(error = %err, "failed to publish DevicePaired event");
    }

    let expires_at = format_expires_at(session.paired_at);
    Ok(Json(PairResponse {
        session_token: token.as_str().to_owned(),
        expires_at,
    }))
}

fn peer_ip(addr: SocketAddr, headers: &HeaderMap) -> std::net::IpAddr {
    // 复用 middleware::effective_peer_ip 的逻辑，避免重复代码。
    super::super::middleware::effective_peer_ip(addr.ip(), headers)
}

/// MVP：默认 30 天后过期；与 `SecurityCfg.auto_revoke_idle_days` 同步设计稍后接入。
fn format_expires_at(paired_at: SystemTime) -> String {
    use std::time::Duration;
    let expires = paired_at + Duration::from_secs(30 * 24 * 3600);
    let dur = expires
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = i64::try_from(dur.as_secs()).unwrap_or(i64::MAX);
    epoch_secs_to_rfc3339(secs)
}

fn epoch_secs_to_rfc3339(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let mut secs_in_day = secs.rem_euclid(86_400);
    let hour = secs_in_day / 3600;
    secs_in_day %= 3600;
    let minute = secs_in_day / 60;
    let second = secs_in_day % 60;
    let (y, mo, d) = civil_from_days(days);
    format!("{y:04}-{mo:02}-{d:02}T{hour:02}:{minute:02}:{second:02}.000Z")
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

// `effective_peer_ip` 已是 pub；通过 `super::super::middleware::effective_peer_ip`
// 直接调用即可，本模块不需要再 re-export。
