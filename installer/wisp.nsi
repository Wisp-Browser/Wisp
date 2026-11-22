; Wisp installer (NSIS). Per-user, no admin rights needed.
;
; Build (from the repo root):
;   makensis -DVERSION=0.1.0 -DEXE_PATH=..\target\release\Wisp.exe -DOUT_FILE=..\dist\Wisp-0.1.0-setup.exe installer\wisp.nsi
;
; Silent install flags:  /S  /NODESKTOP  /DEFAULTBROWSER  /OLLAMA  /D=C:\custom\path
; Silent uninstall:      Uninstall.exe /S [/DELETEDATA]

Unicode true
ManifestDPIAware true
SetFont "Segoe UI" 9
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
BrandingText " "
; go straight on to the finish page when the files are in
AutoCloseWindow true
ShowInstDetails nevershow
ShowUninstDetails nevershow
CRCCheck on

;; The Windows file version must be numeric (X.X.X.X): drop any pre-release suffix such as "-beta.1".
!searchparse /noerrors "${VERSION}-" "" VERSION_NUM "-"
VIProductVersion "${VERSION_NUM}.0"
VIAddVersionKey "ProductName" "Wisp"
VIAddVersionKey "FileDescription" "Wisp installer"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "MIT License"

; ---------------------------------------------------------------- look
; The installer wears the app's dark theme (ui/theme.css): flat #18181E background, #F2F2F7 text,
; the indigo accent, Segoe UI, and on/off switches drawn like the ones in Settings. The artwork
; (header, logo, switches) is drawn at 2x so it stays sharp on high-DPI screens.
!define C_BG "18181E"
!define C_FG "F2F2F7"
!define C_SUB "A6A6B0"
!define C_LINE "2A2A34"

!define MUI_ICON "..\assets\icon.ico"
!define MUI_UNICON "..\assets\icon.ico"
!define MUI_HEADERIMAGE
!define MUI_HEADERIMAGE_BITMAP "header.bmp"
!define MUI_HEADERIMAGE_UNBITMAP "header.bmp"
!define MUI_HEADERIMAGE_RIGHT
!define MUI_BGCOLOR "${C_BG}"
!define MUI_TEXTCOLOR "${C_FG}"
!define MUI_INSTFILESPAGE_COLORS "C8C8D0 ${C_BG}"
!define MUI_INSTFILESPAGE_PROGRESSBAR "smooth"
!define MUI_ABORTWARNING
!define MUI_CUSTOMFUNCTION_GUIINIT ThemeWindow
!define MUI_CUSTOMFUNCTION_UNGUIINIT un.ThemeWindow

Var Dlg
Var DesktopShortcut
Var SetDefault
Var InstallOllama
Var LaunchNow
Var OptionsShown
Var FontTitle
Var FontBig
; each switch row: the switch, its two labels, and the switch's current image
Var SwDesk
Var LbDesk1
Var LbDesk2
Var ImDesk
Var SwDef
Var LbDef1
Var LbDef2
Var ImDef
Var SwOll
Var LbOll1
Var LbOll2
Var ImOll
Var SwRun
Var LbRun1
Var ImRun
Var ImHero

; One options page, then the install runs: close to "one click".
Page custom OptionsPage OptionsLeave
!define MUI_PAGE_CUSTOMFUNCTION_SHOW InstFilesShow
!define MUI_PAGE_CUSTOMFUNCTION_LEAVE InstFilesLeave
!insertmacro MUI_PAGE_INSTFILES
Page custom FinishPage FinishLeave

!define MUI_PAGE_CUSTOMFUNCTION_SHOW un.ConfirmShow
!insertmacro MUI_UNPAGE_CONFIRM
!define MUI_PAGE_CUSTOMFUNCTION_SHOW un.InstFilesShow
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

; ---------------------------------------------------------------- theme helpers
!macro HIDE_ITEM ID
  GetDlgItem $0 $HWNDPARENT ${ID}
  ShowWindow $0 ${SW_HIDE}
!macroend
; Swap a push button for a flat one in the app colours: a centred, clickable label with
; the same control ID, so setup treats it exactly like the button it replaces (text, enabling,
; clicks, Enter and Esc). Windows' own dark buttons have a bright outline.
!macro FLAT_BUTTON ID BG
  GetDlgItem $0 $HWNDPARENT ${ID}
  System::Call '*(i,i,i,i)p.r2'
  System::Call 'user32::GetWindowRect(p r0, p r2)'
  System::Call 'user32::MapWindowPoints(p 0, p $HWNDPARENT, p r2, i 2)'
  System::Call '*$2(i.r3,i.r4,i.r5,i.r6)'
  System::Free $2
  IntOp $5 $5 - $3
  IntOp $6 $6 - $4
  System::Call 'user32::GetWindowTextW(p r0, w .r7, i 256)'
  SendMessage $0 ${WM_GETFONT} 0 0 $8
  ; the button's own WS_VISIBLE style (the window itself isn't shown yet)
  System::Call 'user32::GetWindowLongW(p r0, i -16) i.r9'
  IntOp $9 $9 & 0x10000000
  System::Call 'user32::DestroyWindow(p r0)'
  ; WS_CHILD|WS_VISIBLE|SS_CENTERIMAGE|SS_NOTIFY|SS_CENTER
  System::Call 'user32::CreateWindowExW(i 0, w "STATIC", w r7, i 0x50000301, i r3, i r4, i r5, i r6, p $HWNDPARENT, p ${ID}, p 0, p 0) p.r0'
  ${If} $9 == 0
  ${OrIf} ${ID} == 3
    ShowWindow $0 ${SW_HIDE}
  ${EndIf}
  SendMessage $0 ${WM_SETFONT} $8 1
  SetCtlColors $0 FFFFFF ${BG}
!macroend
; Windows' own dark style for scroll bars and list views.
!macro DARK_ITEM PARENT ID
  GetDlgItem $0 ${PARENT} ${ID}
  System::Call 'uxtheme::SetWindowTheme(p r0, w "DarkMode_Explorer", p 0)'
!macroend
; Dark title bar, dark buttons, and the window's own background and labels in the app colours.
!macro THEME_WINDOW
  SetCtlColors $HWNDPARENT ${C_FG} ${C_BG}
  System::Call 'dwmapi::DwmSetWindowAttribute(p $HWNDPARENT, i 20, *i 1, i 4)'
  ; branding text and the etched separator lines don't belong in a flat dark window
  !insertmacro HIDE_ITEM 1028
  !insertmacro HIDE_ITEM 1256
  !insertmacro HIDE_ITEM 1035
  !insertmacro HIDE_ITEM 1036
  !insertmacro HIDE_ITEM 1045
  !insertmacro FLAT_BUTTON 1 5E5CE6
  !insertmacro FLAT_BUTTON 2 2A2A34
  !insertmacro FLAT_BUTTON 3 2A2A34
  CreateFont $FontTitle "Segoe UI Semibold" 10 600
  CreateFont $FontBig "Segoe UI Semibold" 15 600
!macroend
; The progress page: dark page, indigo progress bar on a dark track.
!macro THEME_INSTFILES
  GetDlgItem $0 $HWNDPARENT 1
  ShowWindow $0 ${SW_HIDE}
  GetDlgItem $0 $HWNDPARENT 2
  ShowWindow $0 ${SW_HIDE}
  GetDlgItem $0 $HWNDPARENT 3
  ShowWindow $0 ${SW_HIDE}
  FindWindow $1 "#32770" "" $HWNDPARENT
  SetCtlColors $1 ${C_FG} ${C_BG}
  GetDlgItem $0 $1 1006
  SetCtlColors $0 ${C_FG} ${C_BG}
  GetDlgItem $0 $1 1004
  System::Call 'uxtheme::SetWindowTheme(p r0, w " ", w " ")'
  SendMessage $0 0x0409 0 0x00E65C5E ; PBM_SETBARCOLOR #5E5CE6
  SendMessage $0 0x2001 0 0x00342A2A ; PBM_SETBKCOLOR #2A2A34
  !insertmacro DARK_ITEM $1 1016
  !insertmacro DARK_ITEM $1 1027
!macroend
; Fill the progress bar (setup's range is 0-30000) and hold it there for a moment, so the page
; visibly finishes before it moves on.
!macro FILL_PROGRESS
  FindWindow $1 "#32770" "" $HWNDPARENT
  GetDlgItem $0 $1 1004
  SendMessage $0 0x0402 30000 0 ; PBM_SETPOS
  System::Call 'user32::UpdateWindow(p r0)'
!macroend
; A label in the page colours (COLOR is the text colour, FONT 0 keeps the default).
!macro PAGE_LABEL VAR X Y W H TEXT COLOR FONT
  ${NSD_CreateLabel} ${X} ${Y} ${W} ${H} "${TEXT}"
  Pop ${VAR}
  SetCtlColors ${VAR} ${COLOR} ${C_BG}
  StrCmp ${FONT} 0 +2
    SendMessage ${VAR} ${WM_SETFONT} ${FONT} 1
!macroend
!macro SHOW_SWITCH SW IM STATE
  ${NSD_FreeImage} ${IM}
  ${If} ${STATE} == 1
    ${NSD_SetStretchedImage} ${SW} "$PLUGINSDIR\sw-on.bmp" ${IM}
  ${Else}
    ${NSD_SetStretchedImage} ${SW} "$PLUGINSDIR\sw-off.bmp" ${IM}
  ${EndIf}
!macroend
; One settings row: a title, a description, and a switch on the right. Clicking the row's text or
; the switch flips it.
!macro SWITCH_ROW Y DY TITLE DESC SW LB1 LB2 IM STATE
  !insertmacro PAGE_LABEL ${LB1} 0 ${Y} 252u 11u "${TITLE}" ${C_FG} $FontTitle
  ${NSD_AddStyle} ${LB1} ${SS_NOTIFY}
  ${NSD_OnClick} ${LB1} OnSwitch
  !insertmacro PAGE_LABEL ${LB2} 0 ${DY} 252u 20u "${DESC}" ${C_SUB} 0
  ${NSD_AddStyle} ${LB2} ${SS_NOTIFY}
  ${NSD_OnClick} ${LB2} OnSwitch
  ${NSD_CreateBitmap} 274u ${Y} 22u 12u ""
  Pop ${SW}
  ${NSD_AddStyle} ${SW} ${SS_NOTIFY}
  ${NSD_OnClick} ${SW} OnSwitch
  !insertmacro SHOW_SWITCH ${SW} ${IM} ${STATE}
!macroend
!macro DIVIDER Y
  ${NSD_CreateLabel} 0 ${Y} 100% 1 ""
  Pop $0
  SetCtlColors $0 "" ${C_LINE}
!macroend

Function ThemeWindow
  !insertmacro THEME_WINDOW
FunctionEnd
Function un.ThemeWindow
  !insertmacro THEME_WINDOW
FunctionEnd
Function InstFilesShow
  !insertmacro MUI_HEADER_TEXT "Installing Wisp" "This only takes a few seconds."
  !insertmacro THEME_INSTFILES
FunctionEnd
Function un.InstFilesShow
  !insertmacro THEME_INSTFILES
FunctionEnd
; The bar follows setup's steps, so steps that are skipped (Ollama, say) leave it short of the end.
; Fill it once everything is done and let it show for a moment before the finish page.
Function InstFilesLeave
  !insertmacro FILL_PROGRESS
  Sleep 500
FunctionEnd
Function un.onUninstSuccess
  !insertmacro FILL_PROGRESS
  Sleep 500
FunctionEnd
Function un.ConfirmShow
  FindWindow $1 "#32770" "" $HWNDPARENT
  SetCtlColors $1 ${C_FG} ${C_BG}
  GetDlgItem $0 $1 1006
  SetCtlColors $0 ${C_FG} ${C_BG}
  GetDlgItem $0 $1 1029
  SetCtlColors $0 ${C_SUB} ${C_BG}
  GetDlgItem $0 $1 1000
  SetCtlColors $0 ${C_FG} 202028
  System::Call 'uxtheme::SetWindowTheme(p r0, w "DarkMode_CFD", p 0)'
  SendMessage $0 0x00B1 0 0 ; EM_SETSEL: no highlighted text
FunctionEnd

; ---------------------------------------------------------------- options page
Function .onInit
  InitPluginsDir
  File "/oname=$PLUGINSDIR\sw-on.bmp" "sw-on.bmp"
  File "/oname=$PLUGINSDIR\sw-off.bmp" "sw-off.bmp"
  File "/oname=$PLUGINSDIR\hero.bmp" "hero.bmp"
  StrCpy $LaunchNow 1
  StrCpy $OptionsShown 0
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
  !insertmacro MUI_HEADER_TEXT "Install Wisp ${VERSION}" "Fast, private and free. Installs just for you, no admin rights needed."
  ; Interactive installs start with everything on (silent ones use the /flags instead).
  ${If} $OptionsShown == 0
    StrCpy $OptionsShown 1
    StrCpy $DesktopShortcut 1
    StrCpy $SetDefault 1
    StrCpy $InstallOllama 1
  ${EndIf}
  nsDialogs::Create 1018
  Pop $Dlg
  ${If} $Dlg == error
    Abort
  ${EndIf}
  SetCtlColors $Dlg ${C_FG} ${C_BG}

  !insertmacro SWITCH_ROW 4u 17u "Desktop shortcut" "Put Wisp on your desktop as well as in the Start menu." $SwDesk $LbDesk1 $LbDesk2 $ImDesk $DesktopShortcut
  !insertmacro DIVIDER 42u
  !insertmacro SWITCH_ROW 50u 63u "Make Wisp my default browser" "Windows asks you to confirm this yourself, so Default apps opens when setup finishes." $SwDef $LbDef1 $LbDef2 $ImDef $SetDefault
  !insertmacro DIVIDER 88u
  !insertmacro SWITCH_ROW 96u 109u "Private AI on this PC (Ollama)" "A free extra (about 1 GB) that runs Wisp AI locally, with no account or key. Skipped if you already have it." $SwOll $LbOll1 $LbOll2 $ImOll $InstallOllama

  ; the Next button becomes Install
  GetDlgItem $0 $HWNDPARENT 1
  SendMessage $0 ${WM_SETTEXT} 0 "STR:Install"
  nsDialogs::Show
  ${NSD_FreeImage} $ImDesk
  ${NSD_FreeImage} $ImDef
  ${NSD_FreeImage} $ImOll
FunctionEnd

Function OnSwitch
  Pop $9
  ${If} $9 == $SwDesk
  ${OrIf} $9 == $LbDesk1
  ${OrIf} $9 == $LbDesk2
    IntOp $DesktopShortcut $DesktopShortcut ^ 1
    !insertmacro SHOW_SWITCH $SwDesk $ImDesk $DesktopShortcut
  ${ElseIf} $9 == $SwDef
  ${OrIf} $9 == $LbDef1
  ${OrIf} $9 == $LbDef2
    IntOp $SetDefault $SetDefault ^ 1
    !insertmacro SHOW_SWITCH $SwDef $ImDef $SetDefault
  ${ElseIf} $9 == $SwOll
  ${OrIf} $9 == $LbOll1
  ${OrIf} $9 == $LbOll2
    IntOp $InstallOllama $InstallOllama ^ 1
    !insertmacro SHOW_SWITCH $SwOll $ImOll $InstallOllama
  ${ElseIf} $9 == $SwRun
  ${OrIf} $9 == $LbRun1
    IntOp $LaunchNow $LaunchNow ^ 1
    !insertmacro SHOW_SWITCH $SwRun $ImRun $LaunchNow
  ${EndIf}
FunctionEnd

Function OptionsLeave
FunctionEnd

; ---------------------------------------------------------------- finish page
Function FinishPage
  !insertmacro MUI_HEADER_TEXT "All done" "Wisp is installed and ready to go."
  nsDialogs::Create 1018
  Pop $Dlg
  ${If} $Dlg == error
    Abort
  ${EndIf}
  SetCtlColors $Dlg ${C_FG} ${C_BG}

  ${NSD_CreateBitmap} 0 2u 40u 40u ""
  Pop $0
  ${NSD_SetStretchedImage} $0 "$PLUGINSDIR\hero.bmp" $ImHero
  !insertmacro PAGE_LABEL $1 50u 7u 240u 16u "Wisp is ready" ${C_FG} $FontBig
  !insertmacro PAGE_LABEL $1 50u 26u 240u 20u "Find it in the Start menu. It keeps itself up to date." ${C_SUB} 0
  !insertmacro DIVIDER 54u
  !insertmacro PAGE_LABEL $LbRun1 0 64u 252u 11u "Open Wisp now" ${C_FG} $FontTitle
  ${NSD_AddStyle} $LbRun1 ${SS_NOTIFY}
  ${NSD_OnClick} $LbRun1 OnSwitch
  ${NSD_CreateBitmap} 274u 64u 22u 12u ""
  Pop $SwRun
  ${NSD_AddStyle} $SwRun ${SS_NOTIFY}
  ${NSD_OnClick} $SwRun OnSwitch
  !insertmacro SHOW_SWITCH $SwRun $ImRun $LaunchNow

  ; just "Finish": no going back, nothing left to cancel
  GetDlgItem $0 $HWNDPARENT 1
  SendMessage $0 ${WM_SETTEXT} 0 "STR:Finish"
  GetDlgItem $0 $HWNDPARENT 3
  ShowWindow $0 ${SW_HIDE}
  ; Finish takes Cancel's place in the corner
  GetDlgItem $1 $HWNDPARENT 2
  System::Call '*(i,i,i,i)p.r2'
  System::Call 'user32::GetWindowRect(p r1, p r2)'
  System::Call 'user32::MapWindowPoints(p 0, p $HWNDPARENT, p r2, i 2)'
  System::Call '*$2(i.r3,i.r4,i,i)'
  System::Free $2
  ShowWindow $1 ${SW_HIDE}
  GetDlgItem $0 $HWNDPARENT 1
  System::Call 'user32::SetWindowPos(p r0, p 0, i r3, i r4, i 0, i 0, i 0x55)'
  System::Call 'user32::InvalidateRect(p $HWNDPARENT, p 0, i 1)'
  nsDialogs::Show
  ${NSD_FreeImage} $ImHero
  ${NSD_FreeImage} $ImRun
FunctionEnd

Function FinishLeave
!ifndef PREVIEW
  ${If} $LaunchNow == 1
    Exec '"$INSTDIR\Wisp.exe"'
  ${EndIf}
!endif
FunctionEnd

; ---------------------------------------------------------------- install
Section "Install"
!ifdef PREVIEW
  ; Screenshot build: walks through the progress page without installing anything.
  DetailPrint "Preview build: nothing is installed."
  WriteUninstaller "$TEMP\wisp-preview-uninstall.exe"
  StrCpy $2 0
  preview_loop:
    Sleep 100
    IntOp $2 $2 + 1
    IntCmp $2 20 0 preview_loop 0
  Return
!endif
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
  SetAutoClose true
  StrCpy $DeleteData 0
  ${GetParameters} $R0
  ${GetOptions} $R0 "/DELETEDATA" $R1
  ${IfNot} ${Errors}
    StrCpy $DeleteData 1
  ${EndIf}
  ClearErrors
FunctionEnd

Section "Uninstall"
!ifdef PREVIEW
  DetailPrint "Preview build: nothing is removed."
  Sleep 800
  Return
!endif
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
