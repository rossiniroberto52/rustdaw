; RustDAW NSIS Installer Script
; Generates a Windows x64 installer

!include "MUI2.nsh"
!include "x64.nsh"

; ---------------------------------------------------
; General Settings
; ---------------------------------------------------
Name "RustDAW"
OutFile "RustDAW-Setup-x64.exe"
InstallDir "$PROGRAMFILES64\RustDAW"
InstallDirRegKey HKLM "Software\RustDAW" ""
RequestExecutionLevel admin
ShowInstDetails show

; ---------------------------------------------------
; Modern UI Settings
; ---------------------------------------------------
!define MUI_ABORTWARNING
!define MUI_ICON "${NSISDIR}\Contrib\Graphics\Icons\modern-install.ico"
!define MUI_UNICON "${NSISDIR}\Contrib\Graphics\Icons\modern-uninstall.ico"

!define MUI_WELCOMEPAGE_TITLE "Bem-vindo ao RustDAW Installer"
!define MUI_WELCOMEPAGE_TITLE_3LINES
!define MUI_WELCOMEPAGE_TEXT "Este assistente irá instalar o RustDAW no seu computador.\n\nClique em Próximo para continuar."

!define MUI_DIRECTORYPAGE_TEXT_TOP "Escolha a pasta de instalação:"
!define MUI_DIRECTORYPAGE_TEXT_DESTINATION "Pasta de destino:"

!define MUI_FINISHPAGE_TITLE "Instalação Concluída"
!define MUI_FINISHPAGE_TEXT "O RustDAW foi instalado com sucesso.\n\nClique em Concluir para fechar este assistente."

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_WELCOME
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_UNPAGE_FINISH

; ---------------------------------------------------
; Languages
; ---------------------------------------------------
!insertmacro MUI_LANGUAGE "PortugueseBR"
!insertmacro MUI_LANGUAGE "English"

; ---------------------------------------------------
; Installer Sections
; ---------------------------------------------------
Section "MainSection" SEC_MAIN
    SetOutPath "$INSTDIR"
    
    ; Copy the main executable
    File "rustdaw_app.exe"
    
    ; Create uninstaller
    WriteUninstaller "$INSTDIR\uninstall.exe"
    
    ; Registry entries for uninstaller
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RustDAW" \
        "DisplayName" "RustDAW"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RustDAW" \
        "UninstallString" "$INSTDIR\uninstall.exe"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RustDAW" \
        "DisplayVersion" "0.1.0"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RustDAW" \
        "Publisher" "Nous Research"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RustDAW" \
        "DisplayIcon" "$INSTDIR\rustdaw_app.exe"
    
    ; Registry for app detection
    WriteRegStr HKLM "Software\RustDAW" "" $INSTDIR
    
    ; Create Start Menu shortcuts
    CreateDirectory "$SMPROGRAMS\RustDAW"
    CreateShortcut "$SMPROGRAMS\RustDAW\RustDAW.lnk" "$INSTDIR\rustdaw_app.exe" "" "$INSTDIR\rustdaw_app.exe" 0
    CreateShortcut "$SMPROGRAMS\RustDAW\Desinstalar RustDAW.lnk" "$INSTDIR\uninstall.exe" "" "$INSTDIR\uninstall.exe" 0
    
    ; Create Desktop shortcut (optional)
    CreateShortcut "$DESKTOP\RustDAW.lnk" "$INSTDIR\rustdaw_app.exe" "" "$INSTDIR\rustdaw_app.exe" 0
SectionEnd

; ---------------------------------------------------
; Uninstaller Section
; ---------------------------------------------------
Section "Uninstall"
    ; Remove files
    Delete "$INSTDIR\rustdaw_app.exe"
    Delete "$INSTDIR\uninstall.exe"
    
    ; Remove shortcuts
    Delete "$SMPROGRAMS\RustDAW\RustDAW.lnk"
    Delete "$SMPROGRAMS\RustDAW\Desinstalar RustDAW.lnk"
    Delete "$DESKTOP\RustDAW.lnk"
    
    ; Remove Start Menu folder
    RMDir "$SMPROGRAMS\RustDAW"
    
    ; Remove registry entries
    DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RustDAW"
    DeleteRegKey HKLM "Software\RustDAW"
    
    ; Remove install directory
    RMDir "$INSTDIR"
SectionEnd

; ---------------------------------------------------
; Functions
; ---------------------------------------------------
Function .onInit
    ; Check if running on 64-bit Windows
    ${If} ${RunningX64}
        ; OK, running on 64-bit
    ${Else}
        MessageBox MB_ICONSTOP "Este instalador requer Windows 64-bit."
        Abort
    ${EndIf}
FunctionEnd