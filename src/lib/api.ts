// Typed wrappers around the Tauri IPC commands and events exposed by src-tauri.
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  AppInfo,
  AppSnapshot,
  CommandError,
  DocumentKind,
  LocationKind,
  ManagedWindow,
  Notice,
  Page,
  Settings,
  WindowInfo,
} from './types';

export const api = {
  getSnapshot: () => invoke<AppSnapshot>('get_snapshot'),
  listWindows: () => invoke<WindowInfo[]>('list_windows'),
  pinWindow: (hwnd: number) => invoke<ManagedWindow>('pin_window', { hwnd }),
  unpinWindow: (hwnd: number) => invoke<void>('unpin_window', { hwnd }),
  setWindowOpacity: (hwnd: number, opacity: number) =>
    invoke<ManagedWindow>('set_window_opacity', { hwnd, opacity }),
  unpinAll: () => invoke<void>('unpin_all'),
  activateWindow: (hwnd: number) => invoke<void>('activate_window', { hwnd }),
  setManagementEnabled: (enabled: boolean) => invoke<void>('set_management_enabled', { enabled }),
  /** Validates, persists and applies settings. Resolves with the sanitized result. */
  saveSettings: (settings: Settings) => invoke<AppSnapshot>('save_settings', { settings }),
  resetSettings: () => invoke<AppSnapshot>('reset_settings'),
  /** Applies enabled rules to all current windows; resolves with the number of windows changed. */
  applyRulesNow: () => invoke<number>('apply_rules_now'),
  /** PNG data URL of the executable's icon, or null when unavailable. */
  getWindowIcon: (exePath: string) => invoke<string | null>('get_window_icon', { exePath }),
  getAppInfo: () => invoke<AppInfo>('get_app_info'),
  getKeyNames: () => invoke<string[]>('get_key_names'),
  openLocation: (target: LocationKind) => invoke<void>('open_location', { target }),
  openDocumentation: (doc: DocumentKind) => invoke<void>('open_documentation', { doc }),
};

export const events = {
  /** Fired whenever the set of managed windows or their state changes. */
  onManagedChanged: (cb: (managed: ManagedWindow[]) => void): Promise<UnlistenFn> =>
    listen<ManagedWindow[]>('awwm://managed-changed', (e) => cb(e.payload)),
  /** Informational / warning / error messages from the engine (e.g. elevated windows). */
  onNotice: (cb: (notice: Notice) => void): Promise<UnlistenFn> =>
    listen<Notice>('awwm://notice', (e) => cb(e.payload)),
  /** Fired when state changes outside the UI (tray toggle, hotkey, settings reload). */
  onSnapshot: (cb: (snapshot: AppSnapshot) => void): Promise<UnlistenFn> =>
    listen<AppSnapshot>('awwm://snapshot', (e) => cb(e.payload)),
  /** Tray menu requests a specific page. */
  onNavigate: (cb: (page: Page) => void): Promise<UnlistenFn> =>
    listen<Page>('awwm://navigate', (e) => cb(e.payload)),
};

/** Normalises any rejection from invoke() into a CommandError. */
export function toCommandError(err: unknown): CommandError {
  if (err && typeof err === 'object' && 'code' in err && 'message' in err) {
    const e = err as { code: unknown; message: unknown };
    if (typeof e.code === 'string' && typeof e.message === 'string') {
      return { code: e.code as CommandError['code'], message: e.message };
    }
  }
  return { code: 'engine', message: typeof err === 'string' ? err : String(err) };
}
