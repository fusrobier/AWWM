//! Data types exchanged with the application layer (serialized to the frontend; see
//! `src/lib/types.ts`).

use serde::{Deserialize, Serialize};

/// How a window came to be managed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ManagedSource {
    Manual,
    Rule,
    Recovered,
}

/// A window AWWM manages (pinned and/or transparent).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedWindowInfo {
    pub hwnd: u64,
    pub title: String,
    pub exe_name: String,
    pub exe_path: String,
    pub pid: u32,
    pub pinned: bool,
    /// Pinned, but Windows has not allowed it to become topmost yet (a full-screen
    /// application has the foreground); retried automatically.
    pub pin_pending: bool,
    pub opacity: u8,
    pub source: ManagedSource,
    pub rule_id: Option<String>,
}

/// A top-level application window (window picker).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowInfo {
    pub hwnd: u64,
    pub title: String,
    pub class_name: String,
    pub exe_name: String,
    pub exe_path: String,
    pub pid: u32,
    pub topmost: bool,
    pub managed: bool,
    /// `false` when the window belongs to a higher-integrity (elevated) process.
    pub accessible: bool,
}

/// Severity of a [`Notice`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NoticeLevel {
    Info,
    Warning,
    Error,
}

/// A user-facing message produced by the engine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub level: NoticeLevel,
    pub message: String,
    pub hwnd: Option<u64>,
}

impl Notice {
    #[must_use]
    pub fn new(level: NoticeLevel, message: impl Into<String>, hwnd: Option<u64>) -> Self {
        Self {
            level,
            message: message.into(),
            hwnd,
        }
    }
}

/// Global hotkey actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HotkeyAction {
    TogglePin,
    OpacityUp,
    OpacityDown,
}

/// Registration status of a hotkey.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyStatus {
    pub action: HotkeyAction,
    pub label: String,
    pub registered: bool,
    pub error: Option<String>,
}

/// Events emitted by the engine to the application layer. The callback runs on an engine
/// thread and must return quickly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineEvent {
    /// The managed-window list changed.
    ManagedChanged(Vec<ManagedWindowInfo>),
    /// A message for the user.
    Notice(Notice),
    /// Hotkey registration results changed.
    HotkeysChanged(Vec<HotkeyStatus>),
    /// Another AWWM instance asked this one to show its settings window.
    ShowRequested,
    /// Another program (the installer or uninstaller) asked AWWM to exit normally.
    ExitRequested,
}
