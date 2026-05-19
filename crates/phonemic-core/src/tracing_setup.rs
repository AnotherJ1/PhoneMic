//! tracing 订阅器装配 —— 控制台 + [`crate::rolling_log::RollingBuffer`]。
//!
//! 任务来源：tasks.md 13.2。
//! 设计来源：design.md §8.4。
//!
//! 仅暴露 [`init_tracing`] 一把伞：调用方在桌面端启动期或 CLI 入口
//! 调用一次即可。重复调用会被 [`tracing::subscriber::set_global_default`]
//! 拒绝，本函数把这种"二次调用"视为幂等并仅打 warn。
//!
//! 滚动缓冲区通过 [`SharedRollingLog`] 公开给 `get_logs_tail` 命令
//! （任务 10.5）和 `export_diagnostics`（任务 13.4）。

use std::sync::{Arc, Mutex, OnceLock};

use tracing::Subscriber;
use tracing::field::{Field, Visit};
use tracing_subscriber::fmt;
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{prelude::*, EnvFilter};

use crate::rolling_log::RollingBuffer;

/// 全局共享的滚动日志缓冲区。
pub type SharedRollingLog = Arc<Mutex<RollingBuffer>>;

/// 进程内单例：在 [`init_tracing`] 中初始化，在 `get_logs_tail` /
/// `export_diagnostics` 中读取。
static GLOBAL_BUFFER: OnceLock<SharedRollingLog> = OnceLock::new();

/// 初始化 tracing：组合 `EnvFilter` + 控制台 fmt + 自定义 [`RollingLayer`]。
///
/// `default_level` 为 `RUST_LOG` 缺省时使用的等级（典型值 `"info"`）。
/// 调用幂等：第二次调用仅打 warn 不替换已有 subscriber。
pub fn init_tracing(default_level: &str) -> SharedRollingLog {
    let buffer: SharedRollingLog = GLOBAL_BUFFER
        .get_or_init(|| Arc::new(Mutex::new(RollingBuffer::new())))
        .clone();

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_level));

    let console_layer = fmt::layer().with_target(false);
    let rolling_layer = RollingLayer {
        buffer: buffer.clone(),
    };

    let subscriber = tracing_subscriber::registry()
        .with(env_filter)
        .with(console_layer)
        .with(rolling_layer);

    if tracing::subscriber::set_global_default(subscriber).is_err() {
        tracing::warn!("init_tracing 二次调用：保留首次注册的 subscriber");
    }
    buffer
}

/// 获取在 [`init_tracing`] 中创建的全局滚动缓冲区，未初始化则返回 `None`。
#[must_use]
pub fn shared_log() -> Option<SharedRollingLog> {
    GLOBAL_BUFFER.get().cloned()
}

/// 自定义 tracing Layer：把每个事件渲染为单行字符串后 push 到滚动缓冲区。
///
/// 字段渲染顺序：`<level> <module> <message> {key=value …}`，与
/// `tracing_subscriber::fmt` 的 compact 风格基本对齐，便于诊断包的可读性。
pub struct RollingLayer {
    buffer: SharedRollingLog,
}

impl<S> Layer<S> for RollingLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();
        let mut visitor = LineBuilder::default();
        event.record(&mut visitor);
        let module = metadata.module_path().unwrap_or("?");
        let line = format!(
            "{level:5} {module} {message}{kvs}",
            level = metadata.level(),
            module = module,
            message = visitor.message,
            kvs = visitor.kvs,
        );
        if let Ok(mut buf) = self.buffer.lock() {
            buf.push_line(&line);
        }
    }
}

#[derive(Default)]
struct LineBuilder {
    message: String,
    kvs: String,
}

impl Visit for LineBuilder {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}");
            // strip the surrounding quotes that {:?} adds for &str values
            if self.message.starts_with('"') && self.message.ends_with('"') {
                self.message = self.message[1..self.message.len() - 1].to_string();
            }
        } else {
            use std::fmt::Write;
            let _ = write!(self.kvs, " {}={value:?}", field.name());
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else {
            use std::fmt::Write;
            let _ = write!(self.kvs, " {}={value}", field.name());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// init_tracing 必须返回一个已存活的滚动缓冲区，并把后续 `tracing::info!`
    /// 事件投递进去。该测试是"接入烟雾测试"，并不验证滚动语义本身
    /// （那由 rolling_log 的属性测试覆盖）。
    #[test]
    fn init_tracing_writes_events_to_buffer() {
        let buf = init_tracing("info");
        tracing::info!(test_field = "ok", "rolling layer smoke");
        // 二次调用应当幂等。
        let buf2 = init_tracing("info");
        assert!(Arc::ptr_eq(&buf, &buf2));
        // 给订阅器一点时间（同步 Layer 不需要异步等待）。
        let lines = buf.lock().unwrap().snapshot();
        assert!(
            lines.iter().any(|l| l.contains("rolling layer smoke")),
            "缓冲区中未找到测试事件，内容: {lines:?}"
        );
    }
}
