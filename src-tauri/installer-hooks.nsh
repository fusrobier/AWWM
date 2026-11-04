; NSIS installer hooks for AWWM (referenced by tauri.conf.json → bundle.windows.nsis.installerHooks).

; Remove the per-user "Start with Windows" entry so Windows does not try to launch a
; program that no longer exists after uninstalling.
!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "AWWM"
!macroend
