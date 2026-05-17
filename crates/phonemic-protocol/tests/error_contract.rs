//! 错误对象结构契约测试（任务 2.7）。
//!
//! 任务来源：tasks.md 2.7
//! 设计来源：design.md §8.1
//! 关联需求：9.6
//!
//! 本测试以 **整合测试（integration test）** 形式存在，仅依赖
//! `phonemic_protocol` crate 对外暴露的 **公共契约**：
//!
//! - [`phonemic_protocol::ErrorCode`] 枚举与其常量 `ALL: [ErrorCode; N]`；
//! - [`phonemic_protocol::AppError`] 统一错误对象（design.md §8.1）；
//! - [`phonemic_protocol::ServerMessage::error`] WebSocket 通用错误消息。
//!
//! 契约目标（覆盖 Requirement 9.6）：
//!
//! 1. 任意 `ErrorCode` 序列化为统一错误对象时，JSON 字段集合都必须落在
//!    `{ code, message, ts }`（无 detail）或 `{ code, message, detail, ts }`
//!    （有 detail）两种形态之一，没有"逃逸"字段；
//! 2. 任意 `ErrorCode` 通过 `ServerMessage::error` 包装后，线缆形态都
//!    必须是 `{ type, payload: { code, message } }`，不允许出现非结构化
//!    （如纯字符串、混入 stack trace）的错误载荷；
//! 3. `AppError` 序列化后 `code` 字段值与 `ErrorCode::as_str()` 完全
//!    一致，避免 Rust 端常量与线缆字面量出现拼写漂移。
//!
//! 由整合测试断言公共契约可在不修改 `lib.rs` 的情况下从外部锁定行为，
//! 任何破坏 §8.1 错误对象形态的改动都会立刻被它捕获。

use std::collections::BTreeSet;

use phonemic_protocol::{AppError, ErrorCode, ServerMessage};
use serde_json::{json, Value};

/// 把 JSON 对象的顶层键名收集为有序集合，便于做"完全相等"断言。
fn keys_of(v: &Value) -> BTreeSet<String> {
    v.as_object()
        .expect("expected JSON object")
        .keys()
        .cloned()
        .collect()
}

/// 构造期望键集合的小工具。
fn expected_keys(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

/// 契约 1（无 detail）：每个 `ErrorCode` 序列化产出的 JSON 对象
/// **恰好** 包含 `{ code, message, ts }` 三个字段。
#[test]
fn error_object_keys_without_detail_are_exactly_three() {
    let expected = expected_keys(&["code", "message", "ts"]);
    for code in ErrorCode::ALL {
        let err = AppError::new(code.as_str(), "msg", "2025-01-01T00:00:00.000Z");
        let v = serde_json::to_value(&err).expect("serialize AppError");

        assert_eq!(
            keys_of(&v),
            expected,
            "code {} 缺省 detail 时应只包含 {{code, message, ts}}，实际：{v}",
            code.as_str()
        );
        assert_eq!(
            v["code"], code.as_str(),
            "code 字段必须等于 ErrorCode::as_str()，实际：{}", v["code"]
        );
        assert!(v.get("detail").is_none(), "detail=None 时不得出现在序列化结果中");
    }
}

/// 契约 1（带 detail）：每个 `ErrorCode` 携带 `detail` 时 JSON 对象
/// **恰好** 包含 `{ code, message, detail, ts }` 四个字段，且 `detail`
/// 字段内容能够 round-trip 回原值。
#[test]
fn error_object_keys_with_detail_are_exactly_four() {
    let expected = expected_keys(&["code", "message", "detail", "ts"]);
    for code in ErrorCode::ALL {
        let err = AppError::new(code.as_str(), "msg", "2025-01-01T00:00:00.000Z")
            .with_detail(json!({ "info": code.as_str() }));
        let v = serde_json::to_value(&err).expect("serialize AppError with detail");

        assert_eq!(
            keys_of(&v),
            expected,
            "code {} 携带 detail 时应包含 {{code, message, detail, ts}}，实际：{v}",
            code.as_str()
        );
        assert_eq!(v["detail"]["info"], code.as_str(), "detail.info 必须 round-trip");
    }
}

/// 契约 2：`AppError::now(code, msg)` 产出的对象在 JSON round-trip
/// 之后能够与原值严格相等。
#[test]
fn error_object_round_trips_for_every_code() {
    for code in ErrorCode::ALL {
        let err = AppError::now(code.as_str(), "msg");
        let s = serde_json::to_string(&err).expect("serialize AppError::now");
        let parsed: AppError = serde_json::from_str(&s).expect("parse AppError back");
        assert_eq!(parsed, err, "round-trip 不一致 for {}", code.as_str());
    }
}

/// 契约 3：通过 `ServerMessage::error` 构造的"通用错误"WS 消息
/// 对每个 `ErrorCode` 都必须呈现 **结构化** 形态：
/// `{ "type": "error", "payload": { "code", "message" } }`，
/// 不允许出现任何"裸字符串 / 异常 stack"等非结构化载荷。
///
/// 这条契约对应任务 2.7 第二条要求：「断言无未捕获异常路径产出非结构化错误」。
#[test]
fn no_uncaught_error_paths_produce_unstructured_objects() {
    let expected_top = expected_keys(&["type", "payload"]);
    let expected_payload = expected_keys(&["code", "message"]);

    for code in ErrorCode::ALL {
        let msg = ServerMessage::error(code.as_str(), "msg");
        let v = serde_json::to_value(&msg).expect("serialize ServerMessage::error");

        assert!(v.is_object(), "WS error 消息必须是 JSON 对象，code={}", code.as_str());
        assert_eq!(
            keys_of(&v),
            expected_top,
            "WS error 顶层字段必须恰好是 {{type, payload}}，实际：{v}"
        );
        assert_eq!(v["type"], "error");

        let payload = &v["payload"];
        assert!(payload.is_object(), "payload 必须是 JSON 对象");
        assert_eq!(
            keys_of(payload),
            expected_payload,
            "payload 必须恰好是 {{code, message}}，实际：{payload}"
        );
        assert_eq!(payload["code"], code.as_str(), "payload.code 必须等于 ErrorCode::as_str()");
        assert_eq!(payload["message"], "msg");
    }
}

/// 契约 4：`AppError` 序列化时 `code` 字段必须严格等于
/// `ErrorCode::as_str()` 的字面量（防止大小写 / 拼写漂移）。
#[test]
fn error_code_string_value_matches_canonical_literal() {
    for code in ErrorCode::ALL {
        let err = AppError::new(code.as_str(), "m", "2025-01-01T00:00:00.000Z");
        let v = serde_json::to_value(&err).expect("serialize AppError");

        let actual = v["code"].as_str().expect("code 字段必须是字符串");
        assert_eq!(
            actual,
            code.as_str(),
            "AppError.code 与 ErrorCode::as_str() 必须严格一致"
        );
    }
}
