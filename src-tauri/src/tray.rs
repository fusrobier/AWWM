//! system_tray: the notification-area icon and its menu.
//!
//! Menu items are updated only from the main thread (through `run_on_main_thread`), so an
//! engine callback can never block on the UI thread.

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::state::{AppState, lock};
use crate::ui_window;

const TRAY_ID: &str = "awwm-tray";
const ICON_ENABLED: &[u8] = include_bytes!("../icons/tray.png");
const ICON_DISABLED: &[u8] = include_bytes!("../icons/tray-disabled.png");

/// Menu items that change at runtime.
#[derive(Clone)]
pub struct TrayHandles {
    toggle: CheckMenuItem<Wry>,
    pinned: MenuItem<Wry>,
    tray: TrayIcon<Wry>,
}

impl std::fmt::Debug for TrayHandles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrayHandles").finish_non_exhaustive()
    }
}

fn pinned_label(count: usize) -> String {
    match count {
        0 => "View pinned windows".to_owned(),
        n => format!("View pinned windows ({n})"),
    }
}

fn tooltip(enabled: bool) -> &'static str {
    if enabled {
        "AWWM \u{2014} window management on"
    } else {
        "AWWM \u{2014} window management off"
    }
}

fn icon(enabled: bool) -> Option<Image<'static>> {
    Image::from_bytes(if enabled { ICON_ENABLED } else { ICON_DISABLED }).ok()
}

/// Creates the tray icon.
///
/// # Errors
/// The menu or icon could not be created.
pub fn create(app: &AppHandle, enabled: bool) -> tauri::Result<TrayHandles> {
    let open = MenuItem::with_id(app, "open", "Open settings", true, None::<&str>)?;
    let toggle = CheckMenuItem::with_id(
        app,
        "toggle",
        "Window management enabled",
        true,
        enabled,
        None::<&str>,
    )?;
    let pinned = MenuItem::with_id(app, "pinned", pinned_label(0), true, None::<&str>)?;
    let unpin_all = MenuItem::with_id(app, "unpin_all", "Unpin all windows", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Exit AWWM", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &open,
            &toggle,
            &PredefinedMenuItem::separator(app)?,
            &pinned,
            &unpin_all,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(tooltip(enabled))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => ui_window::show(app, None),
            "pinned" => ui_window::show(app, Some("dashboard")),
            "toggle" => toggle_management(app),
            "unpin_all" => unpin_all_windows(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                ui_window::show(tray.app_handle(), None);
            }
        });
    if let Some(image) = icon(enabled) {
        builder = builder.icon(image);
    }
    let tray = builder.build(app)?;
    Ok(TrayHandles {
        toggle,
        pinned,
        tray,
    })
}

/// Toggles window management from the tray (work runs off the UI thread).
fn toggle_management(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let result = state.update_settings(&app, |s| {
            s.general.management_enabled = !s.general.management_enabled;
        });
        if let Err(e) = result {
            tracing::error!(error = %e.message, "could not toggle window management");
            // The menu item toggled its own check mark; put it back to the real state.
            let current = lock(&state.settings).general.management_enabled;
            set_enabled(&app, current);
        }
    });
}

fn unpin_all_windows(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(e) = app.state::<AppState>().handle.unpin_all() {
            tracing::error!(error = %e, "could not unpin all windows");
        }
    });
}

/// Reflects the management switch in the menu and icon (safe from any thread).
pub fn set_enabled(app: &AppHandle, enabled: bool) {
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        let state = app2.state::<AppState>();
        let handles = lock(&state.tray).clone();
        if let Some(h) = handles {
            let _ = h.toggle.set_checked(enabled);
            let _ = h.tray.set_tooltip(Some(tooltip(enabled)));
            let _ = h.tray.set_icon(icon(enabled));
        }
    });
}

/// Updates the pinned-window count (safe from any thread).
pub fn set_pinned_count(app: &AppHandle, count: usize) {
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(state) = app2.try_state::<AppState>() {
            let handles = lock(&state.tray).clone();
            if let Some(h) = handles {
                let _ = h.pinned.set_text(pinned_label(count));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels() {
        assert_eq!(pinned_label(0), "View pinned windows");
        assert_eq!(pinned_label(3), "View pinned windows (3)");
        assert!(tooltip(false).contains("off"));
        assert!(icon(true).is_some() && icon(false).is_some());
    }
}
