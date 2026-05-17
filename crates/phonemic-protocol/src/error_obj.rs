//! 统一错误对象 `AppError`（任务 2.2）。
//!
//! 设计来源：`design.md` §8.1
//! 关联需求：7.2、7.3、9.6
//!
//! 所有跨 HTTP / WebSocket / 桌面端日志的错误都以此结构序列化，确保
//! 前端集中处理（design.md §8.2 "结构化优先"）。
//!
//! 结构示例（design.md §8.1）：
//!
//! ```json
//! {
//!   "code":    "INJECT_NO_FOCUS_TARGET",
//!   "message": "当前操作系统无可识别的输入焦点",
//!   "detail":  { "platform": "macos" },
//!   "ts":      "2025-01-01T12:00:00.123Z"
//! }
//! ```

use serde::{Deserialize, Serialize};

/// 统一错误对象。
///
/// 字段说明：
/// - `code`：错误码字符串。在任务 2.3 完成后会替换为强类型 `ErrorCode` 枚举。
/// - `message`：可向用户展示的简短文案，禁止包含 Pairing_Code、Session_Token
///   或文本明文（design.md §8.2，Requirement 9.7）。
/// - `detail`：可选的结构化补充信息（任意 JSON 值）；为 `None` 时序列化省略。
/// - `ts`：错误产生时刻，调用方应填入 RFC3339 / ISO-8601 字符串
///   （如 `"2025-01-01T12:00:00.123Z"`）。
//
// TODO(2.3): switch to ErrorCode enum once available
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    /// 错误码（占位为 String，2.3 任务会替换为 `ErrorCode` 枚举）。
    pub code: String,
    /// 用户可见的错误消息，不得包含敏感信息。
    pub message: String,
    /// 结构化补充信息；`None` 时不会出现在 JSON 中。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<serde_json::Value>,
    /// 错误时间戳，RFC3339 字符串（如 `"2025-01-01T12:00:00.123Z"`）。
    pub ts: String,
}

impl AppError {
    /// 使用调用方提供的时间戳构造错误对象。
    ///
    /// `ts` 应为 RFC3339 字符串；调用方未持有时间源时可使用 [`AppError::now`]。
    #[must_use]
    pub fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        ts: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            detail: None,
            ts: ts.into(),
        }
    }

    /// 使用系统时钟（UTC）生成时间戳，构造错误对象。
    ///
    /// 内部依赖 [`std::time::SystemTime`] 与 [`now_rfc3339`]，实现保持最小化，
    /// 仅产出毫秒精度 `Z`-后缀字符串。
    #[must_use]
    pub fn now(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(code, message, now_rfc3339())
    }

    /// 链式 setter：附加结构化 detail。
    #[must_use]
    pub fn with_detail(mut self, value: serde_json::Value) -> Self {
        self.detail = Some(value);
        self
    }
}

/// 返回当前时刻的 RFC3339 / ISO-8601 字符串（UTC，毫秒精度，`Z` 后缀）。
///
/// 实现完全基于 `std::time` 与基础整数运算，避免引入额外日期库依赖。
#[must_use]
pub fn now_rfc3339() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    // 转换为 i64 是安全的：未来数十亿年内不会溢出。
    let total_secs = i64::try_from(dur.as_secs()).unwrap_or(i64::MAX);
    epoch_millis_to_rfc3339(total_secs, dur.subsec_millis())
}

/// 将 UNIX epoch 秒 + 毫秒转换为 `YYYY-MM-DDTHH:MM:SS.mmmZ` 字符串。
///
/// 使用 Howard Hinnant 公开域算法（civil_from_days），在 `[1970, 9999]` 范围内
/// 给出正确的公历分解，可独立通过单元测试验证。
fn epoch_millis_to_rfc3339(secs_since_epoch: i64, millis: u32) -> String {
    let days = secs_since_epoch.div_euclid(86_400);
    let mut secs_in_day = secs_since_epoch.rem_euclid(86_400);
    let hour = secs_in_day / 3600;
    secs_in_day %= 3600;
    let minute = secs_in_day / 60;
    let second = secs_in_day % 60;
    let (y, mo, d) = civil_from_days(days);
    format!(
        "{y:04}-{mo:02}-{d:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z"
    )
}

/// 将"自 1970-01-01 起的天数"分解为 `(year, month, day)`。
///
/// 算法参考 Howard Hinnant 的 `chrono::civil_from_days`（公开域）。
fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32; // [0, 146_096]
    let yoe =
        (doe.wrapping_sub(doe / 1_460) + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = (yoe as i32) + (era as i32) * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeSet;

    fn keys_of(v: &serde_json::Value) -> BTreeSet<String> {
        v.as_object()
            .expect("expected JSON object")
            .keys()
            .cloned()
            .collect()
    }

    #[test]
    fn app_error_without_detail_serializes_three_keys() {
        let err = AppError::new("PAIR_INVALID", "配对码错误", "2025-01-01T12:00:00.000Z");
        let v = serde_json::to_value(&err).unwrap();
        let expected: BTreeSet<String> = ["code", "message", "ts"]
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        assert_eq!(keys_of(&v), expected, "detail=None 时应当被 skip");
    }

    #[test]
    fn app_error_with_detail_serializes_four_keys() {
        let err = AppError::new(
            "INJECT_NO_FOCUS_TARGET",
            "当前操作系统无可识别的输入焦点",
            "2025-01-01T12:00:00.123Z",
        )
        .with_detail(json!({ "platform": "macos" }));
        let v = serde_json::to_value(&err).unwrap();
        let expected: BTreeSet<String> = ["code", "message", "detail", "ts"]
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        assert_eq!(keys_of(&v), expected);
        assert_eq!(v["detail"], json!({ "platform": "macos" }));
    }

    #[test]
    fn app_error_round_trips_without_detail() {
        let err = AppError::new("PAIR_INVALID", "配对码错误", "2025-01-01T12:00:00.000Z");
        let s = serde_json::to_string(&err).unwrap();
        let parsed: AppError = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed, err);
        assert!(parsed.detail.is_none());
    }

    #[test]
    fn app_error_round_trips_with_detail() {
        let err = AppError::new("MSG_BAD_FORMAT", "非法消息", "2025-01-01T12:00:00.500Z")
            .with_detail(json!({ "field": "type" }));
        let s = serde_json::to_string(&err).unwrap();
        let parsed: AppError = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed, err);
        assert_eq!(parsed.detail.as_ref().unwrap()["field"], "type");
    }

    #[test]
    fn app_error_now_produces_rfc3339_ts() {
        let err = AppError::now("LAN_LOST", "未检测到局域网连接");
        // 形如 YYYY-MM-DDTHH:MM:SS.mmmZ，长度 24
        assert_eq!(err.ts.len(), 24, "ts: {}", err.ts);
        assert!(err.ts.ends_with('Z'));
        assert!(err.ts.chars().nth(10) == Some('T'));
    }

    #[test]
    fn epoch_zero_formats_to_unix_epoch_string() {
        let s = epoch_millis_to_rfc3339(0, 0);
        assert_eq!(s, "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn epoch_known_timestamp_round_trip() {
        // 2025-01-01T12:00:00.123Z = 1_735_732_800 秒 + 123 毫秒
        let s = epoch_millis_to_rfc3339(1_735_732_800, 123);
        assert_eq!(s, "2025-01-01T12:00:00.123Z");
    }

    #[test]
    fn civil_from_days_handles_leap_year() {
        // 自 1970-01-01 起，2000-03-01 = 第 11_017 天，向前一天即 2000-02-29 闰日。
        let days_2000_03_01 = 11_017_i64;
        assert_eq!(civil_from_days(days_2000_03_01 - 1), (2000, 2, 29));
        assert_eq!(civil_from_days(days_2000_03_01), (2000, 3, 1));
    }
}
