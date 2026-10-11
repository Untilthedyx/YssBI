; Built only through scripts/package_windows.ps1. No license or product icon is asserted.
#if Ver < EncodeVer(6, 3, 0)
  #error Inno Setup 6.3 or newer is required.
#endif

[Setup]
AppId=com.zjy.yssbi
AppName=YssBI
AppVersion={#AppVersion}
AppVerName=YssBI {#AppVersion}
VersionInfoVersion={#NumericVersion}
VersionInfoProductVersion={#AppVersion}
AppPublisherURL=https://github.com/zhouyi207/YssBI
AppSupportURL=https://github.com/zhouyi207/YssBI/issues
DefaultDirName={localappdata}\Programs\YssBI
DefaultGroupName=YssBI
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed={#AppArchitecture}
ArchitecturesInstallIn64BitMode={#AppArchitecture}
MinVersion=10.0.17763
OutputDir={#PackageOutput}
OutputBaseFilename={#AssetBase}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayIcon={app}\yss-desktop-gpui.exe
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#Payload}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\YssBI"; Filename: "{app}\yss-desktop-gpui.exe"; WorkingDir: "{app}"; AppUserModelID: "com.zjy.yssbi"
Name: "{autodesktop}\YssBI"; Filename: "{app}\yss-desktop-gpui.exe"; WorkingDir: "{app}"; Tasks: desktopicon; AppUserModelID: "com.zjy.yssbi"

[Run]
Filename: "{app}\yss-desktop-gpui.exe"; Description: "{cm:LaunchProgram,YssBI}"; Flags: nowait postinstall skipifsilent

; Deliberately no UninstallDelete or file associations: remove only installed files,
; never user projects, settings, caches, or other user-created content.
