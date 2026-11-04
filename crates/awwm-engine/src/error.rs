//! Engine error type. Messages are user-facing (shown in the settings UI).

use thiserror::Error;

/// Errors returned by engine operations.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EngineError {
    /// The window was closed or the handle is no longer valid.
    #[error("The window no longer exists.")]
    WindowNotFound,
    /// Blocked by Windows security (User Interface Privilege Isolation).
    #[error("{0}")]
    AccessDenied(String),
    /// The window cannot support the requested operation.
    #[error("{0}")]
    Unsupported(String),
    /// Windows (or the application itself) kept the window out of the always-on-top band.
    /// Typical cause: a full-screen application has the foreground.
    #[error("{0}")]
    TopmostRefused(String),
    /// The target application is not responding.
    #[error("The application is not responding. Try again when it responds.")]
    NotResponding,
    /// An argument was out of range or malformed.
    #[error("{0}")]
    InvalidArgument(String),
    /// File-system error (settings, session).
    #[error("{0}")]
    Io(String),
    /// Internal engine failure (thread stopped, timeout).
    #[error("Window manager engine error: {0}")]
    Internal(String),
}

impl EngineError {
    /// Stable machine-readable code (mirrors `ErrorCode` in `src/lib/types.ts`).
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            EngineError::WindowNotFound => "windowNotFound",
            EngineError::AccessDenied(_) => "accessDenied",
            EngineError::Unsupported(_) | EngineError::TopmostRefused(_) => "unsupported",
            EngineError::NotResponding => "notResponding",
            EngineError::InvalidArgument(_) => "invalidArgument",
            EngineError::Io(_) => "io",
            EngineError::Internal(_) => "engine",
        }
    }

    /// Standard message for windows of elevated processes.
    #[must_use]
    pub fn elevated(app: &str) -> Self {
        EngineError::AccessDenied(format!(
            "\u{201c}{app}\u{201d} is running as administrator. Windows does not allow programs \
             without administrator rights to change its windows, so AWWM leaves it alone."
        ))
    }

    /// Maps a failed Win32 call on a window to the most helpful error.
    #[must_use]
    pub fn from_win32(err: &windows::core::Error, app: &str) -> Self {
        use windows::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_INVALID_WINDOW_HANDLE};
        let code = err.code();
        if code == ERROR_ACCESS_DENIED.to_hresult() {
            Self::elevated(app)
        } else if code == ERROR_INVALID_WINDOW_HANDLE.to_hresult() {
            EngineError::WindowNotFound
        } else {
            EngineError::Unsupported(format!(
                "Windows rejected the change for \u{201c}{app}\u{201d}: {}",
                err.message()
            ))
        }
    }
}

/// Engine result alias.
pub type EngineResult<T> = Result<T, EngineError>;

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_INVALID_WINDOW_HANDLE};

    #[test]
    fn codes_are_stable() {
        assert_eq!(EngineError::WindowNotFound.code(), "windowNotFound");
        assert_eq!(EngineError::NotResponding.code(), "notResponding");
        assert_eq!(EngineError::Internal(String::new()).code(), "engine");
    }

    #[test]
    fn win32_errors_map_to_helpful_variants() {
        let denied = windows::core::Error::from_hresult(ERROR_ACCESS_DENIED.to_hresult());
        assert!(
            matches!(EngineError::from_win32(&denied, "x.exe"), EngineError::AccessDenied(m) if m.contains("administrator"))
        );
        let gone = windows::core::Error::from_hresult(ERROR_INVALID_WINDOW_HANDLE.to_hresult());
        assert_eq!(
            EngineError::from_win32(&gone, "x.exe"),
            EngineError::WindowNotFound
        );
    }
}
