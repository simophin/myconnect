; The Windows installer: per user (no administrator rights), into
; %LOCALAPPDATA%\Programs\MyConnect, with a Start menu shortcut and an entry
; in Settings → Apps.
;
;   makensis /DVERSION=1.2.3 /DFILE_VERSION=1.2.3.0 /DAPP=path\to\myconnect-gui.exe
;            /DCLI=path\to\myconnect.exe /DOUT=path\to\setup.exe installer.nsi
;
; The app is installed as myConnect.exe and the CLI as cli\myconnect.exe:
; Windows ignores case, so the two can't share a folder (docs/PLAN_ICED_UI.md,
; "Owner decisions").

Unicode true
!include "MUI2.nsh"

!define APP_ID "org.myconnect.MyConnect"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_ID}"

Name "MyConnect"
OutFile "${OUT}"
InstallDir "$LOCALAPPDATA\Programs\MyConnect"
InstallDirRegKey HKCU "${UNINSTALL_KEY}" "InstallLocation"
RequestExecutionLevel user
SetCompressor /SOLID lzma

VIProductVersion "${FILE_VERSION}"
VIAddVersionKey "ProductName" "MyConnect"
VIAddVersionKey "FileDescription" "MyConnect installer"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "Copyright © 2026 MyConnect"

!define MUI_ICON "..\..\assets\windows\app_icon.ico"
!define MUI_UNICON "..\..\assets\windows\app_icon.ico"
!define MUI_FINISHPAGE_RUN "$INSTDIR\myConnect.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Start MyConnect"

!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Section "MyConnect"
  SetOutPath "$INSTDIR"
  File "/oname=myConnect.exe" "${APP}"
  SetOutPath "$INSTDIR\cli"
  File "/oname=myconnect.exe" "${CLI}"
  SetOutPath "$INSTDIR"
  WriteUninstaller "$INSTDIR\uninstall.exe"

  CreateShortcut "$SMPROGRAMS\MyConnect.lnk" "$INSTDIR\myConnect.exe"

  ; The notification id (AUMID) the app sends toasts as, registered the way
  ; Windows documents for apps that aren't packaged.
  WriteRegStr HKCU "Software\Classes\AppUserModelId\${APP_ID}" "DisplayName" "MyConnect"
  WriteRegStr HKCU "Software\Classes\AppUserModelId\${APP_ID}" "IconUri" "$INSTDIR\myConnect.exe"

  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "MyConnect"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\myConnect.exe"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "MyConnect"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "URLInfoAbout" "https://github.com/simophin/myconnect"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
SectionEnd

; Leaves the data (identity, trust, settings) and received files alone.
Section "Uninstall"
  Delete "$SMPROGRAMS\MyConnect.lnk"
  Delete "$INSTDIR\myConnect.exe"
  Delete "$INSTDIR\cli\myconnect.exe"
  RMDir "$INSTDIR\cli"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  DeleteRegKey HKCU "Software\Classes\AppUserModelId\${APP_ID}"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"
SectionEnd
