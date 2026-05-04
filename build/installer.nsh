; installer.nsh — context menu registration for Note++

!macro customInstall
  ; "Open with Note++" on any file (background + file)
  WriteRegStr HKCU "Software\Classes\*\shell\Note++"          ""          "Open with Note++"
  WriteRegStr HKCU "Software\Classes\*\shell\Note++"          "Icon"      "$INSTDIR\Note++.exe,0"
  WriteRegStr HKCU "Software\Classes\*\shell\Note++\command"  ""          '"$INSTDIR\Note++.exe" "%1"'

  ; "Open with Note++" on folders (directory background)
  WriteRegStr HKCU "Software\Classes\Directory\shell\Note++"         ""         "Open Folder with Note++"
  WriteRegStr HKCU "Software\Classes\Directory\shell\Note++"         "Icon"     "$INSTDIR\Note++.exe,0"
  WriteRegStr HKCU "Software\Classes\Directory\shell\Note++\command" ""         '"$INSTDIR\Note++.exe" "%V"'

  ; "Open with Note++" on directory background (right-click inside folder)
  WriteRegStr HKCU "Software\Classes\Directory\Background\shell\Note++"         ""         "Open Folder with Note++"
  WriteRegStr HKCU "Software\Classes\Directory\Background\shell\Note++"         "Icon"     "$INSTDIR\Note++.exe,0"
  WriteRegStr HKCU "Software\Classes\Directory\Background\shell\Note++\command" ""         '"$INSTDIR\Note++.exe" "%V"'
!macroend

!macro customUninstall
  DeleteRegKey HKCU "Software\Classes\*\shell\Note++"
  DeleteRegKey HKCU "Software\Classes\Directory\shell\Note++"
  DeleteRegKey HKCU "Software\Classes\Directory\Background\shell\Note++"
!macroend
