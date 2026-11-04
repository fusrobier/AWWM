//! window_transparency: uniform window opacity through layered-window attributes.
//!
//! A window that is *already* layered either uses `SetLayeredWindowAttributes` (its
//! key/alpha/flags are readable with `GetLayeredWindowAttributes`, and we save and restore
//! them) or per-pixel `UpdateLayeredWindow` (unreadable; switching such a window to uniform
//! alpha would break its rendering, so we refuse with an explanation).

use serde::{Deserialize, Serialize};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::Gdi::{
    RDW_ALLCHILDREN, RDW_ERASE, RDW_FRAME, RDW_INVALIDATE, RedrawWindow,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetLayeredWindowAttributes, LAYERED_WINDOW_ATTRIBUTES_FLAGS, LWA_ALPHA, LWA_COLORKEY,
    SetLayeredWindowAttributes, WINDOW_EX_STYLE, WS_EX_LAYERED,
};

use awwm_core::settings::opacity_to_alpha;

use crate::error::{EngineError, EngineResult};
use crate::win::{WindowHandle, window};

/// The layered state of a window before AWWM changed it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum OriginalLayering {
    /// The window did not have `WS_EX_LAYERED`.
    NotLayered,
    /// The window used `SetLayeredWindowAttributes` with these values.
    Attributes { key: u32, alpha: u8, flags: u32 },
}

/// Current layered attributes, if the window is layered with readable attributes.
#[must_use]
pub fn read_attributes(h: WindowHandle) -> Option<(u32, u8, u32)> {
    if !window::ex_style(h).contains(WS_EX_LAYERED) {
        return None;
    }
    let mut key = COLORREF(0);
    let mut alpha = 0u8;
    let mut flags = LAYERED_WINDOW_ATTRIBUTES_FLAGS(0);
    // SAFETY: all out pointers reference valid stack locals.
    unsafe {
        GetLayeredWindowAttributes(
            h.hwnd(),
            Some(&raw mut key),
            Some(&raw mut alpha),
            Some(&raw mut flags),
        )
    }
    .ok()?;
    Some((key.0, alpha, flags.0))
}

/// Captures the window's current layering so it can be restored later.
///
/// # Errors
/// `Unsupported` when the window uses per-pixel alpha (`UpdateLayeredWindow`).
pub fn capture(h: WindowHandle, app: &str) -> EngineResult<OriginalLayering> {
    if !window::ex_style(h).contains(WS_EX_LAYERED) {
        return Ok(OriginalLayering::NotLayered);
    }
    match read_attributes(h) {
        Some((key, alpha, flags)) => Ok(OriginalLayering::Attributes { key, alpha, flags }),
        None => Err(per_pixel_error(app)),
    }
}

fn per_pixel_error(app: &str) -> EngineError {
    EngineError::Unsupported(format!(
        "\u{201c}{app}\u{201d} draws its own per-pixel transparency (UpdateLayeredWindow). \
         Applying uniform transparency would break its rendering, so it cannot be changed."
    ))
}

/// Changing `GWL_EXSTYLE` sends `WM_STYLECHANGING`/`WM_STYLECHANGED` to the window's
/// thread synchronously; never do that to an application that is not responding, or the
/// engine thread would block with it.
fn ensure_responsive(h: WindowHandle) -> EngineResult<()> {
    if !window::exists(h) {
        Err(EngineError::WindowNotFound)
    } else if window::is_hung(h) {
        Err(EngineError::NotResponding)
    } else {
        Ok(())
    }
}

/// Changes the extended style after checking the window responds. (Only the style change
/// is synchronous with the target; `SetLayeredWindowAttributes` alone never blocks, so a
/// hung window that is already layered can still have its opacity changed or restored.)
fn set_ex_style_checked(h: WindowHandle, ex: WINDOW_EX_STYLE, app: &str) -> EngineResult<()> {
    ensure_responsive(h)?;
    window::set_ex_style(h, ex).map_err(|e| EngineError::from_win32(&e, app))
}

/// The alpha byte AWWM applies for `opacity` given the original layering. When the
/// application already used uniform alpha, the result is relative to its own value.
#[must_use]
pub fn effective_alpha(original: OriginalLayering, opacity: u8) -> u8 {
    let ours = u32::from(opacity_to_alpha(opacity));
    match original {
        OriginalLayering::Attributes { alpha, flags, .. } if flags & LWA_ALPHA.0 != 0 => {
            u8::try_from((ours * u32::from(alpha) + 127) / 255).unwrap_or(u8::MAX)
        }
        _ => u8::try_from(ours).unwrap_or(u8::MAX),
    }
}

/// Applies `opacity` (0–100 %). 100 % restores the original layering.
///
/// # Errors
/// Fails when Windows rejects the style change (elevated target) or the window does not
/// keep the requested attributes.
pub fn apply(
    h: WindowHandle,
    original: OriginalLayering,
    opacity: u8,
    app: &str,
) -> EngineResult<()> {
    if opacity >= 100 {
        return restore(h, original, app);
    }
    let ex = window::ex_style(h);
    if !ex.contains(WS_EX_LAYERED) {
        set_ex_style_checked(h, ex | WS_EX_LAYERED, app)?;
    } else if matches!(original, OriginalLayering::NotLayered) && read_attributes(h).is_none() {
        // The application switched itself to per-pixel layering after we pinned it.
        return Err(per_pixel_error(app));
    }
    let alpha = effective_alpha(original, opacity);
    let (key, flags) = match original {
        OriginalLayering::Attributes { key, flags, .. } if flags & LWA_COLORKEY.0 != 0 => {
            (COLORREF(key), LWA_ALPHA | LWA_COLORKEY)
        }
        _ => (COLORREF(0), LWA_ALPHA),
    };
    // SAFETY: plain value arguments; Win32 validates the handle.
    unsafe { SetLayeredWindowAttributes(h.hwnd(), key, alpha, flags) }
        .map_err(|e| EngineError::from_win32(&e, app))?;
    match read_attributes(h) {
        Some((_, a, f)) if a == alpha && f & LWA_ALPHA.0 != 0 => Ok(()),
        _ => Err(EngineError::Unsupported(format!(
            "\u{201c}{app}\u{201d} did not accept the transparency change."
        ))),
    }
}

/// Restores the original layering.
///
/// # Errors
/// Fails when Windows rejects the change (window gone or protected).
pub fn restore(h: WindowHandle, original: OriginalLayering, app: &str) -> EngineResult<()> {
    match original {
        OriginalLayering::NotLayered => {
            let ex = window::ex_style(h);
            if ex.contains(WS_EX_LAYERED) {
                set_ex_style_checked(h, ex & !WS_EX_LAYERED, app)?;
                // Removing WS_EX_LAYERED discards the redirection bitmap; force a repaint so
                // the window does not show stale or black content.
                // SAFETY: plain value arguments.
                unsafe {
                    let _ = RedrawWindow(
                        Some(h.hwnd()),
                        None,
                        None,
                        RDW_ERASE | RDW_INVALIDATE | RDW_FRAME | RDW_ALLCHILDREN,
                    );
                }
            }
            Ok(())
        }
        OriginalLayering::Attributes { key, alpha, flags } => {
            let ex = window::ex_style(h);
            if !ex.contains(WS_EX_LAYERED) {
                set_ex_style_checked(h, ex | WS_EX_LAYERED, app)?;
            }
            // SAFETY: plain value arguments.
            unsafe {
                SetLayeredWindowAttributes(
                    h.hwnd(),
                    COLORREF(key),
                    alpha,
                    LAYERED_WINDOW_ATTRIBUTES_FLAGS(flags),
                )
            }
            .map_err(|e| EngineError::from_win32(&e, app))
        }
    }
}

/// Whether the window currently shows the alpha AWWM expects (used to re-assert state when
/// an application resets its own styles).
#[must_use]
pub fn is_applied(h: WindowHandle, original: OriginalLayering, opacity: u8) -> bool {
    if opacity >= 100 {
        return true;
    }
    read_attributes(h)
        .is_some_and(|(_, a, f)| f & LWA_ALPHA.0 != 0 && a == effective_alpha(original, opacity))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_alpha_is_relative_to_existing_alpha() {
        assert_eq!(effective_alpha(OriginalLayering::NotLayered, 100), 255);
        assert_eq!(effective_alpha(OriginalLayering::NotLayered, 50), 128);
        assert_eq!(effective_alpha(OriginalLayering::NotLayered, 0), 0);
        let half = OriginalLayering::Attributes {
            key: 0,
            alpha: 128,
            flags: LWA_ALPHA.0,
        };
        assert_eq!(effective_alpha(half, 100), 128);
        assert_eq!(effective_alpha(half, 50), 64);
        let colorkey_only = OriginalLayering::Attributes {
            key: 0xFF00FF,
            alpha: 0,
            flags: LWA_COLORKEY.0,
        };
        assert_eq!(effective_alpha(colorkey_only, 50), 128);
    }

    #[test]
    fn original_layering_serializes_with_tag() {
        let json = serde_json::to_string(&OriginalLayering::Attributes {
            key: 1,
            alpha: 2,
            flags: 3,
        })
        .expect("json");
        assert_eq!(json, r#"{"kind":"attributes","key":1,"alpha":2,"flags":3}"#);
        let back: OriginalLayering =
            serde_json::from_str(r#"{"kind":"notLayered"}"#).expect("parse");
        assert_eq!(back, OriginalLayering::NotLayered);
    }
}
