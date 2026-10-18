; Windows installer, built with Inno Setup 6 by CI:
;   ISCC /DAppVersion=0.2.0 /DNumericVersion=0.2.0 /DSourceDir=<dir with pdftools.exe> pdftools.iss
;
; Installs for the current user by default (no administrator rights needed);
; the first page offers installing for all users instead.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef NumericVersion
  #define NumericVersion "0.0.0"
#endif
#ifndef SourceDir
  #define SourceDir "..\..\dist"
#endif

#define Repository "https://github.com/ivadim/pdf-tools"

[Setup]
; Never change the AppId: upgrades find the previous installation by it.
AppId={{78964553-D500-4B75-915C-AEBE661D21FE}
AppName=PDF Tools
AppVersion={#AppVersion}
AppVerName=PDF Tools {#AppVersion}
AppPublisher=Dmitry Ivanov
AppPublisherURL={#Repository}
AppSupportURL={#Repository}/issues
AppUpdatesURL={#Repository}/releases
VersionInfoVersion={#NumericVersion}
VersionInfoProductName=PDF Tools
VersionInfoDescription=PDF Tools Setup
DefaultDirName={autopf}\PDF Tools
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
LicenseFile=..\..\LICENSE
SetupIconFile=..\icon.ico
UninstallDisplayIcon={app}\pdftools.exe
UninstallDisplayName=PDF Tools
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes
OutputDir=.
OutputBaseFilename=pdftools-{#AppVersion}-windows-x86_64-setup
; Closes a running copy before replacing it.
CloseApplications=yes

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceDir}\pdftools.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\PDF Tools"; Filename: "{app}\pdftools.exe"
Name: "{autodesktop}\PDF Tools"; Filename: "{app}\pdftools.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\pdftools.exe"; Description: "{cm:LaunchProgram,PDF Tools}"; Flags: nowait postinstall skipifsilent
