//! [`VirtualBackend`] —— 测试 / 桌面端"模拟模式"用的内存注入后端。
//!
//! 任务来源：tasks.md 7.12（Property 33）。
//! 设计来源：design.md §4.5 / §9.5。
//!
//! 与平台后端不同，[`VirtualBackend`] **永远不调用 OS API**：
//! - `inject_codepoint` / `inject_enter` 把事件 push 到内部 `Vec<InjectionEvent>`；
//! - `current_focus_app` 返回构造时配置的 [`FocusInfo`]；
//! - `pause` 切换内部 `AtomicBool`；
//! - `set_delay_ms` 切换内部 `AtomicU32`，但**不真正 sleep**（避免拖慢测试）。
//!
//! 该后端用于：
//! 1. `phonemic-injector` 自身的属性测试（Property 12 / 13 / 15 / 33）；
//! 2. `phonemic-app` 的"模拟模式"——在 CI / 无 GUI 环境下运行端到端测试时
//!    替换真实平台后端，避免对真实窗口产生副作用。

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use crate::{EventKind, FocusInfo, InjectError, InjectionEvent, InputInjector};

/// 用于注入测试的可观察事件。比 [`InjectionEvent`] 多一个"是否被
/// `Paused` 拒绝"的语义。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordedEvent {
    /// 成功注入了一个字符。
    Char(u32),
    /// 成功注入了一次回车。
    Enter,
    /// 由于暂停被拒绝。
    PausedRejected,
    /// 由于无焦点被拒绝。
    NoFocusRejected,
    /// 由于权限被拒绝。
    PermissionRejected,
}

/// 内存注入后端。
///
/// 通过 [`VirtualBackend::with_focus`] / [`VirtualBackend::without_focus`] /
/// [`VirtualBackend::deny_permission`] 配置不同的失败模式，便于驱动 Property 33
/// 中"任意失败模式 → 错误码正确传播"的断言。
#[derive(Debug)]
pub struct VirtualBackend {
    paused: AtomicBool,
    delay_ms: AtomicU32,
    focus: Mutex<Option<FocusInfo>>,
    deny_permission: AtomicBool,
    backend_error: Mutex<Option<String>>,
    events: Mutex<Vec<RecordedEvent>>,
    raw_events: Mutex<Vec<InjectionEvent>>,
}

impl Default for VirtualBackend {
    fn default() -> Self {
        Self::with_focus(FocusInfo {
            app: "virtual-app".to_string(),
            title: "virtual-window".to_string(),
        })
    }
}

impl VirtualBackend {
    /// 构造一个具备焦点目标的虚拟后端。
    #[must_use]
    pub fn with_focus(info: FocusInfo) -> Self {
        Self {
            paused: AtomicBool::new(false),
            delay_ms: AtomicU32::new(0),
            focus: Mutex::new(Some(info)),
            deny_permission: AtomicBool::new(false),
            backend_error: Mutex::new(None),
            events: Mutex::new(Vec::new()),
            raw_events: Mutex::new(Vec::new()),
        }
    }

    /// 构造一个**没有焦点**的虚拟后端，用于触发 [`InjectError::NoFocusTarget`]。
    #[must_use]
    pub fn without_focus() -> Self {
        Self {
            paused: AtomicBool::new(false),
            delay_ms: AtomicU32::new(0),
            focus: Mutex::new(None),
            deny_permission: AtomicBool::new(false),
            backend_error: Mutex::new(None),
            events: Mutex::new(Vec::new()),
            raw_events: Mutex::new(Vec::new()),
        }
    }

    /// 构造一个权限被拒的虚拟后端，用于触发 [`InjectError::PermissionDenied`]。
    #[must_use]
    pub fn deny_permission() -> Self {
        let v = Self::default();
        v.deny_permission.store(true, Ordering::Relaxed);
        v
    }

    /// 构造一个会以 [`InjectError::BackendError`] 失败的虚拟后端。
    #[must_use]
    pub fn with_backend_error(detail: impl Into<String>) -> Self {
        let v = Self::default();
        *v.backend_error.lock().unwrap() = Some(detail.into());
        v
    }

    /// 设置当前焦点信息（运行时调整）。
    pub fn set_focus(&self, info: Option<FocusInfo>) {
        *self.focus.lock().unwrap() = info;
    }

    /// 取出全部已记录的事件并清空。
    #[must_use]
    pub fn drain_events(&self) -> Vec<RecordedEvent> {
        std::mem::take(&mut *self.events.lock().unwrap())
    }

    /// 借用当前事件列表的快照。
    #[must_use]
    pub fn snapshot_events(&self) -> Vec<RecordedEvent> {
        self.events.lock().unwrap().clone()
    }

    /// 取出底层 [`InjectionEvent`] 序列（含 `ts` 时间戳）。
    #[must_use]
    pub fn drain_raw_events(&self) -> Vec<InjectionEvent> {
        std::mem::take(&mut *self.raw_events.lock().unwrap())
    }

    fn record(&self, ev: RecordedEvent) {
        self.events.lock().unwrap().push(ev);
    }

    fn record_raw(&self, kind: EventKind, codepoint: Option<u32>) {
        self.raw_events.lock().unwrap().push(InjectionEvent {
            kind,
            codepoint,
            ts: Instant::now(),
        });
    }

    fn check_preconditions(&self) -> Result<(), InjectError> {
        if self.paused.load(Ordering::Relaxed) {
            self.record(RecordedEvent::PausedRejected);
            return Err(InjectError::Paused);
        }
        if self.deny_permission.load(Ordering::Relaxed) {
            self.record(RecordedEvent::PermissionRejected);
            return Err(InjectError::PermissionDenied);
        }
        if self.focus.lock().unwrap().is_none() {
            self.record(RecordedEvent::NoFocusRejected);
            return Err(InjectError::NoFocusTarget);
        }
        if let Some(detail) = self.backend_error.lock().unwrap().clone() {
            return Err(InjectError::BackendError(detail));
        }
        Ok(())
    }
}

impl InputInjector for VirtualBackend {
    fn inject_codepoint(&self, codepoint: u32) -> Result<(), InjectError> {
        self.check_preconditions()?;
        self.record(RecordedEvent::Char(codepoint));
        self.record_raw(EventKind::Char(codepoint), Some(codepoint));
        Ok(())
    }

    fn inject_enter(&self) -> Result<(), InjectError> {
        self.check_preconditions()?;
        self.record(RecordedEvent::Enter);
        self.record_raw(EventKind::Enter, None);
        Ok(())
    }

    fn pause(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

    fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    fn delay_ms(&self) -> u32 {
        self.delay_ms.load(Ordering::Relaxed)
    }

    fn set_delay_ms(&self, delay_ms: u32) {
        self.delay_ms.store(delay_ms, Ordering::Relaxed);
    }

    fn current_focus_app(&self) -> Option<FocusInfo> {
        self.focus.lock().unwrap().clone()
    }

    /// 重写默认实现：跳过实际 sleep，仅按字符 / 换行调用底层注入。
    /// 这让测试在 `delay_ms > 0` 时仍能秒级完成。
    fn inject_text(&self, text: &str) -> Result<(), InjectError> {
        if self.is_paused() {
            self.record(RecordedEvent::PausedRejected);
            return Err(InjectError::Paused);
        }
        if self.current_focus_app().is_none() {
            self.record(RecordedEvent::NoFocusRejected);
            return Err(InjectError::NoFocusTarget);
        }
        if self.deny_permission.load(Ordering::Relaxed) {
            self.record(RecordedEvent::PermissionRejected);
            return Err(InjectError::PermissionDenied);
        }
        if let Some(detail) = self.backend_error.lock().unwrap().clone() {
            return Err(InjectError::BackendError(detail));
        }
        for ch in text.chars() {
            if ch == '\n' {
                self.record(RecordedEvent::Enter);
                self.record_raw(EventKind::Enter, None);
            } else {
                let cp = ch as u32;
                self.record(RecordedEvent::Char(cp));
                self.record_raw(EventKind::Char(cp), Some(cp));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn virtual_backend_default_records_chars_and_enter() {
        let b = VirtualBackend::default();
        b.inject_text("hi\n").unwrap();
        let events = b.drain_events();
        assert_eq!(
            events,
            vec![
                RecordedEvent::Char('h' as u32),
                RecordedEvent::Char('i' as u32),
                RecordedEvent::Enter,
            ]
        );
    }

    #[test]
    fn virtual_backend_no_focus_returns_error() {
        let b = VirtualBackend::without_focus();
        let err = b.inject_text("x").unwrap_err();
        assert_eq!(err, InjectError::NoFocusTarget);
        assert_eq!(err.code(), "INJECT_NO_FOCUS_TARGET");
    }

    #[test]
    fn virtual_backend_paused_blocks_all_injections() {
        let b = VirtualBackend::default();
        b.pause(true);
        assert_eq!(b.inject_text("a").unwrap_err(), InjectError::Paused);
        assert_eq!(b.inject_codepoint('a' as u32).unwrap_err(), InjectError::Paused);
        assert_eq!(b.inject_enter().unwrap_err(), InjectError::Paused);
        let events = b.drain_events();
        assert!(events.iter().all(|e| matches!(e, RecordedEvent::PausedRejected)));
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn virtual_backend_permission_denied_propagates() {
        let b = VirtualBackend::deny_permission();
        let err = b.inject_codepoint('a' as u32).unwrap_err();
        assert_eq!(err, InjectError::PermissionDenied);
        assert_eq!(err.code(), "INJECT_PERMISSION_DENIED");
    }

    #[test]
    fn virtual_backend_backend_error_propagates() {
        let b = VirtualBackend::with_backend_error("xkb-not-loaded");
        let err = b.inject_codepoint('a' as u32).unwrap_err();
        assert_eq!(err, InjectError::BackendError("xkb-not-loaded".to_string()));
        assert_eq!(err.code(), "INJECT_BACKEND_ERROR");
    }

    proptest! {
        /// Property 15：注入暂停。
        ///
        /// 任意"请求 / 暂停切换"序列下，暂停期间不应产生任何 `Char` / `Enter`
        /// 事件，恢复后能够再次注入。
        #[test]
        fn property_15_pause_blocks_injections(
            ops in proptest::collection::vec(
                prop_oneof![
                    Just(VirtualOp::ToggleOn),
                    Just(VirtualOp::ToggleOff),
                    "[a-z]".prop_map(|s| VirtualOp::Inject(s)),
                ],
                0..32,
            )
        ) {
            let b = VirtualBackend::default();
            let mut paused = false;
            let mut expected_chars: Vec<u32> = Vec::new();
            for op in ops {
                match op {
                    VirtualOp::ToggleOn => { b.pause(true); paused = true; }
                    VirtualOp::ToggleOff => { b.pause(false); paused = false; }
                    VirtualOp::Inject(s) => {
                        let res = b.inject_text(&s);
                        if paused {
                            prop_assert_eq!(res, Err(InjectError::Paused));
                        } else {
                            prop_assert!(res.is_ok());
                            for ch in s.chars() {
                                expected_chars.push(ch as u32);
                            }
                        }
                    }
                }
            }
            // 把记录中所有 Char 事件取出，应等于 expected_chars。
            let actual: Vec<u32> = b
                .drain_events()
                .into_iter()
                .filter_map(|e| if let RecordedEvent::Char(cp) = e { Some(cp) } else { None })
                .collect();
            prop_assert_eq!(actual, expected_chars);
        }

        /// Property 33：任意失败模式都通过 `InjectError` 报告，不会 panic。
        #[test]
        fn property_33_failure_propagation(
            mode in 0u8..4,
            text in "[a-z]{0,8}",
        ) {
            let backend: VirtualBackend = match mode {
                0 => VirtualBackend::default(),
                1 => VirtualBackend::without_focus(),
                2 => VirtualBackend::deny_permission(),
                _ => VirtualBackend::with_backend_error("synthetic"),
            };
            let res = backend.inject_text(&text);
            match mode {
                0 => prop_assert!(res.is_ok() || text.is_empty() || res.is_ok()),
                1 => prop_assert_eq!(res, Err(InjectError::NoFocusTarget)),
                2 => prop_assert_eq!(res, Err(InjectError::PermissionDenied)),
                _ => {
                    if !text.is_empty() {
                        prop_assert!(matches!(res, Err(InjectError::BackendError(_))));
                    }
                }
            }
        }
    }

    #[derive(Debug, Clone)]
    enum VirtualOp {
        ToggleOn,
        ToggleOff,
        Inject(String),
    }
}
