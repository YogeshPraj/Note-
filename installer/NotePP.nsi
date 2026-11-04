; Note++ installer (NSIS). Built by scripts\installer.ps1; see that script for the options.
;
; - Installs per user (no administrator rights needed) into %LOCALAPPDATA%\Programs\Note++, so the
;   in-app updater can replace files without a prompt.
; - Replaces an installed 1.x release (the Electron-based one): it is uninstalled silently first.
;   A machine-wide 1.x install needs administrator rights to remove; only that one step is elevated.
; - Works as the silent update target of the 1.x updater: "NotePP-Setup.exe --updated /S --force-run".

Unicode true
!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"
!include "x64.nsh"
!include "WinMessages.nsh"

!insertmacro GetParameters
!insertmacro GetOptions
!insertmacro GetParent
!insertmacro GetFileName

; ---- build-time settings (scripts\installer.ps1 passes the real values) -----------------------------
!ifndef VERSION
  !define VERSION "0.0.0"
!endif
!ifndef APP_DIR
  !define APP_DIR "..\dist\NotePP"
!endif
!ifndef OUT_FILE
  !define OUT_FILE "..\dist\release\NotePP-Setup-${VERSION}.exe"
!endif
; The uninstall entry of the 1.x app, and the program it runs. Tests point these elsewhere.
!ifndef OLD_UNINST_KEY
  !define OLD_UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\7c34ccc9-73a7-507d-9cd9-b1055b611193"
!endif
!ifndef OLD_EXE_NAME
  !define OLD_EXE_NAME "Note++.exe"
!endif
; Names this installer registers (tests use other names so a real install is never touched).
!ifndef LINK_NAME
  !define LINK_NAME "Note++"
!endif
!ifndef MENU_NAME
  !define MENU_NAME "Note++"
!endif
!ifndef PROGID
  !define PROGID "NotePP.File"
!endif
!ifndef NEW_UNINST_KEY
  !define NEW_UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\NotePP"
!endif
!ifndef APP_KEY
  !define APP_KEY "Software\NotePP"
!endif

!define APP_EXE "notepp.exe"
!ifndef WINDOW_CLASS
  !define WINDOW_CLASS "NoteppHost"
!endif

Name "Note++"
Caption "Note++ ${VERSION} Setup"
OutFile "${OUT_FILE}"
InstallDir "$LOCALAPPDATA\Programs\Note++"
InstallDirRegKey HKCU "${APP_KEY}" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID lzma
BrandingText "Note++ ${VERSION}"
ManifestDPIAware true

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "Note++"
VIAddVersionKey "CompanyName" "Yogesh Prajapati"
VIAddVersionKey "FileDescription" "Note++ Setup"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "Yogesh Prajapati"

!define MUI_ICON "..\assets\app.ico"
!define MUI_UNICON "..\assets\app.ico"
!define MUI_ABORTWARNING
!define MUI_WELCOMEFINISHPAGE_BITMAP "sidebar.bmp"
!define MUI_UNWELCOMEFINISHPAGE_BITMAP "sidebar.bmp"
!define MUI_WELCOMEPAGE_TITLE "Welcome to Note++ ${VERSION}"
!define MUI_WELCOMEPAGE_TEXT "Note++ is rewritten in Rust and built on native code, bringing stronger security, the functionality you rely on and better performance.$\r$\n$\r$\nAn earlier version is replaced automatically. Your open tabs, settings and encryption profile are kept."
!define MUI_FINISHPAGE_RUN "$INSTDIR\${APP_EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "Start Note++"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

; ---- file types: Note++ is offered in "Open with" for these ---------------------------------------
!macro ExtList M
  !insertmacro ${M} "txt"
  !insertmacro ${M} "log"
  !insertmacro ${M} "md"
  !insertmacro ${M} "markdown"
  !insertmacro ${M} "ini"
  !insertmacro ${M} "cfg"
  !insertmacro ${M} "conf"
  !insertmacro ${M} "js"
  !insertmacro ${M} "jsx"
  !insertmacro ${M} "mjs"
  !insertmacro ${M} "cjs"
  !insertmacro ${M} "ts"
  !insertmacro ${M} "tsx"
  !insertmacro ${M} "json"
  !insertmacro ${M} "jsonc"
  !insertmacro ${M} "excalidraw"
  !insertmacro ${M} "html"
  !insertmacro ${M} "htm"
  !insertmacro ${M} "css"
  !insertmacro ${M} "scss"
  !insertmacro ${M} "less"
  !insertmacro ${M} "xml"
  !insertmacro ${M} "svg"
  !insertmacro ${M} "yaml"
  !insertmacro ${M} "yml"
  !insertmacro ${M} "toml"
  !insertmacro ${M} "py"
  !insertmacro ${M} "rb"
  !insertmacro ${M} "php"
  !insertmacro ${M} "java"
  !insertmacro ${M} "c"
  !insertmacro ${M} "h"
  !insertmacro ${M} "cpp"
  !insertmacro ${M} "hpp"
  !insertmacro ${M} "cs"
  !insertmacro ${M} "go"
  !insertmacro ${M} "rs"
  !insertmacro ${M} "swift"
  !insertmacro ${M} "kt"
  !insertmacro ${M} "lua"
  !insertmacro ${M} "sql"
  !insertmacro ${M} "csv"
  !insertmacro ${M} "tsv"
  !insertmacro ${M} "env"
  !insertmacro ${M} "gitignore"
  !insertmacro ${M} "http"
  !insertmacro ${M} "mmd"
  !insertmacro ${M} "mermaid"
  !insertmacro ${M} "ps1"
  !insertmacro ${M} "bat"
!macroend

!macro AssocAdd EXT
  WriteRegStr HKCU "Software\Classes\.${EXT}\OpenWithProgids" "${PROGID}" ""
!macroend

!macro AssocRemove EXT
  DeleteRegValue HKCU "Software\Classes\.${EXT}\OpenWithProgids" "${PROGID}"
!macroend

; ---- helpers ----------------------------------------------------------------------------------------

; Ask a running Note++ to close (it saves its session), then stop it if it does not.
!macro CloseAppFn P
  Function ${P}CloseRunningApp
    StrCpy $R9 0
    ${Do}
      FindWindow $R0 "${WINDOW_CLASS}"
      ${If} $R0 = 0
        ${Break}
      ${EndIf}
      ${If} $R9 = 0
        SendMessage $R0 ${WM_CLOSE} 0 0 $R1 /TIMEOUT=2000
      ${EndIf}
      IntOp $R9 $R9 + 1
      ${If} $R9 > 16
        nsExec::Exec '"$SYSDIR\taskkill.exe" /F /IM ${APP_EXE}'
        Pop $R1
        Sleep 600
        ${Break}
      ${EndIf}
      Sleep 500
    ${Loop}
  FunctionEnd
!macroend
!insertmacro CloseAppFn ""
!insertmacro CloseAppFn "un."

; In: $R0 = a command line. Out: $R2 = the program it starts (the first quoted part, or all of it).
Function ExtractExe
  StrCpy $R2 $R0
  StrCpy $R1 $R0 1
  ${If} $R1 == '"'
    StrCpy $R3 1
    ${Do}
      StrCpy $R1 $R0 1 $R3
      ${If} $R1 == '"'
      ${OrIf} $R1 == ""
        ${Break}
      ${EndIf}
      IntOp $R3 $R3 + 1
    ${Loop}
    IntOp $R4 $R3 - 1
    StrCpy $R2 $R0 $R4 1
  ${EndIf}
FunctionEnd

; Uninstall the 1.x release registered under ROOT (HKCU = this user, HKLM = machine-wide), silently.
!macro RemoveOldFrom ROOT
  ReadRegStr $R0 ${ROOT} "${OLD_UNINST_KEY}" "UninstallString"
  ${If} $R0 != ""
    Call ExtractExe
    ${GetParent} $R2 $R5
    ${If} ${FileExists} $R2
      nsExec::Exec '"$SYSDIR\taskkill.exe" /F /IM "${OLD_EXE_NAME}"'
      Pop $R6
      Sleep 800
      ; "_?=" keeps the uninstaller in place so this waits for it to finish.
      ExecWait '$R0 /S _?=$R5' $R6
      Delete "$R2"
      RMDir "$R5"
    ${EndIf}
    DeleteRegKey ${ROOT} "${OLD_UNINST_KEY}"
  ${EndIf}
!macroend

Function RemoveOldVersions
  ReadRegStr $0 HKLM "${OLD_UNINST_KEY}" "UninstallString"
  ${If} $0 != ""
    System::Call 'shell32::IsUserAnAdmin()i .r1'
    ${If} $1 = 1
      !insertmacro RemoveOldFrom HKLM
    ${Else}
      ; Only this step needs administrator rights: run the installer again, elevated, just for it.
      ExecShellWait "runas" "$EXEPATH" "/UNINSTALLOLD" SW_HIDE
      ReadRegStr $0 HKLM "${OLD_UNINST_KEY}" "UninstallString"
      ${If} $0 != ""
        MessageBox MB_OK|MB_ICONINFORMATION "The previous version of Note++ could not be removed because administrator rights were not granted.$\r$\n$\r$\nYou can remove it later from Settings > Apps." /SD IDOK
      ${EndIf}
    ${EndIf}
  ${EndIf}
  !insertmacro RemoveOldFrom HKCU
FunctionEnd

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_OK|MB_ICONSTOP "Note++ needs 64-bit Windows." /SD IDOK
    Abort
  ${EndIf}
  SetRegView 64
  ClearErrors
  ${GetParameters} $0
  ${GetOptions} $0 "/UNINSTALLOLD" $1
  ${IfNot} ${Errors}
    SetSilent silent
    !insertmacro RemoveOldFrom HKLM
    Quit
  ${EndIf}
FunctionEnd

; After a silent install by the old updater ("--force-run"), start the new app, like a normal update would.
Function .onInstSuccess
  ${If} ${Silent}
    ClearErrors
    ${GetParameters} $0
    ${GetOptions} $0 "--force-run" $1
    ${IfNot} ${Errors}
      Exec '"$INSTDIR\${APP_EXE}"'
    ${EndIf}
  ${EndIf}
FunctionEnd

; ---- install ----------------------------------------------------------------------------------------

Section "Note++" SecMain
  SetShellVarContext current
  SetRegView 64

  ; Keep everything in a folder called Note++, so uninstalling never touches a shared folder.
  StrCpy $1 $INSTDIR 1 -1
  ${If} $1 == "\"
    StrCpy $INSTDIR $INSTDIR -1
  ${EndIf}
  ${GetFileName} $INSTDIR $0
  ${If} $0 != "Note++"
    StrCpy $INSTDIR "$INSTDIR\Note++"
  ${EndIf}

  Call RemoveOldVersions
  Call CloseRunningApp

  SetOutPath "$INSTDIR"
  File /r "${APP_DIR}\*.*"
  WriteUninstaller "$INSTDIR\Uninstall.exe"

  CreateShortCut "$SMPROGRAMS\${LINK_NAME}.lnk" "$INSTDIR\${APP_EXE}" "" "$INSTDIR\${APP_EXE}" 0
  CreateShortCut "$DESKTOP\${LINK_NAME}.lnk" "$INSTDIR\${APP_EXE}" "" "$INSTDIR\${APP_EXE}" 0

  ; The program as Windows lists it in "Open with".
  WriteRegStr HKCU "Software\Classes\${PROGID}" "" "Note++ document"
  WriteRegStr HKCU "Software\Classes\${PROGID}\DefaultIcon" "" "$INSTDIR\${APP_EXE},0"
  WriteRegStr HKCU "Software\Classes\${PROGID}\shell\open\command" "" '"$INSTDIR\${APP_EXE}" "%1"'
  WriteRegStr HKCU "Software\Classes\Applications\${APP_EXE}" "FriendlyAppName" "Note++"
  WriteRegStr HKCU "Software\Classes\Applications\${APP_EXE}\shell\open\command" "" '"$INSTDIR\${APP_EXE}" "%1"'
  !insertmacro ExtList AssocAdd

  ; Explorer context menu.
  WriteRegStr HKCU "Software\Classes\*\shell\${MENU_NAME}" "" "Edit with Note++"
  WriteRegStr HKCU "Software\Classes\*\shell\${MENU_NAME}" "Icon" "$INSTDIR\${APP_EXE},0"
  WriteRegStr HKCU "Software\Classes\*\shell\${MENU_NAME}\command" "" '"$INSTDIR\${APP_EXE}" "%1"'
  WriteRegStr HKCU "Software\Classes\Directory\shell\${MENU_NAME}" "" "Open Folder with Note++"
  WriteRegStr HKCU "Software\Classes\Directory\shell\${MENU_NAME}" "Icon" "$INSTDIR\${APP_EXE},0"
  WriteRegStr HKCU "Software\Classes\Directory\shell\${MENU_NAME}\command" "" '"$INSTDIR\${APP_EXE}" "%V"'
  WriteRegStr HKCU "Software\Classes\Directory\Background\shell\${MENU_NAME}" "" "Open with Note++"
  WriteRegStr HKCU "Software\Classes\Directory\Background\shell\${MENU_NAME}" "Icon" "$INSTDIR\${APP_EXE},0"
  WriteRegStr HKCU "Software\Classes\Directory\Background\shell\${MENU_NAME}\command" "" '"$INSTDIR\${APP_EXE}" "%V"'

  WriteRegStr HKCU "${APP_KEY}" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "${NEW_UNINST_KEY}" "DisplayName" "Note++"
  WriteRegStr HKCU "${NEW_UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${NEW_UNINST_KEY}" "Publisher" "Yogesh Prajapati"
  WriteRegStr HKCU "${NEW_UNINST_KEY}" "DisplayIcon" "$INSTDIR\${APP_EXE}"
  WriteRegStr HKCU "${NEW_UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${NEW_UNINST_KEY}" "URLInfoAbout" "https://github.com/YogeshPraj/Note-"
  WriteRegStr HKCU "${NEW_UNINST_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKCU "${NEW_UNINST_KEY}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
  WriteRegDWORD HKCU "${NEW_UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${NEW_UNINST_KEY}" "NoRepair" 1

  ; Tell Explorer the associations changed.
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
SectionEnd

; ---- uninstall --------------------------------------------------------------------------------------

Section "Uninstall"
  SetShellVarContext current
  SetRegView 64
  Call un.CloseRunningApp

  Delete "$SMPROGRAMS\${LINK_NAME}.lnk"
  Delete "$DESKTOP\${LINK_NAME}.lnk"

  DeleteRegKey HKCU "Software\Classes\${PROGID}"
  DeleteRegKey HKCU "Software\Classes\Applications\${APP_EXE}"
  !insertmacro ExtList AssocRemove
  DeleteRegKey HKCU "Software\Classes\*\shell\${MENU_NAME}"
  DeleteRegKey HKCU "Software\Classes\Directory\shell\${MENU_NAME}"
  DeleteRegKey HKCU "Software\Classes\Directory\Background\shell\${MENU_NAME}"
  DeleteRegKey HKCU "${NEW_UNINST_KEY}"
  DeleteRegKey HKCU "${APP_KEY}"

  ; The whole folder goes only when it is the Note++ folder this installer made.
  ${GetFileName} $INSTDIR $0
  ${If} $0 == "Note++"
    RMDir /r "$INSTDIR"
  ${Else}
    Delete "$INSTDIR\${APP_EXE}"
    Delete "$INSTDIR\Scintilla.dll"
    Delete "$INSTDIR\WebView2Loader.dll"
    RMDir /r "$INSTDIR\whiteboard"
    Delete "$INSTDIR\Uninstall.exe"
    RMDir "$INSTDIR"
  ${EndIf}

  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
SectionEnd
