; Note++ Custom NSIS Install/Uninstall hooks
; Adds Windows context menu entries and cleans them up on uninstall

!macro customInstall

  ; ── "Edit with Note++" on any file ────────────────────────────────────────
  WriteRegStr HKCU "Software\Classes\*\shell\Note++" \
              "" "Edit with Note++"
  WriteRegStr HKCU "Software\Classes\*\shell\Note++" \
              "Icon" "$INSTDIR\Note++.exe,0"
  WriteRegStr HKCU "Software\Classes\*\shell\Note++\command" \
              "" '"$INSTDIR\Note++.exe" "%1"'

  ; ── "Open Folder with Note++" on a directory ──────────────────────────────
  WriteRegStr HKCU "Software\Classes\Directory\shell\Note++" \
              "" "Open Folder with Note++"
  WriteRegStr HKCU "Software\Classes\Directory\shell\Note++" \
              "Icon" "$INSTDIR\Note++.exe,0"
  WriteRegStr HKCU "Software\Classes\Directory\shell\Note++\command" \
              "" '"$INSTDIR\Note++.exe" "%V"'

  ; ── "Open with Note++" on folder background (right-click inside a folder) ─
  WriteRegStr HKCU "Software\Classes\Directory\Background\shell\Note++" \
              "" "Open with Note++"
  WriteRegStr HKCU "Software\Classes\Directory\Background\shell\Note++" \
              "Icon" "$INSTDIR\Note++.exe,0"
  WriteRegStr HKCU "Software\Classes\Directory\Background\shell\Note++\command" \
              "" '"$INSTDIR\Note++.exe" "%V"'

!macroend


!macro customUninstall

  ; Remove all context menu entries
  DeleteRegKey HKCU "Software\Classes\*\shell\Note++"
  DeleteRegKey HKCU "Software\Classes\Directory\shell\Note++"
  DeleteRegKey HKCU "Software\Classes\Directory\Background\shell\Note++"

!macroend
