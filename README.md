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
- **Pixel orientation.** While you move a window, a label shows its position in pixels
  relative to its screen and how far it has moved, and thin guides mark edges and centres
  that line up exactly with screen edges, other windows or centre lines.
- **Visible pixel grid.** An optional, subtle grid on every screen while you move or
  resize, anchored at each screen's top-left corner and scaled per screen. Adjustable size
  (4–200 px) with a live preview, and optional snapping to its lines.
- **Global keyboard shortcuts.** Pin or unpin the active window and change its opacity
  from any application.
- **System tray.** Open the settings, pause window management, view pinned windows, unpin
  all windows, or exit. Minimizing the settings window hides it in the tray (can be turned
  off).
- **Start with Windows.** Per-user startup entry; no administrator rights needed.
- **Export and import.** Save all settings and automatic rules to a file and load them
  again (General → Backup), e.g. to move them to another PC.
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

Download the installer from the
[GitHub releases page](https://github.com/fusrobier/AWWM/releases). Full details, including
unattended installation, are in [Installing, updating and removing AWWM](docs/INSTALLATION.md).

| Installer | Install scope |
|---|---|
| `AWWM_1.0.0_x64-setup.exe` (recommended) | Current user, in `%LOCALAPPDATA%\AWWM`; no administrator rights needed |
| `AWWM_1.0.0_x64_en-US.msi` | All users, in `C:\Program Files\AWWM`; Windows asks for administrator approval |

**Requirements:** Windows 10 version 1809 or later, or Windows 11, 64-bit.

**Dependencies:** the only runtime dependency is the Microsoft Edge **WebView2 Runtime**,
used by the settings window. It is preinstalled on Windows 11 and on most Windows 10 PCs.
Both installers check for it and leave an installed runtime alone. If it is missing, the
recommended `setup.exe` explains why it is needed and asks before running Microsoft's
digitally signed WebView2 installer, which is included in AWWM's Setup and downloads the
runtime from Microsoft (internet connection required); the MSI runs the same Microsoft
installer without a question of its own. AWWM needs no Rust, Node.js, Visual C++ Redistributable or other
development tools: the C runtime is linked statically and everything else is part of
Windows.

The installers are not code-signed yet; Windows SmartScreen may ask for confirmation.
Compare the download with `SHA256SUMS.txt` from the release (`Get-FileHash <file>`).

The first start of AWWM opens the settings window; see the
[user guide](docs/USER_GUIDE.md) for a walkthrough.

### Updating

Run the installer of the new version over the existing installation. A running AWWM is
asked to close first and restores the windows it manages; settings, rules and shortcuts are
kept, and **Start with Windows** stays on (if Setup removed the old version first, the entry
is written again when AWWM next starts, for example from Setup's last page). Options added in a new version get their defaults only if
your settings file does not contain them yet.

### Uninstalling

Use **Settings → Apps → Installed apps → AWWM → Uninstall**. Program files and shortcuts
are removed, and the NSIS uninstaller also removes the startup entry (for an MSI
installation, turn **Start with Windows** off first). Settings in
`%APPDATA%\dev.awwm.windowmanager` are kept unless you tick **Delete the application data**.

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

Or run the complete release pipeline: prerequisite checks, `npm ci`, formatting, lints,
all tests, the production build, and verification of the result (the embedded WebView2
installer's Microsoft signature, the version, the program's DLL imports), with the
installers, `awwm.exe` and `SHA256SUMS.txt` copied to `release\<version>\`:

```powershell
.\scripts\build-release.ps1
```

`-SkipTests` skips only the Rust integration tests, which open real windows on the
desktop.

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

- [Installing, updating and removing AWWM](docs/INSTALLATION.md): requirements,
  dependencies, installers, unattended installation.
- [User guide](docs/USER_GUIDE.md): how to use every feature, troubleshooting, uninstalling.
- [Windows limitations](docs/WINDOWS_LIMITATIONS.md): elevated apps, UWP apps, games,
  transparency and other restrictions imposed by Windows.
- [Architecture](docs/ARCHITECTURE.md): threads, coordinates and DPI, persistence,
  security boundaries.
- [Changelog](CHANGELOG.md) and [release notes for 1.0.0](docs/RELEASE_NOTES_1.0.0.md),
  including known limitations.

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
installation, when Microsoft's WebView2 installer fetches the runtime because it is missing
(with `setup.exe`, only after you agreed).

## Test results

Release verification of 1.0.0 on 2026-11-06, Windows 11 Pro (10.0.26200), three monitors
(1680×1440, 2560×1440, 1920×1080, all at 100 % scaling):

| Check | Result |
|-------|--------|
| `cargo test --workspace -- --test-threads=1` | **262 passed**, 0 failed: awwm-core 116, awwm-engine 45 unit + 88 integration (real windows created by `awwm-testapp`), app shell 13. After the final review fixes: core 118 and every integration file re-passed; `window_events` re-passed (11) once no full-screen game had the input focus |
| `npm test` (vitest) | **100 passed** (10 files) |
| `npm run typecheck` | no errors |
| `cargo clippy --workspace --all-targets -- -D warnings` | no warnings |
| `cargo fmt --all -- --check` | clean |
| `scripts\build-release.ps1` | NSIS installer (4.4 MiB) and MSI (5.8 MiB) built and verified: embedded WebView2 installer signed by Microsoft Corporation (valid chain), `awwm.exe` version 1.0.0, no Visual C++ runtime imports |
| Pixel grid latency | building the grid on 3 monitors takes 60–160 ms on its own thread; with a 4 px grid, 20 drag steps took 418–422 ms versus 408 ms without the grid (the gesture thread is never blocked) |
| Install over a pre-release build (silent) | installed, started, settings file byte-identical, **Start with Windows** entry restored, previous instance closed with `restore=true` |
| `awwm.exe --exit` | exit code 0, AWWM closed normally (windows restored); with nothing running: exit code 0, nothing started |
| Reinstall while AWWM runs | the installer's close request ends AWWM normally (no forced kill); settings kept |
| Uninstall (silent) | program files, Start menu and desktop shortcuts, Add/Remove Programs entry and startup entry removed; settings kept |
| Reinstall after uninstall | settings from before the uninstall used, startup entry restored |
| Minimize to tray | 8/8: minimize button and Win+D/Win+M-style minimize hide the window to the tray, a second launch restores it with a normal window style, closing keeps AWWM running |
| NSIS hook branches | old Windows, WebView2 missing (`/S`, `/P`, `/UPDATE`), graceful close (fast, slow, ignored, stuck) exercised against a test harness |

The integration tests cover pinning, transparency (including already-layered and per-pixel
layered windows), restoring, rules (new, existing, title-based, de-duplication, pending pins),
handle changes, session recovery, move/resize/snap gestures with real `SetWindowPos`
geometry on every monitor, pin frames, the pixel grid (one window per monitor, directly
below the dragged window, removed on release, cancel, abort and very short drags), pixel-grid
snapping, the position label and the exit request.

Integration tests that need new windows to become topmost cannot pass while a full-screen
application in the foreground receives input (Windows refuses topmost then); the test harness
detects this and says so.

Not covered (verify manually): monitors with different scaling factors (all test monitors
ran at 100 %; the DPI arithmetic is covered by unit tests), the MSI at runtime (built and
inspected only), UWP/Store apps, and windows of elevated programs.

## License

AWWM is released under the MIT License. See [LICENSE](LICENSE).
