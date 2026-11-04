//! IPC commands — the only entry points available to the settings UI.
//!
//! Every command is `async` so it never runs on the UI thread; blocking engine and Win32
//! work runs on Tauri's blocking pool.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, PCWSTR, w};

use awwm_core::settings::{KEY_NAMES, Settings};
use awwm_engine::{ManagedWindowInfo, WindowInfo};

use crate::state::{AppSnapshot, AppState, CommandError, lock};

type CmdResult<T> = Result<T, CommandError>;

/// Runs blocking work off the async runtime threads.
async fn blocking<T, F>(f: F) -> CmdResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> CmdResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| CommandError::engine(format!("background task failed: {e}")))?
}

fn engine(state: &State<'_, AppState>) -> awwm_engine::EngineHandle {
    state.handle.clone()
}

#[tauri::command]
pub async fn get_snapshot(app: AppHandle) -> CmdResult<AppSnapshot> {
    blocking(move || app.state::<AppState>().snapshot()).await
}

#[tauri::command]
pub async fn list_windows(state: State<'_, AppState>) -> CmdResult<Vec<WindowInfo>> {
    let handle = engine(&state);
    blocking(move || handle.list_windows().map_err(Into::into)).await
}

#[tauri::command]
pub async fn pin_window(state: State<'_, AppState>, hwnd: u64) -> CmdResult<ManagedWindowInfo> {
    let handle = engine(&state);
    blocking(move || handle.pin(hwnd).map_err(Into::into)).await
}

#[tauri::command]
pub async fn unpin_window(state: State<'_, AppState>, hwnd: u64) -> CmdResult<()> {
    let handle = engine(&state);
    blocking(move || handle.unpin(hwnd).map_err(Into::into)).await
}

#[tauri::command]
pub async fn set_window_opacity(
    state: State<'_, AppState>,
    hwnd: u64,
    opacity: u8,
) -> CmdResult<ManagedWindowInfo> {
    if opacity > 100 {
        return Err(CommandError::invalid("Opacity must be between 0 and 100."));
    }
    let handle = engine(&state);
    blocking(move || handle.set_opacity(hwnd, opacity).map_err(Into::into)).await
}

#[tauri::command]
pub async fn unpin_all(state: State<'_, AppState>) -> CmdResult<()> {
    let handle = engine(&state);
    blocking(move || handle.unpin_all().map_err(Into::into)).await
}

#[tauri::command]
pub async fn activate_window(state: State<'_, AppState>, hwnd: u64) -> CmdResult<()> {
    let handle = engine(&state);
    blocking(move || handle.activate(hwnd).map_err(Into::into)).await
}

#[tauri::command]
pub async fn set_management_enabled(app: AppHandle, enabled: bool) -> CmdResult<()> {
    blocking(move || {
        app.state::<AppState>()
            .update_settings(&app, |s| s.general.management_enabled = enabled)
            .map(|_| ())
    })
    .await
}

#[tauri::command]
pub async fn save_settings(app: AppHandle, settings: Settings) -> CmdResult<AppSnapshot> {
    blocking(move || {
        // The master switch is owned by `set_management_enabled` and the tray: a settings
        // save from the UI never changes it, so a save in flight cannot undo a tray toggle.
        app.state::<AppState>()
            .update_settings(&app, move |current| {
                let enabled = current.general.management_enabled;
                *current = settings;
                current.general.management_enabled = enabled;
            })
    })
    .await
}

#[tauri::command]
pub async fn reset_settings(app: AppHandle) -> CmdResult<AppSnapshot> {
    blocking(move || {
        let state = app.state::<AppState>();
        *lock(&state.settings_warning) = None;
        state.apply_settings(&app, Settings::default())
    })
    .await
}

/// Shows the native file dialog on the UI thread (modal to the settings window) and
/// returns the chosen path, or `None` when cancelled.
async fn pick_file(app: AppHandle, kind: crate::backup::DialogKind) -> CmdResult<Option<PathBuf>> {
    blocking(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        let owner = app
            .get_webview_window(crate::ui_window::MAIN)
            .and_then(|w| w.hwnd().ok());
        // HWND is not Send; pass the raw value to the UI thread.
        let owner_raw = owner.map(|h| h.0 as isize);
        app.run_on_main_thread(move || {
            let owner = owner_raw
                .map(|raw| windows::Win32::Foundation::HWND(raw as *mut core::ffi::c_void));
            let _ = tx.send(crate::backup::pick_file(kind, owner));
        })
        .map_err(|e| CommandError::engine(format!("Could not show the file dialog: {e}")))?;
        rx.recv()
            .map_err(|_| CommandError::engine("The file dialog was closed unexpectedly."))?
            .map_err(CommandError::io)
    })
    .await
}

/// Exports all settings (including automatic rules) to a file the user chooses.
/// Resolves with the written path, or `null` when the dialog was cancelled.
#[tauri::command]
pub async fn export_settings(app: AppHandle) -> CmdResult<Option<String>> {
    let Some(path) = pick_file(app.clone(), crate::backup::DialogKind::Export).await? else {
        return Ok(None);
    };
    blocking(move || {
        let settings = lock(&app.state::<AppState>().settings).clone();
        crate::backup::write_export(&path, &settings).map_err(CommandError::io)?;
        tracing::info!(path = %path.display(), "settings exported");
        Ok(Some(path.display().to_string()))
    })
    .await
}

/// Imports settings from a file the user chooses and applies them (the window-management
/// switch keeps its current state). Resolves with the new snapshot, or `null` when the
/// dialog was cancelled.
#[tauri::command]
pub async fn import_settings(app: AppHandle) -> CmdResult<Option<AppSnapshot>> {
    let Some(path) = pick_file(app.clone(), crate::backup::DialogKind::Import).await? else {
        return Ok(None);
    };
    blocking(move || {
        let imported = crate::backup::read_import(&path).map_err(CommandError::invalid)?;
        let state = app.state::<AppState>();
        let snapshot = state.update_settings(&app, move |current| {
            let enabled = current.general.management_enabled;
            *current = imported;
            current.general.management_enabled = enabled;
        })?;
        tracing::info!(path = %path.display(), rules = snapshot.settings.rules.len(), "settings imported");
        Ok(Some(snapshot))
    })
    .await
}

#[tauri::command]
pub async fn apply_rules_now(state: State<'_, AppState>) -> CmdResult<u32> {
    let handle = engine(&state);
    blocking(move || handle.apply_rules_now().map_err(Into::into)).await
}

/// Accepts only absolute paths on a local drive letter to existing `.exe` files (the
/// paths the engine reported). UNC, device and network paths are refused so the icon
/// lookup can never open a network connection.
fn validated_exe(path: &str) -> Option<PathBuf> {
    use std::path::{Component, Prefix};
    let p = Path::new(path);
    let local_drive = matches!(
        p.components().next(),
        Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_))
    );
    let is_exe = p.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe"));
    (local_drive && p.is_absolute() && is_exe && p.is_file()).then(|| p.to_path_buf())
}

#[tauri::command]
pub async fn get_window_icon(app: AppHandle, exe_path: String) -> CmdResult<Option<String>> {
    if let Some(cached) = lock(&app.state::<AppState>().icons).get(&exe_path) {
        return Ok(cached.clone());
    }
    blocking(move || {
        let icon = validated_exe(&exe_path)
            .and_then(|p| awwm_engine::win::icon::exe_icon_data_url(&p.to_string_lossy()));
        let state = app.state::<AppState>();
        let mut cache = lock(&state.icons);
        if cache.len() > 512 {
            cache.clear();
        }
        cache.insert(exe_path, icon.clone());
        Ok(icon)
    })
    .await
}

/// `AppInfo` in `src/lib/types.ts`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    name: String,
    version: String,
    config_dir: String,
    log_dir: String,
    settings_path: String,
    license: String,
}

#[tauri::command]
pub async fn get_app_info(app: AppHandle, state: State<'_, AppState>) -> CmdResult<AppInfo> {
    Ok(AppInfo {
        name: "AWWM \u{2014} Advanced Window Manager".into(),
        version: app.package_info().version.to_string(),
        config_dir: state.paths.config_dir.display().to_string(),
        log_dir: state.paths.log_dir.display().to_string(),
        settings_path: state.store.path().display().to_string(),
        license: "MIT".into(),
    })
}

#[tauri::command]
pub async fn get_key_names() -> CmdResult<Vec<String>> {
    Ok(KEY_NAMES.iter().map(|s| (*s).to_owned()).collect())
}

/// Folders the UI may open.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LocationKind {
    Config,
    Logs,
}

/// Bundled documents the UI may open.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DocumentKind {
    UserGuide,
    Limitations,
    Architecture,
    Readme,
    License,
}

impl DocumentKind {
    const fn file_name(self) -> &'static str {
        match self {
            DocumentKind::UserGuide => "USER_GUIDE.md",
            DocumentKind::Limitations => "WINDOWS_LIMITATIONS.md",
            DocumentKind::Architecture => "ARCHITECTURE.md",
            DocumentKind::Readme => "README.md",
            DocumentKind::License => "LICENSE.txt",
        }
    }
}

/// Opens a file or folder with its default handler; `.md` files fall back to Notepad when
/// no application is associated with them.
fn shell_open(path: &Path) -> CmdResult<()> {
    let target = HSTRING::from(path.as_os_str());
    // SAFETY: all strings are NUL-terminated and outlive the call; no window handle.
    let r = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            &target,
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if r.0 as isize > 32 {
        return Ok(());
    }
    if path.is_file()
        && let Some(notepad) = system_notepad()
    {
        // Notepad by its full System32 path (never searched for), file argument quoted.
        let notepad = HSTRING::from(notepad.as_os_str());
        let args = HSTRING::from(format!("\"{}\"", path.display()));
        // SAFETY: as above; opens the file with Notepad.
        let r = unsafe {
            ShellExecuteW(
                None,
                w!("open"),
                &notepad,
                &args,
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        if r.0 as isize > 32 {
            return Ok(());
        }
    }
    Err(CommandError::io(format!(
        "Could not open {}.",
        path.display()
    )))
}

/// `%SystemRoot%\System32\notepad.exe`, resolved through the system directory API.
fn system_notepad() -> Option<PathBuf> {
    use windows::Win32::System::SystemInformation::GetSystemDirectoryW;
    let mut buf = [0u16; 260];
    // SAFETY: `buf` is a writable buffer; the function writes at most its length.
    let len = unsafe { GetSystemDirectoryW(Some(&mut buf)) } as usize;
    if len == 0 || len >= buf.len() {
        return None;
    }
    let path = PathBuf::from(String::from_utf16_lossy(&buf[..len])).join("notepad.exe");
    path.is_file().then_some(path)
}

#[tauri::command]
pub async fn open_location(state: State<'_, AppState>, target: LocationKind) -> CmdResult<()> {
    let dir = match target {
        LocationKind::Config => state.paths.config_dir.clone(),
        LocationKind::Logs => state.paths.log_dir.clone(),
    };
    std::fs::create_dir_all(&dir)
        .map_err(|e| CommandError::io(format!("Could not create {}: {e}", dir.display())))?;
    blocking(move || shell_open(&dir)).await
}

#[tauri::command]
pub async fn open_documentation(app: AppHandle, doc: DocumentKind) -> CmdResult<()> {
    let dir = app
        .path()
        .resource_dir()
        .map_err(|e| CommandError::io(format!("Documentation folder not found: {e}")))?;
    let path = dir.join("docs").join(doc.file_name());
    if !path.is_file() {
        return Err(CommandError::io(format!(
            "Documentation file is missing: {}",
            path.display()
        )));
    }
    blocking(move || shell_open(&path)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exe_validation() {
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
        assert!(validated_exe(&format!(r"{windir}\explorer.exe")).is_some());
        assert!(validated_exe("explorer.exe").is_none());
        assert!(validated_exe(&format!(r"{windir}\win.ini")).is_none());
        assert!(validated_exe(r"C:\does\not\exist.exe").is_none());
        assert!(validated_exe(r"\\server\share\app.exe").is_none());
        assert!(validated_exe(r"\\.\pipe\x.exe").is_none());
        assert!(validated_exe(&format!(r"\\?\{windir}\explorer.exe")).is_none());
    }

    #[test]
    fn notepad_is_resolved_from_the_system_directory() {
        let p = system_notepad().expect("notepad");
        assert!(p.is_absolute());
        assert!(
            p.to_string_lossy()
                .to_lowercase()
                .ends_with(r"system32\notepad.exe")
        );
    }
}
