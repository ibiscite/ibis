; Tauri's documented installer hooks. Use PowerShell's registry API to avoid
; NSIS string-length limits and preserve unexpanded entries in the user PATH.
!macro NSIS_HOOK_POSTINSTALL
  nsExec::ExecToStack '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$INSTDIR\packaging\path.ps1" -Action Add -Directory "$INSTDIR"'
  Pop $0
  Pop $1
  ${If} $0 != 0
    Abort "Could not add the ibis command to your user PATH. $1"
  ${EndIf}
!macroend
!macro NSIS_HOOK_PREUNINSTALL
  ; Tauri asks whether to close the running app after this hook. Retain the
  ; helper before files are deleted, but leave PATH alone if the user cancels.
  InitPluginsDir
  ClearErrors
  CopyFiles /SILENT "$INSTDIR\packaging\path.ps1" "$PLUGINSDIR"
  ${If} ${Errors}
    Abort "Could not prepare removal of the ibis command from your user PATH."
  ${EndIf}
!macroend
!macro NSIS_HOOK_POSTUNINSTALL
  nsExec::ExecToStack '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PLUGINSDIR\path.ps1" -Action Remove -Directory "$INSTDIR"'
  Pop $0
  Pop $1
  ${If} $0 != 0
    Abort "Could not remove the ibis command from your user PATH. $1"
  ${EndIf}
  Delete "$PLUGINSDIR\path.ps1"
!macroend
