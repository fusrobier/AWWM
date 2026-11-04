# AWWM architecture

AWWM (Advanced Window Manager) is a Windows 10/11 x64 utility written in Rust with a
Tauri 2 + React settings interface. This document describes how the pieces fit together
and why. Windows API limitations are covered separately in
[WINDOWS_LIMITATIONS.md](WINDOWS_LIMITATIONS.md).

## 1. Repository layout

```
AWWM/
├─ Cargo.toml                    Cargo workspace
├─ crates/
│  ├─ awwm-core/                 Platform-independent logic (no Win32, no unsafe)
│  │   geometry.rs               Point / Size / Rect / MonitorInfo / DPI helpers
│  │   resize.rs                 Resize-edge detection, constrained resize math
│  │   snap.rs                   snap_engine: move/resize snapping + guide generation
│  │   gesture.rs                Modifier/button model, gesture classification
│  │   rules.rs                  automation_rules: rule model + window matching
│  │   settings.rs               Settings model, defaults, validation, migration
│  └─ awwm-engine/               Win32 window-management engine (Windows only)
│      win/                      Thin, documented wrappers over unsafe Win32 calls
│        window.rs               HWND queries: geometry, styles, title, class, pid
│        process.rs              process_monitor: exe path, integrity level, UWP host
│        monitor.rs              Monitor enumeration and DPI
│        icon.rs                 Icon extraction -> PNG data URL
│      enumerator.rs             window_enumerator: Alt-Tab-eligible windows
│      pinning.rs                window_pinning: topmost apply / restore
│      transparency.rs           window_transparency: layered alpha apply / restore
│      manager.rs                window_manager: managed-window registry, reattach,
│                                session persistence, crash recovery
│      events.rs                 WinEvent hook registration / dispatch
│      engine_thread.rs          The engine thread (owner of all managed state):
│                                commands, WinEvents, rule execution + de-duplication,
│                                pin frames, hotkeys, debounced session writes
│      session.rs                Crash-recovery record (session.json)
│      input_hook.rs             global_mouse_hook + keyboard_state (LL hooks thread)
│      gesture.rs                window_movement + window_resizing (gesture thread)
│      overlay.rs                overlay_manager: frames, size label, snap guides
│      hotkeys.rs                Global keyboard shortcuts (RegisterHotKey)
│      bin/awwm-testapp.rs       Controllable test windows for integration tests
│      tests/                    Integration tests against real windows
├─ src-tauri/                    Tauri application shell
│   commands.rs                  IPC commands (the only frontend entry points)
│   settings_store.rs            settings_manager: atomic JSON persistence + backup
│   tray.rs                      system_tray
│   autostart.rs                 HKCU\…\Run integration
│   single_instance.rs           Named mutex + activation of the running instance
│   logging.rs                   tracing + rolling log files
│   ui_window.rs                 Create / destroy the settings WebView on demand
└─ src/                          React + TypeScript settings interface (frontend)
```

`awwm-core` has no Windows dependency, so the snap engine, resize math, rule matching,
gesture classification and settings validation are tested with plain `cargo test` on
synthetic data. `awwm-engine` exposes a small Rust API (`Engine`) that knows nothing about
Tauri, so it is exercised directly by integration tests with real windows created by
`awwm-testapp`.

## 2. Threads

```
┌────────────────────┐ commands (mpsc + PostThreadMessage wake)   ┌──────────────────────┐
│ Tauri main thread  │──────────────────────────────────────────▶│ Engine thread        │
│ tray, IPC, WebView │◀──────────── EngineEvent callback ─────────│ WinEvent hooks       │
└────────────────────┘                                            │ managed windows      │
                                                                  │ rules, pin frames    │
┌────────────────────┐  GestureStart / Update / End (posted msg)  │ global hotkeys       │
│ Input-hook thread  │─────────────────┐                          └──────────────────────┘
│ WH_MOUSE_LL        │                 ▼
│ WH_KEYBOARD_LL     │        ┌──────────────────────┐
│ (decides only)     │        │ Gesture thread       │
└────────────────────┘        │ SetWindowPos, snap,  │
                              │ resize overlay/guides│
                              └──────────────────────┘
```

* **Input-hook thread.** Installs `WH_MOUSE_LL` and `WH_KEYBOARD_LL` and runs a
  `GetMessageW` loop (low-level hooks are called on the installing thread through its
  message loop). Callbacks only: read a lock-free/short-lock configuration snapshot,
  track modifier state, decide whether to swallow a button event, and post a message
  to the gesture thread. No window repositioning, enumeration or logging I/O happens
  inside a hook callback. The thread runs at the highest normal priority (`THREAD_PRIORITY_HIGHEST`) to keep the system
  hook chain fast (Windows silently removes LL hooks that repeatedly exceed
  `LowLevelHooksTimeout`).
* **Gesture thread.** Owns the active move/resize gesture and its overlay windows
  (outline, size label, snap guides). Mouse moves are *coalesced*: the hook stores the
  latest cursor position in a shared slot and posts a wake-up message only if one is not
  already pending, so a slow target application never builds up a backlog. Repositioning
  uses synchronous `SetWindowPos` on this thread, which naturally rate-limits to what the
  target can process and lets us read back the real size the application accepted
  (minimum/maximum track sizes) to keep the anchored edge fixed.
* **Engine thread.** Single owner of all managed-window state (no shared mutable state
  across threads besides small read-mostly snapshots). Hosts out-of-context WinEvent
  hooks (`SetWinEventHook` with `WINEVENT_OUTOFCONTEXT`), the pinned-window frame
  overlays, rule execution, session persistence and global hotkeys. Tauri talks to it
  through `EngineHandle` commands with a reply channel and a timeout.
* **Tauri main thread.** Tray icon, IPC commands (all `async`, so they never run on the
  UI thread), and the settings WebView. The WebView is created on demand and destroyed
  when closed, so the idle footprint of the background process is the Rust engine only.

## 3. Coordinates and DPI

All engine threads call `SetThreadDpiAwarenessContext(PER_MONITOR_AWARE_V2)`, so every
coordinate the engine handles (cursor, `GetWindowRect`, monitor rects) is in **physical
pixels**. Two rectangles are tracked for each window:

* *window rect* – `GetWindowRect`, what `SetWindowPos` expects;
* *visible frame* – `DwmGetWindowAttribute(DWMWA_EXTENDED_FRAME_BOUNDS)`, which excludes the
  invisible resize borders Windows 10/11 add around normal windows.

Snapping, outlines and size labels use the visible frame; the per-window border margins
(window rect − visible frame) are re-measured after each reposition and added back
before calling `SetWindowPos`. Dimensions shown to the user are converted to logical
pixels using the effective DPI of the monitor the window is on
(`GetDpiForMonitor(MDT_EFFECTIVE_DPI)`), e.g. a 1500 px wide window on a 150 % monitor is
displayed as 1000 px. Snap distance and border thickness are configured in logical
pixels and converted per monitor.

When a moved window crosses into a monitor with a different DPI, a per-monitor-aware
application resizes itself (`WM_DPICHANGED`). The move gesture detects the size change
and rescales the cursor-to-window offset proportionally so the pointer stays at the same
relative spot of the window.

## 4. Managed windows (pinning and transparency)

A *managed window* is a top-level window AWWM changed. The engine stores, per window:

```
ManagedWindow {
  hwnd, pid, identity { exe_path, class_name, title },
  pinned: bool, opacity: u8 (percent),
  original { ex_style, was_topmost, layered: None | Attributes{key, alpha, flags} },
  source: Manual | Rule(id) | Recovered,
}
```

* **Pin** – `SetWindowPos(hwnd, HWND_TOPMOST, …, SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE)`;
  never activates or focuses the window. Unpin restores `HWND_NOTOPMOST` unless the window
  was already topmost before AWWM touched it.
* **Transparency** – adds `WS_EX_LAYERED` and calls
  `SetLayeredWindowAttributes(LWA_ALPHA)`. 100 % = opaque, 0 % = fully transparent.
  If the window *already* was layered, `GetLayeredWindowAttributes` tells the two cases
  apart: windows using `SetLayeredWindowAttributes` have their key/alpha/flags saved and
  restored; windows using per-pixel `UpdateLayeredWindow` are rejected with an explanatory
  error because switching them to uniform alpha would break their rendering. Restoring
  removes `WS_EX_LAYERED` only if AWWM added it and forces a repaint (`RedrawWindow`).
* **Unpin = release** – restores topmost state, ex-style and layered attributes, removes the
  pin frame, and forgets the window.
* **Visual indicator** – each pinned window gets a click-through frame window
  (`WS_EX_LAYERED|WS_EX_TRANSPARENT|WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE`, shaped with
  `SetWindowRgn`) kept directly above it in the Z-order. It follows the window via
  `EVENT_OBJECT_LOCATIONCHANGE` hooks registered **per pinned process** (not globally, to
  avoid waking for every cursor move system-wide) and hides on minimize, hide and cloak
  (virtual desktop switches).
* **Topmost re-assertion** – some applications reset their own Z-order. On
  `EVENT_SYSTEM_FOREGROUND`, minimize-end and move/size-end for a managed window the engine
  verifies `WS_EX_TOPMOST` and the layered alpha and re-applies them if needed.
  No polling timers are used.
* **Lifetime** – `EVENT_OBJECT_DESTROY` removes the entry. Every operation validates that the
  HWND still exists *and* still belongs to the recorded PID (HWND values can be recycled).
* **Handle changes / restarts** – when a managed window is destroyed, its identity and
  settings are kept for a configurable grace period (`reattachGraceSecs`). A new top-level
  window of the same executable and class (title used as a tiebreaker) that appears within
  that period inherits the pin and opacity. Rules cover the long-term case.
* **Crash recovery** – the set of managed windows (HWND, PID, original styles) is persisted
  to `session.json` on every change. On start-up, entries whose HWND still exists with the
  same PID are re-adopted, so a crash or forced kill of AWWM never leaves windows
  permanently topmost/transparent without a way to restore them.
* **Exit** – with *Restore windows when exiting* enabled (default), all managed windows are
  released on shutdown.

## 5. Automation rules

A rule matches on executable (file name, or a full path when the pattern contains `\`;
case-insensitive) and an optional window-title matcher (contains / equals / starts with /
ends with / wildcard `*` `?`). The first enabled matching rule (list order) wins and applies
`pin` and `opacity`.

* Windows are evaluated on `EVENT_OBJECT_SHOW` for top-level, Alt-Tab-eligible windows,
  at start-up for all existing windows, and on demand ("Apply rules now").
* Because many windows get their final title or real process only after being shown
  (UWP apps are hosted by `ApplicationFrameHost.exe` until the `CoreWindow` attaches), a
  window that did not match yet is re-evaluated on `EVENT_OBJECT_NAMECHANGE` (hook only
  installed while a title-based rule exists) and by two one-shot timers (500 ms, 2 s).
* **De-duplication** – the engine records `(hwnd → rule)` once a rule has been applied.
  A window is never auto-processed twice, so a user who unpins an auto-pinned window is not
  overridden by the next title change. The record is dropped when the window is destroyed.
* Multiple windows of the same application are handled independently.
* Editing a rule updates windows currently managed by that rule.

## 6. Mouse gestures

| Gesture                 | Default                  |
|-------------------------|--------------------------|
| Move                    | Alt + Left button drag   |
| Move with snapping      | Alt + Ctrl + Left drag   |
| Resize                  | Alt + Right button drag  |
| Resize with snapping    | Alt + Ctrl + Right drag  |
| Cancel active gesture   | Esc (restores the start geometry) |

The activation modifier, snap modifier and buttons are configurable. A button-down event
starts a gesture only when the activation modifier is held, no unrelated modifier is held,
and the window under the cursor (`WindowFromPoint` → `GetAncestor(GA_ROOT)`) is eligible:
visible, not the desktop/taskbar, not an AWWM window, not excluded, not full-screen (when
configured), accessible at our integrity level, not hung, and pinned if the scope is
*pinned windows only*. Otherwise the event passes through untouched.

During a gesture the button-down/up pair is swallowed so the target application never
sees a click. Mouse-move events are never swallowed (the cursor keeps moving). To stop the
Alt (or Win) key-up from activating the target's menu bar (or Start), a neutral unassigned
virtual key (0xE8) is injected once at gesture start. Snap-modifier key presses that
happen during a gesture are swallowed (and their matching key-ups) so the target does not
receive stray Ctrl shortcuts.

* **Move** keeps the initial cursor-to-frame offset. Maximized windows are restored first
  (optional) and placed proportionally under the cursor.
* **Resize** chooses edges from the 3×3 zone of the initial cursor position (corner zones
  resize two edges, side zones one edge, the centre zone uses the bottom-right corner).
  The opposite edge/corner stays fixed. After each `SetWindowPos` the real size is read
  back; if the application clamped it (min/max track size, character grid), the window is
  re-anchored so the fixed edge does not drift. The accepted size is deliberately not
  stored as a limit, because applications that round their size would otherwise lock it.
* **Feedback** – an accent outline around the visible frame and a label with the current
  logical width × height, updated on each step and removed immediately on release.
* **Robustness** – a gesture ends on button-up, Esc (cancel), target destruction, desktop
  switch (UAC prompt, lock screen) or a second press of the same button (missed up-event).

## 7. Snap engine

Pure functions in `awwm-core::snap`:

* `snap_move(rect, env, cfg)` aligns the left/right/centre and top/bottom/centre lines of the
  moving frame to candidate lines: monitor bounds, work areas (taskbar-aware), edges of
  other visible windows (only when the two windows overlap or nearly overlap on the
  perpendicular axis), optional work-area centre lines and optional grid lines. X and Y are
  solved independently, giving corner snaps for free.
* `snap_resize(rect, edges, env, cfg, constraints)` only moves the edges being dragged;
  opposite edges never move and a snap that would violate size constraints is discarded.
* Each successful snap yields a `SnapGuide` (orientation, position, extent, source) that the
  gesture thread renders as thin guide lines; the window itself is placed at the snapped
  geometry, so the outline shows the exact final result.

Snapping is active only while the snap modifier is held; pressing or releasing it during a
gesture re-evaluates immediately.

## 8. Settings and persistence

Settings live in `%APPDATA%\dev.awwm.windowmanager\settings.json` (schema-versioned, unknown fields
ignored, missing fields defaulted, values clamped by `Settings::sanitize`). Writes are
atomic (`settings.json.tmp` + rename) and the previous file is kept as
`settings.json.bak`; a corrupt file is moved aside and defaults are used, with a warning
shown in the UI. Logs are written with `tracing` to `%LOCALAPPDATA%\dev.awwm.windowmanager\logs` (daily
rotation, 7 files retained).

## 9. Security boundaries

* The app runs as the invoking user (`asInvoker`) and never requests elevation. Windows
  belonging to processes with a higher integrity level (elevated apps, UAC) cannot be
  modified (User Interface Privilege Isolation). AWWM detects this up front by comparing
  token integrity levels and reports a clear message instead of failing silently.
* The frontend can only call the explicit command set in `commands.rs`; the Tauri
  capability file grants no file-system, shell or HTTP permissions. Opening documentation
  and folders is done by fixed, Rust-side paths only.
* A strict CSP is configured for the WebView.
