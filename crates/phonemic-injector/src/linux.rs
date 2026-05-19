//! Linux 平台后端：使用 X11 `XTestFakeKeyEvent` + `XKeysymToKeycode`。
//!
//! 任务来源：tasks.md 7.8。
//! 设计来源：design.md §3.5、§4.5。
//!
//! 实现要点：
//! - 仅支持 X11 / XWayland 会话；纯 Wayland 会话下 `XOpenDisplay` 会失败，
//!   相应返回 [`InjectError::BackendError`] 并附 `"wayland_unsupported"`。
//! - Unicode 码点转 X11 keysym：BMP 内 ≥ 0x100 的字符使用 `0x01000000 | cp` 形式
//!   （X11 Unicode keysym 约定）。
//! - `current_focus_app` 通过 `_NET_ACTIVE_WINDOW` root window 属性获取活动窗口
//!   并读取 `WM_CLASS` / `WM_NAME` 文本。

use std::ffi::CString;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

use x11::xlib::{
    Display, XCloseDisplay, XDefaultRootWindow, XFlush, XGetWindowProperty, XKeysymToKeycode,
    XOpenDisplay, _XPrivDisplay, AnyPropertyType, XA_WINDOW,
};
use x11::xtest::XTestFakeKeyEvent;

use crate::{FocusInfo, InjectError, InputInjector};

/// Linux X11 后端。
#[derive(Debug)]
pub struct X11XTestBackend {
    paused: AtomicBool,
    delay_ms: AtomicU32,
    /// 缓存的 Display* 指针（线程安全包装）。`None` 表示初始化失败。
    display: Mutex<DisplayHandle>,
}

#[derive(Debug)]
struct DisplayHandle(*mut Display);

// SAFETY: Xlib displays must serialize calls; we wrap access in a Mutex above.
unsafe impl Send for DisplayHandle {}

impl Drop for DisplayHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: Display owned by this handle is closed exactly once.
            unsafe {
                XCloseDisplay(self.0);
            }
            self.0 = std::ptr::null_mut();
        }
    }
}

impl Default for X11XTestBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl X11XTestBackend {
    /// 构造新的后端实例（尝试打开默认 Display）。
    #[must_use]
    pub fn new() -> Self {
        // SAFETY: XOpenDisplay accepts NULL to use $DISPLAY env var.
        let dpy = unsafe { XOpenDisplay(std::ptr::null()) };
        Self {
            paused: AtomicBool::new(false),
            delay_ms: AtomicU32::new(0),
            display: Mutex::new(DisplayHandle(dpy)),
        }
    }

    fn with_display<F, R>(&self, f: F) -> Result<R, InjectError>
    where
        F: FnOnce(*mut Display) -> Result<R, InjectError>,
    {
        let guard = self.display.lock().unwrap();
        if guard.0.is_null() {
            return Err(InjectError::BackendError("wayland_unsupported".to_string()));
        }
        f(guard.0)
    }

    fn codepoint_to_keysym(cp: u32) -> u64 {
        if cp < 0x100 {
            u64::from(cp)
        } else {
            // X11 Unicode keysym：高位 0x01000000，低 24 位为 codepoint。
            0x0100_0000 | u64::from(cp)
        }
    }

    fn fake_key(dpy: *mut Display, keysym: u64, press: bool) -> Result<(), InjectError> {
        // SAFETY: dpy is a valid Display under the Mutex; keysym is u64.
        let keycode = unsafe { XKeysymToKeycode(dpy, keysym) };
        if keycode == 0 {
            return Err(InjectError::BackendError(format!(
                "no keycode for keysym U+{keysym:X}"
            )));
        }
        // SAFETY: XTest extension calls; we flush after the pair of events.
        let ok = unsafe {
            XTestFakeKeyEvent(
                dpy,
                u32::from(keycode),
                i32::from(press),
                0, // CurrentTime
            )
        };
        if ok == 0 {
            return Err(InjectError::BackendError(
                "XTestFakeKeyEvent returned 0".to_string(),
            ));
        }
        Ok(())
    }
}

impl InputInjector for X11XTestBackend {
    fn inject_codepoint(&self, codepoint: u32) -> Result<(), InjectError> {
        if self.is_paused() {
            return Err(InjectError::Paused);
        }
        self.with_display(|dpy| {
            let keysym = Self::codepoint_to_keysym(codepoint);
            Self::fake_key(dpy, keysym, true)?;
            Self::fake_key(dpy, keysym, false)?;
            // SAFETY: dpy is valid.
            unsafe {
                XFlush(dpy);
            }
            Ok(())
        })
    }

    fn inject_enter(&self) -> Result<(), InjectError> {
        if self.is_paused() {
            return Err(InjectError::Paused);
        }
        self.with_display(|dpy| {
            // XK_Return = 0xff0d
            const XK_RETURN: u64 = 0xff0d;
            Self::fake_key(dpy, XK_RETURN, true)?;
            Self::fake_key(dpy, XK_RETURN, false)?;
            // SAFETY: dpy is valid.
            unsafe {
                XFlush(dpy);
            }
            Ok(())
        })
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
        let guard = self.display.lock().unwrap();
        if guard.0.is_null() {
            return None;
        }
        let dpy = guard.0;
        // SAFETY: dpy valid.
        let root = unsafe { XDefaultRootWindow(dpy) };
        let prop_name = CString::new("_NET_ACTIVE_WINDOW").ok()?;
        // SAFETY: standard Xlib property lookup.
        let atom = unsafe {
            x11::xlib::XInternAtom(dpy, prop_name.as_ptr(), 0)
        };
        if atom == 0 {
            return None;
        }
        let mut actual_type: x11::xlib::Atom = 0;
        let mut actual_format: i32 = 0;
        let mut nitems: u64 = 0;
        let mut bytes_after: u64 = 0;
        let mut prop: *mut u8 = std::ptr::null_mut();
        // SAFETY: pointers are output-only; we free via XFree below.
        let status = unsafe {
            XGetWindowProperty(
                dpy,
                root,
                atom,
                0,
                1,
                0,
                XA_WINDOW,
                &mut actual_type,
                &mut actual_format,
                &mut nitems,
                &mut bytes_after,
                &mut prop,
            )
        };
        if status != 0 || prop.is_null() || nitems == 0 {
            // SAFETY: free even on partial success.
            if !prop.is_null() {
                unsafe {
                    x11::xlib::XFree(prop.cast());
                }
            }
            return None;
        }
        // SAFETY: prop points to a Window (XID) when actual_type == XA_WINDOW.
        let active = unsafe { *(prop as *const x11::xlib::Window) };
        // SAFETY: free immediately after read.
        unsafe {
            x11::xlib::XFree(prop.cast());
        }
        if active == 0 {
            return None;
        }
        // 仅取 WM_NAME；WM_CLASS 解析需要更多 API，留作后续增强。
        let mut name_ptr: *mut i8 = std::ptr::null_mut();
        // SAFETY: XFetchName allocates name_ptr; we free via XFree.
        let ok = unsafe { x11::xlib::XFetchName(dpy, active, &mut name_ptr) };
        let title = if ok != 0 && !name_ptr.is_null() {
            // SAFETY: name_ptr is a valid C string.
            let cstr = unsafe { std::ffi::CStr::from_ptr(name_ptr) };
            let s = cstr.to_string_lossy().into_owned();
            // SAFETY: free name buffer.
            unsafe {
                x11::xlib::XFree(name_ptr.cast());
            }
            s
        } else {
            String::new()
        };
        Some(FocusInfo {
            app: format!("x11-window-{active:#x}"),
            title,
        })
    }
}

// 简单 silence "unused"：在 _XPrivDisplay 仅作类型路径占位时不会触发使用。
#[allow(dead_code)]
fn _ensure_priv_display_link(_p: *mut _XPrivDisplay) {}
#[allow(dead_code)]
fn _ensure_any_property_type() -> i32 {
    AnyPropertyType
}
