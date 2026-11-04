# AWWM — Advanced Window Manager

AWWM is a small Windows 10/11 (x64) utility that keeps chosen windows always on top, makes
them translucent, applies automatic rules to windows as they open, and lets you move and
resize any window by holding a key and dragging anywhere inside it, with optional snapping.
It runs in the notification area (system tray); the settings window opens only when you
need it. The engine is written in Rust; the settings interface uses Tauri 2 and React.

AWWM is an independent project. It is not affiliated with or endorsed by Microsoft.

## Features

- **Always on top.** Pin any normal window so it stays above other windows. Pinned windows
  get a coloured, click-through frame (on by default, colour and thickness configurable).
- **Per-window transparency.** Set the opacity of each managed window from 0 % to 100 %.
- **Automatic rules.** Pin windows and/or set their opacity automatically, matched by
  executable (file name, full path or `*`/`?` pattern) and an optional window-title
  condition.
- **Move with Alt + left button.** Drag anywhere inside a window, not just the title bar.
- **Resize with Alt + right button.** The nearest edge or corner follows the pointer, with
  an outline and a live size label (width × height in logical pixels).
- **Snapping.** Hold Ctrl as well to snap to monitor edges, work areas (the screen area
  not covered by the taskbar), other windows, work-area centre lines and an optional grid,
  with guide lines showing where the window snapped.
- **Global keyboard shortcuts.** Pin or unpin the active window and change its opacity
  from any application.
- **System tray.** Open the settings, pause window management, view pinned windows, unpin
  all windows, or exit.
- **Start with Windows.** Per-user startup entry; no administrator rights needed.
- **Crash recovery.** AWWM records which windows it changed. After a crash or forced
  shutdown, the next start adopts those windows again or restores their original state,
  so no window stays stuck on top or transparent.

## Default shortcuts

All mouse buttons, keys and shortcuts can be changed on the **Mouse controls** page.

| Action | Default |
|---|---|
| Move a window | Alt + left-button drag |
| Move with snapping | Alt + Ctrl + left-button drag |
| Resize a window | Alt + right-button drag |
| Resize with snapping | Alt + Ctrl + right-button drag |
| Cancel a move or resize (restores the original position and size) | Esc, during the drag |
| Pin or unpin the active window | Win+Alt+P |
| Increase opacity of the active window | Win+Alt+PageUp |
| Decrease opacity of the active window | Win+Alt+PageDown |

Each opacity shortcut press changes the opacity by 10 % (the **Opacity step** setting).

## Installation

`npm run tauri build` produces two installers. Use either one.

| Installer | Path | Install scope |
|---|---|---|
| NSIS (recommended) | `target\release\bundle\nsis\AWWM_1.0.0_x64-setup.exe` | Current user only; no administrator rights needed |
| MSI | `target\release\bundle\msi\AWWM_1.0.0_x64_en-US.msi` | All users; Windows asks for administrator approval |

AWWM needs the Microsoft Edge **WebView2 Runtime** for its settings window. It is
preinstalled on Windows 11. If it is missing, the installer downloads and installs it
silently, which requires an internet connection during setup.

The first start of AWWM opens the settings window; see the
[user guide](docs/USER_GUIDE.md) for a walkthrough.

## Building from source

### Prerequisites

- Windows 10 or 11, x64.
- [Rust](https://rustup.rs/) stable, installed with rustup, using the MSVC toolchain
  (`stable-x86_64-pc-windows-msvc`). The workspace requires Rust 1.90 or newer
  (edition 2024).
- Visual Studio 2022 Build Tools with the **Desktop development with C++** workload
  (MSVC compiler and a Windows SDK).
- Node.js 22.12 or newer, with npm. (Vite builds with Node.js 20.19+, but the Vitest
  unit tests need 22.12+.)
- Microsoft Edge WebView2 Runtime (see [Installation](#installation)).

### Commands

Run all commands from the repository root.

Install the JavaScript dependencies:

```powershell
npm install
```

Run AWWM in development mode (starts the Vite dev server on `http://127.0.0.1:1420` and a
debug build of the app):

```powershell
npm run tauri dev
```

Build an optimised release and both installers (written to `target\release\bundle\`):

```powershell
npm run tauri build
```

### Tests and checks

Rust unit and integration tests. The engine's integration tests create real windows on
the desktop with the controllable test application `awwm-testapp`
(`crates/awwm-engine/src/bin/awwm-testapp.rs`), so run them single-threaded in a signed-in,
unlocked session:

```powershell
cargo test --workspace -- --test-threads=1
```

Lints (warnings are errors):

```powershell
cargo clippy --workspace --all-targets -- -D warnings
```

Formatting check:

```powershell
cargo fmt --all -- --check
```

Frontend unit tests (Vitest):

```powershell
npm test
```

TypeScript type check:

```powershell
npm run typecheck
```

## Project layout

```
AWWM/
├─ crates/
│  ├─ awwm-core/      Platform-independent logic: geometry, resize math, snapping,
│  │                  gestures, rule matching, settings model and validation
│  └─ awwm-engine/    Win32 engine: pinning, transparency, hooks, gestures, overlays,
│                     rules, hotkeys; awwm-testapp and integration tests
├─ src-tauri/         Tauri application shell: IPC commands, settings file, tray,
│                     autostart, single instance, logging
├─ src/               React + TypeScript settings interface
└─ docs/              Architecture, Windows limitations, user guide
```

## Documentation

- [User guide](docs/USER_GUIDE.md): how to use every feature, troubleshooting, uninstalling.
- [Windows limitations](docs/WINDOWS_LIMITATIONS.md): elevated apps, UWP apps, games,
  transparency and other restrictions imposed by Windows.
- [Architecture](docs/ARCHITECTURE.md): threads, coordinates and DPI, persistence,
  security boundaries.

## Configuration files and logs

| What | Location |
|---|---|
| Settings | `%APPDATA%\dev.awwm.windowmanager\settings.json` (previous version kept as `settings.json.bak`) |
| Managed-window record for crash recovery | `%LOCALAPPDATA%\dev.awwm.windowmanager\session.json` |
| Logs (daily files, last 7 kept) | `%LOCALAPPDATA%\dev.awwm.windowmanager\logs\` |

The **General** page has buttons to open the settings and log folders, and the **About**
page shows the exact paths.

## Privacy

AWWM does not access the network and contains no telemetry. Its settings window may only
call AWWM's own commands: the Tauri capability grants no file-system, shell or HTTP
permissions, and the content security policy allows no outside connections. Settings and
logs stay on your computer in the folders listed above. The only download happens during
installation, when the installer fetches the WebView2 Runtime if it is missing.

## Test results

Release verification run of 2026-11-04 on Windows 11 Pro (10.0.26200), three monitors
(2560×1440, 1680×1440, 1920×1080, all at 100 % scaling):

| Check | Result |
|-------|--------|
| `cargo test --workspace -- --test-threads=1` | **229 passed**, 0 failed: awwm-core 97, awwm-engine 43 unit + 80 integration (real windows created by `awwm-testapp`), app shell 9 |
| `npm test` (vitest) | **80 passed** (9 files) |
| `npm run typecheck` | no errors |
| `cargo clippy --workspace --all-targets -- -D warnings` | no warnings |
| `cargo fmt --all -- --check` | clean |
| `npm run tauri build` | release build plus NSIS installer (2.6 MiB) and MSI (3.9 MiB) |
| Release smoke test | starts in the tray without a WebView process, ~18–28 MB working set, all default shortcuts registered |
| Real-input test of the installed NSIS build | **24/24 checks**: Alt+drag move, Alt+right-drag resize (corner and edge), Alt+Ctrl snapping (Ctrl held before and pressed mid-drag), Esc cancel, Win+Alt+P pin/unpin, opacity shortcut, pin frame, settings dashboard, crash recovery and re-adoption, tray mode after closing the window, no stuck keys or buttons. Input was synthesized with `SendInput` and went through the real low-level hooks. |

The integration tests cover pinning, transparency (including already-layered and per-pixel
layered windows), restoring, rules (new, existing, title-based, de-duplication, pending pins),
handle changes, session recovery, move/resize/snap gestures with real `SetWindowPos`
geometry on every monitor, pin frames, and error paths.

Not covered (verify manually): monitors with different scaling factors (all test monitors
ran at 100 %), UWP/Store apps, and windows of elevated programs.

## License

AWWM is released under the MIT License. See [LICENSE](LICENSE).
