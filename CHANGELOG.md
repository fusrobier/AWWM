# Changelog

All notable changes to AWWM are documented in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and AWWM uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-11-06

First public release.

### Window management

- Always on top: pin any normal window; pinned windows get a coloured, click-through frame
  (colour and thickness configurable).
- Per-window transparency from 0 % to 100 %.
- Automatic rules that pin windows and/or set their opacity, matched by executable (file
  name, full path or `*`/`?` pattern) and an optional window-title condition.
- Global keyboard shortcuts: pin or unpin the active window (Win+Alt+P) and change its
  opacity (Win+Alt+PageUp / PageDown).
- Crash recovery: windows AWWM changed are adopted again, or restored, after a crash or a
  forced shutdown.

### Mouse controls

- Move any window with Alt + left-button drag and resize it with Alt + right-button drag,
  from anywhere inside the window. Esc cancels and restores the original geometry.
- Snapping (hold Ctrl as well) to screen edges, work areas, other windows, work-area centre
  lines and an optional grid, with guide lines.
- **Pixel orientation** (on by default): while moving, a label shows the window's position
  in pixels relative to its screen and the distance moved; when not snapping, thin guide
  lines mark edges and centres that line up exactly with screen edges, other windows,
  centre lines or grid lines. It never changes where the window goes.
- **Visible pixel grid** (off by default): a subtle grid on every screen while moving or
  resizing, anchored at each screen's top-left corner, scaled per screen DPI and drawn on
  whole physical pixels. Every fifth line is solid. The grid is drawn below the window
  being dragged, lets clicks pass through and disappears as soon as the drag ends.
  Adjustable **Grid size** (4–200 px, default 20 px) with a live preview, and optional
  **Snap to the pixel grid** while the snap key is held.

### Application

- Runs in the notification area (system tray) with **Open settings**, a pause switch,
  **View pinned windows**, **Unpin all windows** and **Exit AWWM**.
- **Minimize to the tray** (on by default): minimizing the settings window hides it in the
  tray instead of the taskbar. Existing settings files get the default only when they do
  not contain the option yet; a stored choice is never changed.
- Start with Windows (per-user, no administrator rights), start minimized, export and
  import of all settings and rules, light/dark theme.
- AWWM exits cleanly (restoring managed windows when **Restore windows when exiting** is
  on) when the installer or uninstaller asks it to, instead of being terminated.

### Installation

- Per-user NSIS installer (no administrator rights) and a per-machine MSI.
- The only runtime dependency is the Microsoft Edge WebView2 Runtime (preinstalled on
  Windows 11). Both installers detect it and install nothing when it is present. When it is
  missing, `setup.exe` explains what will be installed and asks before running Microsoft's
  digitally signed WebView2 bootstrapper (the MSI runs it without asking); the bootstrapper
  is embedded in the installers and its signature is verified at build time. Nothing is
  downloaded by AWWM's own installers.
- `setup.exe` refuses Windows versions older than Windows 10 version 1809 with a clear
  message.
- If AWWM cannot open its settings window (for example because WebView2 is missing or
  damaged), it explains why and how to fix it instead of failing silently.
- Updating keeps settings, rules and the **Start with Windows** setting. Uninstalling removes
  the program files and shortcuts (the NSIS uninstaller also the startup entry); settings are
  kept unless **Delete the application data** is ticked.

[1.0.0]: https://github.com/fusrobier/AWWM/releases/tag/v1.0.0
