//! The settings window. It is created on demand and destroyed when closed, so AWWM's
//! background footprint does not include a WebView.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::window::Color;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::core::w;

use awwm_core::settings::Theme;

use crate::state::{AppState, event, lock};

/// Label of the settings window (referenced by `capabilities/default.json`).
pub const MAIN: &str = "main";

/// Whether Windows apps use the dark theme.
fn system_prefers_dark() -> bool {
    let mut value: u32 = 1;
    let mut size = 4u32;
    // SAFETY: `value` is a 4-byte out buffer matching RRF_RT_REG_DWORD.
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&raw mut value).cast()),
            Some(&raw mut size),
        )
    };
    r.is_ok() && value == 0
}

fn background(app: &AppHandle) -> Color {
    let theme = app
        .try_state::<AppState>()
        .map_or(Theme::System, |s| lock(&s.settings).general.theme);
    let dark = match theme {
        Theme::Dark => true,
        Theme::Light => false,
        Theme::System => system_prefers_dark(),
    };
    if dark {
        Color(32, 32, 32, 255)
    } else {
        Color(243, 243, 243, 255)
    }
}

/// Shows (creating if needed) the settings window, optionally on a specific page.
/// Must be called on the main thread or from a context that may create windows.
pub fn show(app: &AppHandle, page: Option<&'static str>) {
    if let Some(window) = app.get_webview_window(MAIN) {
        // Show before restoring: restoring a hidden (minimized-to-tray) window first would
        // briefly show it, hide it again and then show it (a flash).
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        if let Some(page) = page {
            let _ = app.emit_to(MAIN, event::NAVIGATE, page);
        }
        return;
    }
    let url = WebviewUrl::App(match page {
        Some(p) => format!("index.html#{p}").into(),
        None => "index.html".into(),
    });
    let result = WebviewWindowBuilder::new(app, MAIN, url)
        // Defense in depth: the settings window may only ever show AWWM's own bundled UI
        // (or the local dev server in debug builds), never remote content.
        .on_navigation(|url| {
            let host = url.host_str();
            host == Some("tauri.localhost") || (cfg!(debug_assertions) && host == Some("127.0.0.1"))
        })
        .title("AWWM \u{2014} Advanced Window Manager")
        .inner_size(980.0, 680.0)
        .min_inner_size(820.0, 560.0)
        .center()
        .background_color(background(app))
        .build();
    match result {
        Ok(window) => {
            let _ = window.set_focus();
        }
        Err(e) => {
            tracing::error!(error = %e, "could not create the settings window");
            explain_missing_webview(&e.to_string());
        }
    }
}

/// Tells the user once per run why the settings window cannot open. The usual cause is a
/// missing or damaged WebView2 Runtime (for example the portable `awwm.exe` on a PC without
/// it); everything else keeps working in the background, so a log entry alone would leave
/// the user with a tray icon that seems to do nothing.
fn explain_missing_webview(error: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{
        MB_ICONWARNING, MB_OK, MB_SETFOREGROUND, MessageBoxW,
    };
    static SHOWN: AtomicBool = AtomicBool::new(false);
    if SHOWN.swap(true, Ordering::AcqRel) {
        return;
    }
    let text = windows::core::HSTRING::from(format!(
        "AWWM could not open its settings window.\n\n\
         This usually means that the Microsoft Edge WebView2 Runtime is missing or damaged. \
         Run the AWWM installer again, or install the runtime from \
         https://developer.microsoft.com/microsoft-edge/webview2/\n\n\
         Pinned windows, rules, mouse gestures and shortcuts keep working.\n\n\
         Details: {error}"
    ));
    // SAFETY: modal message box without an owner; both strings outlive the call.
    unsafe {
        MessageBoxW(
            None,
            &text,
            w!("AWWM"),
            MB_OK | MB_ICONWARNING | MB_SETFOREGROUND,
        );
    }
}

/// Shows the window from any thread.
pub fn show_from_any_thread(app: &AppHandle, page: Option<&'static str>) {
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || show(&app2, page));
}
