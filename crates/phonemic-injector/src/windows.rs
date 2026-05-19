//! Windows 平台后端：使用 `SendInput` + `KEYEVENTF_UNICODE`。
//!
//! 任务来源：tasks.md 7.6。
//! 设计来源：design.md §3.5、§4.5。
//!
//! 实现要点：
//! - 单个 BMP 码点（≤ 0xFFFF）作为 `wScan` 直接 `SendInput`；
//! - 非 BMP 码点（≥ 0x10000）拆分为 UTF-16 代理对，依次发送两次 `KEYEVENTF_UNICODE`；
//! - 回车键使用 `VK_RETURN`；
//! - `current_focus_app` 通过 `GetForegroundWindow` + `GetWindowThreadProcessId`
//!   + `QueryFullProcessImageNameW` 获取进程可执行文件名。

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

use windows::Win32::Foundation::{HWND, MAX_PATH};
use windows::Win32::System::ProcessStatus::GetModuleBaseNameW;
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    VIRTUAL_KEY, VK_RETURN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
};

use crate::{FocusInfo, InjectError, InputInjector};

/// Windows 后端：包含暂停状态、字符延迟与进程级别共享标志。
#[derive(Debug)]
pub struct WinSendInputBackend {
    paused: AtomicBool,
    delay_ms: AtomicU32,
    last_focus: Mutex<Option<FocusInfo>>,
}

impl Default for WinSendInputBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl WinSendInputBackend {
    /// 构造新的后端实例。
    #[must_use]
    pub fn new() -> Self {
        Self {
            paused: AtomicBool::new(false),
            delay_ms: AtomicU32::new(0),
            last_focus: Mutex::new(None),
        }
    }

    fn send_unicode(scan: u16, key_up: bool) -> Result<(), InjectError> {
        let mut input = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: scan,
                    dwFlags: if key_up {
                        KEYEVENTF_UNICODE | KEYEVENTF_KEYUP
                    } else {
                        KEYEVENTF_UNICODE
                    },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        // SAFETY: SendInput is documented to accept a pointer + count;
        // we pass exactly one INPUT struct by mutable slice.
        let sent = unsafe { SendInput(std::slice::from_mut(&mut input), std::mem::size_of::<INPUT>() as i32) };
        if sent == 0 {
            return Err(InjectError::BackendError("SendInput returned 0".to_string()));
        }
        Ok(())
    }

    fn send_vkey(vk: VIRTUAL_KEY, key_up: bool) -> Result<(), InjectError> {
        let mut input = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: if key_up {
                        KEYEVENTF_KEYUP
                    } else {
                        windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(0)
                    },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        // SAFETY: see send_unicode.
        let sent = unsafe { SendInput(std::slice::from_mut(&mut input), std::mem::size_of::<INPUT>() as i32) };
        if sent == 0 {
            return Err(InjectError::BackendError("SendInput VK returned 0".to_string()));
        }
        Ok(())
    }
}

impl InputInjector for WinSendInputBackend {
    fn inject_codepoint(&self, codepoint: u32) -> Result<(), InjectError> {
        if self.is_paused() {
            return Err(InjectError::Paused);
        }
        if codepoint <= 0xFFFF {
            // BMP：直接发一次 down + up。
            #[allow(clippy::cast_possible_truncation)]
            let scan = codepoint as u16;
            Self::send_unicode(scan, false)?;
            Self::send_unicode(scan, true)?;
        } else {
            // 非 BMP：拆为 UTF-16 代理对。
            let v = codepoint - 0x10000;
            #[allow(clippy::cast_possible_truncation)]
            let high = 0xD800 + ((v >> 10) as u16);
            #[allow(clippy::cast_possible_truncation)]
            let low = 0xDC00 + ((v & 0x3FF) as u16);
            Self::send_unicode(high, false)?;
            Self::send_unicode(high, true)?;
            Self::send_unicode(low, false)?;
            Self::send_unicode(low, true)?;
        }
        Ok(())
    }

    fn inject_enter(&self) -> Result<(), InjectError> {
        if self.is_paused() {
            return Err(InjectError::Paused);
        }
        Self::send_vkey(VK_RETURN, false)?;
        Self::send_vkey(VK_RETURN, true)?;
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
        // SAFETY: GetForegroundWindow/GetWindowThreadProcessId are safe to call;
        // returned HWND may be null (no foreground window).
        let hwnd: HWND = unsafe { GetForegroundWindow() };
        if hwnd.0.is_null() {
            return None;
        }
        let mut pid: u32 = 0;
        // SAFETY: pid pointer is valid for the lifetime of this call.
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        let mut title_buf = [0u16; 512];
        // SAFETY: buffer is valid; GetWindowTextW writes up to len chars.
        let title_len = unsafe { GetWindowTextW(hwnd, &mut title_buf) };
        let title = String::from_utf16_lossy(&title_buf[..title_len.max(0) as usize]);
        let app = read_process_image_name(pid).unwrap_or_else(|| format!("pid:{pid}"));
        let info = FocusInfo { app, title };
        *self.last_focus.lock().unwrap() = Some(info.clone());
        Some(info)
    }
}

fn read_process_image_name(pid: u32) -> Option<String> {
    // SAFETY: OpenProcess returns a HANDLE; we close it explicitly via Drop.
    let handle = unsafe {
        OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, false, pid).ok()?
    };
    let mut buf = [0u16; MAX_PATH as usize];
    // SAFETY: handle is valid; buf is properly sized.
    let len = unsafe { GetModuleBaseNameW(handle, None, &mut buf) };
    // SAFETY: Always close the handle.
    unsafe {
        let _ = windows::Win32::Foundation::CloseHandle(handle);
    }
    if len == 0 {
        None
    } else {
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_state_round_trips() {
        let b = WinSendInputBackend::new();
        assert!(!b.is_paused());
        b.pause(true);
        assert!(b.is_paused());
        b.pause(false);
        assert!(!b.is_paused());
    }

    #[test]
    fn delay_state_round_trips() {
        let b = WinSendInputBackend::new();
        assert_eq!(b.delay_ms(), 0);
        b.set_delay_ms(123);
        assert_eq!(b.delay_ms(), 123);
    }

    #[test]
    fn paused_inject_returns_paused_error() {
        let b = WinSendInputBackend::new();
        b.pause(true);
        assert_eq!(b.inject_codepoint('a' as u32).unwrap_err(), InjectError::Paused);
        assert_eq!(b.inject_enter().unwrap_err(), InjectError::Paused);
    }
}
