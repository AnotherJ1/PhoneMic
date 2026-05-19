//! 任务 13.1 / 13.2 / 13.4 / 13.5 集成测试 —— worker-injector-desktop。
//!
//! 单独放在 `tests/` 里以避免被 worker-backend 的 lib 单元测试编译失败拖累。

use phonemic_core::diagnostics::{export_diagnostics, tail_log, MAX_DIAGNOSTICS_LOG_BYTES};
use phonemic_core::error::{code_to_i18n_key, render_error, AppErrorExt, IntoAppError};
use phonemic_core::i18n::Lang;
use phonemic_core::rolling_log::RollingBuffer;
use phonemic_core::tracing_setup::init_tracing;
use phonemic_protocol::{AppConfig, AppError, ErrorCode};
use std::io;

#[test]
fn into_app_error_wraps_io() {
    let io_err = io::Error::new(io::ErrorKind::PermissionDenied, "config.toml denied");
    let app: AppError = io_err.into_app_error(ErrorCode::PortUnavailable);
    assert_eq!(app.code, "PORT_UNAVAILABLE");
    assert!(app.message.contains("config.toml"));
}

#[test]
fn app_error_from_code_uses_canonical_literal() {
    let e = AppError::from_code(ErrorCode::InjectPaused, "paused");
    assert_eq!(e.code, "INJECT_PAUSED");
}

#[test]
fn render_error_falls_back_to_message_on_unknown_code() {
    let err = AppError::now("UNKNOWN_CODE", "raw");
    let s = render_error(Lang::EnUS, &err);
    assert_eq!(s, "raw");
}

#[test]
fn render_error_uses_dictionary_for_known_code() {
    let err = AppError::from_code(ErrorCode::LanLost, "raw");
    let s = render_error(Lang::ZhCN, &err);
    assert!(!s.is_empty());
    // 当字典命中时，应当不是 raw 回退（zh-CN 字典里 LAN_LOST 已存在）。
    assert_ne!(s, "raw");
}

#[test]
fn code_to_i18n_key_covers_every_error_variant() {
    for code in ErrorCode::ALL {
        let key = code_to_i18n_key(code);
        assert!(key.starts_with("error."));
        assert!(key.ends_with(code.as_str()));
    }
}

#[test]
fn tail_log_returns_at_most_max_bytes() {
    let mut buf = RollingBuffer::new();
    for i in 0..200 {
        buf.push_line(&format!("line {i:04}"));
    }
    let s = tail_log(&buf, 64);
    assert!(s.len() <= 64);
    assert!(s.contains("0199"));
}

#[test]
fn export_diagnostics_round_trip() {
    let dir = std::env::temp_dir().join(format!(
        "phonemic-it-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = AppConfig::default();
    let mut buf = RollingBuffer::new();
    buf.push_line("integration sample");
    let path = export_diagnostics(&dir, &cfg, &buf, "0.1.0").unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("integration sample"));
    assert!(text.contains("===== platform ====="));
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir(&dir);
}

#[test]
fn init_tracing_smoke() {
    let buf = init_tracing("info");
    tracing::info!(field = "x", "smoke event");
    let snap = buf.lock().unwrap().snapshot();
    assert!(snap.iter().any(|l| l.contains("smoke event")));
}

#[allow(dead_code)]
fn _link_max_bytes() -> usize {
    MAX_DIAGNOSTICS_LOG_BYTES
}
