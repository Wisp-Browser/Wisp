; Wisp installer (NSIS). Per-user, no admin rights needed.
;
; Build (from the repo root):
;   makensis -DVERSION=0.1.0 -DEXE_PATH=..\target\release\Wisp.exe -DOUT_FILE=..\dist\Wisp-0.1.0-setup.exe installer\wisp.nsi
;
; Silent install flags:  /S  /NODESKTOP  /DEFAULTBROWSER  /OLLAMA  /D=C:\custom\path
; Silent uninstall:      Uninstall.exe /S [/DELETEDATA]

Unicode true
!include "MUI2.nsh"
!include "nsDialogs.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"
!include "WinMessages.nsh"

!ifndef VERSION
  !define VERSION "0.0.0"
!endif
!ifndef EXE_PATH
  !define EXE_PATH "..\target\release\Wisp.exe"
!endif
!ifndef OUT_FILE
  !define OUT_FILE "..\dist\Wisp-${VERSION}-setup.exe"
!endif

!define APP_NAME "Wisp"
!define PUBLISHER "Wisp"
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Wisp"
!define CAP_KEY "Software\Clients\StartMenuInternet\Wisp"

Name "${APP_NAME}"
OutFile "${OUT_FILE}"
RequestExecutionLevel user
InstallDir "$LOCALAPPDATA\Programs\Wisp"
InstallDirRegKey HKCU "${UNINST_KEY}" "InstallLocation"
SetCompressor /SOLID lzma
BrandingText "Wisp ${VERSION}"
ShowInstDetails show
ShowUninstDetails show
CRCCheck on

;; The Windows file version must be numeric (X.X.X.X): drop any pre-release suffix such as "-beta.1".
!searchparse /noerrors "${VERSION}-" "" VERSION_NUM "-"
VIProductVersion "${VERSION_NUM}.0"
VIAddVersionKey "ProductName" "Wisp"
VIAddVersionKey "FileDescription" "Wisp installer"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "MIT License"

!define MUI_ICON "..\assets\icon.ico"
!define MUI_UNICON "..\assets\icon.ico"
!define MUI_HEADERIMAGE
!define MUI_HEADERIMAGE_BITMAP "header.bmp"
!define MUI_HEADERIMAGE_RIGHT
!define MUI_WELCOMEFINISHPAGE_BITMAP "sidebar.bmp"
!define MUI_UNWELCOMEFINISHPAGE_BITMAP "sidebar.bmp"
!define MUI_ABORTWARNING

Var Dlg
Var CbDesktop
Var CbDefault
Var CbOllama
Var DesktopShortcut
Var SetDefault
Var InstallOllama

; One options page, then the install runs: close to "one click".
Page custom OptionsPage OptionsLeave
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_TITLE "Wisp is ready"
!define MUI_FINISHPAGE_RUN "$INSTDIR\Wisp.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Launch Wisp"
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

; ---------------------------------------------------------------- options page
Function .onInit
  ; defaults (also used for silent installs)
  StrCpy $DesktopShortcut 1
  StrCpy $SetDefault 0
  StrCpy $InstallOllama 0
  ${GetParameters} $R0
  ${GetOptions} $R0 "/NODESKTOP" $R1
  ${IfNot} ${Errors}
    StrCpy $DesktopShortcut 0
  ${EndIf}
  ClearErrors
  ${GetOptions} $R0 "/DEFAULTBROWSER" $R1
  ${IfNot} ${Errors}
    StrCpy $SetDefault 1
  ${EndIf}
  ClearErrors
  ${GetOptions} $R0 "/OLLAMA" $R1
  ${IfNot} ${Errors}
    StrCpy $InstallOllama 1
  ${EndIf}
  ClearErrors
FunctionEnd

Function OptionsPage
  !insertmacro MUI_HEADER_TEXT "Install Wisp ${VERSION}" "Choose what to set up. You can change all of this later."
  nsDialogs::Create 1018
  Pop $Dlg
  ${If} $Dlg == error
    Abort
  ${EndIf}

  ${NSD_CreateLabel} 0 0 100% 20u "Wisp installs just for your account, so no administrator rights are needed."
  Pop $0

  ${NSD_CreateCheckbox} 0 28u 100% 12u "Create a Desktop shortcut"
  Pop $CbDesktop
  ${NSD_Check} $CbDesktop

  ${NSD_CreateCheckbox} 0 46u 100% 12u "Make Wisp my default browser"
  Pop $CbDefault
  ${NSD_Check} $CbDefault
  ${NSD_CreateLabel} 12u 59u 94% 20u "Windows asks you to confirm this yourself, so Default apps opens when setup finishes."
  Pop $0

  ${NSD_CreateCheckbox} 0 84u 100% 12u "Install Ollama for private AI that runs on this PC (about 1 GB download)"
  Pop $CbOllama
  ${NSD_Check} $CbOllama
  ${NSD_CreateLabel} 12u 97u 94% 30u "Ollama is a separate free program. It lets the AI sidebar run models locally with no account or key. It is skipped if you already have it."
  Pop $0

  ; the Next button becomes Install
  GetDlgItem $0 $HWNDPARENT 1
  SendMessage $0 ${WM_SETTEXT} 0 "STR:Install"
  nsDialogs::Show
FunctionEnd

Function OptionsLeave
  ${NSD_GetState} $CbDesktop $DesktopShortcut
  ${NSD_GetState} $CbDefault $SetDefault
  ${NSD_GetState} $CbOllama $InstallOllama
FunctionEnd

; ---------------------------------------------------------------- install
Section "Install"
  SetShellVarContext current
  SetOutPath "$INSTDIR"

  ; A running Wisp can be renamed but not overwritten, so move it aside first.
  IfFileExists "$INSTDIR\Wisp.exe" 0 +4
    Delete "$INSTDIR\Wisp.exe.old"
    ClearErrors
    Rename "$INSTDIR\Wisp.exe" "$INSTDIR\Wisp.exe.old"

  File "${EXE_PATH}"
  File "/oname=LICENSE.txt" "..\LICENSE"
  WriteUninstaller "$INSTDIR\Uninstall.exe"

  ; shortcuts
  CreateShortcut "$SMPROGRAMS\Wisp.lnk" "$INSTDIR\Wisp.exe" "" "$INSTDIR\Wisp.exe" 0
  ${If} $DesktopShortcut == 1
    CreateShortcut "$DESKTOP\Wisp.lnk" "$INSTDIR\Wisp.exe" "" "$INSTDIR\Wisp.exe" 0
  ${EndIf}

  ; Apps & features entry
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "Wisp"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "${PUBLISHER}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\Wisp.exe"
  WriteRegStr HKCU "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKCU "${UNINST_KEY}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
  WriteRegStr HKCU "${UNINST_KEY}" "URLInfoAbout" "https://github.com/Wisp-Browser/Wisp"
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKCU "${UNINST_KEY}" "EstimatedSize" "$0"

  ; browser registration: makes Wisp appear under Settings > Default apps
  WriteRegStr HKCU "Software\Classes\WispHTML" "" "Wisp HTML Document"
  WriteRegStr HKCU "Software\Classes\WispHTML\DefaultIcon" "" "$INSTDIR\Wisp.exe,0"
  WriteRegStr HKCU "Software\Classes\WispHTML\shell\open\command" "" '"$INSTDIR\Wisp.exe" "%1"'
  WriteRegStr HKCU "Software\Classes\WispURL" "" "Wisp URL"
  WriteRegStr HKCU "Software\Classes\WispURL" "URL Protocol" ""
  WriteRegStr HKCU "Software\Classes\WispURL\DefaultIcon" "" "$INSTDIR\Wisp.exe,0"
  WriteRegStr HKCU "Software\Classes\WispURL\shell\open\command" "" '"$INSTDIR\Wisp.exe" "%1"'
  WriteRegStr HKCU "${CAP_KEY}" "" "Wisp"
  WriteRegStr HKCU "${CAP_KEY}\DefaultIcon" "" "$INSTDIR\Wisp.exe,0"
  WriteRegStr HKCU "${CAP_KEY}\shell\open\command" "" '"$INSTDIR\Wisp.exe"'
  WriteRegStr HKCU "${CAP_KEY}\Capabilities" "ApplicationName" "Wisp"
  WriteRegStr HKCU "${CAP_KEY}\Capabilities" "ApplicationDescription" "A light, glassy browser built on the system web engine."
  WriteRegStr HKCU "${CAP_KEY}\Capabilities" "ApplicationIcon" "$INSTDIR\Wisp.exe,0"
  WriteRegStr HKCU "${CAP_KEY}\Capabilities\FileAssociations" ".htm" "WispHTML"
  WriteRegStr HKCU "${CAP_KEY}\Capabilities\FileAssociations" ".html" "WispHTML"
  WriteRegStr HKCU "${CAP_KEY}\Capabilities\FileAssociations" ".xhtml" "WispHTML"
  WriteRegStr HKCU "${CAP_KEY}\Capabilities\URLAssociations" "http" "WispURL"
  WriteRegStr HKCU "${CAP_KEY}\Capabilities\URLAssociations" "https" "WispURL"
  WriteRegStr HKCU "Software\RegisteredApplications" "Wisp" "Software\Clients\StartMenuInternet\Wisp\Capabilities"
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'

  ${If} $InstallOllama == 1
    Call InstallOllama
  ${EndIf}

  ${If} $SetDefault == 1
  ${AndIfNot} ${Silent}
    ExecShell "open" "ms-settings:defaultapps"
  ${EndIf}
SectionEnd

Function InstallOllama
  IfFileExists "$LOCALAPPDATA\Programs\Ollama\ollama.exe" 0 download
    DetailPrint "Ollama is already installed. Skipping."
    Return
  download:
  DetailPrint "Downloading Ollama (large download, this can take a while)..."
  StrCpy $0 "$TEMP\OllamaSetup.exe"
  Delete "$0"
  nsExec::ExecToLog `powershell -NoProfile -ExecutionPolicy Bypass -Command "[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; $$ProgressPreference = 'SilentlyContinue'; Invoke-WebRequest -UseBasicParsing -Uri 'https://ollama.com/download/OllamaSetup.exe' -OutFile '$0'"`
  Pop $1
  ${If} $1 != 0
    DetailPrint "Couldn't download Ollama (code $1). You can install it later from ollama.com."
    Return
  ${EndIf}
  IfFileExists "$0" +3 0
    DetailPrint "Couldn't download Ollama. You can install it later from ollama.com."
    Return
  DetailPrint "Installing Ollama..."
  ExecWait '"$0" /SILENT /NORESTART' $1
  DetailPrint "Ollama setup finished (code $1)."
  Delete "$0"
FunctionEnd

; ---------------------------------------------------------------- uninstall
Var DeleteData

Function un.onInit
  StrCpy $DeleteData 0
  ${GetParameters} $R0
  ${GetOptions} $R0 "/DELETEDATA" $R1
  ${IfNot} ${Errors}
    StrCpy $DeleteData 1
  ${EndIf}
  ClearErrors
FunctionEnd

Section "Uninstall"
  SetShellVarContext current

  ; Ask about user data unless this is a silent uninstall. Keeping it is the default.
  ${IfNot} ${Silent}
    MessageBox MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON2 "Also delete your Wisp data (bookmarks, history, settings and browser profiles)?$\r$\n$\r$\nChoose No to keep it for a future install." IDYES +2
    Goto +2
    StrCpy $DeleteData 1
  ${EndIf}

  Delete "$INSTDIR\Wisp.exe"
  Delete "$INSTDIR\Wisp.exe.old"
  Delete "$INSTDIR\Wisp.exe.prev"
  Delete "$INSTDIR\Wisp.exe.bad"
  Delete "$INSTDIR\LICENSE.txt"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"

  Delete "$SMPROGRAMS\Wisp.lnk"
  Delete "$DESKTOP\Wisp.lnk"

  ; registry: uninstall entry and every default-browser registration
  DeleteRegKey HKCU "${UNINST_KEY}"
  DeleteRegKey HKCU "Software\Classes\WispHTML"
  DeleteRegKey HKCU "Software\Classes\WispURL"
  DeleteRegKey HKCU "${CAP_KEY}"
  DeleteRegValue HKCU "Software\RegisteredApplications" "Wisp"
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'

  ${If} $DeleteData == 1
    RMDir /r "$APPDATA\Wisp"
  ${EndIf}
SectionEnd
