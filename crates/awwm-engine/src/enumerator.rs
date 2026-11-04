//! window_enumerator: lists top-level windows (`EnumWindows`) and filters application
//! windows.

use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::UI::WindowsAndMessaging::EnumWindows;
use windows::core::BOOL;

use awwm_core::Rect;

use crate::types::WindowInfo;
use crate::win::{WindowHandle, process, window};

unsafe extern "system" fn collect(hwnd: HWND, data: LPARAM) -> BOOL {
    // SAFETY: `data` is the address of the Vec owned by `top_level_windows`, which outlives
    // the synchronous EnumWindows call that invokes this callback.
    let out = unsafe { &mut *(data.0 as *mut Vec<WindowHandle>) };
    out.push(WindowHandle::from_hwnd(hwnd));
    BOOL::from(true)
}

/// All top-level windows in Z-order (top-most first).
#[must_use]
pub fn top_level_windows() -> Vec<WindowHandle> {
    let mut out: Vec<WindowHandle> = Vec::with_capacity(256);
    // SAFETY: the callback only runs during this call and receives a pointer to `out`.
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM((&raw mut out) as isize));
    }
    out
}

/// Application windows (Alt+Tab-eligible) not owned by `own_pid`, in Z-order.
#[must_use]
pub fn app_windows(own_pid: u32) -> Vec<WindowHandle> {
    top_level_windows()
        .into_iter()
        .filter(|&h| window::is_app_window(h) && window::process_id(h) != own_pid)
        .collect()
}

/// Describes a window for the window picker.
#[must_use]
pub fn window_info(h: WindowHandle, managed: bool) -> WindowInfo {
    let pid = window::process_id(h);
    let effective = process::effective_pid(h);
    let exe_path = process::exe_path(effective).unwrap_or_default();
    WindowInfo {
        hwnd: h.as_u64(),
        title: window::title(h),
        class_name: window::class_name(h),
        exe_name: process::exe_name(&exe_path),
        exe_path,
        pid: effective,
        topmost: window::is_topmost(h),
        managed,
        accessible: process::access(pid) != process::Access::HigherIntegrity,
    }
}

/// Visible frames of the windows a gesture can snap to, top-most first, excluding the
/// window being dragged and AWWM's own windows.
#[must_use]
pub fn snap_targets(exclude: WindowHandle, own_pid: u32) -> Vec<Rect> {
    top_level_windows()
        .into_iter()
        .filter(|&h| h != exclude)
        .filter(|&h| window::is_app_window(h) && !window::is_minimized(h))
        .filter(|&h| window::process_id(h) != own_pid)
        .filter_map(window::frame_rect)
        .filter(|r| !r.is_empty())
        .take(64)
        .collect()
}
