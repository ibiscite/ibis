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
  ; The Microsoft Store copy shares this copy's settings folder
  ; (packaging/msix/AppxManifest.xml), so while it is installed "Delete app
  ; data" leaves that folder alone. Its package folder exists once it has run;
  ; this is the last hook before Tauri deletes app data, so a Store copy
  ; installed after the uninstaller starts is not seen.
  !if "${BUNDLEID}" == "com.ibiscite.ibis"
    !define IBIS_STORE_PACKAGE "Ibiscite.Ibis"
  !else
    !define IBIS_STORE_PACKAGE "Ibiscite.IbisNightly"
  !endif
  ReadEnvStr $2 LOCALAPPDATA
  FindFirst $0 $1 "$2\Packages\${IBIS_STORE_PACKAGE}_*"
  FindClose $0
  ${If} $1 != ""
    StrCpy $DeleteAppDataCheckboxState 0
  ${EndIf}
  !undef IBIS_STORE_PACKAGE
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
