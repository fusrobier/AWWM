//! Thin, documented wrappers around the Win32 APIs used by the engine.
//!
//! Every `unsafe` block in this module tree calls a Win32 function with arguments that are
//! valid for the duration of the call (stack buffers, owned structs, handles that may be
//! stale). Win32 tolerates stale window handles — the functions fail with an error instead
//! of invoking undefined behaviour — so passing a handle that was destroyed concurrently
//! is always memory-safe. Callers treat failures as "window gone".

pub mod icon;
pub mod monitor;
pub mod process;
pub mod window;

use std::fmt;

use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext,
};

use awwm_core::{Point, Rect};

/// A `Send + Sync` copyable window handle.
///
/// `HWND` wraps a raw pointer and is therefore neither `Send` nor `Sync`, but window
/// handles are plain system-wide identifiers (always representable in 32 bits so 32- and
/// 64-bit processes can exchange them). Storing the integer value is the documented way to
/// pass them between threads.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WindowHandle(isize);

impl WindowHandle {
    /// Wraps a raw `HWND`.
    #[must_use]
    pub fn from_hwnd(hwnd: HWND) -> Self {
        Self(hwnd.0 as isize)
    }

    /// Returns the raw `HWND`.
    #[must_use]
    pub fn hwnd(self) -> HWND {
        HWND(self.0 as *mut core::ffi::c_void)
    }

    /// `true` for the null handle.
    #[must_use]
    pub const fn is_null(self) -> bool {
        self.0 == 0
    }

    /// Value used in the IPC layer and persisted session files.
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0 as u64
    }

    /// Converts an IPC value back into a handle. Returns `None` for 0 and for values that
    /// cannot be window handles.
    #[must_use]
    pub fn from_u64(value: u64) -> Option<Self> {
        let v = isize::try_from(value).ok()?;
        (v != 0).then_some(Self(v))
    }

    /// Raw integer value (for `WPARAM`/`LPARAM` packing).
    #[must_use]
    pub const fn raw(self) -> isize {
        self.0
    }

    /// Builds a handle from a raw integer value.
    #[must_use]
    pub const fn from_raw(raw: isize) -> Self {
        Self(raw)
    }
}

impl fmt::Debug for WindowHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HWND({:#x})", self.0)
    }
}

impl fmt::Display for WindowHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x}", self.0)
    }
}

/// Makes the calling thread per-monitor-DPI-aware (v2) so every coordinate it handles is
/// in physical pixels, independent of the process default.
pub fn make_thread_dpi_aware() {
    // SAFETY: plain call with a predefined pseudo-handle constant; it only affects the
    // calling thread and returns the previous context (ignored).
    unsafe {
        SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

/// Ensures the calling thread has a message queue (so `PostThreadMessageW` to it cannot
/// fail) and returns its thread id.
#[must_use]
pub fn init_message_queue() -> u32 {
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::{MSG, PM_NOREMOVE, PeekMessageW, WM_USER};
    let mut msg = MSG::default();
    // SAFETY: PeekMessageW with a valid out buffer forces creation of the thread's queue;
    // the filter range matches nothing that is removed (PM_NOREMOVE).
    unsafe {
        let _ = PeekMessageW(&raw mut msg, None, WM_USER, WM_USER, PM_NOREMOVE);
        GetCurrentThreadId()
    }
}

/// Converts a Win32 `RECT` into a core [`Rect`].
#[must_use]
pub const fn rect_from_win(r: RECT) -> Rect {
    Rect::new(r.left, r.top, r.right, r.bottom)
}

/// Converts a core [`Rect`] into a Win32 `RECT`.
#[must_use]
pub const fn rect_to_win(r: Rect) -> RECT {
    RECT {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    }
}

/// Converts a Win32 `POINT` into a core [`Point`].
#[must_use]
pub const fn point_from_win(p: POINT) -> Point {
    Point::new(p.x, p.y)
}

/// Converts a core [`Point`] into a Win32 `POINT`.
#[must_use]
pub const fn point_to_win(p: Point) -> POINT {
    POINT { x: p.x, y: p.y }
}

/// Encodes a Rust string as a NUL-terminated UTF-16 buffer.
#[must_use]
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Decodes a UTF-16 buffer up to `len` code units (lossy).
#[must_use]
pub fn from_wide(buf: &[u16], len: usize) -> String {
    String::from_utf16_lossy(&buf[..len.min(buf.len())])
}

/// Packs two `i32` coordinates into an `LPARAM` (like `MAKELPARAM`, but lossless for
/// negative multi-monitor coordinates on 64-bit).
#[must_use]
pub fn pack_point(p: Point) -> LPARAM {
    let packed = (i64::from(p.x) << 32) | i64::from(p.y as u32);
    LPARAM(packed as isize)
}

/// Inverse of [`pack_point`].
#[must_use]
pub fn unpack_point(l: LPARAM) -> Point {
    let v = l.0 as i64;
    Point::new((v >> 32) as i32, v as i32)
}

/// `WPARAM` helper.
#[must_use]
pub const fn wparam(v: usize) -> WPARAM {
    WPARAM(v)
}

/// Returns the last Win32 error as a `windows::core::Error`.
#[must_use]
pub fn last_error() -> windows::core::Error {
    windows::core::Error::from_thread()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_packing_round_trips_negative_coordinates() {
        for p in [
            Point::new(0, 0),
            Point::new(-2560, 178),
            Point::new(3600, -1),
            Point::new(i32::MIN, i32::MAX),
        ] {
            assert_eq!(unpack_point(pack_point(p)), p);
        }
    }

    #[test]
    fn handle_conversion() {
        assert!(WindowHandle::from_u64(0).is_none());
        let h = WindowHandle::from_u64(0x1234).expect("valid");
        assert_eq!(h.as_u64(), 0x1234);
        assert_eq!(WindowHandle::from_hwnd(h.hwnd()), h);
        assert_eq!(format!("{h}"), "0x1234");
    }

    #[test]
    fn wide_strings() {
        let w = wide("ab");
        assert_eq!(w, vec![97, 98, 0]);
        assert_eq!(from_wide(&w, 2), "ab");
        assert_eq!(from_wide(&w, 99), "ab\0");
    }
}
