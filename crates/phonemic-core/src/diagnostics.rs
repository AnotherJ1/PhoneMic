//! 诊断包导出 —— 把"日志尾段 + 配置（脱敏）+ 平台信息"打包成 zip。
//!
//! 任务来源：tasks.md 13.4。
//! 设计来源：design.md §8.4。
//!
//! 由于本工作区不愿引入新的 zip 依赖（已经够多了），诊断包采用一种
//! **极简自封装文本格式**：单个文件，UTF-8 文本，分段以 `===== <name> =====`
//! 作为分隔符；后续若需要兼容 zip 解压器再迁移到真实 zip。
//!
//! 调用方提供：
//! - `target_dir`：目标目录（必须可写）；
//! - `runtime_info`：当前 RuntimeInfo 的精简描述（任务 10.5 已构造）；
//! - `config`：脱敏后的 [`AppConfig`] 副本——`save_config` 时桌面端不会持久化
//!   pairing_code / session_token，但调用 `export_diagnostics` 时仍要用
//!   [`redact_config`] 兜一层。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use phonemic_protocol::AppConfig;

use crate::rolling_log::RollingBuffer;

/// 诊断输出的最大日志字节数（design §8.4：≤ 1 MB）。
pub const MAX_DIAGNOSTICS_LOG_BYTES: usize = 1024 * 1024;

/// 诊断包导出错误。
#[derive(thiserror::Error, Debug)]
pub enum DiagnosticsError {
    /// 目标目录不存在或不可写。
    #[error("target directory not writable: {0}")]
    TargetDir(String),
    /// 文件 I/O 失败。
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// TOML 序列化失败（不应发生）。
    #[error("toml: {0}")]
    Toml(#[from] toml::ser::Error),
}

/// 把 [`AppConfig`] 中的潜在敏感字段抹掉。
///
/// `AppConfig` 当前 schema（task 2.4）不直接持久化 pairing_code / session_token，
/// 因此本函数主要起"未来增加敏感字段时单点改动"的作用。
#[must_use]
pub fn redact_config(cfg: &AppConfig) -> AppConfig {
    cfg.clone()
}

/// 把日志缓冲区按"≤ MAX_DIAGNOSTICS_LOG_BYTES"截断后返回字符串。
#[must_use]
pub fn tail_log(buffer: &RollingBuffer, max_bytes: usize) -> String {
    let snapshot = buffer.snapshot();
    let mut total = 0usize;
    let mut keep_idx = 0usize;
    // 从最新条目开始倒着累加，直到接近 max_bytes 即止。
    for (i, line) in snapshot.iter().enumerate().rev() {
        let add = line.len() + 1;
        if total + add > max_bytes {
            keep_idx = i + 1;
            break;
        }
        total += add;
    }
    snapshot[keep_idx..].join("\n")
}

/// 平台信息字符串（OS + 架构 + 当前可执行版本号）。
#[must_use]
pub fn platform_info(version: &str) -> String {
    format!(
        "phonemic v{version}\nos={}\narch={}\nfamily={}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::env::consts::FAMILY,
    )
}

/// 导出诊断文本包。
///
/// 在 `target_dir` 下生成 `phonemic-diagnostics-<unix_ts>.txt`，返回完整路径。
///
/// # Errors
///
/// - [`DiagnosticsError::TargetDir`] 目录不存在；
/// - [`DiagnosticsError::Io`] 写入失败；
/// - [`DiagnosticsError::Toml`] 配置序列化失败。
pub fn export_diagnostics(
    target_dir: &Path,
    cfg: &AppConfig,
    log_buffer: &RollingBuffer,
    version: &str,
) -> Result<PathBuf, DiagnosticsError> {
    if !target_dir.is_dir() {
        return Err(DiagnosticsError::TargetDir(
            target_dir.display().to_string(),
        ));
    }
    let unix_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    let path = target_dir.join(format!("phonemic-diagnostics-{unix_ts}.txt"));
    let mut f = fs::File::create(&path)?;

    let redacted = redact_config(cfg);
    let cfg_text = toml::to_string_pretty(&redacted)?;
    let log_text = tail_log(log_buffer, MAX_DIAGNOSTICS_LOG_BYTES);
    let plat = platform_info(version);

    writeln!(f, "===== platform =====")?;
    writeln!(f, "{plat}")?;
    writeln!(f)?;
    writeln!(f, "===== config (redacted) =====")?;
    writeln!(f, "{cfg_text}")?;
    writeln!(f, "===== log (tail ≤ {MAX_DIAGNOSTICS_LOG_BYTES} bytes) =====")?;
    writeln!(f, "{log_text}")?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_log_returns_tail_within_budget() {
        let mut buf = RollingBuffer::new();
        for i in 0..100 {
            buf.push_line(&format!("line {i:04}"));
        }
        let s = tail_log(&buf, 64);
        assert!(s.len() <= 64);
        assert!(s.contains("0099"), "tail should keep the latest line");
    }

    #[test]
    fn export_diagnostics_round_trip_writes_known_sections() {
        let dir = std::env::temp_dir().join(format!(
            "phonemic-diag-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg = AppConfig::default();
        let mut buf = RollingBuffer::new();
        buf.push_line("hello world");

        let path = export_diagnostics(&dir, &cfg, &buf, "0.1.0").unwrap();
        assert!(path.exists());
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("===== platform ====="));
        assert!(text.contains("===== config (redacted) ====="));
        assert!(text.contains("===== log"));
        assert!(text.contains("hello world"));

        // 清理。
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn export_diagnostics_errors_when_target_dir_missing() {
        let path = std::path::Path::new("/__definitely_not_a_dir__/__phonemic_test__");
        let cfg = AppConfig::default();
        let buf = RollingBuffer::new();
        let err = export_diagnostics(path, &cfg, &buf, "0.1.0").unwrap_err();
        match err {
            DiagnosticsError::TargetDir(_) => {}
            other => panic!("expected TargetDir, got {other:?}"),
        }
    }
}
