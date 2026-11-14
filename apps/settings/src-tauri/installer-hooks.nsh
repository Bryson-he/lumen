; Extra steps for Lumen's installer and uninstaller.
;
; Lumen is two programs: the app you open (lumen-settings.exe) and the part
; that stays in the notification area and draws the widgets (lumen-core.exe).
; The installer already knows to close the first; these steps close the second,
; so its file can be replaced or removed, and tidy up what Lumen added to
; Windows outside its own folder.

!macro LumenCloseCore
  ; The same as choosing "Quit" from Lumen's icon in the notification area.
  FindWindow $0 "LumenCoreTray" "Lumen"
  ${If} $0 != 0
    SendMessage $0 ${WM_CLOSE} 0 0 /TIMEOUT=5000
    ; Wait (up to ten seconds) for it to go, so its file is free.
    StrCpy $1 0
    ${Do}
      Sleep 250
      IntOp $1 $1 + 1
      FindWindow $0 "LumenCoreTray" "Lumen"
      ${If} $0 == 0
        ${ExitDo}
      ${EndIf}
    ${LoopUntil} $1 >= 40
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro LumenCloseCore
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro LumenCloseCore
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; "Start with Windows", if it was switched on.
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Lumen"
  ; The colour scheme given to Windows Terminal, if it was switched on.
  Delete "$LOCALAPPDATA\Microsoft\Windows Terminal\Fragments\Lumen\lumen.json"
  RMDir "$LOCALAPPDATA\Microsoft\Windows Terminal\Fragments\Lumen"
  ; Your themes, presets, wallpapers and widgets in %APPDATA%\Lumen are left alone.
!macroend
