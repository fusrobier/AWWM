//! Single-instance guard: a per-session named mutex. A second launch asks the running
//! instance to show its settings window and exits, so two sets of global hooks never
//! compete.

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS, GetLastError, HANDLE,
};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::w;

/// Holds the instance mutex for the lifetime of the process.
#[derive(Debug)]
pub struct InstanceGuard(HANDLE);

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        if self.0.is_invalid() {
            return;
        }
        // SAFETY: the handle was returned by CreateMutexW and is closed once.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Outcome of [`acquire`].
#[derive(Debug)]
pub enum Instance {
    /// This is the first instance.
    Primary(InstanceGuard),
    /// Another instance is already running.
    Secondary,
}

/// Acquires the single-instance mutex.
#[must_use]
pub fn acquire() -> Instance {
    // SAFETY: creates/opens a named mutex in the session namespace; the name is static.
    let created = unsafe { CreateMutexW(None, false, w!(r"Local\AWWM.SingleInstance.6f1c2d4e")) };
    // SAFETY: reads the calling thread's last error immediately after CreateMutexW.
    let already = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    match created {
        Ok(handle) if !already => Instance::Primary(InstanceGuard(handle)),
        Ok(handle) => {
            // SAFETY: we opened an existing mutex; close our handle to it.
            unsafe {
                let _ = CloseHandle(handle);
            }
            Instance::Secondary
        }
        // Access denied: the mutex exists but belongs to an instance with a higher
        // integrity level (started as administrator). Never run a second set of hooks.
        Err(e) if e.code() == ERROR_ACCESS_DENIED.to_hresult() => Instance::Secondary,
        // Any other failure is unexpected; start normally and log it.
        Err(e) => {
            tracing::warn!(error = %e.message(), "single-instance mutex unavailable");
            Instance::Primary(InstanceGuard(HANDLE::default()))
        }
    }
}
