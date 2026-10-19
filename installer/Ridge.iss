; Ridge -- one-click Windows installer.
;
; Compile with tools\Build-Installer.ps1, not by hand: this script installs
; release\Ridge\ridge.exe, which that script copies (and signs, when signing
; is configured) before calling ISCC.
;
; What it does beyond copying the exe:
;   * installs per user, so it never asks for administrator rights,
;   * puts the install folder on the user's PATH, so `ridge` works in any
;     new terminal, and takes it off again on uninstall,
;   * adds a Start menu "Ridge" shortcut; ridge.exe started without arguments
;     opens its interactive prompt, so the window stays open.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

#define AppName "Ridge"
#define AppExeName "ridge.exe"
#define AppPublisher "Ridge"
#define AppUrl "https://github.com/Ridge-client/Launcher"

[Setup]
; never change the AppId: upgrades and uninstall find the previous install by it.
AppId={{58C8A833-B329-4605-85B6-7E1CE30B9642}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
VersionInfoVersion={#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppUrl}
AppSupportURL={#AppUrl}
AppUpdatesURL={#AppUrl}

; %LocalAppData% and PrivilegesRequired=lowest: no UAC prompt, and the PATH
; entry goes into the user's own environment (HKCU), not the machine's.
DefaultDirName={localappdata}\Ridge
PrivilegesRequired=lowest
UsePreviousAppDir=yes
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
LicenseFile=..\LICENSE
OutputDir=..\release
OutputBaseFilename=Ridge-Setup-{#AppVersion}
; the Ridge icon on the Setup itself, and the wizard's artwork (assets/ridge-icon.svg)
SetupIconFile=..\assets\ridge.ico
WizardImageFile=wizard-image.png
WizardSmallImageFile=wizard-small.png
Compression=lzma2/max
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; tells open programs (Explorer, new terminals) that PATH changed, on install and uninstall.
ChangesEnvironment=yes
WizardStyle=modern dark includetitlebar
UninstallDisplayName={#AppName}
UninstallDisplayIcon={app}\{#AppExeName}
SetupLogging=yes
CloseApplications=yes
; signed only when tools\Build-Installer.ps1 found the signing settings (it defines the tool).
#ifdef ridgeSign
SignTool=ridgesign
SignedUninstaller=yes
#endif

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Files]
Source: "..\release\Ridge\{#AppExeName}"; DestDir: "{app}"; Flags: ignoreversion

[UninstallDelete]
; the app's saved state: last link check, settings (eframe's storage folder)
Type: filesandordirs; Name: "{userappdata}\Ridge"

[Registry]
Root: HKCU; Subkey: "Environment"; ValueType: expandsz; ValueName: "Path"; \
  ValueData: "{olddata};{app}"; Check: NeedsAddPath(ExpandConstant('{app}'))

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExeName}"; WorkingDir: "{%USERPROFILE}"; \
  Comment: "Open Ridge"
Name: "{group}\Uninstall {#AppName}"; Filename: "{uninstallexe}"

[Run]
Filename: "{app}\{#AppExeName}"; WorkingDir: "{%USERPROFILE}"; \
  Description: "Open Ridge"; Flags: postinstall nowait skipifsilent

[Code]
// The PATH value is compared with ';' on both ends, so C:\Ridge never matches C:\Ridge2.
function PathHas(Path, Dir: string): Integer;
begin
  Result := Pos(';' + Uppercase(Dir) + ';', ';' + Uppercase(Path) + ';');
end;

function NeedsAddPath(Dir: string): Boolean;
var
  Path: string;
begin
  if not RegQueryStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', Path) then
    Result := True
  else
    Result := PathHas(Path, Dir) = 0;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Path, Padded, Dir: string;
  P: Integer;
begin
  if CurUninstallStep <> usPostUninstall then
    exit;
  if not RegQueryStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', Path) then
    exit;
  Dir := ExpandConstant('{app}');
  P := PathHas(Path, Dir);
  if P = 0 then
    exit;
  Padded := ';' + Path + ';';
  Delete(Padded, P, Length(Dir) + 1);
  Path := Copy(Padded, 2, Length(Padded) - 2);
  RegWriteExpandStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', Path);
end;
