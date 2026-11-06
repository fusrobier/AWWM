# Installing, updating and removing AWWM

This page covers system requirements, dependencies, the two installers, updates and
uninstalling. For using AWWM, see the [user guide](USER_GUIDE.md).

## System requirements

| Requirement | Details |
|---|---|
| Operating system | Windows 10 version 1809 (build 17763) or later, or Windows 11, 64-bit (x64). On ARM-based PCs, Windows 11 is required (it runs x64 programs). |
| Runtime | Microsoft Edge WebView2 Runtime, used only by the settings window. Preinstalled on Windows 11 and on most Windows 10 PCs. Setup installs it when it is missing (see below). |
| Rights | No administrator rights for the recommended installer. |
| Disk space | About 12 MB. |

AWWM does **not** need Rust, Node.js, the Visual C++ Redistributable or any other
development tool. The program links the C runtime statically and uses only libraries that
are part of Windows (the Universal C Runtime is included in Windows 10 and 11).

## Which installer?

Each release provides two installers with the same program. Use one of them, not both.

| File | Install scope | Install folder | Administrator rights |
|---|---|---|---|
| `AWWM_<version>_x64-setup.exe` (recommended) | Current user | `%LOCALAPPDATA%\AWWM` | Not needed |
| `AWWM_<version>_x64_en-US.msi` | All users of the PC | `C:\Program Files\AWWM` | Required (Windows asks) |

The release also contains `SHA256SUMS.txt`. To check a download, compare its hash with the
line for that file:

```powershell
Get-FileHash .\AWWM_1.0.0_x64-setup.exe -Algorithm SHA256
```

The installers are not code-signed yet, so Windows SmartScreen may show
"Windows protected your PC". Select **More info**, check that the file name is right, then
**Run anyway**.

## Installing

1. Run `AWWM_<version>_x64-setup.exe`.
2. Setup checks the system first and lists every step in its progress window:
   - Windows older than Windows 10 version 1809, 32-bit Windows, or Windows 10 on an ARM
     PC: Setup explains why and exits **without installing anything**.
   - **WebView2 Runtime present:** Setup shows the version it found and does not reinstall
     it.
   - **WebView2 Runtime missing:** Setup explains that AWWM needs it and asks before doing
     anything. Select **OK** to run Microsoft's official, digitally signed WebView2
     installer, which is included in AWWM's Setup. It downloads the runtime from Microsoft
     (an internet connection is required) and shows its own progress window. Select
     **Cancel** to exit Setup.
   - If the WebView2 installation fails (for example without an internet connection),
     Setup stops with an error message before AWWM's files are copied. Fix the cause and run
     Setup again; it starts from the beginning.
3. Choose the folder and the Start menu entry, then install. On the last page you can
   create a desktop shortcut and start AWWM.

AWWM opens its settings window on the first start and then runs in the notification area
(system tray).

### Unattended installation

The NSIS installer accepts these command-line options:

| Option | Effect |
|---|---|
| `/S` | Silent: no pages and no questions (WebView2 is installed without asking when it is missing). Microsoft's WebView2 installer still shows its own progress window in that case. |
| `/P` | Passive: shows progress only, asks nothing. |
| `/D=<folder>` | Install folder (must be the last option). |
| `/NS` | No shortcuts. |
| `/R` | Start AWWM after a silent or passive installation. |

The MSI supports the usual `msiexec` options, for example
`msiexec /i AWWM_1.0.0_x64_en-US.msi /qn`. It does not check the Windows version and does not
ask before running Microsoft's WebView2 installer when the runtime is missing.

## Updating

Download the new installer and run it; there is no need to uninstall first.

- A running AWWM is asked to close first. It restores the windows it pinned or made
  transparent (when **Restore windows when exiting** is on) and exits. If it does not close
  within 20 seconds, Setup offers to end it; the windows it managed are then adopted again
  or restored at its next start. The MSI does the same for installations in Program Files
  (its default folder); elsewhere Windows Installer closes AWWM itself.
- Settings, automatic rules and keyboard shortcuts are kept
  (`%APPDATA%\dev.awwm.windowmanager\settings.json`). New options get their default values
  only if they are not in your settings file yet; nothing you have set is changed.
- **Start with Windows** stays as it was. (If you choose to uninstall the old version first
  on Setup's update page, the entry is written again when AWWM next starts, for example
  through **Run AWWM** on Setup's last page.)
- When the same version is already installed, Setup asks whether to reinstall it or to
  uninstall it.

## Uninstalling

1. Open **Settings → Apps → Installed apps** (Windows 10: **Apps & features**), find
   **AWWM** and select **Uninstall**.
2. A running AWWM is closed first and restores its windows, as during an update.
3. The program files and the Start menu and desktop shortcuts are removed. The NSIS
   uninstaller also removes the **Start with Windows** entry; the MSI uninstaller does not, so
   turn **Start with Windows** off in AWWM before uninstalling an MSI installation.
4. Your settings and logs are kept, so a later installation picks them up again. To remove
   them as well, tick **Delete the application data** in the NSIS uninstaller, or delete
   these folders yourself:
   - `%APPDATA%\dev.awwm.windowmanager` (settings and their backup)
   - `%LOCALAPPDATA%\dev.awwm.windowmanager` (logs and the crash-recovery record)

The WebView2 Runtime is part of Windows and shared with other programs; uninstalling AWWM
does not remove it.

Scripts can ask a running AWWM to close normally with `awwm.exe --exit`. It waits up to
20 seconds and returns exit code 0 when no AWWM is running any more, 1 otherwise. It never
starts AWWM.

## Building the installers yourself

See [Building from source](../README.md#building-from-source) in the README.
`scripts\build-release.ps1` runs every check and builds both installers;
`scripts\verify-release.ps1` verifies the result (including the digital signature of the
embedded WebView2 installer) and writes `release\<version>\SHA256SUMS.txt`.
