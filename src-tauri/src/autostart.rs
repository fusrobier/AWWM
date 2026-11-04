//! "Start with Windows" through `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
//! Per-user, so no administrator rights are needed.

use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows::core::{PCWSTR, w};

const RUN_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const VALUE_NAME: PCWSTR = w!("AWWM");
/// Command-line flag marking a launch by Windows at sign-in.
pub const AUTOSTART_FLAG: &str = "--autostart";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The command line registered for autostart.
fn command_line() -> Result<String, String> {
    let exe =
        std::env::current_exe().map_err(|e| format!("Cannot determine the program path: {e}"))?;
    Ok(format!("\"{}\" {AUTOSTART_FLAG}", exe.display()))
}

/// Reads the currently registered command, if any.
#[must_use]
pub fn registered_command() -> Option<String> {
    let mut buf = vec![0u16; 2048];
    let mut size = u32::try_from(buf.len() * 2).unwrap_or(4096);
    // SAFETY: `buf` provides `size` writable bytes; constants are static wide strings.
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            VALUE_NAME,
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&raw mut size),
        )
    };
    if r != ERROR_SUCCESS {
        return None;
    }
    let len = (size as usize / 2).saturating_sub(1).min(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]))
}

/// Enables or disables autostart so it matches `enabled`.
///
/// # Errors
/// A user-facing message when the registry cannot be written.
pub fn set_enabled(enabled: bool) -> Result<(), String> {
    if enabled {
        let cmd = command_line()?;
        if registered_command().as_deref() == Some(cmd.as_str()) {
            return Ok(());
        }
        let data = wide(&cmd);
        let bytes =
            u32::try_from(data.len() * 2).map_err(|_| "Program path is too long.".to_owned())?;
        // SAFETY: `data` is a NUL-terminated UTF-16 buffer of `bytes` bytes.
        let r = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                VALUE_NAME,
                REG_SZ.0,
                Some(data.as_ptr().cast()),
                bytes,
            )
        };
        if r != ERROR_SUCCESS {
            return Err(format!(
                "Could not enable start with Windows (error {}).",
                r.0
            ));
        }
        tracing::info!(command = %cmd, "autostart enabled");
    } else {
        // SAFETY: deletes our own value; static wide-string arguments.
        let r = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE_NAME) };
        if r != ERROR_SUCCESS && r != ERROR_FILE_NOT_FOUND {
            return Err(format!(
                "Could not disable start with Windows (error {}).",
                r.0
            ));
        }
        tracing::info!("autostart disabled");
    }
    Ok(())
}

/// Whether this process was started by the autostart entry.
#[must_use]
pub fn launched_at_sign_in() -> bool {
    std::env::args().any(|a| a == AUTOSTART_FLAG)
}
