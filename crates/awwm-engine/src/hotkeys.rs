//! Global keyboard shortcuts registered with `RegisterHotKey` on the engine's
//! message-only window.

use windows::Win32::Foundation::ERROR_HOTKEY_ALREADY_REGISTERED;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey,
    UnregisterHotKey,
};

use awwm_core::settings::{Hotkey, HotkeySettings};

use crate::types::{HotkeyAction, HotkeyStatus};
use crate::win::WindowHandle;

const ACTIONS: [HotkeyAction; 3] = [
    HotkeyAction::TogglePin,
    HotkeyAction::OpacityUp,
    HotkeyAction::OpacityDown,
];

const fn id_for(action: HotkeyAction) -> i32 {
    match action {
        HotkeyAction::TogglePin => 1,
        HotkeyAction::OpacityUp => 2,
        HotkeyAction::OpacityDown => 3,
    }
}

/// Maps a `WM_HOTKEY` id back to its action.
#[must_use]
pub fn action_for_id(id: i32) -> Option<HotkeyAction> {
    ACTIONS.into_iter().find(|&a| id_for(a) == id)
}

fn modifiers(hk: &Hotkey) -> HOT_KEY_MODIFIERS {
    let mut m = MOD_NOREPEAT;
    if hk.ctrl {
        m |= MOD_CONTROL;
    }
    if hk.alt {
        m |= MOD_ALT;
    }
    if hk.shift {
        m |= MOD_SHIFT;
    }
    if hk.win {
        m |= MOD_WIN;
    }
    m
}

/// The registered hotkeys of the engine window.
#[derive(Debug)]
pub struct Hotkeys {
    owner: WindowHandle,
    registered: Vec<i32>,
    status: Vec<HotkeyStatus>,
}

impl Hotkeys {
    /// Creates an empty set bound to `owner` (the engine's message window).
    #[must_use]
    pub const fn new(owner: WindowHandle) -> Self {
        Self {
            owner,
            registered: Vec::new(),
            status: Vec::new(),
        }
    }

    /// Current registration status.
    #[must_use]
    pub fn status(&self) -> Vec<HotkeyStatus> {
        self.status.clone()
    }

    fn unregister_all(&mut self) {
        for id in self.registered.drain(..) {
            // SAFETY: unregisters an id we registered on our own window.
            unsafe {
                let _ = UnregisterHotKey(Some(self.owner.hwnd()), id);
            }
        }
    }

    /// (Re-)registers the configured hotkeys. With `enabled == false` all are removed.
    pub fn apply(&mut self, settings: &HotkeySettings, enabled: bool) -> Vec<HotkeyStatus> {
        self.unregister_all();
        self.status.clear();
        for action in ACTIONS {
            let hk = match action {
                HotkeyAction::TogglePin => &settings.toggle_pin,
                HotkeyAction::OpacityUp => &settings.opacity_up,
                HotkeyAction::OpacityDown => &settings.opacity_down,
            };
            let Some(hk) = hk else { continue };
            let label = hk.label();
            let Some(vk) = hk.virtual_key().filter(|_| hk.is_valid()) else {
                self.status.push(HotkeyStatus {
                    action,
                    label,
                    registered: false,
                    error: Some("Invalid shortcut.".into()),
                });
                continue;
            };
            if !enabled {
                self.status.push(HotkeyStatus {
                    action,
                    label,
                    registered: false,
                    error: Some("Inactive while window management is turned off.".into()),
                });
                continue;
            }
            let id = id_for(action);
            // SAFETY: registers a hotkey for our own message window.
            match unsafe {
                RegisterHotKey(Some(self.owner.hwnd()), id, modifiers(hk), u32::from(vk))
            } {
                Ok(()) => {
                    self.registered.push(id);
                    self.status.push(HotkeyStatus {
                        action,
                        label,
                        registered: true,
                        error: None,
                    });
                }
                Err(e) => {
                    let error = if e.code() == ERROR_HOTKEY_ALREADY_REGISTERED.to_hresult() {
                        "Already used by another application or by Windows.".to_owned()
                    } else {
                        format!("Could not register: {}", e.message())
                    };
                    tracing::warn!(shortcut = %label, %error, "hotkey registration failed");
                    self.status.push(HotkeyStatus {
                        action,
                        label,
                        registered: false,
                        error: Some(error),
                    });
                }
            }
        }
        self.status()
    }
}

impl Drop for Hotkeys {
    fn drop(&mut self) {
        self.unregister_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip() {
        for a in ACTIONS {
            assert_eq!(action_for_id(id_for(a)), Some(a));
        }
        assert_eq!(action_for_id(99), None);
    }

    #[test]
    fn modifier_flags() {
        let hk = Hotkey {
            ctrl: true,
            alt: false,
            shift: true,
            win: true,
            key: "T".into(),
        };
        let m = modifiers(&hk);
        assert!(
            m.contains(MOD_CONTROL)
                && m.contains(MOD_SHIFT)
                && m.contains(MOD_WIN)
                && m.contains(MOD_NOREPEAT)
        );
        assert!(!m.contains(MOD_ALT));
    }
}
