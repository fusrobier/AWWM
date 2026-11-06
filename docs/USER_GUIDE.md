# AWWM user guide

This guide shows you how to use AWWM — Advanced Window Manager: pinning windows on top,
changing their opacity, automatic rules, moving and resizing windows with the mouse, and
the settings that control them. For installation, see the [README](../README.md). For
restrictions that come from Windows itself (administrator windows, games, Store apps), see
[Windows limitations](WINDOWS_LIMITATIONS.md).

## Contents

- [First start](#first-start)
- [Pin windows and change their opacity](#pin-windows-and-change-their-opacity)
- [Keyboard shortcuts](#keyboard-shortcuts)
- [Automatic rules](#automatic-rules)
- [Mouse controls: move, resize and snap](#mouse-controls-move-resize-and-snap)
- [General settings](#general-settings)
- [Tray menu](#tray-menu)
- [Troubleshooting](#troubleshooting)
- [Uninstalling](#uninstalling)

## First start

- The first time you start AWWM, the settings window opens on the **Dashboard**. Later
  starts open it too, unless you turn on **Start minimized to the tray** (see
  [General settings](#general-settings)).
- AWWM adds an icon to the notification area (system tray). Click it to open the settings
  window; right-click it for the [tray menu](#tray-menu).
- When you close the settings window, AWWM keeps running in the tray as long as
  **Keep running in the background when this window is closed** is on (the default). Pins,
  rules, mouse gestures and shortcuts keep working. With the option off, closing the window
  exits AWWM.
- When you minimize the settings window, AWWM hides it instead of leaving a button on the
  taskbar, as long as **Minimize to the tray** is on (the default). AWWM keeps running. Click
  the tray icon, select **Open settings**, or start AWWM again to show the window with the
  same page and any input you had not saved yet.
- Only one copy of AWWM runs at a time. Starting it again brings the settings window of
  the running copy to the front.
- Changes in the settings window are saved automatically; settings pages show
  **Saving…** in their header while a change is being written.

The settings window has five pages: **Dashboard**, **Automatic rules**,
**Mouse controls**, **General** and **About**.

### Pausing AWWM

The **Window management** switch at the top of the Dashboard (also
**Window management enabled** in the tray menu) pauses AWWM. While it is off, automatic
rules, mouse gestures, keyboard shortcuts and pin frames are inactive. Windows that are
already pinned stay on top and keep their opacity. The switch position is saved and
survives a restart.

## Pin windows and change their opacity

The **Managed windows** list on the Dashboard shows every window AWWM currently changes.

### Pin a window

1. On the Dashboard, select **Pin a window…**.
2. Find the window: type part of its title or program name in
   **Search by title or program**, or select **Refresh** to reload the list.
3. Select the window. It stays above all normal windows from now on and gets a coloured
   frame.

A pinned window gets the **Default opacity** from the **General** page (100 % unless you
changed it). Windows that are already pinned are marked **Already pinned**. Windows of
programs running as administrator are greyed out; see
[Elevated windows](#a-window-cannot-be-pinned-moved-or-made-transparent).

You can also pin the active window with the **Pin or unpin the active window** shortcut
(default Win+Alt+P).

### Change opacity

Drag the **Opacity** slider of a window in the list. The window changes while you drag:
100 % is fully opaque, 0 % is fully invisible. Below 10 % the row shows the warning
*Window will be nearly invisible — use this list or the tray to restore it*.

A window can be translucent without being pinned (for example through the opacity
shortcuts or an opacity-only rule). Such windows show **Not pinned** and a **Pin** button.

### Bring to front and unpin

| Button | What it does |
|---|---|
| **Bring to front** | Restores the window if it is minimized and activates it. |
| **Pin** | Shown for windows that are managed but not pinned. Pins the window and keeps its opacity. |
| **Unpin** | Releases the window completely: it returns to its normal stacking order and its original opacity, and leaves the list. A window that was already always on top before AWWM touched it stays on top. |
| **Unpin all** | Releases every managed window after a confirmation. |

The badge next to the program name shows why AWWM manages a window:

| Badge | Meaning |
|---|---|
| **Manual** | You pinned it or changed its opacity. |
| **Rule: _name_** | An automatic rule changed it. |
| **Rule (deleted)** | A rule changed it and that rule has since been deleted. |
| **Recovered** | AWWM adopted it again after a restart or crash (see **Recover managed windows after a restart or crash**). |

## Keyboard shortcuts

The shortcuts work in every application and act on the active (foreground) window. They
never act on AWWM's own window, the desktop or the taskbar, and they are inactive while
window management is paused.

| Action | Default | Effect |
|---|---|---|
| **Pin or unpin the active window** | Win+Alt+P | Pins the window with the default opacity, or releases it completely if it is already pinned (this also restores its opacity). |
| **Increase opacity** | Win+Alt+PageUp | Makes the window more opaque by the opacity step. A window that is not pinned and reaches 100 % is released. |
| **Decrease opacity** | Win+Alt+PageDown | Makes the window more transparent by the opacity step, down to 0 %. Works on any window, pinned or not. |

### Change a shortcut

Open **Mouse controls** and scroll to **Keyboard shortcuts**.

1. Toggle the modifier chips **Ctrl**, **Alt**, **Shift** and **Win**. At least one
   modifier is required.
2. Choose the key from the list. Choose **Disabled** to turn the shortcut off.

A valid change takes effect immediately. The line under each shortcut shows its state,
for example *Win+Alt+P is active*. AWWM refuses a combination that another AWWM shortcut
already uses. **Opacity step** (1–50 %, default 10 %) sets how much one press of an opacity
shortcut changes the opacity.

## Automatic rules

Rules pin windows and/or set their opacity automatically. Manage them on the
**Automatic rules** page.

### When rules are applied

- **When a window opens.** If a new window does not match yet (many programs set their
  final title, or Store apps their real process, shortly after the window appears), AWWM
  checks it again after 0.5 s and 2 s, and whenever its title changes while an enabled rule
  has a title condition.
- **At startup**, for windows that are already open, when **Apply rules at startup** is on
  (the default).
- **When you save rule changes**, for open windows that no rule has processed yet.
- **When you turn window management back on.**
- **When you select Apply rules now** (see below).

### How a window is matched

- Rules are checked from top to bottom. The first enabled rule that matches a window wins;
  later rules are ignored for that window.
- A window that AWWM already manages (for example one you pinned yourself) is left alone.
- **Each window is processed by rules only once.** After a rule has been applied to a
  window, AWWM does not apply rules to it again on its own, not even when its title
  changes. If you unpin a window that a rule pinned, it stays unpinned until it is closed
  and opened again, or until you select **Apply rules now**.
- Each window of a program is handled separately.
- Editing a rule updates the windows it currently manages. Turning a rule off or deleting
  it does not release windows it already changed; unpin them on the Dashboard.

### Create a rule

1. Select **New rule**.
2. **Name** (optional): shown in the rule list. Leave it empty to use the program name.
3. **Executable** (required): type the program, or select
   **Choose from running windows…** and pick a window. Picking a window fills in the
   executable (and the name, if empty) and offers
   **Also use its title "…" as a title condition**.
4. Optional: under **Window title**, turn on
   **Only match windows with a specific title** and set the condition (see
   [Title conditions](#title-conditions)).
5. Under **Effect**, choose **Pin matching windows** and/or move the **Opacity** slider.
6. Keep **Enable this rule** on and select **Create rule**.

### Executable patterns

Executable matching is never case-sensitive. Surrounding spaces and quotes are removed.

| You enter | Matches |
|---|---|
| `notepad.exe` | `notepad.exe` in any folder |
| `notepad` | The same: a name without an extension also matches `notepad.exe` |
| `C:\Tools\app.exe` | Only that exact path (a pattern containing `\` is compared with the full path) |
| `C:\Program Files\Discord\*\Discord.exe` | `Discord.exe` in any sub-folder of `C:\Program Files\Discord` |
| `disc*` | Any program whose file name starts with `disc` |

`*` matches any run of characters (including none) and `?` exactly one character.

> **Calculator and other Store apps.** The Windows 11 Calculator runs as
> `CalculatorApp.exe`, not `calc.exe`. Packaged (Microsoft Store) apps are matched by
> their real executable, not by `ApplicationFrameHost.exe`. When in doubt, use
> **Choose from running windows…** to get the exact name.

### Title conditions

Turn on **Only match windows with a specific title**, then choose a mode under **Title**
and enter the **Text**. Without a title condition, the rule applies to every window of the
program. Title matching ignores case unless you tick **Case-sensitive**.

| Title mode | Text | Matches the title |
|---|---|---|
| **Contains** | `notes` | `My Notes - Editor` |
| **Is exactly** | `calculator` | `Calculator` (the whole title, ignoring case) |
| **Starts with** | `my` | `My Notes` |
| **Ends with** | `editor` | `My Notes - Editor` |
| **Matches pattern (\* and ?)** | `* - discord` | `#general - Discord` |

### Opacity-only rules

To make windows translucent without keeping them on top, clear **Pin matching windows**
and set **Opacity** below 100 %. The rule list shows such a rule as **Opacity 70 %** (for
example) instead of **Pin**.

A rule that neither pins nor lowers the opacity has no effect: the editor warns
*This rule has no effect…*, the list shows **No effect**, and the rule never matches. Below
10 % opacity the editor warns that matching windows will be nearly invisible.

### Edit, turn off, reorder and delete rules

Each row in the rule list has:

- a switch that turns the rule on or off;
- up and down arrows that move the rule (order matters: the first match wins);
- an edit button that opens the rule in the editor (**Save rule** stores the change);
- a delete button that removes the rule after a confirmation. Deleting cannot be undone.

### Example rules

A new installation contains three example rules. They are all off, so AWWM changes no
window until you turn one on.

| Rule | Executable | Effect |
|---|---|---|
| Notepad | `notepad.exe` | Pin · 85 % |
| Calculator | `CalculatorApp.exe` | Pin · 70 % |
| Discord | `Discord.exe` | Pin · 90 % |

### Apply rules now

**Apply rules now** applies the rules to every open window that AWWM does not manage at
the moment, including windows a rule has processed before (for example a window you
unpinned). A message reports how many windows were changed. While window management is
paused, it reports an error instead.

## Mouse controls: move, resize and snap

Mouse gestures let you move and resize a window by dragging anywhere inside it, not just
on the title bar. The **Cheat sheet** at the top of the **Mouse controls** page always
shows your current combination. While AWWM handles a drag, the program under the pointer
does not receive the mouse click.

### Move a window

1. Hold **Alt**.
2. Press the left mouse button anywhere inside the window and drag.
3. Release the button.

The window keeps its position relative to the pointer. A maximized window is restored to
its normal size and follows the pointer (**Restore maximized windows when moving**, on by
default); with this option off, AWWM does not move maximized windows.

### Resize a window

1. Hold **Alt**.
2. Press the right mouse button inside the window and drag.
3. Release the button.

Where you press decides which edge or corner moves. AWWM divides the window into a 3×3
grid of zones:

| | Left third | Middle third | Right third |
|---|---|---|---|
| **Top third** | Top-left corner | Top edge | Top-right corner |
| **Middle third** | Left edge | Bottom-right corner | Right edge |
| **Bottom third** | Bottom-left corner | Bottom edge | Bottom-right corner |

The opposite edge or corner stays where it is. Programs keep their own minimum and maximum
sizes. While you resize, an outline and a label with the current size (width × height in
logical pixels) are shown, if **Show outline and size while resizing** is on. Maximized
windows cannot be resized with the mouse; restore them first.

### Cancel a move or resize

Press **Esc** before you release the mouse button. The window returns to its original
position and size (a window that was maximized is maximized again).

### Snapping

Hold **Ctrl** (the **Snap key**) as well to make the window snap to nearby edges. You can
press or release Ctrl at any time during the drag; snapping switches on or off
immediately.

- When moving, the window's edges snap to nearby edges and grid lines, and its centre snaps
  to the work-area centre lines. The horizontal and vertical directions snap independently,
  so a window can snap into a corner.
- When resizing, only the edges you drag snap (centre lines are not used); the opposite
  edges never move.
- **Snap distance** (2–100 px, default 16 px, in logical pixels) sets how close an edge must
  be before the window snaps to it.
- **Show snap guides** (on by default) draws a thin line where the window snapped. The
  colour is the **Outline and guide colour** (default `#3B82F6`).

Choose what windows snap to:

| Setting | Default | Snaps to |
|---|---|---|
| **Screen edges** | On | The edges of each monitor |
| **Work area edges** | On | The edges of the screen area not covered by the taskbar |
| **Other windows** | On | Edges of visible windows next to the one you drag |
| **Centre lines** | On | The horizontal and vertical centre of each work area (moving only; the window's centre snaps to them) |
| **Grid** | Off | An invisible grid that divides each work area into **Grid columns** × **Grid rows** cells (2–24 each, default 4 × 4) |
| **Snap to the pixel grid** | On | The lines of the [visible pixel grid](#pixel-grid-and-orientation), while that grid is shown. Used only when none of the other targets is within reach. |

When two targets are equally close, AWWM prefers work area edges, then screen edges, other
windows, the **Grid** and last the centre lines.

### Pixel grid and orientation

The **Pixel grid and orientation** card on the **Mouse controls** page, below **Snapping**,
helps you place windows to the pixel. It has no effect while **Mouse gestures** are off.

| Setting | Default | What it does |
|---|---|---|
| **Pixel orientation** | On | While you move a window, shows its position and marks exact alignments (see below). Moving only. |
| **Visible pixel grid** | Off | Shows a grid of lines on every monitor while you move or resize a window. |
| **Grid size** | 20 px | Distance between two grid lines, 4–200 px in logical pixels. A **Preview** shows the grid at actual size. |
| **Snap to the pixel grid** | On | While the grid is visible and the snap key is held, window edges also snap to the grid lines. |

**Pixel orientation.** A label at the centre of the window shows, for example,
`X 640 · Y 320 · Δ +120, −45`:

- `X` and `Y` are the position of the window's top-left corner (the visible frame) in
  logical pixels, measured from the top-left corner of the monitor that holds most of the
  window.
- `Δ` is how far the window has moved since the drag started. It is shown only while the
  window is on the monitor where the drag started, because monitors with different scaling
  have no common logical unit. On other monitors the label shows only `X` and `Y`.

While the snap key is not held, thin guide lines in the **Outline and guide colour** appear
where an edge or centre of the window lines up exactly (0 px apart) with a screen edge, a
work area edge, another window (within the snap distance), a centre line or a line of the
**Grid**, as far as those targets are turned on. The lines of the pixel grid are not
marked. These guide lines never move the window. While you hold the snap key, normal
snapping and its guides apply instead.

**Visible pixel grid.** While you move or resize a window, AWWM draws lines on every
monitor, starting at the top-left corner of each monitor:

- The lines are **Grid size** logical pixels apart. On a monitor with display scaling they
  are spaced wider by the scale factor (at 150 %, 20 px lines are 30 physical pixels apart)
  and always fall on whole physical pixels.
- Every fifth line is solid, the others are dotted.
- The grid uses the **Outline and guide colour** at about 38 % opacity.
- It is drawn directly below the window you drag, never over it. Clicks pass through it.
- It disappears the moment you release the button or cancel with **Esc**.

**Snap to the pixel grid.** Hold the snap key while the grid is visible and window edges
also snap to its lines when no screen edge, other window or other snap target is within the
**Snap distance**. When moving, this is decided separately for the horizontal and vertical
direction; when resizing, for each edge you drag. The lines of the monitor that holds most
of the window are used. The **Cheat sheet** lists the grid and this snapping when they are on.

### Which windows gestures work on

**Works on** chooses between **All eligible windows** (default) and
**Pinned windows only**. AWWM never starts a gesture on:

- its own windows, the desktop or the taskbar;
- minimized windows;
- full-screen windows such as games and videos, while **Ignore full-screen windows** is on
  (the default);
- windows of programs that are not responding;
- programs in the **Excluded applications** list;
- windows of programs running as administrator. AWWM shows a notice instead (at most once
  every 30 seconds per window); see
  [Windows limitations](WINDOWS_LIMITATIONS.md#elevated-administrator-applications).

A gesture also does not start when another modifier key is held, for example
Alt + Shift + drag. Only the activation key and, while snapping is on, the snap key may be
down. In all these cases the click reaches the program normally.

### Excluded applications

Add programs in which mouse gestures should never start, for example programs that use
Alt + click themselves. Type a name or pattern and select **Add**, or select
**Choose from running windows…**. Entries use the same matching as rule executables
(`blender.exe`, a full path, or a `*`/`?` pattern; not case-sensitive). Remove an entry
with its remove button.

### Change keys and buttons

| Setting | Options | Default |
|---|---|---|
| **Mouse gestures** | On / off | On |
| **Activation key** | Alt, Ctrl, Shift, Win | Alt |
| **Move with** | Left, Right, Middle, Disabled | Left |
| **Resize with** | Left, Right, Middle, Disabled | Right |
| **Snapping** | On / off | On |
| **Snap key** | Any modifier except the activation key | Ctrl |

One mouse button cannot both move and resize. If you choose the current snap key as the
activation key, AWWM picks a different snap key automatically.

## General settings

### Startup

| Setting | Default | What it does |
|---|---|---|
| **Start with Windows** | Off | Starts AWWM when you sign in. Uses the per-user startup list, so no administrator rights are needed. |
| **Start minimized to the tray** | Off | Starts AWWM without opening the settings window. The very first start always opens it. |
| **Apply rules at startup** | On | Applies automatic rules to windows that are already open when AWWM starts. |

### Background and recovery

| Setting | Default | What it does |
|---|---|---|
| **Minimize to the tray** | On | Minimizing the settings window hides it in the notification area, so it has no taskbar button. AWWM keeps running; click the tray icon, select **Open settings** or start AWWM again to show the window. When off, minimizing works as usual. Closing the window is controlled by the next setting. |
| **Keep running in the background when this window is closed** | On | Closing the settings window leaves AWWM running in the tray. When off, closing the window exits AWWM. |
| **Restore windows when exiting** | On | Releases all managed windows (always on top and opacity) when AWWM exits. When off, the windows stay as they are, and the next start adopts or restores them. |
| **Recover managed windows after a restart or crash** | On | At start, windows that AWWM managed when it last stopped and that are still open are adopted again (badge **Recovered** for manually managed ones). When off, those windows are restored to their original state instead. |
| **Reattach grace period** | 30 seconds | When a managed window closes, a new window of the same program (same executable and window class) that appears within this time inherits its pin and opacity. This covers programs that re-create their window. 0 turns it off; maximum 600 seconds. |

### Pinned windows

| Setting | Default | What it does |
|---|---|---|
| **Show a frame around pinned windows** | On | Draws a coloured, click-through border around pinned windows. |
| **Frame colour** | `#F59E0B` (amber) | Colour of the frame. |
| **Frame thickness** | 2 px | 1–16 px. |
| **Default opacity** | 100 % | Opacity applied when you pin a window manually (picker or shortcut). Below 10 %, a warning reminds you that new pins will be nearly invisible. |
| **Reassert always-on-top and opacity** | On | Re-applies them when a program resets its own window state. AWWM checks when a managed window is activated, restored or finishes moving. |

### Appearance and diagnostics

| Setting | Default | Options |
|---|---|---|
| **Theme** | Use system setting | Use system setting, Light, Dark |
| **Log level** | Information (default) | Errors only, Warnings and errors, Information (default), Debug, Trace (very detailed) |

### Backup

Every change is saved to `settings.json` immediately, so settings and rules survive a
restart or a forced end of the task. To move them to another PC or keep a copy:

- **Export…** saves all settings, keyboard shortcuts and automatic rules to a `.json` file
  you choose (suggested name `AWWM-settings.json`).
- **Import…** loads such a file and replaces the current settings and rules. The file is
  checked first: files that are not AWWM exports, are damaged or larger than 1 MB are
  rejected and nothing changes. Out-of-range values are corrected. The **Window management**
  switch keeps its current state.

### Maintenance

- **Open settings folder** opens `%APPDATA%\dev.awwm.windowmanager`, which contains `settings.json` and its
  backup.
- **Open log folder** opens `%LOCALAPPDATA%\dev.awwm.windowmanager\logs`.
- **Reset all settings…** replaces every setting, keyboard shortcut and automatic rule with
  its default after a confirmation. This cannot be undone.

The **About** page shows the version, opens the bundled documentation, and lists the exact
paths of the settings folder, settings file and log folder.

## Tray menu

Click the tray icon to open the settings window, including after you minimized it to the tray. Right-click it for this menu:

| Item | What it does |
|---|---|
| **Open settings** | Opens the settings window. |
| **Window management enabled** | Ticked while AWWM is active. Select it to pause or resume. While paused, the tray icon changes and its tooltip reads *AWWM — window management off*. |
| **View pinned windows (_n_)** | Opens the Dashboard. The number is the count of pinned windows. |
| **Unpin all windows** | Releases every managed window immediately (always on top and opacity are restored). |
| **Exit AWWM** | Exits AWWM. With **Restore windows when exiting** on, all managed windows are released first. |

## Troubleshooting

### Alt + drag does nothing

Check, in this order:

1. Window management is on (Dashboard switch or tray menu).
2. **Mouse gestures** is on and **Move with** / **Resize with** are not **Disabled**
   (**Mouse controls** page; the **Cheat sheet** shows the active combination).
3. No other modifier key is held (Alt + Shift + drag does not start a gesture).
4. **Works on** is not set to **Pinned windows only**, or the window is pinned.
5. The program is not in **Excluded applications**, the window is not full-screen,
   minimized or maximized (for resizing), and the program is responding.
6. The program is not running as administrator (see the next section).

### A window cannot be pinned, moved or made transparent

Programs started with *Run as administrator* cannot be changed by a program that runs
without administrator rights. AWWM reports *"… is running as administrator…"*, greys
these windows out in the picker as
*Runs as administrator — Windows does not let AWWM change it*, and lets mouse clicks on them
through. Running AWWM itself as administrator is not supported (see
[Windows limitations](WINDOWS_LIMITATIONS.md#elevated-administrator-applications)).

Some windows draw their own transparency and can be pinned but not made translucent.
These cases and other Windows restrictions are described in
[Windows limitations](WINDOWS_LIMITATIONS.md).

### A shortcut says it is already in use

If another program or Windows has already registered the same combination, the
**Keyboard shortcuts** section shows, for example,
*Win+Alt+P could not be registered: Already used by another application or by Windows.*
Choose a different modifier or key; AWWM registers the new combination immediately.

A shortcut showing *inactive while window management is paused* starts working again when
you turn window management back on.

### A program uses Alt + click itself

Some programs (for example code editors with multi-cursor editing, or 3D tools) use
Alt + click. While gestures are on, those clicks go to AWWM instead. Either add the program
to **Excluded applications**, or choose another **Activation key** on the
**Mouse controls** page.

### A window became invisible (0 % opacity)

An invisible window is still pinned and still listed. To get it back, use one of these:

- Open the Dashboard (tray menu **View pinned windows**) and move its **Opacity** slider up,
  or select **Unpin**.
- Select **Unpin all windows** in the tray menu.
- Make the window active (Dashboard **Bring to front**), then press the
  **Increase opacity** shortcut (default Win+Alt+PageUp) a few times.

### A rule does not apply

- Check that the rule is on, window management is on, and the rule pins or lowers the
  opacity (not **No effect**).
- Check the executable name: use **Choose from running windows…** to fill it in. The
  Windows 11 Calculator is `CalculatorApp.exe`.
- Check the title condition against the window's actual title.
- Check the order: an earlier rule may match the window first.
- A window that is already managed, or that a rule has already processed (for example one
  you unpinned), is not changed automatically. Select **Apply rules now**.

### Where settings and logs are

| File | Location |
|---|---|
| Settings | `%APPDATA%\dev.awwm.windowmanager\settings.json` |
| Previous settings (written before each save) | `%APPDATA%\dev.awwm.windowmanager\settings.json.bak` |
| Managed-window record for crash recovery | `%LOCALAPPDATA%\dev.awwm.windowmanager\session.json` |
| Logs (one file per day, the last 7 kept) | `%LOCALAPPDATA%\dev.awwm.windowmanager\logs\` |

When you report a problem, set **Log level** to **Debug**, reproduce the problem, and
attach the newest log file.

### Reset settings

- In the app: **General** → **Maintenance** → **Reset all settings…**.
- Without the app: exit AWWM from the tray, then delete or rename
  `%APPDATA%\dev.awwm.windowmanager\settings.json`. The next start behaves like a first start: it writes the
  default settings and opens the settings window.

### The settings file was damaged

If `settings.json` cannot be read as valid settings, AWWM still starts with the default
settings. It renames the damaged file to `settings.corrupt-<number>.json` in the same
folder and shows **Settings were not loaded** at the top of the **General** page, with the
new file name. To go back to your previous settings, do this before you change any setting
(each save replaces the backup): exit AWWM, then copy `settings.json.bak` to
`settings.json` in the same folder.

Values that are merely out of range (for example a snap distance above 100) are corrected
automatically when the file is loaded, and unknown entries are ignored.

## Uninstalling

1. If **Start with Windows** is on, turn it off first (the NSIS uninstaller also removes the
   startup entry; the MSI uninstaller does not).
2. You do not need to exit AWWM first. The uninstaller and the installer (when you update)
   close a running AWWM automatically by asking it to exit the normal way, so with **Restore
   windows when exiting** on, all managed windows return to normal first. This applies to
   the NSIS installation and to MSI installations in Program Files (the MSI default). You can
   also select **Exit AWWM** in the tray
   menu, or run `awwm.exe --exit` (for example in a script): it asks a running AWWM to exit,
   waits up to 20 seconds for it to end and returns exit code 0 when no AWWM is left or 1
   when it is still running. It never starts AWWM or shows a window.
3. Open Windows **Settings** → **Apps** → **Installed apps** (Windows 10: **Apps & features**),
   find **AWWM** and select **Uninstall**.
4. Your settings (`%APPDATA%\dev.awwm.windowmanager`) and the logs and recovery record
   (`%LOCALAPPDATA%\dev.awwm.windowmanager`) are removed when you tick *Delete the application
   data* in the NSIS uninstaller; otherwise they stay and you can delete both folders yourself.
   The MSI uninstaller always keeps them, and also keeps the *Start with Windows* entry if it
   was on, so turn that setting off before uninstalling an MSI installation.
