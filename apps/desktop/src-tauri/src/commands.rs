//! Tauri 命令实现 —— 桌面端 UI 与 `phonemic-core` 业务层之间的桥。
//!
//! 任务来源：tasks.md 10.5 / 10.7 / 13.4。
//! 设计来源：design.md §4.1。
//!
//! 命令的字段、错误码契约与 worker-injector-desktop 在 SendMessage
//! "Injector handoff: traits + Tauri commands" 中对外公布的表格保持一致。
//! 任何破坏性改动都需要先发消息通知 worker-mobile-e2e 与 team-lead。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use phonemic_core::diagnostics::{export_diagnostics, MAX_DIAGNOSTICS_LOG_BYTES};
use phonemic_core::error::{render_error, AppErrorExt};
use phonemic_core::i18n::{self, decide_lang, dict_for, Lang};
use phonemic_core::tracing_setup::shared_log;
use phonemic_protocol::{config::MAX_INJECT_DELAY_MS, AppConfig, AppError, ErrorCode};
use serde::Serialize;

use crate::app_state::{DesktopState, SessionView};

/// 调用方友好的命令错误：直接序列化为 [`AppError`] 字段。
#[derive(Debug, Clone, Serialize)]
pub struct CommandError {
    /// 错误码（`SCREAMING_SNAKE_CASE`，与 [`ErrorCode`] 对齐）。
    pub code: String,
    /// 已经过 i18n 渲染的人类可读消息。
    pub message: String,
}

impl CommandError {
    fn from_app_error(err: AppError, lang: Lang) -> Self {
        let message = render_error(lang, &err);
        Self {
            code: err.code,
            message,
        }
    }
    fn from_code(code: ErrorCode, msg: impl Into<String>, lang: Lang) -> Self {
        Self::from_app_error(AppError::from_code(code, msg), lang)
    }
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for CommandError {}

/// `get_runtime_info` 返回值 —— 主连接面板需要的运行时快照。
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeInfo {
    pub scheme: String,
    pub port: u16,
    pub ips: Vec<String>,
    pub urls: Vec<String>,
    pub version: String,
    pub uptime_secs: u64,
    pub lan_disabled: bool,
    pub banner: Option<String>,
    pub paused: bool,
    pub inject_delay_ms: u16,
}

/// `get_pairing_code` / `regenerate_code` 返回值。
#[derive(Debug, Clone, Serialize)]
pub struct PairingCodeView {
    pub code: String,
    pub qr_svg: String,
}

/// `revoke_all_sessions` 返回值。
#[derive(Debug, Clone, Serialize)]
pub struct RevokeAllResult {
    pub revoked: u32,
}

/// `get_logs_tail` 返回值。
#[derive(Debug, Clone, Serialize)]
pub struct LogsTail {
    pub lines: Vec<String>,
    pub total_bytes: u32,
}

/// `get_i18n_dict` 返回值。
#[derive(Debug, Clone, Serialize)]
pub struct I18nDict {
    pub lang: String,
    pub entries: HashMap<String, String>,
}

/// `export_diagnostics` 返回值。
#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticsBundle {
    pub path: String,
    pub bytes: u32,
}

// ---------- 简单工具 ----------

fn parse_lang(input: &str) -> Lang {
    match Lang::from_str(input) {
        Some(l) => l,
        None => decide_lang(input),
    }
}

fn ui_lang_for(state: &DesktopState) -> Lang {
    match state.config().ui.language {
        phonemic_protocol::config::UiLanguage::ZhCN => Lang::ZhCN,
        phonemic_protocol::config::UiLanguage::EnUS => Lang::EnUS,
        phonemic_protocol::config::UiLanguage::Auto => decide_lang(
            sys_locale::get_locale().unwrap_or_else(|| "en-US".to_string()).as_str(),
        ),
    }
}

// ---------- 命令实现 ----------

#[tauri::command]
pub async fn get_runtime_info(
    state: tauri::State<'_, Arc<DesktopState>>,
) -> Result<RuntimeInfo, CommandError> {
    Ok(state.runtime_info())
}

#[tauri::command]
pub async fn get_pairing_code(
    state: tauri::State<'_, Arc<DesktopState>>,
) -> Result<PairingCodeView, CommandError> {
    Ok(state.pairing_code_view())
}

#[tauri::command]
pub async fn regenerate_code(
    state: tauri::State<'_, Arc<DesktopState>>,
) -> Result<PairingCodeView, CommandError> {
    Ok(state.regenerate_pairing_code())
}

#[tauri::command]
pub async fn list_sessions(
    state: tauri::State<'_, Arc<DesktopState>>,
) -> Result<Vec<SessionView>, CommandError> {
    Ok(state.list_sessions())
}

#[tauri::command]
pub async fn revoke_session(
    state: tauri::State<'_, Arc<DesktopState>>,
    device_id: String,
) -> Result<(), CommandError> {
    let lang = ui_lang_for(&state);
    state.revoke_session(&device_id).map_err(|e| {
        CommandError::from_code(ErrorCode::AuthRequired, e.to_string(), lang)
    })
}

#[tauri::command]
pub async fn revoke_all_sessions(
    state: tauri::State<'_, Arc<DesktopState>>,
) -> Result<RevokeAllResult, CommandError> {
    let revoked = state.revoke_all_sessions();
    Ok(RevokeAllResult { revoked })
}

#[tauri::command]
pub async fn get_config(
    state: tauri::State<'_, Arc<DesktopState>>,
) -> Result<AppConfig, CommandError> {
    Ok(state.config())
}

#[tauri::command]
pub async fn save_config(
    state: tauri::State<'_, Arc<DesktopState>>,
    config: AppConfig,
) -> Result<(), CommandError> {
    let lang = ui_lang_for(&state);
    config.validate().map_err(|e| {
        CommandError::from_code(ErrorCode::MsgBadFormat, e.to_string(), lang)
    })?;
    state.save_config(config).map_err(|e| {
        CommandError::from_code(ErrorCode::PortUnavailable, e.to_string(), lang)
    })
}

#[tauri::command]
pub async fn set_inject_paused(
    state: tauri::State<'_, Arc<DesktopState>>,
    paused: bool,
) -> Result<(), CommandError> {
    state.set_inject_paused(paused);
    Ok(())
}

#[tauri::command]
pub async fn set_inject_delay_ms(
    state: tauri::State<'_, Arc<DesktopState>>,
    delay_ms: u16,
) -> Result<(), CommandError> {
    let lang = ui_lang_for(&state);
    if delay_ms > MAX_INJECT_DELAY_MS {
        return Err(CommandError::from_code(
            ErrorCode::MsgBadFormat,
            format!("inject_delay_ms must be 0..={MAX_INJECT_DELAY_MS}"),
            lang,
        ));
    }
    state.set_inject_delay_ms(delay_ms);
    Ok(())
}

#[tauri::command]
pub async fn get_logs_tail(
    max_bytes: Option<u32>,
) -> Result<LogsTail, CommandError> {
    let buf = match shared_log() {
        Some(b) => b,
        None => return Ok(LogsTail { lines: vec![], total_bytes: 0 }),
    };
    let cap = max_bytes
        .map(|n| n as usize)
        .unwrap_or(MAX_DIAGNOSTICS_LOG_BYTES);
    let guard = buf.lock().unwrap();
    let snapshot = guard.snapshot();
    let mut total = 0usize;
    let mut keep_idx = 0usize;
    for (i, line) in snapshot.iter().enumerate().rev() {
        let add = line.len() + 1;
        if total + add > cap {
            keep_idx = i + 1;
            break;
        }
        total += add;
    }
    let lines = snapshot[keep_idx..].to_vec();
    Ok(LogsTail {
        lines,
        total_bytes: u32::try_from(total).unwrap_or(u32::MAX),
    })
}

#[tauri::command]
pub async fn get_i18n_dict(lang: String) -> Result<I18nDict, CommandError> {
    let resolved = parse_lang(&lang);
    let dict = dict_for(resolved);
    let entries: HashMap<String, String> = dict
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    Ok(I18nDict {
        lang: resolved.as_str().to_string(),
        entries,
    })
}

#[tauri::command]
pub async fn export_diagnostics_cmd(
    state: tauri::State<'_, Arc<DesktopState>>,
    target_dir: String,
) -> Result<DiagnosticsBundle, CommandError> {
    let lang = ui_lang_for(&state);
    let buf_arc = shared_log().ok_or_else(|| {
        CommandError::from_code(
            ErrorCode::PortUnavailable,
            "tracing buffer not initialised",
            lang,
        )
    })?;
    let cfg = state.config();
    let path: PathBuf = PathBuf::from(target_dir);
    let buf = buf_arc.lock().unwrap();
    let out = export_diagnostics(&path, &cfg, &buf, env!("CARGO_PKG_VERSION")).map_err(|e| {
        CommandError::from_code(ErrorCode::PortUnavailable, e.to_string(), lang)
    })?;
    let bytes = std::fs::metadata(&out)
        .map(|m| u32::try_from(m.len()).unwrap_or(u32::MAX))
        .unwrap_or(0);
    Ok(DiagnosticsBundle {
        path: out.to_string_lossy().into_owned(),
        bytes,
    })
}

// silence unused warning when i18n module re-export trips clippy
#[allow(dead_code)]
fn _ensure_i18n_link() -> Lang {
    i18n::Lang::EnUS
}
