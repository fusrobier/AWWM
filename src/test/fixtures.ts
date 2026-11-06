// Test-only builders. Values mirror the backend defaults in crates/awwm-core/src/settings.rs.
// Not imported by application code.
import type { ManagedWindow, Rule, Settings } from '../lib/types';

export function makeSettings(): Settings {
  return {
    schemaVersion: 1,
    general: {
      managementEnabled: true,
      startWithWindows: false,
      startMinimized: false,
      runInBackground: true,
      minimizeToTray: true,
      restoreOnExit: true,
      recoverSession: true,
      reattachGraceSecs: 30,
      applyRulesOnStartup: true,
      theme: 'system',
      logLevel: 'info',
    },
    pinning: {
      defaultOpacity: 100,
      showBorder: true,
      borderColor: '#F59E0B',
      borderThickness: 2,
      reassertState: true,
    },
    gestures: {
      enabled: true,
      modifier: 'alt',
      moveButton: 'left',
      resizeButton: 'right',
      scope: 'allWindows',
      restoreMaximizedOnMove: true,
      ignoreFullscreen: true,
      showResizeOverlay: true,
      overlayColor: '#3B82F6',
      excludedApps: [],
    },
    snapping: {
      enabled: true,
      modifier: 'ctrl',
      distance: 16,
      monitorEdges: true,
      workAreaEdges: true,
      windowEdges: true,
      centerLines: true,
      gridEnabled: false,
      gridColumns: 4,
      gridRows: 4,
      showGuides: true,
    },
    alignment: {
      pixelOrientation: true,
      showGrid: false,
      gridSize: 20,
      snapToGrid: true,
    },
    hotkeys: {
      togglePin: { ctrl: false, alt: true, shift: false, win: true, key: 'T' },
      opacityUp: { ctrl: false, alt: true, shift: false, win: true, key: 'PageUp' },
      opacityDown: { ctrl: false, alt: true, shift: false, win: true, key: 'PageDown' },
      opacityStep: 10,
    },
    rules: [makeRule({ id: 'a', name: 'Notepad', exe: 'notepad.exe', opacity: 85 })],
  };
}

export function makeRule(overrides: Partial<Rule> = {}): Rule {
  return {
    id: 'rule',
    name: 'Rule',
    enabled: true,
    exe: 'app.exe',
    title: null,
    pin: true,
    opacity: 100,
    ...overrides,
  };
}

export function makeManaged(overrides: Partial<ManagedWindow> = {}): ManagedWindow {
  return {
    hwnd: 100,
    title: 'Untitled - Notepad',
    exeName: 'notepad.exe',
    exePath: 'C:\\Windows\\System32\\notepad.exe',
    pid: 42,
    pinned: true,
    pinPending: false,
    opacity: 100,
    source: 'manual',
    ruleId: null,
    ...overrides,
  };
}
