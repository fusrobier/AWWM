// IPC contract shared with the Rust backend (src-tauri/src/commands.rs and
// crates/awwm-core/src/settings.rs). Field names are the camelCase serde names.
// Keep this file in sync with the Rust types.

export type Modifier = 'alt' | 'ctrl' | 'shift' | 'win';
export type MouseButton = 'left' | 'right' | 'middle';
export type Theme = 'system' | 'light' | 'dark';
export type LogLevel = 'error' | 'warn' | 'info' | 'debug' | 'trace';
export type GestureScope = 'allWindows' | 'pinnedOnly';
export type TitleMatchMode = 'contains' | 'equals' | 'startsWith' | 'endsWith' | 'wildcard';

export interface TitleMatcher {
  mode: TitleMatchMode;
  pattern: string;
  caseSensitive: boolean;
}

export interface Rule {
  /** Stable unique id (the UI generates one with crypto.randomUUID()). */
  id: string;
  name: string;
  enabled: boolean;
  /** Executable file name (`notepad.exe`), full path, or wildcard pattern. */
  exe: string;
  title: TitleMatcher | null;
  /** Pin matching windows (always on top). */
  pin: boolean;
  /** Opacity in percent, 0–100 (100 = opaque). */
  opacity: number;
}

export interface GeneralSettings {
  managementEnabled: boolean;
  startWithWindows: boolean;
  startMinimized: boolean;
  runInBackground: boolean;
  restoreOnExit: boolean;
  recoverSession: boolean;
  /** 0–600 seconds. */
  reattachGraceSecs: number;
  applyRulesOnStartup: boolean;
  theme: Theme;
  logLevel: LogLevel;
}

export interface PinningSettings {
  /** 0–100. */
  defaultOpacity: number;
  showBorder: boolean;
  /** `#RRGGBB`. */
  borderColor: string;
  /** 1–16 logical px. */
  borderThickness: number;
  reassertState: boolean;
}

export interface GestureSettings {
  enabled: boolean;
  modifier: Modifier;
  moveButton: MouseButton | null;
  resizeButton: MouseButton | null;
  scope: GestureScope;
  restoreMaximizedOnMove: boolean;
  ignoreFullscreen: boolean;
  showResizeOverlay: boolean;
  /** `#RRGGBB`. */
  overlayColor: string;
  /** Executable names / patterns. */
  excludedApps: string[];
}

export interface SnapSettings {
  enabled: boolean;
  /** Must differ from GestureSettings.modifier. */
  modifier: Modifier;
  /** 2–100 logical px. */
  distance: number;
  monitorEdges: boolean;
  workAreaEdges: boolean;
  windowEdges: boolean;
  centerLines: boolean;
  gridEnabled: boolean;
  /** 2–24. */
  gridColumns: number;
  /** 2–24. */
  gridRows: number;
  showGuides: boolean;
}

export interface Hotkey {
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
  win: boolean;
  /** One of the names returned by getKeyNames(). */
  key: string;
}

export interface HotkeySettings {
  togglePin: Hotkey | null;
  opacityUp: Hotkey | null;
  opacityDown: Hotkey | null;
  /** 1–50 percent. */
  opacityStep: number;
}

export interface Settings {
  schemaVersion: number;
  general: GeneralSettings;
  pinning: PinningSettings;
  gestures: GestureSettings;
  snapping: SnapSettings;
  hotkeys: HotkeySettings;
  rules: Rule[];
}

export type ManagedSource = 'manual' | 'rule' | 'recovered';

/** A window AWWM currently manages (pinned and/or transparent). */
export interface ManagedWindow {
  /** Window handle as a number (always fits in 32 bits). */
  hwnd: number;
  title: string;
  exeName: string;
  exePath: string;
  pid: number;
  pinned: boolean;
  /**
   * Pinned, but Windows has not allowed it to become topmost yet (a full-screen
   * application has the foreground). AWWM retries automatically.
   */
  pinPending: boolean;
  /** 0–100. */
  opacity: number;
  source: ManagedSource;
  /** Set when source === 'rule'. */
  ruleId: string | null;
}

/** A top-level window that can be managed (window picker). */
export interface WindowInfo {
  hwnd: number;
  title: string;
  className: string;
  exeName: string;
  exePath: string;
  pid: number;
  /** Window is already topmost (by itself or through AWWM). */
  topmost: boolean;
  /** AWWM already manages this window. */
  managed: boolean;
  /**
   * False when the window belongs to a process with a higher integrity level
   * (e.g. an elevated app) that AWWM cannot modify without running elevated.
   */
  accessible: boolean;
}

export type HotkeyAction = 'togglePin' | 'opacityUp' | 'opacityDown';

export interface HotkeyStatus {
  action: HotkeyAction;
  /** e.g. "Win+Alt+T" */
  label: string;
  registered: boolean;
  /** Set when registration failed (usually: already used by another application). */
  error: string | null;
}

export interface AppSnapshot {
  managementEnabled: boolean;
  managed: ManagedWindow[];
  settings: Settings;
  hotkeys: HotkeyStatus[];
  /** Non-null when settings could not be loaded and defaults were used. */
  settingsWarning: string | null;
}

export interface AppInfo {
  name: string;
  version: string;
  configDir: string;
  logDir: string;
  settingsPath: string;
  license: string;
}

export type NoticeLevel = 'info' | 'warning' | 'error';

export interface Notice {
  level: NoticeLevel;
  message: string;
  hwnd: number | null;
}

export type ErrorCode =
  | 'windowNotFound'
  | 'accessDenied'
  | 'unsupported'
  | 'notResponding'
  | 'invalidArgument'
  | 'io'
  | 'engine';

/** Shape of every rejected command promise. */
export interface CommandError {
  code: ErrorCode;
  message: string;
}

export type Page = 'dashboard' | 'rules' | 'mouse' | 'general' | 'about';

export type DocumentKind = 'userGuide' | 'limitations' | 'architecture' | 'readme' | 'license';
export type LocationKind = 'config' | 'logs';
