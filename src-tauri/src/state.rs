//! Application state shared by IPC commands, the tray and engine callbacks.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use awwm_core::settings::Settings;
use awwm_engine::{Engine, EngineError, EngineHandle, HotkeyStatus, ManagedWindowInfo};

use crate::logging::Logging;
use crate::paths::AppPaths;
use crate::settings_store::SettingsStore;
use crate::tray::TrayHandles;

/// Event names (mirrored in `src/lib/api.ts`).
pub mod event {
    pub const MANAGED_CHANGED: &str = "awwm://managed-changed";
    pub const NOTICE: &str = "awwm://notice";
    pub const SNAPSHOT: &str = "awwm://snapshot";
    pub const NAVIGATE: &str = "awwm://navigate";
}

/// Locks a mutex, recovering the data if a panic poisoned it (all guarded values are
/// replaced wholesale, so they are always consistent).
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Error returned by every IPC command (`CommandError` in `src/lib/types.ts`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: &'static str,
    pub message: String,
}

impl CommandError {
    pub fn io(message: impl Into<String>) -> Self {
        Self {
            code: "io",
            message: message.into(),
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: "invalidArgument",
            message: message.into(),
        }
    }

    pub fn engine(message: impl Into<String>) -> Self {
        Self {
            code: "engine",
            message: message.into(),
        }
    }
}

impl From<EngineError> for CommandError {
    fn from(e: EngineError) -> Self {
        Self {
            code: e.code(),
            message: e.to_string(),
        }
    }
}

/// Everything the UI needs to render (`AppSnapshot` in `src/lib/types.ts`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub management_enabled: bool,
    pub managed: Vec<ManagedWindowInfo>,
    pub settings: Settings,
    pub hotkeys: Vec<HotkeyStatus>,
    pub settings_warning: Option<String>,
}

/// Global application state (`tauri::State<AppState>`).
pub struct AppState {
    pub paths: AppPaths,
    pub store: SettingsStore,
    pub settings: Mutex<Settings>,
    pub settings_warning: Mutex<Option<String>>,
    pub hotkeys: Mutex<Vec<HotkeyStatus>>,
    pub engine: Mutex<Option<Engine>>,
    pub handle: EngineHandle,
    pub icons: Mutex<HashMap<String, Option<String>>>,
    pub logging: Logging,
    pub tray: Mutex<Option<TrayHandles>>,
    /// Serializes settings changes (UI saves, tray toggle, reset) so the settings file,
    /// the engine and this state never diverge through interleaved updates.
    pub apply_lock: Mutex<()>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("paths", &self.paths)
            .finish_non_exhaustive()
    }
}

impl AppState {
    /// Builds the current snapshot.
    ///
    /// # Errors
    /// The engine did not answer.
    pub fn snapshot(&self) -> Result<AppSnapshot, CommandError> {
        let managed = self.handle.managed()?;
        let settings = lock(&self.settings).clone();
        Ok(AppSnapshot {
            management_enabled: settings.general.management_enabled,
            managed,
            settings,
            hotkeys: lock(&self.hotkeys).clone(),
            settings_warning: lock(&self.settings_warning).clone(),
        })
    }

    /// Validates, persists and applies new settings (autostart, engine, log level, tray)
    /// and notifies the UI. Returns the resulting snapshot.
    ///
    /// # Errors
    /// The settings could not be saved, autostart could not be changed, or the engine did
    /// not answer.
    pub fn apply_settings(
        &self,
        app: &AppHandle,
        new: Settings,
    ) -> Result<AppSnapshot, CommandError> {
        let _serialized = lock(&self.apply_lock);
        self.apply_locked(app, new)
    }

    /// Applies a change to the *current* settings atomically with respect to other
    /// settings changes (read-modify-write under the apply lock).
    ///
    /// # Errors
    /// As [`AppState::apply_settings`].
    pub fn update_settings(
        &self,
        app: &AppHandle,
        change: impl FnOnce(&mut Settings),
    ) -> Result<AppSnapshot, CommandError> {
        let _serialized = lock(&self.apply_lock);
        let mut next = lock(&self.settings).clone();
        change(&mut next);
        self.apply_locked(app, next)
    }

    fn apply_locked(&self, app: &AppHandle, new: Settings) -> Result<AppSnapshot, CommandError> {
        let mut new = new.sanitized();
        let old = lock(&self.settings).clone();
        let mut autostart_error = None;
        if new.general.start_with_windows != old.general.start_with_windows
            && let Err(e) = crate::autostart::set_enabled(new.general.start_with_windows)
        {
            new.general.start_with_windows = old.general.start_with_windows;
            autostart_error = Some(e);
        }
        self.store.save(&new).map_err(CommandError::io)?;
        let statuses = self.handle.apply_settings(new.clone())?;
        *lock(&self.hotkeys) = statuses;
        if new.general.log_level != old.general.log_level {
            self.logging.set_level(new.general.log_level);
        }
        let enabled = new.general.management_enabled;
        *lock(&self.settings) = new;
        crate::tray::set_enabled(app, enabled);
        let snapshot = self.snapshot()?;
        let _ = app.emit(event::SNAPSHOT, &snapshot);
        tracing::info!(enabled, "settings applied");
        match autostart_error {
            Some(e) => Err(CommandError::io(e)),
            None => Ok(snapshot),
        }
    }
}

/// Returns the managed state, if it is registered yet.
pub fn try_state(app: &AppHandle) -> Option<tauri::State<'_, AppState>> {
    app.try_state::<AppState>()
}
