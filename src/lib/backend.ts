// Backend availability and error presentation.
import { api, toCommandError } from './api';
import { cacheOnSuccess } from './cache';
import { createIconCache } from './iconCache';

export const BACKEND_UNAVAILABLE_TITLE = 'Backend unavailable — run inside AWWM';
export const BACKEND_UNAVAILABLE_MESSAGE =
  'This settings interface talks to the AWWM engine and only works inside the AWWM application window. ' +
  'Start AWWM and open its settings from the tray icon.';

/** True when the page runs inside the Tauri WebView (the IPC bridge is present). */
export function isBackendAvailable(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/** The user-facing message of any command rejection. */
export function errorMessage(error: unknown): string {
  return toCommandError(error).message;
}

/** Key names never change while the app runs; fetched once. */
export const loadKeyNames = cacheOnSuccess(() => api.getKeyNames());

/** Version and paths never change while the app runs; fetched once. */
export const loadAppInfo = cacheOnSuccess(() => api.getAppInfo());

/** Shared icon cache for all window lists. */
export const windowIcons = createIconCache(
  (exePath) => api.getWindowIcon(exePath),
  (exePath, error) => {
    // Icons are decorative: the generic window glyph is shown instead.
    console.warn(`AWWM: no icon for ${exePath}: ${errorMessage(error)}`);
  },
);
