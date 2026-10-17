; Inno Setup script for audioviz. Built by scripts/release.ps1 (locally or in CI):
;   ISCC.exe /DAppVersion=0.1.0 /DPublisher="Gregory Douglas" /DSourceDir=..\target\release installer\audioviz.iss
; Per-user install (no admin prompt), Start Menu shortcut, optional desktop icon and
; "start when I sign in" (minimised to the tray). The uninstaller offers to delete
; settings and otherwise leaves %APPDATA%\audioviz alone.
;
; Wizard flow (docs/phase6-ui.md, "Installer"): the Welcome page is the Checks page
; (it is the only page Inno paints the large image strip on), so it carries the
; title, the upgrade note, the environment checks and the two options. Next is the
; directory page on a first install only (DisableDirPage=auto), then install, then
; the Finished page with "Launch", "Open Spotify too" and "Start fullscreen on my
; second monitor". The Tasks and Ready pages are never shown.
; Wizard images come from `cargo xtask installer-images` (assets/installer/).

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
; Whole-megabyte size of the exe for the footer line on the first page.
#define ExeMB (FileSize(SourceDir + "\audioviz.exe") + 524288) / 1048576
#define UninstKey "Software\Microsoft\Windows\CurrentVersion\Uninstall\{A7C3F1E2-5B64-4D8A-9E1F-2C0B7D3A4F10}_is1"

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
DisableWelcomePage=no
DisableReadyPage=yes
PrivilegesRequired=lowest
OutputDir={#OutputDir}
OutputBaseFilename=audioviz-{#AppVersion}-setup
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\audioviz.exe
UninstallDisplayName=audioviz
Compression=lzma2/max
SolidCompression=yes
; Light modern look to match the mockup: white page, dark image strip with the mark.
WizardStyle=modern
WizardSizePercent=120
WizardImageFile=..\assets\installer\wizard-large.bmp,..\assets\installer\wizard-large-150.bmp,..\assets\installer\wizard-large-200.bmp
WizardSmallImageFile=..\assets\installer\wizard-small.bmp,..\assets\installer\wizard-small-150.bmp,..\assets\installer\wizard-small-200.bmp
WizardImageBackColor=#0b0e10
WizardSmallImageBackColor=#0b0e10
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Per-process loopback capture needs Windows 10 2004 (build 19041) or later.
MinVersion=10.0.19041
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Messages]
; Welcome and Finished pages get their text from [Code]; these stay as fallbacks.
WelcomeLabel1=Install audioviz
WelcomeLabel2=For this user only. No administrator rights needed.
FinishedHeadingLabel=Installed
FinishedLabel=audioviz {#AppVersion} is ready.
FinishedLabelNoIcons=audioviz {#AppVersion} is ready.

[Tasks]
; The Tasks page itself is skipped; these are driven by the checkboxes on the first page.
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "startup"; Description: "Start when I sign in, minimised to the tray"; GroupDescription: "Startup:"; Flags: unchecked

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
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "audioviz"; ValueData: """{app}\audioviz.exe"" --tray"; Flags: uninsdeletevalue; Tasks: startup

[Code]
// ---------------------------------------------------------------------------
// Environment checks (inform only; MinVersion is the one hard gate).
// ---------------------------------------------------------------------------

function GetSystemMetrics(nIndex: Integer): Integer;
  external 'GetSystemMetrics@user32.dll stdcall';

const
  SM_CMONITORS = 80;
  SectionColor = '#7d8a86';
  OkColor = '#1f9d55';
  WarnColor = '#c2410c';
  BannerBack = '#e6ffef';
  BannerText = '#1f6b3a';
  TipBack = '#f3f6f5';

var
  PrevVersion: String;      // DisplayVersion of the installed copy, '' on a first install
  SpotifyPath: String;      // '' when Spotify was not found
  // Welcome (checks) page
  DesktopBox, StartupBox: TNewCheckBox;
  // Finished page
  FinishedBuilt: Boolean;
  LaunchBox, SpotifyBox, FullscreenBox: TNewCheckBox;

function WindowsLine: String;
var
  V: TWindowsVersion;
  Name: String;
begin
  GetWindowsVersionEx(V);
  if V.Build >= 22000 then Name := 'Windows 11' else Name := 'Windows 10';
  Result := Format('%s, build %d', [Name, V.Build]);
end;

// First display adapter from WMI, skipping virtual ones when a real one exists.
function GpuName: String;
var
  Locator, Service, Objs, Obj: Variant;
  I: Integer;
  Name, All, Real: String;
begin
  All := '';
  Real := '';
  try
    Locator := CreateOleObject('WbemScripting.SWbemLocator');
    Service := Locator.ConnectServer('.', 'root\CIMV2');
    Objs := Service.ExecQuery('SELECT Name FROM Win32_VideoController');
    for I := 0 to Objs.Count - 1 do
    begin
      Obj := Objs.ItemIndex(I);
      Name := Obj.Name;
      if Name = '' then Continue;
      if All <> '' then All := All + ' / ';
      All := All + Name;
      if Pos('virtual', Lowercase(Name)) = 0 then
      begin
        if Real <> '' then Real := Real + ' / ';
        Real := Real + Name;
      end;
    end;
  except
    All := '';
    Real := '';
  end;
  if Real <> '' then Result := Real else Result := All;
end;

function FindSpotify: String;
var
  P: String;
begin
  Result := '';
  P := ExpandConstant('{userappdata}\Spotify\Spotify.exe');
  if FileExists(P) then begin Result := P; exit; end;
  P := ExpandConstant('{localappdata}\Microsoft\WindowsApps\Spotify.exe');
  if FileExists(P) then Result := P;
end;

function MonitorCount: Integer;
begin
  Result := GetSystemMetrics(SM_CMONITORS);
  if Result < 1 then Result := 1;
end;

// ---------------------------------------------------------------------------
// Small layout helpers (all positions through ScaleX/ScaleY for DPI).
// ---------------------------------------------------------------------------

function AddText(Parent: TWinControl; L, T, W: Integer; const S: String): TNewStaticText;
begin
  Result := TNewStaticText.Create(WizardForm);
  Result.Parent := Parent;
  Result.Left := L;
  Result.Top := T;
  Result.Width := W;
  Result.WordWrap := True;
  Result.AutoSize := True;
  Result.Caption := S;
end;

function AddSection(Parent: TWinControl; L, T, W: Integer; const S: String): TNewStaticText;
begin
  Result := AddText(Parent, L, T, W, S);
  Result.Font.Style := [fsBold];
  Result.Font.Size := Result.Font.Size - 1;
  Result.Font.Color := StrToColor(SectionColor);
end;

// "check mark + text" row. Returns the next free Top.
function AddCheckRow(Parent: TWinControl; L, T, W: Integer; Ok: Boolean; const S: String): Integer;
var
  Glyph, Txt: TNewStaticText;
begin
  Glyph := AddText(Parent, L, T, ScaleX(14), '');
  Glyph.WordWrap := False;
  Glyph.Font.Style := [fsBold];
  if Ok then
  begin
    Glyph.Caption := #$2713;
    Glyph.Font.Color := StrToColor(OkColor);
  end else begin
    Glyph.Caption := '!';
    Glyph.Font.Color := StrToColor(WarnColor);
  end;
  Txt := AddText(Parent, L + ScaleX(18), T, W - ScaleX(18), S);
  Result := T + Txt.Height + ScaleY(6);
end;

function AddBox(Parent: TWinControl; L, T, W: Integer; const S: String; On: Boolean): TNewCheckBox;
begin
  Result := TNewCheckBox.Create(WizardForm);
  Result.Parent := Parent;
  Result.Left := L;
  Result.Top := T;
  Result.Width := W;
  Result.Height := ScaleY(17);
  Result.Caption := S;
  Result.Checked := On;
end;

function AddPanel(Parent: TWinControl; L, T, W, H: Integer; const Back: String): TPanel;
begin
  Result := TPanel.Create(WizardForm);
  Result.Parent := Parent;
  Result.Left := L;
  Result.Top := T;
  Result.Width := W;
  Result.Height := H;
  Result.BevelOuter := bvNone;
  Result.ParentBackground := False;
  Result.Color := StrToColor(Back);
end;

// ---------------------------------------------------------------------------
// Welcome page = Checks page.
// ---------------------------------------------------------------------------

procedure BuildWelcomePage;
var
  Page: TNewNotebookPage;
  L, W, T: Integer;
  Banner: TPanel;
  Txt: TNewStaticText;
  Gpu: String;
begin
  Page := WizardForm.WelcomePage;
  L := WizardForm.WelcomeLabel2.Left;
  W := WizardForm.WelcomeLabel2.Width;

  if PrevVersion <> '' then
    WizardForm.WelcomeLabel1.Caption := 'Upgrade audioviz'
  else
    WizardForm.WelcomeLabel1.Caption := 'Install audioviz';
  WizardForm.WelcomeLabel2.Caption := 'For this user only. No administrator rights needed.';
  WizardForm.WelcomeLabel2.AutoSize := True;  // Inno gives it the whole page; shrink to the text
  T := WizardForm.WelcomeLabel2.Top + WizardForm.WelcomeLabel2.Height + ScaleY(12);

  if PrevVersion <> '' then
  begin
    Banner := AddPanel(Page, L, T, W, ScaleY(26), BannerBack);
    if PrevVersion = '{#AppVersion}' then
      Txt := AddText(Banner, ScaleX(10), ScaleY(5), W - ScaleX(20), 'Reinstalling {#AppVersion}. Your settings are kept.')
    else
      Txt := AddText(Banner, ScaleX(10), ScaleY(5), W - ScaleX(20), 'Upgrading ' + PrevVersion + ' ' + #$2192 + ' {#AppVersion}. Your settings are kept.');
    Txt.Font.Color := StrToColor(BannerText);
    T := T + Banner.Height + ScaleY(14);
  end;

  AddSection(Page, L, T, W, 'CHECKS');
  T := T + ScaleY(18);
  T := AddCheckRow(Page, L, T, W, True, WindowsLine);
  Gpu := GpuName;
  if Gpu <> '' then
    T := AddCheckRow(Page, L, T, W, True, 'GPU: ' + Gpu)
  else
    T := AddCheckRow(Page, L, T, W, False, 'GPU: could not query');
  if SpotifyPath <> '' then
    T := AddCheckRow(Page, L, T, W, True, 'Spotify installed')
  else
    T := AddCheckRow(Page, L, T, W, False, 'Spotify not found ' + #$2014 + ' audioviz follows what Spotify plays; install it from spotify.com');

  T := T + ScaleY(8);
  AddSection(Page, L, T, W, 'LOCATION');
  T := T + ScaleY(18);
  Txt := AddText(Page, L, T, W, WizardDirValue);
  T := T + Txt.Height + ScaleY(2);
  if PrevVersion = '' then
  begin
    Txt := AddText(Page, L, T, W, 'Change it on the next page.');
    Txt.Font.Color := StrToColor(SectionColor);
    T := T + Txt.Height + ScaleY(2);
  end;
  T := T + ScaleY(12);

  AddSection(Page, L, T, W, 'OPTIONS');
  T := T + ScaleY(18);
  DesktopBox := AddBox(Page, L, T, W, 'Desktop shortcut', WizardIsTaskSelected('desktopicon'));
  T := T + ScaleY(22);
  StartupBox := AddBox(Page, L, T, W, 'Start when I sign in, minimised to the tray', WizardIsTaskSelected('startup'));
  T := T + ScaleY(22);
  T := T + ScaleY(6);
  Txt := AddText(Page, L, T, W, '{#ExeMB} MB ' + #$00B7 + ' unsigned, SmartScreen may ask once ' + #$00B7 + ' SHA-256 on the release page');
  Txt.Font.Color := StrToColor(SectionColor);
end;

// ---------------------------------------------------------------------------
// Finished page: what happened, what to do now, one tip.
// ---------------------------------------------------------------------------

procedure LaunchBoxClick(Sender: TObject);
begin
  FullscreenBox.Enabled := LaunchBox.Checked;
end;

procedure BuildFinishedPage;
var
  Page: TNewNotebookPage;
  L, W, T: Integer;
  Tip: TPanel;
  Txt: TNewStaticText;
begin
  Page := WizardForm.FinishedPage;
  L := WizardForm.FinishedLabel.Left;
  W := WizardForm.FinishedLabel.Width;
  WizardForm.FinishedHeadingLabel.Caption := 'Installed';
  WizardForm.FinishedLabel.Caption := 'audioviz {#AppVersion} is ready.';
  WizardForm.FinishedLabel.AutoSize := True;
  T := WizardForm.FinishedLabel.Top + WizardForm.FinishedLabel.Height + ScaleY(12);

  T := AddCheckRow(Page, L, T, W, True, 'Installed to ' + ExpandConstant('{app}'));
  T := AddCheckRow(Page, L, T, W, True, 'Start Menu shortcut added');
  if WizardIsTaskSelected('desktopicon') then
    T := AddCheckRow(Page, L, T, W, True, 'Desktop shortcut added');
  if WizardIsTaskSelected('startup') then
    T := AddCheckRow(Page, L, T, W, True, 'Starts at sign-in, minimised to the tray');
  if PrevVersion <> '' then
    T := AddCheckRow(Page, L, T, W, True, 'Settings kept from ' + PrevVersion);

  T := T + ScaleY(8);
  AddSection(Page, L, T, W, 'NOW');
  T := T + ScaleY(18);
  LaunchBox := AddBox(Page, L, T, W, 'Launch audioviz', True);
  LaunchBox.OnClick := @LaunchBoxClick;
  T := T + ScaleY(22);
  if SpotifyPath <> '' then
    SpotifyBox := AddBox(Page, L, T, W, 'Open Spotify too', True)
  else
  begin
    SpotifyBox := AddBox(Page, L, T, W, 'Open Spotify too (not installed)', False);
    SpotifyBox.Enabled := False;
  end;
  T := T + ScaleY(22);
  FullscreenBox := AddBox(Page, L, T, W, 'Start fullscreen on my second monitor', False);
  if MonitorCount > 1 then
    T := T + ScaleY(22)
  else
    FullscreenBox.Visible := False;

  T := T + ScaleY(10);
  Tip := AddPanel(Page, L, T, W, ScaleY(62), TipBack);
  AddSection(Tip, ScaleX(10), ScaleY(8), W - ScaleX(20), 'TIP');
  Txt := AddText(Tip, ScaleX(10), ScaleY(26), W - ScaleX(20),
    'Tab opens settings. C calibrates the beat to your speakers. F11 goes fullscreen. ? shows every shortcut.');
  Tip.Height := Txt.Top + Txt.Height + ScaleY(8);
end;

// Finish: Spotify first (so audioviz finds it), then audioviz with or without the
// fullscreen settings. Never on a silent install (the old [Run] entry was skipifsilent).
procedure RunFinishActions;
var
  Params: String;
  Err: Integer;
begin
  if WizardSilent or (not FinishedBuilt) then exit;
  if SpotifyBox.Enabled and SpotifyBox.Checked then
    ShellExec('open', SpotifyPath, '', '', SW_SHOWNORMAL, ewNoWait, Err);
  if LaunchBox.Checked then
  begin
    Params := '';
    if FullscreenBox.Visible and FullscreenBox.Enabled and FullscreenBox.Checked then
      Params := '--set start_fullscreen=true --set monitor=1';
    Exec(ExpandConstant('{app}\audioviz.exe'), Params, ExpandConstant('{app}'), SW_SHOWNORMAL, ewNoWait, Err);
  end;
end;

// ---------------------------------------------------------------------------
// Wizard events.
// ---------------------------------------------------------------------------

procedure InitializeWizard;
begin
  if not RegQueryStringValue(HKCU, '{#UninstKey}', 'DisplayVersion', PrevVersion) then
    PrevVersion := '';
  SpotifyPath := FindSpotify;
  FinishedBuilt := False;
  BuildWelcomePage;
end;

function ShouldSkipPage(PageID: Integer): Boolean;
begin
  // Tasks live on the first page as checkboxes.
  Result := PageID = wpSelectTasks;
end;

procedure CurPageChanged(CurPageID: Integer);
begin
  // The button after the last visible pre-install page starts the install.
  if ((CurPageID = wpWelcome) and (PrevVersion <> '')) or (CurPageID = wpSelectDir) then
    WizardForm.NextButton.Caption := '&Install';
  // Silent installs walk the pages too, so never build (or act on) the finish options there.
  if (CurPageID = wpFinished) and (not FinishedBuilt) and (not WizardSilent) then
  begin
    FinishedBuilt := True;
    if not WizardForm.YesRadio.Visible then
      BuildFinishedPage;
  end;
end;

function NextButtonClick(CurPageID: Integer): Boolean;
var
  Tasks: String;
begin
  Result := True;
  if CurPageID = wpWelcome then
  begin
    Tasks := '';
    if DesktopBox.Checked then Tasks := 'desktopicon';
    if StartupBox.Checked then
    begin
      if Tasks <> '' then Tasks := Tasks + ',';
      Tasks := Tasks + 'startup';
    end;
    WizardSelectTasks(Tasks);
  end
  else if CurPageID = wpFinished then
    RunFinishActions;
end;

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
