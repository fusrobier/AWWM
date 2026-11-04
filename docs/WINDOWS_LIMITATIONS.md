# Windows API limitations

AWWM changes windows that belong to other programs. Windows deliberately limits what one
program can do to another program's windows, and some applications render themselves in
ways that resist outside changes. This page lists every known limitation, why it exists,
and how AWWM behaves.

## Elevated (administrator) applications

**Limitation.** User Interface Privilege Isolation (UIPI) prevents a program from changing
the windows of a program running at a higher integrity level. AWWM runs as a normal user
and never asks for administrator rights, so it cannot pin, make transparent, move or resize
windows of applications started with *Run as administrator* (for example an elevated
terminal, Registry Editor, or Task Manager on many systems). Windows also does not deliver
mouse input over such windows to AWWM's global mouse hook in every situation.

**Behaviour.** AWWM compares integrity levels *before* it tries anything:

* Pinning or changing opacity shows: *"… is running as administrator. Windows does not allow
  programs without administrator rights to change its windows, so AWWM leaves it alone."*
* Alt + drag on such a window is passed through untouched and a notice is shown (at most
  once every 30 seconds per window).
* The window picker greys out elevated windows with the same explanation.

**Running AWWM itself as administrator is not supported.** AWWM never elevates on its own,
and it should not be started with *Run as administrator*: the per-user installation lives
in a folder your normal account can write to, so an elevated AWWM would let any program
running under your account gain administrator rights by modifying it. Mouse input that is
synthesized by programs running at a lower integrity level is also ignored by AWWM's
gesture hook for the same reason.

## Protected and system windows

The desktop, taskbar, Start menu, notification area overflow, task view and other shell
surfaces are excluded. Protected processes (anti-malware services, some DRM players) cannot
even be inspected; AWWM reports these as not modifiable.

## Universal Windows Platform (UWP) / packaged apps

* Classic UWP apps (for example *Calculator*) are hosted in a frame window owned by
  `ApplicationFrameHost.exe`. AWWM resolves the real application by looking at the hosted
  `Windows.UI.Core.CoreWindow` child, so a rule for `CalculatorApp.exe` works. Note that the
  Windows 11 Calculator executable is `CalculatorApp.exe`, not `calc.exe` or
  `Calculator.exe`; use *Choose from running windows* in the rule editor to get the exact
  name.
* The real process is attached shortly after the frame window appears, so rules for UWP
  apps are re-evaluated 0.5 s and 2 s after a window opens.
* Suspended UWP apps are *cloaked* by the system and are not listed.
* Transparency on UWP frames is applied through the frame window. On some builds parts of
  the content are composed separately by DWM and may not become translucent.

## Transparency

* Transparency uses layered windows (`WS_EX_LAYERED` + `SetLayeredWindowAttributes`). It
  works for the vast majority of Win32, WPF, WinForms, Qt, Electron and Chromium windows.
* **Per-pixel layered windows.** Applications that draw their own transparency with
  `UpdateLayeredWindow` (some launchers, widgets, custom-shaped windows) cannot be given
  uniform transparency without breaking their rendering. AWWM detects them and refuses
  with an explanation; they can still be pinned.
* **Applications that already use uniform transparency** keep their own alpha and colour
  key; AWWM's opacity is applied *relative* to it and their original values are restored
  when you unpin.
* **Direct3D / Vulkan / OpenGL full-screen or flip-model swap chains** (games, some video
  players) may ignore window-level transparency or show black content. This is decided by
  the application's renderer and cannot be detected reliably from outside.
* **0 % opacity** makes a window invisible. It stays pinned and listed on the dashboard and
  in the tray; use the dashboard, *Unpin all windows* in the tray menu, or the opacity
  shortcut to make it visible again.
* Some applications reset their window styles (for example when switching themes). With
  *Reassert always-on-top and opacity* enabled, AWWM re-applies its state
  when the window is activated, restored or finishes moving.

## Always on top

* "Topmost" is a single band in the Windows Z-order. Several pinned windows are all above
  normal windows; among themselves the most recently activated one is on top.
* Some applications (games, video players, screen recorders, other "always on top" tools)
  make themselves topmost or reorder themselves. They can cover pinned windows.
* **Full-screen applications in the foreground.** While a full-screen application (typically
  a game) is in the foreground and receiving input, Windows may silently refuse to make *other*
  windows topmost: the call succeeds but the window stays behind. When this happens to a pin
  that AWWM applies automatically (a rule, a re-created window, recovery after a restart) the
  window is kept as **Waiting to pin** on the dashboard (with a single notice) and pinned as
  soon as the foreground changes. A pin you request yourself reports the refusal immediately;
  use **Pin now** to retry.
* A few applications remove `WS_EX_TOPMOST` again when they are activated; AWWM re-applies it
  when the *Reassert always-on-top and opacity* option is on.
* The coloured frame around pinned windows is a separate click-through overlay window that
  follows the pinned window. It may lag behind a fast-moving window by a frame.

## Mouse gestures

* Gestures use a system-wide low-level mouse hook (`WH_MOUSE_LL`). If Windows considers the
  hook too slow it silently removes it; AWWM keeps its hook callback minimal to avoid this.
* While a gesture is active the target application does not receive the mouse button
  press. Applications that use **Alt + click** themselves (for example multi-cursor editing
  in some code editors, or 3D tools) lose that function while gestures are enabled. Add them
  to *Excluded applications* or choose another activation modifier.
* Other programs can install low-level keyboard hooks of their own, and a hook installed
  later runs earlier and can hide key presses from AWWM. A focused WebView does this (AWWM's
  own settings window included): key-downs never reach AWWM's hook while key-ups do. AWWM
  therefore reads the activation modifier from the system key state when a button is
  pressed, re-checks Esc and the snap modifier while the pointer moves during a gesture, and
  also accepts an Esc *release* as the cancel signal.
* Releasing **Alt** (or **Win**) on its own normally opens the menu bar (or Start). AWWM sends
  a neutral, unassigned key (virtual-key 0xE8) at the start of a gesture to prevent this.
* Full-screen applications are ignored by default so games are never moved accidentally.
* Maximized windows are restored when you start moving them (configurable); they cannot be
  resized with the gesture while maximized.
* **Interrupted gestures.** If a UAC prompt or the lock screen appears during a gesture, the
  button release may never reach AWWM. AWWM ends the gesture on the desktop switch, on Esc
  (which also restores the original position), when the window closes, or on the next press
  of the same button.
* Windows of hung (not responding) applications are skipped.

## Size limits and DPI

* Applications enforce their own minimum and maximum sizes (`WM_GETMINMAXINFO`). AWWM cannot
  query them from another process; it reads back the size the application accepted after
  each step and moves the window so the opposite edge stays in place. The accepted size is not
  remembered as a hard limit, so applications that round their size (character grids) keep resizing
  smoothly.
* Windows 10/11 windows have invisible resize borders of a few pixels. AWWM measures the
  *visible* frame (`DWMWA_EXTENDED_FRAME_BOUNDS`) for snapping and size display, so windows
  snap flush to screen edges and each other.
* AWWM is per-monitor DPI aware (v2) and works in physical pixels internally. Displayed sizes
  and the snap distance are in logical pixels of the monitor the window is on (a 1500 px
  wide window on a 150 % monitor is shown as 1000 px). Applications that are not DPI aware
  are scaled by Windows; their sizes are still reported correctly.
* When a window is dragged to a monitor with a different scale factor, per-monitor-aware
  applications resize themselves. AWWM keeps the pointer at the same relative position in
  the window.

## Window identity

* Window handles are reused by Windows after a window closes. AWWM always checks that a
  handle still belongs to the same process before acting on it.
* Some applications destroy and re-create their window (theme changes, restarts). Within
  the *reattach grace period* (default 30 s) a new window of the same executable and window
  class inherits the pin and opacity. For longer-lived behaviour, use a rule.

## Startup and shutdown

* *Start with Windows* uses the per-user `Run` registry key, so it needs no administrator
  rights. If the startup entry is disabled in Task Manager, Windows will not start AWWM even
  when the setting is on.
* If AWWM is terminated abruptly (crash, forced kill), Windows does not restore the windows
  it changed. AWWM records managed windows in `%LOCALAPPDATA%\dev.awwm.windowmanager\session.json`; on the next
  start it either re-adopts them (*Recover managed windows after a restart or crash*) or restores
  their original state.
* When Windows signs out or shuts down, applications close anyway, so no restoration is
  necessary.
