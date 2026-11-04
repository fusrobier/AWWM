//! Process information: executable path, integrity level and UWP host resolution.

use std::sync::OnceLock;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{
    GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TOKEN_MANDATORY_LABEL,
    TOKEN_QUERY, TokenIntegrityLevel,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::FindWindowExW;
use windows::core::{PCWSTR, PWSTR, w};

use super::{WindowHandle, window};

/// Owned process handle closed on drop.
struct ProcessHandle(HANDLE);

impl ProcessHandle {
    fn open(pid: u32) -> Option<Self> {
        if pid == 0 {
            return None;
        }
        // SAFETY: plain call; PROCESS_QUERY_LIMITED_INFORMATION is granted for almost every
        // process, including elevated ones, but not for protected system processes.
        let h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
        Some(Self(h))
    }
}

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        // SAFETY: the handle was returned by OpenProcess and is closed exactly once.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Integrity level comparison with the current process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// Same or lower integrity level: windows can be modified.
    Allowed,
    /// Higher integrity level (elevated / UIAccess / system): UIPI blocks modification.
    HigherIntegrity,
    /// The process could not be inspected (protected process, already exited).
    Unknown,
}

/// The current process id.
#[must_use]
pub fn current_pid() -> u32 {
    // SAFETY: no arguments, cannot fail.
    unsafe { GetCurrentProcessId() }
}

/// Full Win32 image path of a process, e.g. `C:\Windows\System32\notepad.exe`.
#[must_use]
pub fn exe_path(pid: u32) -> Option<String> {
    let process = ProcessHandle::open(pid)?;
    let mut buf = vec![0u16; 1024];
    let mut len = u32::try_from(buf.len()).unwrap_or(1024);
    // SAFETY: `buf` is writable for `len` u16 units and `len` is a valid in/out pointer.
    unsafe {
        QueryFullProcessImageNameW(
            process.0,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &raw mut len,
        )
    }
    .ok()?;
    Some(super::from_wide(&buf, len as usize))
}

/// Mandatory integrity level RID of a process token (e.g. 0x2000 medium, 0x3000 high).
fn token_integrity(process: HANDLE) -> Option<u32> {
    let mut token = HANDLE::default();
    // SAFETY: `token` is a valid out pointer; the handle is closed below.
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &raw mut token) }.ok()?;
    let result = (|| {
        let mut needed = 0u32;
        // SAFETY: size query with a null buffer; expected to fail with the needed size.
        let _ =
            unsafe { GetTokenInformation(token, TokenIntegrityLevel, None, 0, &raw mut needed) };
        if needed == 0 {
            return None;
        }
        // u64 storage keeps the TOKEN_MANDATORY_LABEL suitably aligned.
        let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
        // SAFETY: `buf` provides at least `needed` writable, 8-byte aligned bytes.
        unsafe {
            GetTokenInformation(
                token,
                TokenIntegrityLevel,
                Some(buf.as_mut_ptr().cast()),
                needed,
                &raw mut needed,
            )
        }
        .ok()?;
        // SAFETY: on success the buffer starts with a TOKEN_MANDATORY_LABEL whose SID
        // pointer refers into the same buffer, which outlives these reads.
        unsafe {
            let label = &*(buf.as_ptr().cast::<TOKEN_MANDATORY_LABEL>());
            let sid = label.Label.Sid;
            let count = *GetSidSubAuthorityCount(sid);
            if count == 0 {
                return None;
            }
            Some(*GetSidSubAuthority(sid, u32::from(count) - 1))
        }
    })();
    // SAFETY: `token` was opened above and is closed exactly once.
    unsafe {
        let _ = CloseHandle(token);
    }
    result
}

/// Integrity level of the current process (cached).
#[must_use]
pub fn current_integrity() -> u32 {
    static LEVEL: OnceLock<u32> = OnceLock::new();
    // SAFETY: GetCurrentProcess returns a pseudo-handle that needs no closing.
    *LEVEL.get_or_init(|| token_integrity(unsafe { GetCurrentProcess() }).unwrap_or(0x2000))
}

/// Compares the integrity level of `pid` with ours.
#[must_use]
pub fn access(pid: u32) -> Access {
    if pid == current_pid() {
        return Access::Allowed;
    }
    let Some(process) = ProcessHandle::open(pid) else {
        return Access::Unknown;
    };
    match token_integrity(process.0) {
        Some(level) if level > current_integrity() => Access::HigherIntegrity,
        Some(_) => Access::Allowed,
        // Opening the token is denied for some protected processes; Win32 calls will tell.
        None => Access::Unknown,
    }
}

/// The process that actually renders a window. For UWP apps hosted in
/// `ApplicationFrameHost.exe`, this is the process of the hosted `CoreWindow` child.
#[must_use]
pub fn effective_pid(h: WindowHandle) -> u32 {
    let pid = window::process_id(h);
    if window::class_name(h) != "ApplicationFrameWindow" {
        return pid;
    }
    // SAFETY: plain query on a child window class; returns an error when not found.
    let core = unsafe {
        FindWindowExW(
            Some(h.hwnd()),
            None,
            w!("Windows.UI.Core.CoreWindow"),
            PCWSTR::null(),
        )
    };
    match core {
        Ok(c) if !c.is_invalid() => {
            let child_pid = window::process_id(WindowHandle::from_hwnd(c));
            if child_pid == 0 { pid } else { child_pid }
        }
        _ => pid,
    }
}

/// File name portion of a path.
#[must_use]
pub fn exe_name(path: &str) -> String {
    awwm_core::rules::file_name(path).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_process_is_inspectable() {
        let pid = current_pid();
        let path = exe_path(pid).expect("own exe path");
        assert!(path.to_lowercase().ends_with(".exe"), "{path}");
        assert_eq!(access(pid), Access::Allowed);
        assert!(current_integrity() >= 0x1000);
    }

    #[test]
    fn invalid_pid_yields_nothing() {
        assert!(exe_path(0).is_none());
        assert_eq!(access(0), Access::Unknown);
    }
}
