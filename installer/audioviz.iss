; Inno Setup script for audioviz. Built by scripts/release.ps1 (locally or in CI):
;   ISCC.exe /DAppVersion=0.1.0 /DPublisher="Gregory Douglas" /DSourceDir=..\target\release installer\audioviz.iss
; Per-user install (no admin prompt), Start Menu shortcut, optional desktop icon and
; "start when I sign in". The uninstaller offers to delete settings and otherwise
; leaves %APPDATA%\audioviz alone.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef Publisher
  #define Publisher "Gregory Douglas"
#endif
#ifndef RepoUrl
  #define RepoUrl "https://github.com/gregodouglas/audioviz"
#endif
#ifndef SourceDir
  #define SourceDir "..\target\release"
#endif
#ifndef OutputDir
  #define OutputDir "..\dist"
#endif

[Setup]
; Stable across versions so upgrades replace in place. Also the winget ProductCode ("{...}_is1").
AppId={{A7C3F1E2-5B64-4D8A-9E1F-2C0B7D3A4F10}
AppName=audioviz
AppVersion={#AppVersion}
AppVerName=audioviz {#AppVersion}
AppPublisher={#Publisher}
AppPublisherURL={#RepoUrl}
AppSupportURL={#RepoUrl}/issues
AppUpdatesURL={#RepoUrl}/releases
AppCopyright=Copyright (C) 2026 Gregory Douglas. MIT License.
VersionInfoVersion={#AppVersion}
DefaultDirName={userpf}\audioviz
DefaultGroupName=audioviz
DisableProgramGroupPage=yes
DisableDirPage=auto
PrivilegesRequired=lowest
OutputDir={#OutputDir}
OutputBaseFilename=audioviz-{#AppVersion}-setup
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\audioviz.exe
UninstallDisplayName=audioviz
LicenseFile=..\LICENSE
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Per-process loopback capture needs Windows 10 2004 (build 19041) or later.
MinVersion=10.0.19041
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "startup"; Description: "Start audioviz when I sign in to Windows"; GroupDescription: "Startup:"; Flags: unchecked

[Files]
Source: "{#SourceDir}\audioviz.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion
Source: "..\PRIVACY.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\audioviz"; Filename: "{app}\audioviz.exe"; Comment: "Music visualizer"
Name: "{group}\Uninstall audioviz"; Filename: "{uninstallexe}"
Name: "{autodesktop}\audioviz"; Filename: "{app}\audioviz.exe"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "audioviz"; ValueData: """{app}\audioviz.exe"""; Flags: uninsdeletevalue; Tasks: startup

[Run]
Filename: "{app}\audioviz.exe"; Description: "{cm:LaunchProgram,audioviz}"; Flags: nowait postinstall skipifsilent

[Code]
// Settings live in %APPDATA%\audioviz. Keep them on uninstall unless the user
// says otherwise; silent uninstalls (winget) never delete them.
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Dir: String;
begin
  if CurUninstallStep = usPostUninstall then
  begin
    Dir := ExpandConstant('{userappdata}\audioviz');
    if DirExists(Dir) and (not UninstallSilent) then
    begin
      if MsgBox('Also delete your audioviz settings?' + #13#10 + Dir, mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
        DelTree(Dir, True, True, True);
    end;
  end;
end;
