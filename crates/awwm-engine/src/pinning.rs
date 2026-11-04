//! window_pinning: topmost (always-on-top) handling.

use windows::Win32::UI::WindowsAndMessaging::{SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE};

use awwm_core::Rect;

use crate::error::{EngineError, EngineResult};
use crate::win::window::{self, ZOrder};
use crate::win::{WindowHandle, process};

/// Checks that AWWM may modify the window: it exists, responds and is not protected by
/// a higher integrity level.
///
/// # Errors
/// `WindowNotFound`, `NotResponding` or `AccessDenied`.
pub fn ensure_modifiable(h: WindowHandle, app: &str) -> EngineResult<()> {
    if !window::exists(h) {
        return Err(EngineError::WindowNotFound);
    }
    if process::access(window::process_id(h)) == process::Access::HigherIntegrity {
        return Err(EngineError::elevated(app));
    }
    if window::is_hung(h) {
        return Err(EngineError::NotResponding);
    }
    Ok(())
}

/// Adds or removes `WS_EX_TOPMOST` without activating or moving the window.
///
/// # Errors
/// Fails when the window is gone, hung or protected.
pub fn set_topmost(h: WindowHandle, on: bool, app: &str) -> EngineResult<()> {
    ensure_modifiable(h, app)?;
    let z = if on {
        ZOrder::Topmost
    } else {
        ZOrder::NoTopmost
    };
    window::set_window_pos(
        h,
        z,
        Rect::default(),
        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
    )
    .map_err(|e| EngineError::from_win32(&e, app))?;
    if window::is_topmost(h) != on {
        // SetWindowPos reported success but the window is not where we asked. Windows does
        // this while a full-screen application has the foreground, and some applications
        // veto Z-order changes in WM_WINDOWPOSCHANGING.
        return Err(if on {
            EngineError::TopmostRefused(format!(
                "Windows did not let \u{201c}{app}\u{201d} stay always on top right now. This \
                 happens while a full-screen application (for example a game) is in the \
                 foreground, or when the application controls its own window order."
            ))
        } else {
            EngineError::Unsupported(format!(
                "\u{201c}{app}\u{201d} did not accept leaving the always-on-top band."
            ))
        });
    }
    Ok(())
}
