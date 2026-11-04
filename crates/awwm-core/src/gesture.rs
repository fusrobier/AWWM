//! Mouse-gesture model: modifiers, buttons and the pure decision whether a button press
//! starts a move or resize gesture.

use serde::{Deserialize, Serialize};

/// A keyboard modifier that can activate gestures or snapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Modifier {
    Alt,
    Ctrl,
    Shift,
    Win,
}

impl Modifier {
    /// Every modifier, in a stable order.
    pub const ALL: [Modifier; 4] = [
        Modifier::Alt,
        Modifier::Ctrl,
        Modifier::Shift,
        Modifier::Win,
    ];

    /// Human-readable name.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Modifier::Alt => "Alt",
            Modifier::Ctrl => "Ctrl",
            Modifier::Shift => "Shift",
            Modifier::Win => "Win",
        }
    }
}

/// A mouse button used for gestures.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Snapshot of the physical modifier keys.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ModifierState {
    pub alt: bool,
    pub ctrl: bool,
    pub shift: bool,
    pub win: bool,
}

impl ModifierState {
    /// Whether the given modifier is held.
    #[must_use]
    pub const fn is_down(&self, m: Modifier) -> bool {
        match m {
            Modifier::Alt => self.alt,
            Modifier::Ctrl => self.ctrl,
            Modifier::Shift => self.shift,
            Modifier::Win => self.win,
        }
    }

    /// Returns a copy with the given modifier set to `down`.
    #[must_use]
    pub const fn with(mut self, m: Modifier, down: bool) -> Self {
        match m {
            Modifier::Alt => self.alt = down,
            Modifier::Ctrl => self.ctrl = down,
            Modifier::Shift => self.shift = down,
            Modifier::Win => self.win = down,
        }
        self
    }
}

/// The kind of gesture a button press starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GestureKind {
    Move,
    Resize,
}

/// The configuration subset needed to classify input in the low-level hook. It is small
/// and `Copy` so the hook can read it without allocations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GestureBindings {
    /// Master switch (window management enabled *and* gestures enabled).
    pub enabled: bool,
    /// Modifier that must be held to start a gesture.
    pub modifier: Modifier,
    /// Button that starts a move gesture.
    pub move_button: Option<MouseButton>,
    /// Button that starts a resize gesture.
    pub resize_button: Option<MouseButton>,
    /// Modifier that enables snapping, if snapping is enabled.
    pub snap_modifier: Option<Modifier>,
}

impl GestureBindings {
    /// Decides whether a button press with the given modifiers starts a gesture.
    ///
    /// A gesture starts only when the activation modifier is held and every other held
    /// modifier is the snap modifier. This prevents accidental activation by unrelated
    /// shortcuts such as `Ctrl+Alt+Shift+click` while still allowing
    /// `Alt+Ctrl+drag` for snapping.
    #[must_use]
    pub fn classify(&self, button: MouseButton, mods: ModifierState) -> Option<GestureKind> {
        if !self.enabled || !mods.is_down(self.modifier) {
            return None;
        }
        let foreign_modifier_held = Modifier::ALL
            .iter()
            .any(|&m| m != self.modifier && Some(m) != self.snap_modifier && mods.is_down(m));
        if foreign_modifier_held {
            return None;
        }
        if self.move_button == Some(button) {
            Some(GestureKind::Move)
        } else if self.resize_button == Some(button) {
            Some(GestureKind::Resize)
        } else {
            None
        }
    }

    /// Whether snapping is active for the given modifier state.
    #[must_use]
    pub fn snap_active(&self, mods: ModifierState) -> bool {
        self.snap_modifier.is_some_and(|m| mods.is_down(m))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bindings() -> GestureBindings {
        GestureBindings {
            enabled: true,
            modifier: Modifier::Alt,
            move_button: Some(MouseButton::Left),
            resize_button: Some(MouseButton::Right),
            snap_modifier: Some(Modifier::Ctrl),
        }
    }

    fn mods(alt: bool, ctrl: bool, shift: bool, win: bool) -> ModifierState {
        ModifierState {
            alt,
            ctrl,
            shift,
            win,
        }
    }

    #[test]
    fn alt_left_moves_and_alt_right_resizes() {
        let b = bindings();
        let alt = mods(true, false, false, false);
        assert_eq!(b.classify(MouseButton::Left, alt), Some(GestureKind::Move));
        assert_eq!(
            b.classify(MouseButton::Right, alt),
            Some(GestureKind::Resize)
        );
        assert_eq!(b.classify(MouseButton::Middle, alt), None);
    }

    #[test]
    fn requires_activation_modifier() {
        let b = bindings();
        assert_eq!(
            b.classify(MouseButton::Left, ModifierState::default()),
            None
        );
        assert_eq!(
            b.classify(MouseButton::Left, mods(false, true, false, false)),
            None
        );
    }

    #[test]
    fn snap_modifier_is_allowed_but_foreign_modifiers_are_not() {
        let b = bindings();
        assert_eq!(
            b.classify(MouseButton::Left, mods(true, true, false, false)),
            Some(GestureKind::Move)
        );
        assert!(b.snap_active(mods(true, true, false, false)));
        assert!(!b.snap_active(mods(true, false, false, false)));
        assert_eq!(
            b.classify(MouseButton::Left, mods(true, false, true, false)),
            None
        );
        assert_eq!(
            b.classify(MouseButton::Left, mods(true, false, false, true)),
            None
        );
    }

    #[test]
    fn disabled_bindings_never_match() {
        let b = GestureBindings {
            enabled: false,
            ..bindings()
        };
        assert_eq!(
            b.classify(MouseButton::Left, mods(true, false, false, false)),
            None
        );
    }

    #[test]
    fn snapping_disabled_makes_ctrl_foreign() {
        let b = GestureBindings {
            snap_modifier: None,
            ..bindings()
        };
        assert_eq!(
            b.classify(MouseButton::Left, mods(true, true, false, false)),
            None
        );
        assert!(!b.snap_active(mods(true, true, false, false)));
    }

    #[test]
    fn modifier_state_with() {
        let m = ModifierState::default().with(Modifier::Win, true);
        assert!(m.win && !m.alt);
        assert!(!m.with(Modifier::Win, false).win);
    }
}
