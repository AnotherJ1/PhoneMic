//! `phonemic-injector` —— 跨平台键盘注入抽象。
//!
//! 任务来源：tasks.md 7.x
//! 设计来源：design.md §3.5、§4.5
//!
//! 本 crate 提供：
//! - [`InputInjector`] trait：跨平台键盘注入接口；
//! - [`InjectionPlanner`] 纯函数：把文本翻译为 [`InjectionEvent`] 序列；
//! - [`VirtualBackend`]：测试 / 桌面端模拟用的内存后端；
//! - [`InjectorEventSink`] trait：把 `inject.error` 事件桥接到 WS 出站层。
//! 平台后端（Windows / macOS / Linux）在子模块 `windows` / `macos` / `linux`
//! 中实现，仅在对应 `target_os` 时编译。

//! 平台后端（Windows / macOS / Linux）必须通过 `unsafe` 调用 OS FFI；
//! 因此本 crate 不使用 `forbid(unsafe_code)`，而是在每处 `unsafe` 块上
//! 加 `// SAFETY:` 注释解释不变量。

pub mod planner;
pub mod virtual_backend;

#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "linux")]
pub mod linux;

use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};

pub use planner::{plan_injection, InjectionPlanner};
pub use virtual_backend::VirtualBackend;

/// 注入事件类型。`Char(cp)` 为 Unicode 标量值；`Enter` 为换行键。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    /// 注入一个 Unicode 码点（例如 `'你'` -> 0x4F60，emoji -> 非 BMP 码点）。
    Char(u32),
    /// 注入一次回车（VK_RETURN / kVK_Return / XK_Return）。
    Enter,
}

/// 计划中的单次注入事件，时间戳 `ts` 仅用于属性测试中验证延迟（Property 14）。
#[derive(Debug, Clone, Copy)]
pub struct InjectionEvent {
    /// 事件类型。
    pub kind: EventKind,
    /// 该事件代表的码点（仅 `Char` 时有值，`Enter` 为 `None`）。
    pub codepoint: Option<u32>,
    /// 计划中这一事件预计投递的时间戳。
    pub ts: Instant,
}

impl PartialEq for InjectionEvent {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.codepoint == other.codepoint && self.ts == other.ts
    }
}

impl Eq for InjectionEvent {}

/// 当前前台窗口的轻量描述（用于 `INJECT_NO_FOCUS_TARGET` 诊断）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FocusInfo {
    /// 应用 / 进程名（如 `notepad.exe` / `TextEdit` / `xterm`）。
    pub app: String,
    /// 窗口标题（可能为空）。
    pub title: String,
}

/// 注入失败原因。所有变体可被序列化为统一错误码（参见
/// `phonemic_protocol::ErrorCode`）。
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum InjectError {
    /// 当前没有可用的前台焦点窗口。
    #[error("no focus target")]
    NoFocusTarget,
    /// 平台权限不足（macOS 辅助功能、Wayland 限制等）。
    #[error("permission denied")]
    PermissionDenied,
    /// 注入处于暂停状态。
    #[error("injection paused")]
    Paused,
    /// 平台后端报错；`detail` 用于诊断（不会回显给最终用户）。
    #[error("backend error: {0}")]
    BackendError(String),
}

impl InjectError {
    /// 与 [`phonemic_protocol::ErrorCode`] 对齐的错误码字面量。
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoFocusTarget => "INJECT_NO_FOCUS_TARGET",
            Self::PermissionDenied => "INJECT_PERMISSION_DENIED",
            Self::Paused => "INJECT_PAUSED",
            Self::BackendError(_) => "INJECT_BACKEND_ERROR",
        }
    }
}

/// 跨平台键盘注入接口。
///
/// 实现者：
/// - [`VirtualBackend`]：测试 / 桌面端"模拟模式"使用，记录事件而非调用 OS API；
/// - 平台后端（`windows::WinSendInputBackend`、`macos::CGEventBackend`、
///   `linux::X11XTestBackend`）：仅在对应 `target_os` 下编译。
///
/// # Default `inject_text`
///
/// trait 提供了"按码点循环 + 节流 + `\n -> inject_enter`"的默认实现，使得
/// 平台后端只需要专心实现单个码点 / 单次回车的"原子注入"。
pub trait InputInjector: Send + Sync {
    /// 注入单个 Unicode 码点。
    ///
    /// 实现方应把 BMP 之外的码点正确编码为代理对（Windows）或一次写入
    /// `CGEventKeyboardSetUnicodeString`（macOS）等。
    fn inject_codepoint(&self, codepoint: u32) -> Result<(), InjectError>;

    /// 注入一次回车键。
    fn inject_enter(&self) -> Result<(), InjectError>;

    /// 设置注入暂停状态。
    fn pause(&self, paused: bool);

    /// 当前是否暂停。
    fn is_paused(&self) -> bool;

    /// 当前的注入间字符延迟（毫秒）。0 表示无节流。
    fn delay_ms(&self) -> u32;

    /// 设置注入间字符延迟（毫秒）。超过 500 ms 应被调用方提前夹紧
    /// （参见 `phonemic-protocol` 的 `MAX_INJECT_DELAY_MS`）。
    fn set_delay_ms(&self, delay_ms: u32);

    /// 当前前台窗口（用于 `INJECT_NO_FOCUS_TARGET` 诊断）。
    fn current_focus_app(&self) -> Option<FocusInfo>;

    /// 注入一段文本。默认实现为：
    /// 1. 检查暂停 -> [`InjectError::Paused`]；
    /// 2. 检查焦点 -> 无焦点时 [`InjectError::NoFocusTarget`]；
    /// 3. 按码点循环：`'\n'` -> [`Self::inject_enter`]，否则 [`Self::inject_codepoint`]；
    /// 4. 相邻字符之间 sleep `delay_ms` 毫秒。
    fn inject_text(&self, text: &str) -> Result<(), InjectError> {
        if self.is_paused() {
            return Err(InjectError::Paused);
        }
        if self.current_focus_app().is_none() {
            return Err(InjectError::NoFocusTarget);
        }
        let delay = self.delay_ms();
        let mut first = true;
        for ch in text.chars() {
            if !first && delay > 0 {
                std::thread::sleep(std::time::Duration::from_millis(u64::from(delay)));
            }
            first = false;
            if ch == '\n' {
                self.inject_enter()?;
            } else {
                self.inject_codepoint(ch as u32)?;
            }
        }
        Ok(())
    }
}

/// `inject.error` / `inject.ack` 等事件的出站接收器。
///
/// 由 worker-backend 在 WebSocket 出站层（`WsOutbound`）实现，注入器
/// 通过 [`Arc<dyn InjectorEventSink>`] 调用，避免对 `phonemic-core::web`
/// 的循环依赖。
pub trait InjectorEventSink: Send + Sync {
    /// 上报一次注入失败：错误码、人类可读消息、可选的 `id`（来自 `text.submit`）。
    fn on_inject_error(&self, code: &str, message: &str, request_id: Option<&str>);

    /// 上报一次注入成功（`inject.ack`）。`chars` 为已注入的字符数。
    fn on_inject_ack(&self, request_id: Option<&str>, chars: usize);
}

/// 一个无副作用的占位 sink，便于初始化阶段尚未拿到真实通道时使用。
#[derive(Debug, Default, Clone, Copy)]
pub struct NullSink;

impl InjectorEventSink for NullSink {
    fn on_inject_error(&self, _code: &str, _message: &str, _request_id: Option<&str>) {}
    fn on_inject_ack(&self, _request_id: Option<&str>, _chars: usize) {}
}

/// 把任意 `Arc<dyn InjectorEventSink>` 的弱引用做克隆，便于注入器持有。
#[must_use]
pub fn null_sink() -> Arc<dyn InjectorEventSink> {
    Arc::new(NullSink)
}

/// 写入文件的 [`InjectorEventSink`] 实现 —— E2E 测试（任务 12.1）专用。
///
/// 行为：每次 `on_inject_error` / `on_inject_ack` 把一行 JSON Lines 追加到
/// 由构造时传入的文件路径。文件以 `OpenOptions::append + create` 打开，
/// 因此可以多次重启进程而不丢历史记录；E2E harness 在测试结束时直接
/// 读取文件并断言。
///
/// 通过 [`FileSink::from_env`] 读取 `PHONEMIC_TEST_INJECT_FILE` 环境变量。
pub struct FileSink {
    path: std::path::PathBuf,
    inner: std::sync::Mutex<Option<std::fs::File>>,
}

impl std::fmt::Debug for FileSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileSink").field("path", &self.path).finish()
    }
}

impl FileSink {
    /// 用显式路径构造 sink。文件会在第一次写入时按"create + append"模式打开。
    #[must_use]
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            path: path.into(),
            inner: std::sync::Mutex::new(None),
        }
    }

    /// 读取 `PHONEMIC_TEST_INJECT_FILE` 环境变量；未设置时返回 `None`。
    /// E2E harness（任务 12.1）通过该变量把 sink 接入桌面端。
    #[must_use]
    pub fn from_env() -> Option<Self> {
        std::env::var("PHONEMIC_TEST_INJECT_FILE").ok().map(Self::new)
    }

    fn write_line(&self, line: &str) {
        use std::io::Write;
        let mut guard = self.inner.lock().unwrap();
        if guard.is_none() {
            match std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
            {
                Ok(f) => *guard = Some(f),
                Err(e) => {
                    tracing::warn!(path = %self.path.display(), error = %e, "FileSink open failed");
                    return;
                }
            }
        }
        if let Some(f) = guard.as_mut() {
            if let Err(e) = writeln!(f, "{line}") {
                tracing::warn!(error = %e, "FileSink write failed");
            }
        }
    }
}

impl InjectorEventSink for FileSink {
    fn on_inject_error(&self, code: &str, message: &str, request_id: Option<&str>) {
        let line = format!(
            r#"{{"kind":"error","code":{code:?},"message":{message:?},"request_id":{rid:?}}}"#,
            code = code,
            message = message,
            rid = request_id.unwrap_or(""),
        );
        self.write_line(&line);
    }

    fn on_inject_ack(&self, request_id: Option<&str>, chars: usize) {
        let line = format!(
            r#"{{"kind":"ack","request_id":{rid:?},"chars":{chars}}}"#,
            rid = request_id.unwrap_or(""),
            chars = chars,
        );
        self.write_line(&line);
    }
}

/// 把多个 sink 串联：每次事件按顺序广播给每个内部 sink。
///
/// 桌面端典型用法：`MultiSink::new(vec![Arc::new(BridgeEventSink::new(tx)), Arc::new(file_sink)])`
/// 让生产 sink + E2E sink 同时工作。
pub struct MultiSink {
    sinks: Vec<Arc<dyn InjectorEventSink>>,
}

impl std::fmt::Debug for MultiSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MultiSink").field("count", &self.sinks.len()).finish()
    }
}

impl MultiSink {
    /// 用一组 sink 构造。空集合等价于 [`NullSink`]。
    #[must_use]
    pub fn new(sinks: Vec<Arc<dyn InjectorEventSink>>) -> Self {
        Self { sinks }
    }
}

impl InjectorEventSink for MultiSink {
    fn on_inject_error(&self, code: &str, message: &str, request_id: Option<&str>) {
        for s in &self.sinks {
            s.on_inject_error(code, message, request_id);
        }
    }
    fn on_inject_ack(&self, request_id: Option<&str>, chars: usize) {
        for s in &self.sinks {
            s.on_inject_ack(request_id, chars);
        }
    }
}

/// `inject_text_with_sink` —— 任务 7.11：把一次注入请求 + 自动 sink 上报合并。
///
/// - 成功：调用 [`InjectorEventSink::on_inject_ack`] 上报字符数；
/// - 失败：调用 [`InjectorEventSink::on_inject_error`] 上报错误码 + 消息，
///   并把错误从 `tracing::warn!` 同步落盘到 [`crate::lib`] 的滚动日志（不含明文文本，
///   仅记录长度与 SHA-256 摘要前 8 字节，design.md §8.4 / Property 33）。
///
/// 调用方典型用法：
/// ```ignore
/// let res = phonemic_injector::inject_text_with_sink(
///     injector.as_ref(),
///     sink.as_ref(),
///     &text,
///     request_id.as_deref(),
/// );
/// ```
pub fn inject_text_with_sink(
    injector: &dyn InputInjector,
    sink: &dyn InjectorEventSink,
    text: &str,
    request_id: Option<&str>,
) -> Result<(), InjectError> {
    match injector.inject_text(text) {
        Ok(()) => {
            sink.on_inject_ack(request_id, text.chars().count());
            Ok(())
        }
        Err(err) => {
            // 文本明文不能落盘；仅以长度 + SHA8 摘要标识。
            let summary = secret_summary(text);
            tracing::warn!(
                code = err.code(),
                request_id = request_id.unwrap_or(""),
                text_len = text.chars().count(),
                text_sha8 = %summary,
                "inject failed"
            );
            sink.on_inject_error(err.code(), &err.to_string(), request_id);
            Err(err)
        }
    }
}

fn secret_summary(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(s.as_bytes());
    let mut out = String::with_capacity(16);
    for b in &digest[..8] {
        use std::fmt::Write;
        let _ = write!(out, "{b:02x}");
    }
    out
}

#[cfg(test)]
mod sink_tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct CaptureSink {
        errors: Mutex<Vec<(String, String, Option<String>)>>,
        acks: Mutex<Vec<(Option<String>, usize)>>,
    }

    impl InjectorEventSink for CaptureSink {
        fn on_inject_error(&self, code: &str, message: &str, request_id: Option<&str>) {
            self.errors.lock().unwrap().push((
                code.to_string(),
                message.to_string(),
                request_id.map(str::to_string),
            ));
        }
        fn on_inject_ack(&self, request_id: Option<&str>, chars: usize) {
            self.acks
                .lock()
                .unwrap()
                .push((request_id.map(str::to_string), chars));
        }
    }

    #[test]
    fn inject_text_with_sink_emits_ack_on_success() {
        let injector = VirtualBackend::default();
        let sink = CaptureSink::default();
        inject_text_with_sink(&injector, &sink, "hi", Some("req-1")).unwrap();
        let acks = sink.acks.lock().unwrap();
        assert_eq!(acks.len(), 1);
        assert_eq!(acks[0].0.as_deref(), Some("req-1"));
        assert_eq!(acks[0].1, 2);
        assert!(sink.errors.lock().unwrap().is_empty());
    }

    #[test]
    fn inject_text_with_sink_emits_error_on_no_focus() {
        let injector = VirtualBackend::without_focus();
        let sink = CaptureSink::default();
        let err = inject_text_with_sink(&injector, &sink, "x", Some("req-2")).unwrap_err();
        assert_eq!(err.code(), "INJECT_NO_FOCUS_TARGET");
        let errors = sink.errors.lock().unwrap();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].0, "INJECT_NO_FOCUS_TARGET");
        assert_eq!(errors[0].2.as_deref(), Some("req-2"));
    }

    #[test]
    fn file_sink_writes_jsonl_lines() {
        let path = std::env::temp_dir().join(format!(
            "phonemic-filesink-{}-{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        ));
        let sink = FileSink::new(&path);
        sink.on_inject_ack(Some("req-1"), 5);
        sink.on_inject_error("INJECT_NO_FOCUS_TARGET", "no focus", Some("req-2"));
        let text = std::fs::read_to_string(&path).expect("read sink output");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains(r#""kind":"ack""#));
        assert!(lines[0].contains(r#""chars":5"#));
        assert!(lines[1].contains(r#""kind":"error""#));
        assert!(lines[1].contains("INJECT_NO_FOCUS_TARGET"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn multi_sink_fans_out_to_each_inner_sink() {
        let s1 = std::sync::Arc::new(CaptureSink::default());
        let s2 = std::sync::Arc::new(CaptureSink::default());
        let multi = MultiSink::new(vec![s1.clone(), s2.clone()]);
        multi.on_inject_ack(Some("r"), 3);
        assert_eq!(s1.acks.lock().unwrap().len(), 1);
        assert_eq!(s2.acks.lock().unwrap().len(), 1);
        multi.on_inject_error("INJECT_PAUSED", "paused", None);
        assert_eq!(s1.errors.lock().unwrap().len(), 1);
        assert_eq!(s2.errors.lock().unwrap().len(), 1);
    }
}
