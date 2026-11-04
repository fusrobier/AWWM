//! AWWM application shell: wires the window-management engine to the Tauri settings UI,
//! the system tray, persistence and Windows integration.

mod autostart;
mod backup;
mod commands;
mod logging;
mod paths;
mod settings_store;
mod single_instance;
mod state;
mod tray;
mod ui_window;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};

use awwm_engine::{Engine, EngineEvent, EngineOptions};

use crate::paths::AppPaths;
use crate::settings_store::SettingsStore;
use crate::state::{AppState, event, lock};

/// Forwards engine events to the UI and tray. Runs on engine threads: never blocks on the
/// main thread.
fn on_engine_event(app: &AppHandle, ev: EngineEvent) {
    match ev {
        EngineEvent::ManagedChanged(list) => {
            tray::set_pinned_count(app, list.iter().filter(|w| w.pinned).count());
            let _ = app.emit(event::MANAGED_CHANGED, &list);
        }
        EngineEvent::Notice(notice) => {
            let _ = app.emit(event::NOTICE, &notice);
        }
        EngineEvent::HotkeysChanged(statuses) => {
            if let Some(state) = state::try_state(app) {
                *lock(&state.hotkeys) = statuses;
            }
        }
        EngineEvent::ShowRequested => ui_window::show_from_any_thread(app, None),
    }
}

fn setup(
    app: &mut tauri::App,
    paths: AppPaths,
    logging: logging::Logging,
) -> Result<(), Box<dyn std::error::Error>> {
    let store = SettingsStore::new(paths.settings_file());
    let loaded = store.load();
    let settings = loaded.settings;
    if loaded.first_run {
        // Persist defaults so the user can find and back up the file.
        if let Err(e) = store.save(&settings) {
            tracing::warn!(error = %e, "could not write default settings");
        }
    }
    logging.set_level(settings.general.log_level);
    if let Err(e) = autostart::set_enabled(settings.general.start_with_windows) {
        tracing::warn!(error = %e, "could not synchronise autostart");
    }

    let handle = app.handle().clone();
    let sink: awwm_engine::EventSink = Arc::new(move |ev| on_engine_event(&handle, ev));
    let engine = Engine::start(
        EngineOptions {
            settings: settings.clone(),
            session_path: Some(paths.session_file()),
        },
        sink,
    )?;
    let engine_handle = engine.handle();
    let hotkeys = engine_handle.hotkey_status().unwrap_or_default();
    let enabled = settings.general.management_enabled;
    let show_window = loaded.first_run || !settings.general.start_minimized;

    app.manage(AppState {
        paths,
        store,
        settings: Mutex::new(settings),
        settings_warning: Mutex::new(loaded.warning),
        hotkeys: Mutex::new(hotkeys),
        engine: Mutex::new(Some(engine)),
        handle: engine_handle.clone(),
        icons: Mutex::new(HashMap::new()),
        logging,
        tray: Mutex::new(None),
        apply_lock: Mutex::new(()),
    });
    let tray = tray::create(app.handle(), enabled)?;
    *lock(&app.state::<AppState>().tray) = Some(tray);
    if let Ok(list) = engine_handle.managed() {
        tray::set_pinned_count(app.handle(), list.iter().filter(|w| w.pinned).count());
    }
    if show_window {
        ui_window::show(app.handle(), None);
    }
    tracing::info!(
        version = %app.package_info().version,
        autostart = autostart::launched_at_sign_in(),
        "AWWM started"
    );
    Ok(())
}

/// Stops the engine, restoring windows if configured.
fn shutdown(app: &AppHandle) {
    let Some(state) = state::try_state(app) else {
        return;
    };
    let restore = lock(&state.settings).general.restore_on_exit;
    let engine = lock(&state.engine).take();
    if let Some(engine) = engine {
        engine.shutdown(restore);
    }
    tracing::info!(restore, "AWWM exited");
}

/// AWWM is meant to run without administrator rights. When it was started elevated
/// anyway, explain the risk and let the user exit. Returns `false` to exit.
fn confirm_elevated_start() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        IDCANCEL, MB_ICONWARNING, MB_OKCANCEL, MB_SETFOREGROUND, MessageBoxW,
    };
    const HIGH_INTEGRITY: u32 = 0x3000;
    if awwm_engine::win::process::current_integrity() < HIGH_INTEGRITY {
        return true;
    }
    tracing::warn!("AWWM was started with administrator rights");
    let text = windows::core::HSTRING::from(
        "AWWM is running as administrator, which is not supported.\n\n\
         The per-user installation lives in a folder your normal account can modify, so \
         running it elevated would let other programs gain administrator rights.\n\n\
         Choose Cancel to exit, then start AWWM normally. Choose OK to continue anyway.",
    );
    // SAFETY: modal message box without an owner; both strings outlive the call.
    let choice = unsafe {
        MessageBoxW(
            None,
            &text,
            windows::core::w!("AWWM"),
            MB_OKCANCEL | MB_ICONWARNING | MB_SETFOREGROUND,
        )
    };
    choice != IDCANCEL
}

/// Application entry point.
pub fn run() {
    let paths = match AppPaths::resolve() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("AWWM cannot start: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = paths.ensure() {
        eprintln!("AWWM: cannot create its data folders: {e}");
    }
    let logging = logging::Logging::init(&paths.log_dir, awwm_core::settings::LogLevel::Info);

    if !confirm_elevated_start() {
        return;
    }

    let _instance = match single_instance::acquire() {
        single_instance::Instance::Primary(guard) => guard,
        single_instance::Instance::Secondary => {
            let notified = awwm_engine::signal_running_instance();
            tracing::info!(
                notified,
                "another instance is running; asked it to show its window"
            );
            return;
        }
    };

    let app = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::list_windows,
            commands::pin_window,
            commands::unpin_window,
            commands::set_window_opacity,
            commands::unpin_all,
            commands::activate_window,
            commands::set_management_enabled,
            commands::save_settings,
            commands::reset_settings,
            commands::apply_rules_now,
            commands::get_window_icon,
            commands::get_app_info,
            commands::get_key_names,
            commands::open_location,
            commands::open_documentation,
            commands::export_settings,
            commands::import_settings,
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { .. } = event {
                let app = window.app_handle();
                let keep_running = state::try_state(app)
                    .is_none_or(|s| lock(&s.settings).general.run_in_background);
                if !keep_running {
                    app.exit(0);
                }
            }
        })
        .setup(move |app| setup(app, paths, logging))
        .build(tauri::generate_context!());
    let app = match app {
        Ok(app) => app,
        Err(e) => {
            tracing::error!(error = %e, "failed to start");
            eprintln!("AWWM failed to start: {e}");
            std::process::exit(1);
        }
    };
    app.run(|app, event| match event {
        // The last window was closed: keep running in the tray.
        RunEvent::ExitRequested {
            code: None, api, ..
        } => api.prevent_exit(),
        RunEvent::Exit => shutdown(app),
        _ => {}
    });
}
