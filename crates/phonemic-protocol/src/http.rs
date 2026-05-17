//! HTTP API 类型定义（任务 2.2）。
//!
//! 设计来源：`design.md` §5.2
//! 关联需求：7.2、7.3
//!
//! 涵盖的端点：
//! - `POST /api/pair`：请求 [`PairRequest`]，响应 [`PairResponse`]
//! - `GET  /api/health`：响应 [`HealthResponse`]
//!
//! 所有结构体使用 `#[serde(rename_all = "camelCase")]`，与设计文档中
//! 给出的 JSON 示例字段保持一致，方便移动端 / 桌面端共享同一份契约。

use serde::{Deserialize, Serialize};

/// `POST /api/pair` 的请求体（design.md §5.2）。
///
/// 字段说明：
/// - `pairing_code`：桌面端当前展示的 8 位配对码（`[A-Z0-9]`，去除易混字符）。
/// - `fingerprint`：移动端生成的设备指纹，hex 编码（design.md §4.3）。
/// - `device_label`：人类可读设备标签（如 `"iPhone 15"`），用于桌面端列表展示。
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PairRequest {
    /// 配对码明文，仅在 LAN 内传输；恒定时间比较由服务端完成（任务 3.11）。
    pub pairing_code: String,
    /// 设备指纹，hex 编码。
    pub fingerprint: String,
    /// 设备标签，由移动端从 UA / 屏幕分辨率推导。
    pub device_label: String,
}

/// `POST /api/pair` 的成功响应体（design.md §5.2）。
///
/// 字段说明：
/// - `session_token`：256 位随机数，Base64URL 编码（design.md §4.3）。
/// - `expires_at`：会话过期时间，RFC3339 / ISO-8601 字符串。
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PairResponse {
    /// 移动端后续 WebSocket / HTTP 调用使用的 Bearer Token。
    pub session_token: String,
    /// 过期时间，RFC3339 字符串（如 `"2025-01-01T12:00:00Z"`）。
    pub expires_at: String,
}

/// `GET /api/health` 的响应体（design.md §5.2）。
///
/// 字段说明：
/// - `version`：桌面端语义化版本号，形如 `"x.y.z"`。
/// - `uptime`：服务进程已运行秒数。
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    /// 桌面端版本号。
    pub version: String,
    /// 进程已运行秒数。
    pub uptime: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn keys_of(v: &serde_json::Value) -> BTreeSet<String> {
        v.as_object()
            .expect("expected JSON object")
            .keys()
            .cloned()
            .collect()
    }

    #[test]
    fn pair_request_uses_camel_case_keys() {
        let req = PairRequest {
            pairing_code: "ABCD2345".into(),
            fingerprint: "deadbeef".into(),
            device_label: "iPhone 15".into(),
        };
        let json = serde_json::to_value(&req).expect("serialize");
        let expected: BTreeSet<String> = ["pairingCode", "fingerprint", "deviceLabel"]
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        assert_eq!(keys_of(&json), expected);
    }

    #[test]
    fn pair_request_round_trips_through_json() {
        let req = PairRequest {
            pairing_code: "ABCD2345".into(),
            fingerprint: "deadbeef".into(),
            device_label: "iPhone 15".into(),
        };
        let s = serde_json::to_string(&req).unwrap();
        let parsed: PairRequest = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn pair_response_uses_camel_case_keys() {
        let resp = PairResponse {
            session_token: "abc.def".into(),
            expires_at: "2025-01-01T12:00:00Z".into(),
        };
        let json = serde_json::to_value(&resp).unwrap();
        let expected: BTreeSet<String> = ["sessionToken", "expiresAt"]
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        assert_eq!(keys_of(&json), expected);
    }

    #[test]
    fn pair_response_round_trips_through_json() {
        let resp = PairResponse {
            session_token: "abc.def".into(),
            expires_at: "2025-01-01T12:00:00Z".into(),
        };
        let s = serde_json::to_string(&resp).unwrap();
        let parsed: PairResponse = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn health_response_has_exactly_version_and_uptime_keys() {
        let resp = HealthResponse {
            version: "0.1.0".into(),
            uptime: 42,
        };
        let json = serde_json::to_value(&resp).unwrap();
        let expected: BTreeSet<String> = ["version", "uptime"]
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        assert_eq!(keys_of(&json), expected);
    }

    #[test]
    fn health_response_round_trips_through_json() {
        let resp = HealthResponse {
            version: "0.1.0".into(),
            uptime: 42,
        };
        let s = serde_json::to_string(&resp).unwrap();
        let parsed: HealthResponse = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed, resp);
    }
}
