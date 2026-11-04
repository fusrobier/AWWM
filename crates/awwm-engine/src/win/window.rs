//! Safe queries and mutations on top-level windows.
//!
//! None of the query functions here send messages to the target window's thread, so they
//! never block on a hung application (`GetWindowTextW` reads the cached caption of windows
//! owned by other processes without sending `WM_GETTEXT`).

use windows::Win32::Foundation::{HWND, RECT, SetLastError, WIN32_ERROR};
use windows::Win32::Graphics::Dwm::{
    DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GA_ROOT, GW_HWNDPREV, GW_OWNER, GWL_EXSTYLE, GWL_STYLE, GetAncestor, GetClassNameW,
    GetDesktopWindow, GetShellWindow, GetWindow, GetWindowLongPtrW, GetWindowRect,
    GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, HWND_NOTOPMOST, HWND_TOPMOST,
    IsHungAppWindow, IsIconic, IsWindow, IsWindowVisible, IsZoomed, SET_WINDOW_POS_FLAGS,
    SetWindowLongPtrW, SetWindowPos, WINDOW_EX_STYLE, WINDOW_STYLE, WS_CAPTION, WS_CHILD,
    WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_THICKFRAME,
};

use awwm_core::{Rect, Size};

use super::{WindowHandle, rect_from_win};

/// Shell and system window classes that must never be managed or moved.
const SHELL_CLASSES: &[&str] = &[
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "NotifyIconOverflowWindow",
    "TopLevelWindowForOverflowXamlIsland",
    "Windows.UI.Core.CoreWindow",
    "XamlExplorerHostIslandWindow",
    "MultitaskingViewFrame",
    "ForegroundStaging",
    "#32768",
];

/// Margins between `GetWindowRect` and the visible DWM frame (the invisible resize
/// borders of Windows 10/11). Each value is `>= 0` for normal windows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameMargins {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl FrameMargins {
    /// Computes the margins from a window rect and its visible frame.
    #[must_use]
    pub const fn between(window: Rect, frame: Rect) -> Self {
        Self {
            left: frame.left - window.left,
            top: frame.top - window.top,
            right: window.right - frame.right,
            bottom: window.bottom - frame.bottom,
        }
    }

    /// Converts a desired visible frame into the window rect to pass to `SetWindowPos`.
    #[must_use]
    pub const fn frame_to_window(&self, frame: Rect) -> Rect {
        frame.inflate(self.left, self.top, self.right, self.bottom)
    }

    /// Total horizontal and vertical margin.
    #[must_use]
    pub const fn total(&self) -> Size {
        Size::new(self.left + self.right, self.top + self.bottom)
    }
}

/// `IsWindow`.
#[must_use]
pub fn exists(h: WindowHandle) -> bool {
    // SAFETY: IsWindow accepts any handle value, including stale ones.
    unsafe { IsWindow(Some(h.hwnd())).as_bool() }
}

/// `IsWindowVisible` (the WS_VISIBLE style of the window and its ancestors).
#[must_use]
pub fn is_visible(h: WindowHandle) -> bool {
    // SAFETY: read-only query on an arbitrary handle.
    unsafe { IsWindowVisible(h.hwnd()).as_bool() }
}

/// `IsIconic`.
#[must_use]
pub fn is_minimized(h: WindowHandle) -> bool {
    // SAFETY: read-only query on an arbitrary handle.
    unsafe { IsIconic(h.hwnd()).as_bool() }
}

/// `IsZoomed`.
#[must_use]
pub fn is_maximized(h: WindowHandle) -> bool {
    // SAFETY: read-only query on an arbitrary handle.
    unsafe { IsZoomed(h.hwnd()).as_bool() }
}

/// `IsHungAppWindow`: the owning thread has not pumped messages for ~5 seconds.
#[must_use]
pub fn is_hung(h: WindowHandle) -> bool {
    // SAFETY: read-only query on an arbitrary handle.
    unsafe { IsHungAppWindow(h.hwnd()).as_bool() }
}

/// `GetWindowRect` in physical screen coordinates (the caller thread is PMv2 aware).
#[must_use]
pub fn window_rect(h: WindowHandle) -> Option<Rect> {
    let mut r = RECT::default();
    // SAFETY: `r` is a valid out pointer for the duration of the call.
    unsafe { GetWindowRect(h.hwnd(), &mut r) }.ok()?;
    Some(rect_from_win(r))
}

/// The visible frame (`DWMWA_EXTENDED_FRAME_BOUNDS`), falling back to the window rect for
/// windows without a DWM frame.
#[must_use]
pub fn frame_rect(h: WindowHandle) -> Option<Rect> {
    let mut r = RECT::default();
    // SAFETY: `r` is a valid, correctly sized out buffer for this attribute.
    let dwm = unsafe {
        DwmGetWindowAttribute(
            h.hwnd(),
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&raw mut r).cast(),
            u32::try_from(std::mem::size_of::<RECT>()).unwrap_or(16),
        )
    };
    match dwm {
        Ok(()) if !rect_from_win(r).is_empty() => Some(rect_from_win(r)),
        _ => window_rect(h),
    }
}

/// Window rect, visible frame and the margins between them.
#[must_use]
pub fn geometry(h: WindowHandle) -> Option<(Rect, Rect, FrameMargins)> {
    let window = window_rect(h)?;
    let frame = frame_rect(h)?;
    let margins = FrameMargins::between(window, frame);
    // Defensive: a frame larger than the window (seen with some custom-drawn windows)
    // would give negative margins; treat the window rect as the frame instead.
    if margins.left < 0 || margins.top < 0 || margins.right < 0 || margins.bottom < 0 {
        return Some((window, window, FrameMargins::default()));
    }
    Some((window, frame, margins))
}

/// `true` when the window is cloaked by DWM (other virtual desktop, suspended UWP app,
/// hidden shell surfaces).
#[must_use]
pub fn is_cloaked(h: WindowHandle) -> bool {
    let mut cloaked: u32 = 0;
    // SAFETY: `cloaked` is a valid 4-byte out buffer for DWMWA_CLOAKED.
    let r = unsafe { DwmGetWindowAttribute(h.hwnd(), DWMWA_CLOAKED, (&raw mut cloaked).cast(), 4) };
    r.is_ok() && cloaked != 0
}

/// Extended window style.
#[must_use]
pub fn ex_style(h: WindowHandle) -> WINDOW_EX_STYLE {
    // SAFETY: read-only query; the value is a bit field that fits in 32 bits.
    let v = unsafe { GetWindowLongPtrW(h.hwnd(), GWL_EXSTYLE) };
    WINDOW_EX_STYLE(v as u32)
}

/// Window style.
#[must_use]
pub fn style(h: WindowHandle) -> WINDOW_STYLE {
    // SAFETY: read-only query; the value is a bit field that fits in 32 bits.
    let v = unsafe { GetWindowLongPtrW(h.hwnd(), GWL_STYLE) };
    WINDOW_STYLE(v as u32)
}

/// Sets the extended window style.
///
/// # Errors
/// Fails when the window is gone or belongs to a process with a higher integrity level
/// (`ERROR_ACCESS_DENIED`).
pub fn set_ex_style(h: WindowHandle, ex: WINDOW_EX_STYLE) -> windows::core::Result<()> {
    // SAFETY: SetLastError/SetWindowLongPtrW are called on this thread in sequence so the
    // last-error value distinguishes "previous value was 0" from failure.
    unsafe {
        SetLastError(WIN32_ERROR(0));
        let previous = SetWindowLongPtrW(h.hwnd(), GWL_EXSTYLE, ex.0 as isize);
        if previous == 0 {
            let err = windows::core::Error::from_thread();
            if err.code().is_err() {
                return Err(err);
            }
        }
    }
    Ok(())
}

/// Window caption (empty when none).
#[must_use]
pub fn title(h: WindowHandle) -> String {
    // SAFETY: read-only query.
    let len = unsafe { GetWindowTextLengthW(h.hwnd()) };
    let Ok(len) = usize::try_from(len) else {
        return String::new();
    };
    if len == 0 {
        return String::new();
    }
    let mut buf = vec![0u16; len + 1];
    // SAFETY: `buf` is a valid writable buffer; the function writes at most buf.len() units.
    let copied = unsafe { GetWindowTextW(h.hwnd(), &mut buf) };
    super::from_wide(&buf, usize::try_from(copied).unwrap_or(0))
}

/// Window class name.
#[must_use]
pub fn class_name(h: WindowHandle) -> String {
    let mut buf = [0u16; 256];
    // SAFETY: `buf` is a valid writable buffer.
    let len = unsafe { GetClassNameW(h.hwnd(), &mut buf) };
    super::from_wide(&buf, usize::try_from(len).unwrap_or(0))
}

/// Process id owning the window (0 when the window is gone).
#[must_use]
pub fn process_id(h: WindowHandle) -> u32 {
    let mut pid = 0u32;
    // SAFETY: `pid` is a valid out pointer.
    unsafe { GetWindowThreadProcessId(h.hwnd(), Some(&raw mut pid)) };
    pid
}

/// `true` when the window still exists and still belongs to `pid` (guards against
/// recycled handle values).
#[must_use]
pub fn is_same_window(h: WindowHandle, pid: u32) -> bool {
    exists(h) && process_id(h) == pid
}

/// The root (top-level) ancestor of a window.
#[must_use]
pub fn root(h: WindowHandle) -> WindowHandle {
    // SAFETY: read-only query; returns null for invalid handles.
    WindowHandle::from_hwnd(unsafe { GetAncestor(h.hwnd(), GA_ROOT) })
}

/// The owner window, if any.
#[must_use]
pub fn owner(h: WindowHandle) -> Option<WindowHandle> {
    // SAFETY: read-only query.
    let o = unsafe { GetWindow(h.hwnd(), GW_OWNER) }.ok()?;
    let o = WindowHandle::from_hwnd(o);
    (!o.is_null()).then_some(o)
}

/// The window directly above `h` in the Z-order.
#[must_use]
pub fn window_above(h: WindowHandle) -> Option<WindowHandle> {
    // SAFETY: read-only query.
    let p = unsafe { GetWindow(h.hwnd(), GW_HWNDPREV) }.ok()?;
    let p = WindowHandle::from_hwnd(p);
    (!p.is_null()).then_some(p)
}

/// `true` when the window has `WS_EX_TOPMOST`.
#[must_use]
pub fn is_topmost(h: WindowHandle) -> bool {
    ex_style(h).contains(WS_EX_TOPMOST)
}

/// `true` for the desktop, shell and taskbar windows that must never be managed.
#[must_use]
pub fn is_shell_window(h: WindowHandle) -> bool {
    // SAFETY: both functions take no arguments and only return handles.
    let (desktop, shell) = unsafe { (GetDesktopWindow(), GetShellWindow()) };
    if h.hwnd() == desktop || h.hwnd() == shell {
        return true;
    }
    let class = class_name(h);
    SHELL_CLASSES.iter().any(|c| c.eq_ignore_ascii_case(&class))
}

/// Whether the window would appear in Alt+Tab / the taskbar: the set of "application
/// windows" AWWM manages and lists.
#[must_use]
pub fn is_app_window(h: WindowHandle) -> bool {
    if h.is_null() || !exists(h) || !is_visible(h) || root(h) != h {
        return false;
    }
    let st = style(h);
    if st.contains(WS_CHILD) {
        return false;
    }
    let ex = ex_style(h);
    let app_window = ex.contains(WS_EX_APPWINDOW);
    if !app_window && (ex.contains(WS_EX_TOOLWINDOW) || ex.contains(WS_EX_NOACTIVATE)) {
        return false;
    }
    if !app_window && owner(h).is_some() {
        return false;
    }
    if is_cloaked(h) || is_shell_window(h) {
        return false;
    }
    frame_rect(h).is_some_and(|r| !r.is_empty())
}

/// Whether the window covers its whole monitor without a caption or sizing border
/// (games, video players, browsers in F11 mode).
#[must_use]
pub fn is_fullscreen(h: WindowHandle, monitor_bounds: Rect) -> bool {
    let Some(r) = window_rect(h) else {
        return false;
    };
    let covers = r.left <= monitor_bounds.left
        && r.top <= monitor_bounds.top
        && r.right >= monitor_bounds.right
        && r.bottom >= monitor_bounds.bottom;
    let st = style(h);
    covers && !st.contains(WS_CAPTION) && !st.contains(WS_THICKFRAME)
}

/// Z-order placement for [`set_window_pos`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZOrder {
    /// Keep the current Z-order (`SWP_NOZORDER`).
    Keep,
    /// `HWND_TOPMOST`.
    Topmost,
    /// `HWND_NOTOPMOST`.
    NoTopmost,
    /// Directly below the given window.
    After(WindowHandle),
}

/// `SetWindowPos` wrapper. `rect` is ignored when `flags` contains both `SWP_NOMOVE` and
/// `SWP_NOSIZE`.
///
/// # Errors
/// Fails when the window is gone or protected by UIPI (higher integrity level).
pub fn set_window_pos(
    h: WindowHandle,
    z: ZOrder,
    rect: Rect,
    flags: SET_WINDOW_POS_FLAGS,
) -> windows::core::Result<()> {
    use windows::Win32::UI::WindowsAndMessaging::SWP_NOZORDER;
    let (after, flags): (Option<HWND>, SET_WINDOW_POS_FLAGS) = match z {
        ZOrder::Keep => (None, flags | SWP_NOZORDER),
        ZOrder::Topmost => (Some(HWND_TOPMOST), flags),
        ZOrder::NoTopmost => (Some(HWND_NOTOPMOST), flags),
        ZOrder::After(w) => (Some(w.hwnd()), flags),
    };
    // SAFETY: all arguments are plain values; Win32 validates the handles.
    unsafe {
        SetWindowPos(
            h.hwnd(),
            after,
            rect.left,
            rect.top,
            rect.width(),
            rect.height(),
            flags,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn margins_convert_frame_to_window() {
        let window = Rect::new(93, 93, 1007, 707);
        let frame = Rect::new(100, 100, 1000, 700);
        let m = FrameMargins::between(window, frame);
        assert_eq!(
            m,
            FrameMargins {
                left: 7,
                top: 7,
                right: 7,
                bottom: 7
            }
        );
        assert_eq!(m.frame_to_window(frame), window);
        assert_eq!(
            m.frame_to_window(frame.translate(50, -20)),
            window.translate(50, -20)
        );
        assert_eq!(m.total(), Size::new(14, 14));
    }
}
