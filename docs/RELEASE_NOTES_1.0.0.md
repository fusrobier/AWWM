# AWWM 1.0.0 — release notes

AWWM (Advanced Window Manager) is a small Windows utility that keeps chosen windows on top,
makes them translucent, applies automatic rules to windows as they open, and lets you move
and resize any window with Alt + drag, with snapping, a live pixel read-out and an optional
pixel grid. This is the first public release.

## Download

| File | Use |
|---|---|
| `AWWM_1.0.0_x64-setup.exe` | Recommended. Installs for the current user, no administrator rights. |
| `AWWM_1.0.0_x64_en-US.msi` | Installs for all users (administrator rights required), e.g. for managed PCs. |
| `awwm.exe` | The program alone, for users who manage it themselves (no Start menu entry or uninstaller; needs the WebView2 Runtime, which it does not install). |
| `SHA256SUMS.txt` | SHA-256 hashes of the files above. |

Requirements: Windows 10 version 1809 or later, or Windows 11, 64-bit. The only runtime
dependency is the Microsoft Edge WebView2 Runtime, which Windows 11 already includes. Both
installers detect it; when it is missing, `setup.exe` asks and then runs Microsoft's signed
WebView2 installer, and the MSI runs it without a question of its own. See [Installing, updating and removing AWWM](INSTALLATION.md).

## Highlights

- **Always on top and transparency** for any normal window, with a coloured click-through
  frame around pinned windows.
- **Automatic rules** by executable and window title.
- **Alt + drag to move, Alt + right-drag to resize** from anywhere inside a window; hold
  Ctrl to snap to screen edges, work areas, other windows, centre lines and a grid.
- **Pixel orientation** (new, on by default): while moving, a label shows the window's
  position in pixels and how far it has moved, and thin guides mark exact alignments.
- **Visible pixel grid** (new, off by default): a subtle grid on every screen while moving
  or resizing, with adjustable size, live preview and optional snapping to its lines.
- **Minimize to the tray** (new, on by default): the settings window hides in the
  notification area when minimized; AWWM keeps working in the background.
- Global shortcuts, start with Windows, export/import of settings, crash recovery.

The full list is in the [changelog](../CHANGELOG.md).

## Upgrading from a pre-release build

Run the installer over the existing installation. Settings, rules and shortcuts are kept;
options that did not exist before (**Minimize to the tray**, **Pixel orientation**, the
pixel grid) get their defaults. Pre-release builds do not understand the installer's close
request; Setup then closes them through Windows' Restart Manager (in the release test the
pre-release build still exited normally and restored its windows). To be sure, exit AWWM from
its tray menu before updating from a pre-release build.

## Known limitations

- **Unsigned binaries.** The installers and `awwm.exe` are not code-signed yet. Windows
  SmartScreen may warn on the first download; check the file against `SHA256SUMS.txt`.
- **Administrator windows.** AWWM runs without administrator rights, so Windows does not
  let it pin, make transparent, move or resize windows of programs running as
  administrator. AWWM shows a notice instead. See
  [Windows limitations](WINDOWS_LIMITATIONS.md).
- **Full-screen games and exclusive full-screen apps.** While such an app has the
  foreground and receives input, Windows may refuse to bring newly pinned windows on top;
  AWWM retries automatically.
- **Mixed display scaling.** Pixel values are logical pixels of the screen a window is on.
  When a window moves to a screen with a different scaling, the **Δ** distance in the
  position label is hidden, because there is no common unit across such screens. Grids,
  labels and snapping on monitors with different scaling were verified with automated
  tests of the pixel arithmetic only; the release test machine ran all monitors at 100 %.
- **WebView2 installation needs the internet.** The included Microsoft installer downloads
  the runtime. For offline PCs, install the WebView2 Runtime separately first (Microsoft
  offers an offline installer).
- **MSI checks.** Only `setup.exe` checks the Windows version and asks before installing
  WebView2; the MSI installs on any 64-bit Windows and runs Microsoft's WebView2 installer
  directly when the runtime is missing.
- **Silent installs and WebView2.** With `/S`, Microsoft's WebView2 installer still shows
  its own progress window when the runtime has to be installed.
- **MSI specifics.** The MSI uninstaller does not remove the **Start with Windows** entry;
  turn that setting off before uninstalling an MSI installation. The MSI asks a running AWWM
  to close normally only when it is installed in Program Files (the default); otherwise, and
  for pre-release builds that do not understand the request, Windows Installer closes AWWM
  and the windows it managed are restored at its next start (crash recovery).
- **Close request.** Any program running as you can ask AWWM to exit (as it can end AWWM
  anyway); programs with lower rights cannot.
- Store (UWP) apps, some games and windows protected by Windows have further restrictions;
  see [Windows limitations](WINDOWS_LIMITATIONS.md).

## Privacy

AWWM does not use the network and contains no telemetry. The only network access during
installation is Microsoft's WebView2 installer, and only when the runtime is missing and
you agree.
