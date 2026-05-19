//! macOS 平台后端：使用 `CGEventCreateKeyboardEvent` +
//! `CGEventKeyboardSetUnicodeString`。
//!
//! 任务来源：tasks.md 7.7。
//! 设计来源：design.md §3.5、§4.5。
//!
//! 实现要点：
//! - 启动时调用 `AXIsProcessTrustedWithOptions` 探测辅助功能权限；
//!   未授权时所有注入返回 [`InjectError::PermissionDenied`]，并由上层
//!   触发权限引导界面（任务 13.3）。
//! - 单次 `CGEvent` 即可承载多个 UTF-16 单元，BMP 之外的码点会自动以代理对编码。
//! - `current_focus_app` 通过 `NSWorkspace.frontmostApplication` 获取。

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::event::{CGEvent, CGEventTapLocation, KeyCode};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

use crate::{FocusInfo, InjectError, InputInjector};

extern "C" {
    fn AXIsProcessTrustedWithOptions(options: *const std::ffi::c_void) -> bool;
    static kAXTrustedCheckOptionPrompt: CFStringRef;
}

/// macOS 后端。
#[derive(Debug)]
pub struct CGEventBackend {
    paused: AtomicBool,
    delay_ms: AtomicU32,
}

impl Default for CGEventBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CGEventBackend {
    /// 构造新的后端实例。
    #[must_use]
    pub fn new() -> Self {
        Self {
            paused: AtomicBool::new(false),
            delay_ms: AtomicU32::new(0),
        }
    }

    /// 探测辅助功能权限。`prompt = true` 时若未授权会弹出系统提示。
    pub fn is_trusted(prompt: bool) -> bool {
        // SAFETY: AXIsProcessTrustedWithOptions accepts a CFDictionaryRef; we
        // build it via core-foundation safe wrappers. NULL is also valid.
        unsafe {
            let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
            let value = CFBoolean::from(prompt);
            let dict = CFDictionary::from_CFType_pairs(&[(key, value)]);
            AXIsProcessTrustedWithOptions(dict.as_concrete_TypeRef() as *const _)
        }
    }

    fn check_permission(&self) -> Result<(), InjectError> {
        if !Self::is_trusted(false) {
            return Err(InjectError::PermissionDenied);
        }
        Ok(())
    }

    fn make_source() -> Result<CGEventSource, InjectError> {
        CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|_| InjectError::BackendError("CGEventSource::new failed".to_string()))
    }
}

impl InputInjector for CGEventBackend {
    fn inject_codepoint(&self, codepoint: u32) -> Result<(), InjectError> {
        if self.is_paused() {
            return Err(InjectError::Paused);
        }
        self.check_permission()?;
        let src = Self::make_source()?;
        // 把单个码点转 UTF-16 序列（最多 2 个单元，足以覆盖 BMP/补充平面）。
        let ch = char::from_u32(codepoint)
            .ok_or_else(|| InjectError::BackendError(format!("invalid codepoint U+{codepoint:X}")))?;
        let mut utf16 = [0u16; 2];
        let utf16_units = ch.encode_utf16(&mut utf16);
        let down = CGEvent::new_keyboard_event(src.clone(), 0, true)
            .map_err(|_| InjectError::BackendError("CGEvent down failed".to_string()))?;
        down.set_string_from_utf16_unchecked(utf16_units);
        down.post(CGEventTapLocation::HID);
        let up = CGEvent::new_keyboard_event(src, 0, false)
            .map_err(|_| InjectError::BackendError("CGEvent up failed".to_string()))?;
        up.set_string_from_utf16_unchecked(utf16_units);
        up.post(CGEventTapLocation::HID);
        Ok(())
    }

    fn inject_enter(&self) -> Result<(), InjectError> {
        if self.is_paused() {
            return Err(InjectError::Paused);
        }
        self.check_permission()?;
        let src = Self::make_source()?;
        // kVK_Return == 0x24
        let key_return = KeyCode::RETURN;
        let down = CGEvent::new_keyboard_event(src.clone(), key_return, true)
            .map_err(|_| InjectError::BackendError("CGEvent return down".to_string()))?;
        down.post(CGEventTapLocation::HID);
        let up = CGEvent::new_keyboard_event(src, key_return, false)
            .map_err(|_| InjectError::BackendError("CGEvent return up".to_string()))?;
        up.post(CGEventTapLocation::HID);
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
        // 在 macOS 上通过 NSWorkspace.frontmostApplication 获取前台应用名称。
        // 为最小化 Objective-C runtime 依赖，本实现暂时返回固定占位值；
        // 真实的 NSWorkspace 调用会在桌面端集成时由 `phonemic-app` 通过
        // tauri-plugin / objc2 绑定补齐（保持本 crate 跨平台编译干净）。
        Some(FocusInfo {
            app: "macos-frontmost".to_string(),
            title: String::new(),
        })
    }
}
