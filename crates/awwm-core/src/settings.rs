//! User settings: model, defaults, validation and migration.
//!
//! The serialized form is camelCase JSON shared with the TypeScript frontend
//! (`src/lib/types.ts`). Every struct uses `#[serde(default)]` so files written by older
//! versions load with defaults for new fields, and [`Settings::sanitize`] clamps every value
//! into its valid range so a hand-edited or corrupt file can never put the engine into an
//! invalid state.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::gesture::{GestureBindings, Modifier, MouseButton};
use crate::rules::{Rule, TitleMatchMode, TitleMatcher, normalize_exe_pattern};

/// Current settings schema version.
pub const SCHEMA_VERSION: u32 = 1;

/// Valid range of the snap distance (logical pixels).
pub const SNAP_DISTANCE_RANGE: (u32, u32) = (2, 100);
/// Valid range of the pin border thickness (logical pixels).
pub const BORDER_THICKNESS_RANGE: (u32, u32) = (1, 16);
/// Valid range of grid rows/columns.
pub const GRID_DIVISIONS_RANGE: (u32, u32) = (2, 24);
/// Valid range of the visible pixel grid spacing (logical pixels).
pub const PIXEL_GRID_SIZE_RANGE: (u32, u32) = (4, 200);
/// Maximum handle-change reattach grace period in seconds.
pub const MAX_REATTACH_GRACE_SECS: u32 = 600;
/// Maximum number of rules (protects the engine from pathological configurations).
pub const MAX_RULES: usize = 500;
/// Maximum number of excluded applications.
pub const MAX_EXCLUSIONS: usize = 500;
/// Maximum length of user-provided strings (names, patterns).
pub const MAX_TEXT_LEN: usize = 1024;

/// UI colour theme.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

/// Log verbosity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LogLevel {
    Error,
    Warn,
    #[default]
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    /// The `tracing` filter directive for this level.
    #[must_use]
    pub const fn as_filter(self) -> &'static str {
        match self {
            LogLevel::Error => "error",
            LogLevel::Warn => "warn",
            LogLevel::Info => "info",
            LogLevel::Debug => "debug",
            LogLevel::Trace => "trace",
        }
    }
}

/// Which windows mouse gestures apply to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GestureScope {
    #[default]
    AllWindows,
    PinnedOnly,
}

/// Application behaviour.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GeneralSettings {
    /// Master switch for pinning frames, rules, gestures and hotkeys.
    pub management_enabled: bool,
    /// Register AWWM in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
    pub start_with_windows: bool,
    /// Start without opening the settings window (tray only).
    pub start_minimized: bool,
    /// Keep running in the tray when the settings window is closed.
    pub run_in_background: bool,
    /// Hide the settings window in the tray (instead of the taskbar) when it is minimized.
    /// Files written before this option existed get the default (`true`).
    pub minimize_to_tray: bool,
    /// Release all managed windows (restore topmost/opacity) when AWWM exits.
    pub restore_on_exit: bool,
    /// Re-adopt windows that were managed when AWWM last stopped (crash recovery).
    pub recover_session: bool,
    /// Seconds a destroyed managed window's settings are kept for a replacement window
    /// of the same application (handle changes and quick restarts). 0 disables.
    pub reattach_grace_secs: u32,
    /// Apply rules to windows that already exist when AWWM starts.
    pub apply_rules_on_startup: bool,
    pub theme: Theme,
    pub log_level: LogLevel,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            management_enabled: true,
            start_with_windows: false,
            start_minimized: false,
            run_in_background: true,
            minimize_to_tray: true,
            restore_on_exit: true,
            recover_session: true,
            reattach_grace_secs: 30,
            apply_rules_on_startup: true,
            theme: Theme::System,
            log_level: LogLevel::Info,
        }
    }
}

/// Always-on-top behaviour.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PinningSettings {
    /// Opacity (percent) applied when a window is pinned manually.
    pub default_opacity: u8,
    /// Draw a coloured frame around pinned windows.
    pub show_border: bool,
    /// Frame colour, `#RRGGBB`.
    pub border_color: String,
    /// Frame thickness in logical pixels.
    pub border_thickness: u32,
    /// Re-apply topmost/opacity when an application resets them.
    pub reassert_state: bool,
}

impl Default for PinningSettings {
    fn default() -> Self {
        Self {
            default_opacity: 100,
            show_border: true,
            border_color: "#F59E0B".to_owned(),
            border_thickness: 2,
            reassert_state: true,
        }
    }
}

/// Mouse move/resize gestures.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GestureSettings {
    pub enabled: bool,
    /// Modifier that must be held to start a gesture.
    pub modifier: Modifier,
    /// Button for moving; `None` disables move gestures.
    pub move_button: Option<MouseButton>,
    /// Button for resizing; `None` disables resize gestures.
    pub resize_button: Option<MouseButton>,
    pub scope: GestureScope,
    /// Restore maximized windows when a move gesture starts on them.
    pub restore_maximized_on_move: bool,
    /// Ignore windows that cover an entire monitor without a frame (games, video).
    pub ignore_fullscreen: bool,
    /// Show the outline and live size label while resizing.
    pub show_resize_overlay: bool,
    /// Outline/guide colour, `#RRGGBB`.
    pub overlay_color: String,
    /// Executable names (or wildcard patterns) for which gestures are disabled.
    pub excluded_apps: Vec<String>,
}

impl Default for GestureSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            modifier: Modifier::Alt,
            move_button: Some(MouseButton::Left),
            resize_button: Some(MouseButton::Right),
            scope: GestureScope::AllWindows,
            restore_maximized_on_move: true,
            ignore_fullscreen: true,
            show_resize_overlay: true,
            overlay_color: "#3B82F6".to_owned(),
            excluded_apps: Vec::new(),
        }
    }
}

/// Snapping during gestures.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SnapSettings {
    pub enabled: bool,
    /// Modifier that activates snapping while a gesture is active.
    pub modifier: Modifier,
    /// Snap distance in logical pixels.
    pub distance: u32,
    pub monitor_edges: bool,
    pub work_area_edges: bool,
    pub window_edges: bool,
    /// Snap window centres to work-area centre lines.
    pub center_lines: bool,
    pub grid_enabled: bool,
    pub grid_columns: u32,
    pub grid_rows: u32,
    pub show_guides: bool,
}

impl Default for SnapSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            modifier: Modifier::Ctrl,
            distance: 16,
            monitor_edges: true,
            work_area_edges: true,
            window_edges: true,
            center_lines: true,
            grid_enabled: false,
            grid_columns: 4,
            grid_rows: 4,
            show_guides: true,
        }
    }
}

/// Pixel orientation and the visible pixel grid during gestures.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AlignmentSettings {
    /// While moving: show the window position in pixels and mark edges that line up
    /// exactly with snap candidates. Never changes where the window goes.
    pub pixel_orientation: bool,
    /// Draw a pixel grid on every monitor while moving or resizing.
    pub show_grid: bool,
    /// Grid spacing in logical pixels.
    pub grid_size: u32,
    /// While snapping is active and the grid is shown, edges also snap to grid lines.
    pub snap_to_grid: bool,
}

impl Default for AlignmentSettings {
    fn default() -> Self {
        Self {
            pixel_orientation: true,
            show_grid: false,
            grid_size: 20,
            snap_to_grid: true,
        }
    }
}

/// A global keyboard shortcut.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Hotkey {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    /// Key name from [`KEY_NAMES`], e.g. `"T"`, `"F9"`, `"PageUp"`.
    pub key: String,
}

impl Hotkey {
    fn new(ctrl: bool, alt: bool, shift: bool, win: bool, key: &str) -> Self {
        Self {
            ctrl,
            alt,
            shift,
            win,
            key: key.to_owned(),
        }
    }

    /// Virtual-key code of the key, if the name is known.
    #[must_use]
    pub fn virtual_key(&self) -> Option<u16> {
        key_name_to_vk(&self.key)
    }

    /// A hotkey is valid when the key is known and Ctrl, Alt or Win is part of it. Shift
    /// alone is not enough: a global Shift+letter shortcut would swallow typing of that
    /// capital letter in every application.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.virtual_key().is_some() && (self.ctrl || self.alt || self.win)
    }

    /// Human-readable label such as `Win+Alt+T`.
    #[must_use]
    pub fn label(&self) -> String {
        let mut parts = Vec::with_capacity(5);
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.win {
            parts.push("Win");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.shift {
            parts.push("Shift");
        }
        parts.push(&self.key);
        parts.join("+")
    }
}

/// Global keyboard shortcuts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HotkeySettings {
    /// Pin/unpin the foreground window.
    pub toggle_pin: Option<Hotkey>,
    /// Increase the opacity of the foreground window (a non-pinned window that reaches
    /// 100 % is released).
    pub opacity_up: Option<Hotkey>,
    /// Decrease the opacity of the foreground window (an unmanaged window becomes managed
    /// as transparent-only, without pinning it).
    pub opacity_down: Option<Hotkey>,
    /// Opacity change per hotkey press, in percent.
    pub opacity_step: u8,
}

impl Default for HotkeySettings {
    fn default() -> Self {
        Self {
            toggle_pin: Some(Hotkey::new(false, true, false, true, "P")),
            opacity_up: Some(Hotkey::new(false, true, false, true, "PageUp")),
            opacity_down: Some(Hotkey::new(false, true, false, true, "PageDown")),
            opacity_step: 10,
        }
    }
}

/// All persisted settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub schema_version: u32,
    pub general: GeneralSettings,
    pub pinning: PinningSettings,
    pub gestures: GestureSettings,
    pub snapping: SnapSettings,
    pub alignment: AlignmentSettings,
    pub hotkeys: HotkeySettings,
    pub rules: Vec<Rule>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            general: GeneralSettings::default(),
            pinning: PinningSettings::default(),
            gestures: GestureSettings::default(),
            snapping: SnapSettings::default(),
            alignment: AlignmentSettings::default(),
            hotkeys: HotkeySettings::default(),
            rules: default_rules(),
        }
    }
}

/// The example rules shipped with a fresh installation. They are disabled so a new
/// installation never changes windows until the user opts in.
#[must_use]
pub fn default_rules() -> Vec<Rule> {
    let example = |id: &str, name: &str, exe: &str, opacity: u8| Rule {
        id: id.to_owned(),
        name: name.to_owned(),
        enabled: false,
        exe: exe.to_owned(),
        title: None,
        pin: true,
        opacity,
    };
    vec![
        example("example-notepad", "Notepad", "notepad.exe", 85),
        example("example-calculator", "Calculator", "CalculatorApp.exe", 70),
        example("example-discord", "Discord", "Discord.exe", 90),
    ]
}

impl Settings {
    /// Returns a sanitized copy: values clamped, invalid colours/hotkeys reset, conflicting
    /// bindings resolved, rule ids made unique, strings trimmed and length-limited.
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        self.sanitize();
        self
    }

    /// In-place variant of [`Settings::sanitized`].
    pub fn sanitize(&mut self) {
        let defaults = Settings::default();
        self.schema_version = SCHEMA_VERSION;

        let g = &mut self.general;
        g.reattach_grace_secs = g.reattach_grace_secs.min(MAX_REATTACH_GRACE_SECS);

        let p = &mut self.pinning;
        p.default_opacity = p.default_opacity.min(100);
        p.border_thickness = p
            .border_thickness
            .clamp(BORDER_THICKNESS_RANGE.0, BORDER_THICKNESS_RANGE.1);
        if parse_hex_color(&p.border_color).is_none() {
            p.border_color = defaults.pinning.border_color.clone();
        }
        p.border_color = p.border_color.to_uppercase();

        let ge = &mut self.gestures;
        if parse_hex_color(&ge.overlay_color).is_none() {
            ge.overlay_color = defaults.gestures.overlay_color.clone();
        }
        ge.overlay_color = ge.overlay_color.to_uppercase();
        if ge.move_button.is_some() && ge.move_button == ge.resize_button {
            // Same button for both gestures is ambiguous: fall back to distinct defaults.
            ge.move_button = Some(MouseButton::Left);
            ge.resize_button = Some(MouseButton::Right);
        }
        ge.excluded_apps = sanitize_string_list(&ge.excluded_apps, MAX_EXCLUSIONS);

        let s = &mut self.snapping;
        s.distance = s
            .distance
            .clamp(SNAP_DISTANCE_RANGE.0, SNAP_DISTANCE_RANGE.1);
        s.grid_columns = s
            .grid_columns
            .clamp(GRID_DIVISIONS_RANGE.0, GRID_DIVISIONS_RANGE.1);
        s.grid_rows = s
            .grid_rows
            .clamp(GRID_DIVISIONS_RANGE.0, GRID_DIVISIONS_RANGE.1);
        if s.modifier == self.gestures.modifier {
            // The snap modifier must differ from the activation modifier.
            s.modifier = Modifier::ALL
                .into_iter()
                .find(|&m| m != self.gestures.modifier && m != Modifier::Win)
                .unwrap_or(Modifier::Ctrl);
        }

        let a = &mut self.alignment;
        a.grid_size = a
            .grid_size
            .clamp(PIXEL_GRID_SIZE_RANGE.0, PIXEL_GRID_SIZE_RANGE.1);

        let h = &mut self.hotkeys;
        for hk in [&mut h.toggle_pin, &mut h.opacity_up, &mut h.opacity_down] {
            if hk.as_ref().is_some_and(|k| !k.is_valid()) {
                *hk = None;
            }
        }
        h.opacity_step = h.opacity_step.clamp(1, 50);

        self.rules.truncate(MAX_RULES);
        let mut seen = HashSet::new();
        for (index, rule) in self.rules.iter_mut().enumerate() {
            sanitize_rule(rule, index);
            while !seen.insert(rule.id.clone()) {
                rule.id = format!("{}-{index}", rule.id);
            }
        }
    }

    /// The gesture bindings derived from these settings.
    #[must_use]
    pub fn gesture_bindings(&self) -> GestureBindings {
        GestureBindings {
            enabled: self.general.management_enabled && self.gestures.enabled,
            modifier: self.gestures.modifier,
            move_button: self.gestures.move_button,
            resize_button: self.gestures.resize_button,
            snap_modifier: self.snapping.enabled.then_some(self.snapping.modifier),
        }
    }
}

fn sanitize_rule(rule: &mut Rule, index: usize) {
    rule.id = truncate(rule.id.trim(), 128);
    if rule.id.is_empty() {
        rule.id = format!("rule-{index}");
    }
    rule.name = truncate(rule.name.trim(), MAX_TEXT_LEN);
    rule.exe = truncate(&normalize_exe_pattern(&rule.exe), MAX_TEXT_LEN);
    if rule.name.is_empty() {
        rule.name = if rule.exe.is_empty() {
            format!("Rule {}", index + 1)
        } else {
            rule.exe.clone()
        };
    }
    rule.opacity = rule.opacity.min(100);
    if let Some(title) = &mut rule.title {
        title.pattern = truncate(&title.pattern, MAX_TEXT_LEN);
        if title.pattern.is_empty() {
            rule.title = None;
        }
    }
    if rule.exe.is_empty() {
        // A rule without an executable can never match; keep it but disabled so the user
        // can fix it in the UI.
        rule.enabled = false;
    }
}

fn sanitize_string_list(items: &[String], max: usize) -> Vec<String> {
    let mut seen = HashSet::new();
    items
        .iter()
        .map(|s| truncate(&normalize_exe_pattern(s), MAX_TEXT_LEN))
        .filter(|s| !s.is_empty() && seen.insert(s.to_lowercase()))
        .take(max)
        .collect()
}

fn truncate(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

/// Parses `#RRGGBB` into `(r, g, b)`.
#[must_use]
pub fn parse_hex_color(s: &str) -> Option<(u8, u8, u8)> {
    let hex = s.strip_prefix('#')?;
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some((channel(0)?, channel(2)?, channel(4)?))
}

/// Converts an opacity percentage (0–100) to a Win32 alpha byte (0–255).
#[must_use]
pub fn opacity_to_alpha(percent: u8) -> u8 {
    let p = u32::from(percent.min(100));
    // (p * 255 + 50) / 100 rounds to nearest and is at most 255.
    u8::try_from((p * 255 + 50) / 100).unwrap_or(u8::MAX)
}

/// Converts a Win32 alpha byte to the nearest opacity percentage.
#[must_use]
pub fn alpha_to_opacity(alpha: u8) -> u8 {
    let a = u32::from(alpha);
    u8::try_from((a * 100 + 127) / 255).unwrap_or(100)
}

/// Supported hotkey key names (for the UI picker and validation).
pub const KEY_NAMES: &[&str] = &[
    "A",
    "B",
    "C",
    "D",
    "E",
    "F",
    "G",
    "H",
    "I",
    "J",
    "K",
    "L",
    "M",
    "N",
    "O",
    "P",
    "Q",
    "R",
    "S",
    "T",
    "U",
    "V",
    "W",
    "X",
    "Y",
    "Z",
    "0",
    "1",
    "2",
    "3",
    "4",
    "5",
    "6",
    "7",
    "8",
    "9",
    "F1",
    "F2",
    "F3",
    "F4",
    "F5",
    "F6",
    "F7",
    "F8",
    "F9",
    "F10",
    "F11",
    "F12",
    "F13",
    "F14",
    "F15",
    "F16",
    "F17",
    "F18",
    "F19",
    "F20",
    "F21",
    "F22",
    "F23",
    "F24",
    "Space",
    "Enter",
    "Tab",
    "Backspace",
    "Insert",
    "Delete",
    "Home",
    "End",
    "PageUp",
    "PageDown",
    "Up",
    "Down",
    "Left",
    "Right",
    "Plus",
    "Minus",
    "Comma",
    "Period",
    "Numpad0",
    "Numpad1",
    "Numpad2",
    "Numpad3",
    "Numpad4",
    "Numpad5",
    "Numpad6",
    "Numpad7",
    "Numpad8",
    "Numpad9",
    "NumpadAdd",
    "NumpadSubtract",
    "NumpadMultiply",
    "NumpadDivide",
    "Pause",
    "ScrollLock",
];

/// Maps a key name from [`KEY_NAMES`] (case-insensitive) to its Windows virtual-key code.
#[must_use]
pub fn key_name_to_vk(name: &str) -> Option<u16> {
    let upper = name.trim().to_ascii_uppercase();
    let bytes = upper.as_bytes();
    if bytes.len() == 1 && (bytes[0].is_ascii_uppercase() || bytes[0].is_ascii_digit()) {
        return Some(u16::from(bytes[0]));
    }
    if let Some(n) = upper.strip_prefix('F').and_then(|n| n.parse::<u16>().ok()) {
        return (1..=24).contains(&n).then_some(0x70 + n - 1);
    }
    if let Some(n) = upper
        .strip_prefix("NUMPAD")
        .and_then(|n| n.parse::<u16>().ok())
    {
        return (n <= 9).then_some(0x60 + n);
    }
    let vk = match upper.as_str() {
        "SPACE" => 0x20,
        "ENTER" => 0x0D,
        "TAB" => 0x09,
        "BACKSPACE" => 0x08,
        "INSERT" => 0x2D,
        "DELETE" => 0x2E,
        "HOME" => 0x24,
        "END" => 0x23,
        "PAGEUP" => 0x21,
        "PAGEDOWN" => 0x22,
        "UP" => 0x26,
        "DOWN" => 0x28,
        "LEFT" => 0x25,
        "RIGHT" => 0x27,
        "PLUS" => 0xBB,
        "MINUS" => 0xBD,
        "COMMA" => 0xBC,
        "PERIOD" => 0xBE,
        "NUMPADADD" => 0x6B,
        "NUMPADSUBTRACT" => 0x6D,
        "NUMPADMULTIPLY" => 0x6A,
        "NUMPADDIVIDE" => 0x6F,
        "PAUSE" => 0x13,
        "SCROLLLOCK" => 0x91,
        _ => return None,
    };
    Some(vk)
}

/// Convenience constructor for a title matcher (used by tests and the UI defaults).
#[must_use]
pub fn title_contains(pattern: &str) -> TitleMatcher {
    TitleMatcher {
        mode: TitleMatchMode::Contains,
        pattern: pattern.to_owned(),
        case_sensitive: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_already_sanitized() {
        let d = Settings::default();
        assert_eq!(d.clone().sanitized(), d);
        assert_eq!(d.rules.len(), 3);
        assert!(d.rules.iter().all(|r| !r.enabled));
    }

    #[test]
    fn json_round_trip_uses_camel_case() {
        let json = serde_json::to_string(&Settings::default()).expect("serialize");
        assert!(json.contains("\"managementEnabled\":true"));
        assert!(json.contains("\"moveButton\":\"left\""));
        let back: Settings = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, Settings::default());
    }

    #[test]
    fn missing_and_unknown_fields_are_tolerated() {
        let s: Settings = serde_json::from_str(
            r#"{"general":{"startWithWindows":true},"futureField":42,"snapping":{"distance":20}}"#,
        )
        .expect("deserialize");
        assert!(s.general.start_with_windows);
        assert!(s.general.management_enabled);
        assert_eq!(s.snapping.distance, 20);
        assert!(s.snapping.window_edges);
    }

    #[test]
    fn files_from_before_the_new_options_get_their_defaults_and_keep_stored_values() {
        // Shape of a settings file written by an earlier build: no `minimizeToTray`, no
        // `alignment` section.
        let s: Settings = serde_json::from_str(
            r#"{"schemaVersion":1,"general":{"runInBackground":false,"startMinimized":true},
                "snapping":{"gridEnabled":true}}"#,
        )
        .expect("deserialize");
        assert!(s.general.minimize_to_tray, "new option defaults to on");
        assert!(!s.general.run_in_background, "stored value kept");
        assert!(s.general.start_minimized, "stored value kept");
        assert_eq!(s.alignment, AlignmentSettings::default());
        assert!(s.alignment.pixel_orientation);
        assert!(!s.alignment.show_grid);
        assert_eq!(s.alignment.grid_size, 20);
        assert!(s.snapping.grid_enabled);
    }

    #[test]
    fn a_stored_minimize_to_tray_choice_is_never_replaced_by_the_default() {
        let s: Settings =
            serde_json::from_str(r#"{"general":{"minimizeToTray":false}}"#).expect("deserialize");
        assert!(!s.general.minimize_to_tray);
        let json = serde_json::to_string(&s.sanitized()).expect("serialize");
        assert!(json.contains("\"minimizeToTray\":false"));
        assert!(json.contains("\"alignment\":{\"pixelOrientation\":true,\"showGrid\":false,\"gridSize\":20,\"snapToGrid\":true}"));
    }

    #[test]
    fn pixel_grid_size_is_clamped() {
        for (raw, expected) in [(0, 4), (3, 4), (4, 4), (37, 37), (200, 200), (5000, 200)] {
            let mut s = Settings::default();
            s.alignment.grid_size = raw;
            assert_eq!(s.sanitized().alignment.grid_size, expected, "{raw}");
        }
    }

    #[test]
    fn sanitize_clamps_and_repairs() {
        let mut s = Settings::default();
        s.snapping.distance = 1000;
        s.snapping.grid_columns = 0;
        s.pinning.border_thickness = 0;
        s.pinning.border_color = "red".into();
        s.pinning.default_opacity = 250;
        s.general.reattach_grace_secs = 99_999;
        s.gestures.overlay_color = "#12345G".into();
        s.hotkeys.opacity_step = 0;
        let s = s.sanitized();
        assert_eq!(s.snapping.distance, SNAP_DISTANCE_RANGE.1);
        assert_eq!(s.snapping.grid_columns, GRID_DIVISIONS_RANGE.0);
        assert_eq!(s.pinning.border_thickness, 1);
        assert_eq!(
            s.pinning.border_color,
            PinningSettings::default().border_color
        );
        assert_eq!(s.pinning.default_opacity, 100);
        assert_eq!(s.general.reattach_grace_secs, MAX_REATTACH_GRACE_SECS);
        assert_eq!(
            s.gestures.overlay_color,
            GestureSettings::default().overlay_color
        );
        assert_eq!(s.hotkeys.opacity_step, 1);
    }

    #[test]
    fn sanitize_resolves_conflicting_bindings() {
        let mut s = Settings::default();
        s.snapping.modifier = Modifier::Alt;
        s.gestures.move_button = Some(MouseButton::Middle);
        s.gestures.resize_button = Some(MouseButton::Middle);
        let s = s.sanitized();
        assert_ne!(s.snapping.modifier, s.gestures.modifier);
        assert_ne!(s.gestures.move_button, s.gestures.resize_button);
    }

    #[test]
    fn sanitize_rules() {
        let s = Settings {
            rules: vec![
                Rule {
                    id: "a".into(),
                    exe: "  \"notepad.exe\" ".into(),
                    ..Rule::default()
                },
                Rule {
                    id: "a".into(),
                    exe: "calc.exe".into(),
                    opacity: 140,
                    ..Rule::default()
                },
                Rule {
                    id: String::new(),
                    exe: String::new(),
                    ..Rule::default()
                },
                Rule {
                    id: "t".into(),
                    exe: "x.exe".into(),
                    title: Some(TitleMatcher {
                        pattern: String::new(),
                        ..TitleMatcher::default()
                    }),
                    ..Rule::default()
                },
            ],
            ..Settings::default()
        };
        let s = s.sanitized();
        assert_eq!(s.rules[0].exe, "notepad.exe");
        assert_eq!(s.rules[0].name, "notepad.exe");
        assert_ne!(s.rules[0].id, s.rules[1].id);
        assert_eq!(s.rules[1].opacity, 100);
        assert!(!s.rules[2].id.is_empty());
        assert!(!s.rules[2].enabled);
        assert!(s.rules[3].title.is_none());
    }

    #[test]
    fn excluded_apps_are_trimmed_and_deduplicated() {
        let mut s = Settings::default();
        s.gestures.excluded_apps = vec![
            " Code.exe ".into(),
            "code.exe".into(),
            String::new(),
            "blender.exe".into(),
        ];
        let s = s.sanitized();
        assert_eq!(
            s.gestures.excluded_apps,
            vec!["Code.exe".to_owned(), "blender.exe".to_owned()]
        );
    }

    #[test]
    fn invalid_hotkeys_are_removed() {
        let mut s = Settings::default();
        s.hotkeys.toggle_pin = Some(Hotkey {
            key: "T".into(),
            ..Hotkey::default()
        });
        s.hotkeys.opacity_up = Some(Hotkey {
            win: true,
            key: "NoSuchKey".into(),
            ..Hotkey::default()
        });
        let s = s.sanitized();
        assert!(s.hotkeys.toggle_pin.is_none());
        assert!(s.hotkeys.opacity_up.is_none());
        assert!(s.hotkeys.opacity_down.is_some());
    }

    #[test]
    fn hex_colors() {
        assert_eq!(parse_hex_color("#FF8000"), Some((255, 128, 0)));
        assert_eq!(parse_hex_color("#ff8000"), Some((255, 128, 0)));
        assert_eq!(parse_hex_color("FF8000"), None);
        assert_eq!(parse_hex_color("#FF800"), None);
        assert_eq!(parse_hex_color("#GG8000"), None);
    }

    #[test]
    fn opacity_alpha_conversion() {
        assert_eq!(opacity_to_alpha(0), 0);
        assert_eq!(opacity_to_alpha(100), 255);
        assert_eq!(opacity_to_alpha(50), 128);
        assert_eq!(opacity_to_alpha(200), 255);
        for p in 0..=100u8 {
            assert_eq!(alpha_to_opacity(opacity_to_alpha(p)), p);
        }
    }

    #[test]
    fn key_names_map_to_virtual_keys() {
        for name in KEY_NAMES {
            assert!(key_name_to_vk(name).is_some(), "{name}");
        }
        assert_eq!(key_name_to_vk("t"), Some(0x54));
        assert_eq!(key_name_to_vk("F1"), Some(0x70));
        assert_eq!(key_name_to_vk("F24"), Some(0x87));
        assert_eq!(key_name_to_vk("F25"), None);
        assert_eq!(key_name_to_vk("PageUp"), Some(0x21));
        assert_eq!(key_name_to_vk("Numpad7"), Some(0x67));
        assert_eq!(key_name_to_vk(""), None);
    }

    #[test]
    fn hotkey_label_and_validity() {
        let hk = Hotkey {
            ctrl: true,
            alt: true,
            shift: false,
            win: true,
            key: "T".into(),
        };
        assert_eq!(hk.label(), "Ctrl+Win+Alt+T");
        assert!(hk.is_valid());
        assert!(
            !Hotkey {
                key: "T".into(),
                ..Hotkey::default()
            }
            .is_valid()
        );
        assert!(
            !Hotkey {
                shift: true,
                key: "T".into(),
                ..Hotkey::default()
            }
            .is_valid()
        );
        assert!(
            Hotkey {
                shift: true,
                alt: true,
                key: "T".into(),
                ..Hotkey::default()
            }
            .is_valid()
        );
    }

    #[test]
    fn gesture_bindings_follow_master_switch() {
        let mut s = Settings::default();
        assert!(s.gesture_bindings().enabled);
        s.general.management_enabled = false;
        assert!(!s.gesture_bindings().enabled);
        s.general.management_enabled = true;
        s.snapping.enabled = false;
        assert_eq!(s.gesture_bindings().snap_modifier, None);
    }

    #[test]
    fn title_contains_helper() {
        assert!(title_contains("abc").matches("xxABCxx"));
    }
}
