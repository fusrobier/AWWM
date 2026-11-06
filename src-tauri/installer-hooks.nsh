; =============================================================================
; AWWM - NSIS installer hooks
; Referenced by src-tauri/tauri.conf.json -> bundle.windows.nsis.installerHooks.
;
; How Tauri's NSIS template (tauri-cli 2.12) uses this file:
;   * It is !included near the top of installer.nsi: after MUI2, FileFunc, x64,
;     LogicLib and the template's utils.nsh, but BEFORE the template's !defines
;     (PRODUCTNAME, MAINBINARYNAME, ...), its variables ($UpdateMode,
;     $PassiveMode, ...), its language strings and all of its sections.
;   * The top-level section below is therefore the first section that runs during
;     installation (before the template's "WebView2" and "Install" sections). It
;     may only use literal text, its own !defines and the registers $0-$9.
;   * The NSIS_HOOK_* macros are expanded later, inside the template's "Install"
;     and "Uninstall" sections, where all template !defines exist.
;   * On an interactive upgrade where the user chose "Uninstall before
;     installing", the template's reinstall page runs the previous uninstaller
;     BEFORE any section, so messages here never claim that nothing changed.
;
; Uninstaller rules: the uninstaller may only call "un." functions. To stay clear
; of that rule this file defines no functions at all; shared logic lives in macros
; that are expanded inline in each section.
;
; What the template already does (so it is deliberately NOT repeated here):
;   * Installs WebView2 with the embedded, Microsoft-signed bootstrapper when it
;     is missing (section "WebView2"), and skips that in /UPDATE mode. Because
;     bundle.windows.webviewInstallMode.silent is false, Microsoft's bootstrapper
;     always shows its own progress window - also when Setup runs with /S or /P.
;   * Force-closes a still running AWWM after the PREINSTALL / PREUNINSTALL hooks
;     (CheckIfAppIsRunning, which uses the Windows Restart Manager).
;   * Removes HKCU\...\CurrentVersion\Run\AWWM ("Start with Windows") on a real
;     uninstall but keeps it during /UPDATE uninstalls, and deletes
;     %APPDATA%\dev.awwm.windowmanager and %LOCALAPPDATA%\dev.awwm.windowmanager
;     only when the user ticks "Delete the application data".
;
; Test-only compile-time switches (never defined by Tauri; used by the makensis
; test harness, e.g. makensis /DAWWM_TEST_FORCE_NO_WEBVIEW2 harness.nsi):
;   AWWM_TEST_FORCE_UNSUPPORTED_WINDOWS  take the "Windows too old" branch
;   AWWM_TEST_FORCE_NO_WEBVIEW2          behave as if WebView2 were missing
;   AWWM_ENGINE_WINDOW_CLASS / AWWM_EXIT_MESSAGE (defined before the !include)
;                                        point the graceful exit at a fake engine
; =============================================================================

!include WinVer.nsh

; Minimum supported Windows: Windows 10 version 1809 (build 17763), 64-bit.
!ifdef AWWM_TEST_FORCE_UNSUPPORTED_WINDOWS
  !echo "AWWM installer hooks: TEST BUILD - every Windows version is rejected"
  !define AWWM_MIN_WINDOWS_BUILD 99999
!else
  !define AWWM_MIN_WINDOWS_BUILD 17763
!endif
; First Windows build that can run x64 programs on ARM64 PCs (Windows 11).
!define AWWM_MIN_ARM64_BUILD 22000
!ifdef AWWM_TEST_FORCE_NO_WEBVIEW2
  !echo "AWWM installer hooks: TEST BUILD - WebView2 is always reported missing"
!endif

; Microsoft Edge WebView2 Runtime client GUID. Same value as the template's
; WEBVIEW2APPGUID, which is not defined yet when this file is included.
!define AWWM_WEBVIEW2_APP_GUID "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"

; Graceful shutdown protocol of a running AWWM: post the registered window
; message AWWM_EXIT_MESSAGE to the engine's message-only window (class
; AWWM_ENGINE_WINDOW_CLASS). AWWM then restores pinned / transparent windows and
; exits. Implemented with System::Call on purpose: during PREINSTALL,
; $INSTDIR\awwm.exe is still the OLD build, so it must not be launched.
!define /ifndef AWWM_ENGINE_WINDOW_CLASS "AWWM.Engine"
!define /ifndef AWWM_EXIT_MESSAGE "AWWM.ExitRequested"
!define AWWM_HWND_MESSAGE -3          ; HWND_MESSAGE, parent of message-only windows
!define AWWM_SYNCHRONIZE 0x00100000   ; process access right needed to wait for exit
!define AWWM_WAIT_TIMEOUT 258         ; WaitForSingleObject: WAIT_TIMEOUT
!define AWWM_POLL_INTERVAL_MS 250
!define AWWM_EXIT_WAIT_POLLS 80       ; 80 x 250 ms = 20 s (engine shutdown can take ~13 s)

; -----------------------------------------------------------------------------
; Helpers (macros only, see "Uninstaller rules" above)
; -----------------------------------------------------------------------------

; Unattended (/S) installs show no message boxes; also write the reason to the
; console of the calling process, the same way the template reports errors.
; Clobbers $9.
!macro AWWM_CONSOLE_MESSAGE_IF_SILENT TEXT
  ${If} ${Silent}
    System::Call 'kernel32::AttachConsole(i -1) i .r9'
    ${If} $9 <> 0
      System::Call 'kernel32::GetStdHandle(i -11) p .r9'
      FileWrite $9 "${TEXT}$\r$\n"
    ${EndIf}
  ${EndIf}
!macroend

; Stops Setup before AWWM is installed. The message box is skipped in silent mode
; (/SD IDOK); Abort shows STATUS in the progress log and makes Setup exit with
; code 2.
!macro AWWM_REFUSE_INSTALL MESSAGE STATUS
  MessageBox MB_OK|MB_ICONSTOP "${MESSAGE}" /SD IDOK
  !insertmacro AWWM_CONSOLE_MESSAGE_IF_SILENT "${STATUS}"
  Abort "${STATUS}"
!macroend

; Asks a running AWWM to exit gracefully so it can restore the windows it pinned
; or made transparent, and waits up to 20 s until its process has ended (or, if
; the process cannot be opened, until its engine window is gone). Older AWWM
; builds ignore the request; in that case (or on timeout) nothing else happens
; here and the template's CheckIfAppIsRunning closes AWWM right after this hook.
; Used by both the installer and the uninstaller. Preserves $0-$5.
!macro AWWM_REQUEST_GRACEFUL_EXIT
  Push $0 ; engine window handle
  Push $1 ; registered message id
  Push $2 ; PostMessage / WaitForSingleObject result
  Push $3 ; process id, then poll counter
  Push $4 ; outcome: "" (still running), "exited" or "window-gone"
  Push $5 ; process handle (0 if it could not be opened)

  System::Call 'user32::FindWindowExW(p ${AWWM_HWND_MESSAGE}, p 0, w "${AWWM_ENGINE_WINDOW_CLASS}", p 0) p .r0'
  ${If} $0 P<> 0
    DetailPrint "AWWM is running. Asking it to close and restore your windows..."

    ; Open the engine's process first so its exit can be awaited precisely.
    StrCpy $3 0
    System::Call 'user32::GetWindowThreadProcessId(p r0, *i .r3) i .r2'
    StrCpy $5 0
    ${If} $3 <> 0
      System::Call 'kernel32::OpenProcess(i ${AWWM_SYNCHRONIZE}, i 0, i r3) p .r5'
    ${EndIf}

    StrCpy $2 0
    System::Call 'user32::RegisterWindowMessageW(w "${AWWM_EXIT_MESSAGE}") i .r1'
    ${If} $1 <> 0
      System::Call 'user32::PostMessageW(p r0, i r1, p 0, p 0) i .r2'
    ${EndIf}

    ${If} $2 = 0
      DetailPrint "Could not send the close request to AWWM. Setup will close it in the next step."
    ${Else}
      StrCpy $3 0
      StrCpy $4 ""
      ${Do}
        ${If} $5 P<> 0
          System::Call 'kernel32::WaitForSingleObject(p r5, i ${AWWM_POLL_INTERVAL_MS}) i .r2'
          ${If} $2 = 0 ; WAIT_OBJECT_0: the process has ended
            StrCpy $4 "exited"
            ${ExitDo}
          ${ElseIf} $2 <> ${AWWM_WAIT_TIMEOUT}
            ; Waiting failed: fall back to watching the engine window.
            System::Call 'kernel32::CloseHandle(p r5)'
            StrCpy $5 0
          ${EndIf}
        ${Else}
          Sleep ${AWWM_POLL_INTERVAL_MS}
          System::Call 'user32::FindWindowExW(p ${AWWM_HWND_MESSAGE}, p 0, w "${AWWM_ENGINE_WINDOW_CLASS}", p 0) p .r0'
          ${If} $0 P= 0
            StrCpy $4 "window-gone"
            ${ExitDo}
          ${EndIf}
        ${EndIf}
        IntOp $3 $3 + 1
      ${LoopUntil} $3 >= ${AWWM_EXIT_WAIT_POLLS}

      ${If} $4 == "exited"
        DetailPrint "AWWM closed and restored your windows."
      ${ElseIf} $4 == "window-gone"
        ; The process itself cannot be observed; give it a moment to finish exiting.
        Sleep 1000
        DetailPrint "AWWM closed and restored your windows."
      ${Else}
        System::Call 'user32::FindWindowExW(p ${AWWM_HWND_MESSAGE}, p 0, w "${AWWM_ENGINE_WINDOW_CLASS}", p 0) p .r0'
        ${If} $0 P<> 0
          DetailPrint "AWWM did not close within 20 seconds. Setup will close it in the next step."
        ${Else}
          DetailPrint "AWWM is still shutting down. Setup will close it in the next step."
        ${EndIf}
      ${EndIf}
    ${EndIf}

    ${If} $5 P<> 0
      System::Call 'kernel32::CloseHandle(p r5)'
    ${EndIf}
  ${EndIf}

  Pop $5
  Pop $4
  Pop $3
  Pop $2
  Pop $1
  Pop $0
!macroend

; -----------------------------------------------------------------------------
; Prerequisites: runs first, before the template's "WebView2" and "Install"
; sections, so AWWM is not installed when a check fails.
; -----------------------------------------------------------------------------
Section "-AWWM prerequisites"
  ; --- Windows version and architecture ------------------------------------
  DetailPrint "Checking system requirements..."
  ${WinVerGetMajor} $0
  ${WinVerGetMinor} $1
  ${WinVerGetBuild} $2

  ${IfNot} ${RunningX64}
    !insertmacro AWWM_REFUSE_INSTALL \
      "AWWM requires 64-bit Windows: Windows 10 version 1809 (build 17763) or later, or Windows 11.$\r$\n$\r$\nThis computer is running a 32-bit edition of Windows, so Setup will now exit. AWWM has not been installed." \
      "Setup was cancelled: AWWM requires 64-bit Windows. AWWM has not been installed."
  ${EndIf}

  ${IfNot} ${AtLeastWin10}
  ${OrIfNot} ${AtLeastBuild} ${AWWM_MIN_WINDOWS_BUILD}
    !insertmacro AWWM_REFUSE_INSTALL \
      "AWWM requires Windows 10 version 1809 (build 17763) or later, or Windows 11.$\r$\n$\r$\nThis computer is running Windows version $0.$1.$2, so Setup will now exit. AWWM has not been installed." \
      "Setup was cancelled: this version of Windows is not supported. AWWM has not been installed."
  ${EndIf}

  ; AWWM is an x64 program; ARM-based PCs can run it only on Windows 11, which
  ; added x64 emulation (Windows 10 on ARM emulates 32-bit x86 programs only).
  ${If} ${IsNativeARM64}
  ${AndIfNot} ${AtLeastBuild} ${AWWM_MIN_ARM64_BUILD}
    !insertmacro AWWM_REFUSE_INSTALL \
      "On ARM-based PCs, AWWM requires Windows 11, which can run x64 programs.$\r$\n$\r$\nThis computer is running Windows version $0.$1.$2, so Setup will now exit. AWWM has not been installed." \
      "Setup was cancelled: this version of Windows is not supported. AWWM has not been installed."
  ${EndIf}

  DetailPrint "Windows version $0.$1.$2 (64-bit) is supported."

  ; --- Microsoft Edge WebView2 Runtime --------------------------------------
  ; AWWM's settings window is rendered by WebView2. The registry values below
  ; are exactly the ones the template's "WebView2" section reads (with the same
  ; registry view, set in .onInit), so this check and the template always agree:
  ; when the runtime is found here, the template does not reinstall it.
  DetailPrint "Checking for Microsoft Edge WebView2 Runtime..."
  ${If} ${RunningX64}
    ReadRegStr $3 HKLM "SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\${AWWM_WEBVIEW2_APP_GUID}" "pv"
  ${Else}
    ReadRegStr $3 HKLM "SOFTWARE\Microsoft\EdgeUpdate\Clients\${AWWM_WEBVIEW2_APP_GUID}" "pv"
  ${EndIf}
  ${If} $3 == ""
    ReadRegStr $3 HKCU "SOFTWARE\Microsoft\EdgeUpdate\Clients\${AWWM_WEBVIEW2_APP_GUID}" "pv"
  ${EndIf}
  !ifdef AWWM_TEST_FORCE_NO_WEBVIEW2
    StrCpy $3 ""
  !endif

  ${If} $3 == "0.0.0.0"
    ; Registered but not usable (interrupted install or removal). The template
    ; treats any registered version as installed, so only point the user to a fix.
    DetailPrint "Microsoft Edge WebView2 Runtime is registered but incomplete (version 0.0.0.0). If the AWWM settings window does not open, install the runtime from https://developer.microsoft.com/microsoft-edge/webview2/"
  ${ElseIf} $3 != ""
    DetailPrint "Found Microsoft Edge WebView2 Runtime version $3. It will not be reinstalled."
  ${Else}
    DetailPrint "Microsoft Edge WebView2 Runtime was not found."
    ClearErrors
    ${GetOptions} $CMDLINE "/UPDATE" $4
    ${IfNot} ${Errors}
      ; Same rule as the template: updates never install WebView2.
      DetailPrint "Updates do not install WebView2. Run the regular AWWM installer to install it."
    ${Else}
      ; Passive mode (/P) is unattended like silent mode (/S, where /SD IDOK
      ; answers the message box): do not wait for an answer there.
      ClearErrors
      ${GetOptions} $CMDLINE "/P" $4
      ${If} ${Errors}
        MessageBox MB_OKCANCEL|MB_ICONINFORMATION \
          "AWWM needs the Microsoft Edge WebView2 Runtime to display its settings window, and it is not installed on this computer.$\r$\n$\r$\nSetup will now run Microsoft's official, digitally signed WebView2 installer. It downloads the runtime from Microsoft, so an internet connection is required, and it shows its own progress window.$\r$\n$\r$\nClick OK to continue, or Cancel to exit Setup without installing AWWM." \
          /SD IDOK IDOK awwm_webview2_accepted
        Abort "Setup was cancelled. AWWM has not been installed because it requires the Microsoft Edge WebView2 Runtime."
        awwm_webview2_accepted:
      ${EndIf}
      DetailPrint "Starting Microsoft's WebView2 installer (internet connection required)..."
    ${EndIf}
  ${EndIf}
SectionEnd

; -----------------------------------------------------------------------------
; Template hooks
; -----------------------------------------------------------------------------

; Before files are copied: let a running AWWM restore its windows and exit.
!macro NSIS_HOOK_PREINSTALL
  !insertmacro AWWM_REQUEST_GRACEFUL_EXIT
!macroend

; Before files are removed (also when an update or reinstall runs the previous
; version's uninstaller): same graceful exit.
!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro AWWM_REQUEST_GRACEFUL_EXIT
!macroend
