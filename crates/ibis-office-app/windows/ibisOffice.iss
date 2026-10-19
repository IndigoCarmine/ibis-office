; SPDX-License-Identifier: AGPL-3.0-only
; Copyright (c) ibis inc.
; Additional terms under AGPL-3.0 section 7(b): see NOTICE.
; The Windows installer (Inno Setup 6), built by scripts/bundle-windows.ps1:
;
;   ISCC.exe /DAppVersion=0.8.1 /DSourceDir=<the ibisOffice folder> /O<output> ibisOffice.iss
;
; It installs the same ibisOffice folder as the release zip, for the current
; user only (no administrator rights), in %LOCALAPPDATA%\Programs\ibisOffice:
; the automatic update (ibisOfficeUpdater) swaps that whole folder for the one
; in the new zip, so it has to be where the user can write, and nothing the
; installer adds may live inside it. The uninstaller is therefore kept in
; %LOCALAPPDATA%\ibisOffice\Uninstall, beside the update's own folders.

#ifndef AppVersion
  #error Pass the version: /DAppVersion=<version>
#endif
#ifndef SourceDir
  #error Pass the folder to install: /DSourceDir=<the ibisOffice folder>
#endif

[Setup]
; The same for every version, so that a newer installer updates the install in place.
AppId={{F7D0150F-23E4-4CB3-964C-2D16D12CBABA}
AppName=ibisOffice
AppVersion={#AppVersion}
AppPublisher=ibis inc.
AppPublisherURL=https://github.com/ibisjp/ibis-office
AppSupportURL=https://github.com/ibisjp/ibis-office/issues
AppUpdatesURL=https://github.com/ibisjp/ibis-office/releases
VersionInfoVersion={#AppVersion}
PrivilegesRequired=lowest
DefaultDirName={localappdata}\Programs\ibisOffice
DisableProgramGroupPage=yes
UninstallFilesDir={localappdata}\ibisOffice\Uninstall
UninstallDisplayIcon={app}\ibisOffice.exe
UninstallDisplayName=ibisOffice
SetupIconFile=ibisOffice.ico
LicenseFile=..\..\..\LICENSE
MinVersion=10.0
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
ChangesAssociations=yes
CloseApplications=yes
OutputBaseFilename=ibisOffice-{#AppVersion}-Windows-Setup
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern

[Languages]
Name: "japanese"; MessagesFile: "compiler:Languages\Japanese.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
japanese.Associate=Word・Excel・PowerPoint のファイル（.docx・.xlsx・.pptx など）の「プログラムから開く」に ibisOffice を加える
english.Associate=Add ibisOffice to "Open with" for Word, Excel and PowerPoint files (.docx, .xlsx, .pptx and others)
japanese.Document=Word 文書
english.Document=Word document
japanese.Workbook=Excel ブック
english.Workbook=Excel workbook
japanese.Presentation=PowerPoint プレゼンテーション
english.Presentation=PowerPoint presentation
japanese.Description=Word・Excel・PowerPoint の文書を表示し、編集し、保存するオフィス スイート
english.Description=An office suite that opens, edits and saves Word, Excel and PowerPoint documents

[Tasks]
Name: "associate"; Description: "{cm:Associate}"
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\ibisOffice"; Filename: "{app}\ibisOffice.exe"
Name: "{autodesktop}\ibisOffice"; Filename: "{app}\ibisOffice.exe"; Tasks: desktopicon

[Registry]
; As on macOS (LSHandlerRank Alternate), ibisOffice is offered for the files
; but does not take them from the app that opens them now: a ProgID for each
; kind, listed under each extension's OpenWithProgids, and the capabilities
; that Settings > Apps > Default apps shows, where the user can choose it.
Root: HKA; Subkey: "Software\Classes\ibisOffice.Document"; ValueType: string; ValueData: "{cm:Document}"; Flags: uninsdeletekey; Tasks: associate
Root: HKA; Subkey: "Software\Classes\ibisOffice.Document\DefaultIcon"; ValueType: string; ValueData: "{app}\ibisOffice.exe,0"; Tasks: associate
Root: HKA; Subkey: "Software\Classes\ibisOffice.Document\shell\open\command"; ValueType: string; ValueData: """{app}\ibisOffice.exe"" ""%1"""; Tasks: associate
Root: HKA; Subkey: "Software\Classes\ibisOffice.Workbook"; ValueType: string; ValueData: "{cm:Workbook}"; Flags: uninsdeletekey; Tasks: associate
Root: HKA; Subkey: "Software\Classes\ibisOffice.Workbook\DefaultIcon"; ValueType: string; ValueData: "{app}\ibisOffice.exe,0"; Tasks: associate
Root: HKA; Subkey: "Software\Classes\ibisOffice.Workbook\shell\open\command"; ValueType: string; ValueData: """{app}\ibisOffice.exe"" ""%1"""; Tasks: associate
Root: HKA; Subkey: "Software\Classes\ibisOffice.Presentation"; ValueType: string; ValueData: "{cm:Presentation}"; Flags: uninsdeletekey; Tasks: associate
Root: HKA; Subkey: "Software\Classes\ibisOffice.Presentation\DefaultIcon"; ValueType: string; ValueData: "{app}\ibisOffice.exe,0"; Tasks: associate
Root: HKA; Subkey: "Software\Classes\ibisOffice.Presentation\shell\open\command"; ValueType: string; ValueData: """{app}\ibisOffice.exe"" ""%1"""; Tasks: associate

Root: HKA; Subkey: "Software\Classes\.docx\OpenWithProgids"; ValueType: string; ValueName: "ibisOffice.Document"; ValueData: ""; Flags: uninsdeletevalue; Tasks: associate
Root: HKA; Subkey: "Software\Classes\.docm\OpenWithProgids"; ValueType: string; ValueName: "ibisOffice.Document"; ValueData: ""; Flags: uninsdeletevalue; Tasks: associate
Root: HKA; Subkey: "Software\Classes\.xlsx\OpenWithProgids"; ValueType: string; ValueName: "ibisOffice.Workbook"; ValueData: ""; Flags: uninsdeletevalue; Tasks: associate
Root: HKA; Subkey: "Software\Classes\.xlsm\OpenWithProgids"; ValueType: string; ValueName: "ibisOffice.Workbook"; ValueData: ""; Flags: uninsdeletevalue; Tasks: associate
Root: HKA; Subkey: "Software\Classes\.pptx\OpenWithProgids"; ValueType: string; ValueName: "ibisOffice.Presentation"; ValueData: ""; Flags: uninsdeletevalue; Tasks: associate
Root: HKA; Subkey: "Software\Classes\.pptm\OpenWithProgids"; ValueType: string; ValueName: "ibisOffice.Presentation"; ValueData: ""; Flags: uninsdeletevalue; Tasks: associate

Root: HKA; Subkey: "Software\Classes\Applications\ibisOffice.exe"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "ibisOffice"; Flags: uninsdeletekey; Tasks: associate
Root: HKA; Subkey: "Software\Classes\Applications\ibisOffice.exe\shell\open\command"; ValueType: string; ValueData: """{app}\ibisOffice.exe"" ""%1"""; Tasks: associate
Root: HKA; Subkey: "Software\Classes\Applications\ibisOffice.exe\SupportedTypes"; ValueType: string; ValueName: ".docx"; ValueData: ""; Tasks: associate
Root: HKA; Subkey: "Software\Classes\Applications\ibisOffice.exe\SupportedTypes"; ValueType: string; ValueName: ".docm"; ValueData: ""; Tasks: associate
Root: HKA; Subkey: "Software\Classes\Applications\ibisOffice.exe\SupportedTypes"; ValueType: string; ValueName: ".xlsx"; ValueData: ""; Tasks: associate
Root: HKA; Subkey: "Software\Classes\Applications\ibisOffice.exe\SupportedTypes"; ValueType: string; ValueName: ".xlsm"; ValueData: ""; Tasks: associate
Root: HKA; Subkey: "Software\Classes\Applications\ibisOffice.exe\SupportedTypes"; ValueType: string; ValueName: ".pptx"; ValueData: ""; Tasks: associate
Root: HKA; Subkey: "Software\Classes\Applications\ibisOffice.exe\SupportedTypes"; ValueType: string; ValueName: ".pptm"; ValueData: ""; Tasks: associate

Root: HKA; Subkey: "Software\ibisOffice"; Flags: uninsdeletekeyifempty; Tasks: associate
Root: HKA; Subkey: "Software\ibisOffice\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "ibisOffice"; Flags: uninsdeletekey; Tasks: associate
Root: HKA; Subkey: "Software\ibisOffice\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "{cm:Description}"; Tasks: associate
Root: HKA; Subkey: "Software\ibisOffice\Capabilities\FileAssociations"; ValueType: string; ValueName: ".docx"; ValueData: "ibisOffice.Document"; Tasks: associate
Root: HKA; Subkey: "Software\ibisOffice\Capabilities\FileAssociations"; ValueType: string; ValueName: ".docm"; ValueData: "ibisOffice.Document"; Tasks: associate
Root: HKA; Subkey: "Software\ibisOffice\Capabilities\FileAssociations"; ValueType: string; ValueName: ".xlsx"; ValueData: "ibisOffice.Workbook"; Tasks: associate
Root: HKA; Subkey: "Software\ibisOffice\Capabilities\FileAssociations"; ValueType: string; ValueName: ".xlsm"; ValueData: "ibisOffice.Workbook"; Tasks: associate
Root: HKA; Subkey: "Software\ibisOffice\Capabilities\FileAssociations"; ValueType: string; ValueName: ".pptx"; ValueData: "ibisOffice.Presentation"; Tasks: associate
Root: HKA; Subkey: "Software\ibisOffice\Capabilities\FileAssociations"; ValueType: string; ValueName: ".pptm"; ValueData: "ibisOffice.Presentation"; Tasks: associate
Root: HKA; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "ibisOffice"; ValueData: "Software\ibisOffice\Capabilities"; Flags: uninsdeletevalue; Tasks: associate

[Run]
Filename: "{app}\ibisOffice.exe"; Description: "{cm:LaunchProgram,ibisOffice}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; The folder as the last update left it (the files of a newer version than
; this installer's are not in its log), the update's leftovers beside it, and
; the downloads. The settings in %APPDATA%\ibisOffice are kept.
Type: filesandordirs; Name: "{app}"
Type: filesandordirs; Name: "{app}.update-backup"
Type: filesandordirs; Name: "{app}.update-new"
Type: filesandordirs; Name: "{localappdata}\ibisOffice\Update"
