; Builds TheJar-Setup-<version>.exe. The release workflow runs:
;
;   makensis -DVERSION=0.1.0 -DFILEVERSION=0.1.0.0 -DEXE=path/to/thejar.exe -DOUTDIR=dist installer.nsi
;
; It installs for the current user only, so it never asks for admin rights. /S installs
; silently, and --run opens The Jar afterwards, which is how The Jar updates itself.

Unicode true
ManifestDPIAware true
SetCompressor /SOLID lzma
RequestExecutionLevel user

!ifndef VERSION | FILEVERSION | EXE
  !error "Pass -DVERSION, -DFILEVERSION and -DEXE"
!endif
!ifndef OUTDIR
  !define OUTDIR "."
!endif

!define NAME "The Jar"
!define APP_EXE "TheJar.exe"
!define PUBLISHER "Joseph Cook"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\TheJar"
!define ROOT "${__FILEDIR__}/../.."

Name "${NAME}"
OutFile "${OUTDIR}/TheJar-Setup-${VERSION}.exe"
InstallDir "$LOCALAPPDATA\Programs\${NAME}"
InstallDirRegKey HKCU "${UNINSTALL_KEY}" "InstallLocation"
BrandingText "${NAME} ${VERSION}"

VIProductVersion "${FILEVERSION}"
VIAddVersionKey "ProductName" "${NAME}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "FileDescription" "${NAME} installer"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "CompanyName" "${PUBLISHER}"
VIAddVersionKey "LegalCopyright" "Copyright 2026 ${PUBLISHER}"

!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"

!define MUI_ICON "${ROOT}/assets/icon.ico"
!define MUI_UNICON "${ROOT}/assets/icon.ico"
!define MUI_FINISHPAGE_RUN "$INSTDIR\${APP_EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "Open ${NAME}"

!insertmacro MUI_PAGE_LICENSE "${ROOT}/LICENSE.md"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Section "Install"
  SetOutPath "$INSTDIR"

  ; When The Jar updates itself it starts this installer and then quits, so its exe can stay
  ; locked for a moment. Wait up to 15 seconds for it rather than failing straight away.
  StrCpy $0 0
  wait_for_exit:
    ClearErrors
    Delete "$INSTDIR\${APP_EXE}"
    IfErrors 0 copy_files
    IntOp $0 $0 + 1
    IntCmp $0 60 still_running
    Sleep 250
    Goto wait_for_exit
  still_running:
    MessageBox MB_OK|MB_ICONEXCLAMATION "${NAME} is still open. Close it, then run this installer again." /SD IDOK
    Abort

  copy_files:
    File "/oname=${APP_EXE}" "${EXE}"
    File "/oname=LICENSE.md" "${ROOT}/LICENSE.md"
    WriteUninstaller "$INSTDIR\Uninstall.exe"

    CreateShortcut "$SMPROGRAMS\${NAME}.lnk" "$INSTDIR\${APP_EXE}"
    ${IfNot} ${Silent}
      CreateShortcut "$DESKTOP\${NAME}.lnk" "$INSTDIR\${APP_EXE}"
    ${EndIf}

    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "${NAME}"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "${PUBLISHER}"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\${APP_EXE},0"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
    WriteRegStr HKCU "${UNINSTALL_KEY}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
    WriteRegStr HKCU "${UNINSTALL_KEY}" "URLInfoAbout" "https://github.com/CookieJarTools/TheJar"
    WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
    WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
    ${GetSize} "$INSTDIR" "/S=0K" $1 $2 $3
    IntFmt $1 "0x%08X" $1
    WriteRegDWORD HKCU "${UNINSTALL_KEY}" "EstimatedSize" $1

    ${GetParameters} $R0
    ClearErrors
    ${GetOptions} $R0 "--run" $R1
    ${IfNot} ${Errors}
      Exec '"$INSTDIR\${APP_EXE}"'
    ${EndIf}
SectionEnd

Section "Uninstall"
  ; Settings and caches in AppData are left alone, like the other CookieJarTools apps do.
  Delete "$INSTDIR\${APP_EXE}"
  Delete "$INSTDIR\LICENSE.md"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\${NAME}.lnk"
  Delete "$DESKTOP\${NAME}.lnk"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"
SectionEnd
