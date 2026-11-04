//! Monitor enumeration and DPI queries.

use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITORINFO,
    MonitorFromPoint, MonitorFromRect,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::MONITORINFOF_PRIMARY;
use windows::core::BOOL;

use awwm_core::geometry::BASE_DPI;
use awwm_core::{MonitorInfo, Point, Rect};

use super::{point_to_win, rect_from_win, rect_to_win};

fn info_for(monitor: HMONITOR) -> Option<MonitorInfo> {
    let mut mi = MONITORINFO {
        cbSize: u32::try_from(std::mem::size_of::<MONITORINFO>()).unwrap_or(40),
        ..MONITORINFO::default()
    };
    // SAFETY: `mi` is a correctly sized, writable MONITORINFO.
    if !unsafe { GetMonitorInfoW(monitor, &raw mut mi) }.as_bool() {
        return None;
    }
    let (mut dpi_x, mut dpi_y) = (BASE_DPI, BASE_DPI);
    // SAFETY: valid out pointers; falls back to 96 DPI on failure.
    if unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &raw mut dpi_x, &raw mut dpi_y) }
        .is_err()
    {
        dpi_x = BASE_DPI;
    }
    Some(MonitorInfo {
        bounds: rect_from_win(mi.rcMonitor),
        work_area: rect_from_win(mi.rcWork),
        dpi: if dpi_x == 0 { BASE_DPI } else { dpi_x },
        primary: mi.dwFlags & MONITORINFOF_PRIMARY != 0,
    })
}

unsafe extern "system" fn collect(
    monitor: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    // SAFETY: `data` is the address of the Vec passed by `all()`, which outlives the
    // synchronous EnumDisplayMonitors call that invokes this callback.
    let out = unsafe { &mut *(data.0 as *mut Vec<MonitorInfo>) };
    if let Some(info) = info_for(monitor) {
        out.push(info);
    }
    BOOL::from(true)
}

/// All monitors, primary first.
#[must_use]
pub fn all() -> Vec<MonitorInfo> {
    let mut out: Vec<MonitorInfo> = Vec::with_capacity(4);
    // SAFETY: the callback only runs during this call and receives a pointer to `out`.
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(collect), LPARAM((&raw mut out) as isize));
    }
    out.sort_by_key(|m| !m.primary);
    out
}

/// The monitor containing (or nearest to) a point.
#[must_use]
pub fn from_point(p: Point) -> Option<MonitorInfo> {
    // SAFETY: plain value arguments.
    let m = unsafe { MonitorFromPoint(point_to_win(p), MONITOR_DEFAULTTONEAREST) };
    info_for(m)
}

/// The monitor with the largest intersection with a rectangle (or the nearest one).
#[must_use]
pub fn from_rect(r: Rect) -> Option<MonitorInfo> {
    let wr = rect_to_win(r);
    // SAFETY: `wr` is a valid RECT for the duration of the call.
    let m = unsafe { MonitorFromRect(&raw const wr, MONITOR_DEFAULTTONEAREST) };
    info_for(m)
}

/// Effective DPI for the monitor showing most of `r` (96 when unknown).
#[must_use]
pub fn dpi_for_rect(r: Rect) -> u32 {
    from_rect(r).map_or(BASE_DPI, |m| m.dpi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_is_at_least_one_monitor_with_sane_values() {
        crate::win::make_thread_dpi_aware();
        let monitors = all();
        assert!(!monitors.is_empty());
        assert!(monitors[0].primary);
        for m in &monitors {
            assert!(!m.bounds.is_empty());
            assert!(m.bounds.intersection(&m.work_area).is_some());
            assert!(m.dpi >= 96);
        }
        let primary = monitors[0];
        let found = from_point(primary.bounds.center()).expect("monitor for point");
        assert_eq!(found.bounds, primary.bounds);
        assert_eq!(
            from_rect(primary.bounds).map(|m| m.bounds),
            Some(primary.bounds)
        );
    }
}
