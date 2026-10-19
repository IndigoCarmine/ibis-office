; SPDX-License-Identifier: AGPL-3.0-only
; Copyright (c) ibis inc.
; Additional terms under AGPL-3.0 section 7(b): see NOTICE.
; The Windows installer (Inno Setup 6), built by scripts/bundle-windows.ps1:
;
;   ISCC.exe /DAppVersion=0.8.1 /DSourceDir=<the Acauda Office folder> /O<output> AcaudaOffice.iss
;
; It installs the same Acauda Office folder as the release zip, for the current
; user only (no administrator rights), in %LOCALAPPDATA%\Programs\AcaudaOffice:
; the automatic update (AcaudaOfficeUpdater) swaps that whole folder for the one
; in the new zip, so it has to be where the user can write, and nothing the
; installer adds may live inside it. The uninstaller is therefore kept in
; %LOCALAPPDATA%\AcaudaOffice\Uninstall, beside the update's own folders.

#ifndef AppVersion
  #error Pass the version: /DAppVersion=<version>
#endif
#ifndef SourceDir
  #error Pass the folder to install: /DSourceDir=<the Acauda Office folder>
#endif

[Setup]
; The same for every version, so that a newer installer updates the install in place.
AppId={{BAFFE743-D96C-48D5-BFCF-36820A4DC6FA}
AppName=Acauda Office
AppVersion={#AppVersion}
AppPublisher=IndigoCarmine
AppPublisherURL=https://github.com/IndigoCarmine/ibis-office
AppSupportURL=https://github.com/IndigoCarmine/ibis-office/issues
AppUpdatesURL=https://github.com/IndigoCarmine/ibis-office/releases
VersionInfoVersion={#AppVersion}
PrivilegesRequired=lowest
DefaultDirName={localappdata}\Programs\AcaudaOffice
DisableProgramGroupPage=yes
UninstallFilesDir={localappdata}\AcaudaOffice\Uninstall
UninstallDisplayIcon={app}\AcaudaOffice.exe
UninstallDisplayName=Acauda Office
SetupIconFile=AcaudaOffice.ico
LicenseFile=..\..\..\LICENSE
MinVersion=10.0
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
ChangesAssociations=yes
CloseApplications=yes
OutputBaseFilename=AcaudaOffice-{#AppVersion}-Windows-Setup
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern

[Languages]
Name: "japanese"; MessagesFile: "compiler:Languages\Japanese.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
japanese.Associate=Word・Excel・PowerPoint のファイル（.docx・.xlsx・.pptx など）の「プログラムから開く」に Acauda Office を加える
english.Associate=Add Acauda Office to "Open with" for Word, Excel and PowerPoint files (.docx, .xlsx, .pptx and others)
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
Name: "{autoprograms}\Acauda Office"; Filename: "{app}\AcaudaOffice.exe"
Name: "{autodesktop}\Acauda Office"; Filename: "{app}\AcaudaOffice.exe"; Tasks: desktopicon

[Registry]
; As on macOS (LSHandlerRank Alternate), Acauda Office is offered for the files
; but does not take them from the app that opens them now: a ProgID for each
; kind, listed under each extension's OpenWithProgids, and the capabilities
; that Settings > Apps > Default apps shows, where the user can choose it.
Root: HKA; Subkey: "Software\Classes\AcaudaOffice.Document"; ValueType: string; ValueData: "{cm:Document}"; Flags: uninsdeletekey; Tasks: associate
Root: HKA; Subkey: "Software\Classes\AcaudaOffice.Document\DefaultIcon"; ValueType: string; ValueData: "{app}\AcaudaOffice.exe,0"; Tasks: associate
Root: HKA; Subkey: "Software\Classes\AcaudaOffice.Document\shell\open\command"; ValueType: string; ValueData: """{app}\AcaudaOffice.exe"" ""%1"""; Tasks: associate
Root: HKA; Subkey: "Software\Classes\AcaudaOffice.Workbook"; ValueType: string; ValueData: "{cm:Workbook}"; Flags: uninsdeletekey; Tasks: associate
Root: HKA; Subkey: "Software\Classes\AcaudaOffice.Workbook\DefaultIcon"; ValueType: string; ValueData: "{app}\AcaudaOffice.exe,0"; Tasks: associate
Root: HKA; Subkey: "Software\Classes\AcaudaOffice.Workbook\shell\open\command"; ValueType: string; ValueData: """{app}\AcaudaOffice.exe"" ""%1"""; Tasks: associate
Root: HKA; Subkey: "Software\Classes\AcaudaOffice.Presentation"; ValueType: string; ValueData: "{cm:Presentation}"; Flags: uninsdeletekey; Tasks: associate
Root: HKA; Subkey: "Software\Classes\AcaudaOffice.Presentation\DefaultIcon"; ValueType: string; ValueData: "{app}\AcaudaOffice.exe,0"; Tasks: associate
Root: HKA; Subkey: "Software\Classes\AcaudaOffice.Presentation\shell\open\command"; ValueType: string; ValueData: """{app}\AcaudaOffice.exe"" ""%1"""; Tasks: associate

Root: HKA; Subkey: "Software\Classes\.docx\OpenWithProgids"; ValueType: string; ValueName: "AcaudaOffice.Document"; ValueData: ""; Flags: uninsdeletevalue; Tasks: associate
Root: HKA; Subkey: "Software\Classes\.docm\OpenWithProgids"; ValueType: string; ValueName: "AcaudaOffice.Document"; ValueData: ""; Flags: uninsdeletevalue; Tasks: associate
Root: HKA; Subkey: "Software\Classes\.xlsx\OpenWithProgids"; ValueType: string; ValueName: "AcaudaOffice.Workbook"; ValueData: ""; Flags: uninsdeletevalue; Tasks: associate
Root: HKA; Subkey: "Software\Classes\.xlsm\OpenWithProgids"; ValueType: string; ValueName: "AcaudaOffice.Workbook"; ValueData: ""; Flags: uninsdeletevalue; Tasks: associate
Root: HKA; Subkey: "Software\Classes\.pptx\OpenWithProgids"; ValueType: string; ValueName: "AcaudaOffice.Presentation"; ValueData: ""; Flags: uninsdeletevalue; Tasks: associate
Root: HKA; Subkey: "Software\Classes\.pptm\OpenWithProgids"; ValueType: string; ValueName: "AcaudaOffice.Presentation"; ValueData: ""; Flags: uninsdeletevalue; Tasks: associate

Root: HKA; Subkey: "Software\Classes\Applications\AcaudaOffice.exe"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "Acauda Office"; Flags: uninsdeletekey; Tasks: associate
Root: HKA; Subkey: "Software\Classes\Applications\AcaudaOffice.exe\shell\open\command"; ValueType: string; ValueData: """{app}\AcaudaOffice.exe"" ""%1"""; Tasks: associate
Root: HKA; Subkey: "Software\Classes\Applications\AcaudaOffice.exe\SupportedTypes"; ValueType: string; ValueName: ".docx"; ValueData: ""; Tasks: associate
Root: HKA; Subkey: "Software\Classes\Applications\AcaudaOffice.exe\SupportedTypes"; ValueType: string; ValueName: ".docm"; ValueData: ""; Tasks: associate
Root: HKA; Subkey: "Software\Classes\Applications\AcaudaOffice.exe\SupportedTypes"; ValueType: string; ValueName: ".xlsx"; ValueData: ""; Tasks: associate
Root: HKA; Subkey: "Software\Classes\Applications\AcaudaOffice.exe\SupportedTypes"; ValueType: string; ValueName: ".xlsm"; ValueData: ""; Tasks: associate
Root: HKA; Subkey: "Software\Classes\Applications\AcaudaOffice.exe\SupportedTypes"; ValueType: string; ValueName: ".pptx"; ValueData: ""; Tasks: associate
Root: HKA; Subkey: "Software\Classes\Applications\AcaudaOffice.exe\SupportedTypes"; ValueType: string; ValueName: ".pptm"; ValueData: ""; Tasks: associate

Root: HKA; Subkey: "Software\AcaudaOffice"; Flags: uninsdeletekeyifempty; Tasks: associate
Root: HKA; Subkey: "Software\AcaudaOffice\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "Acauda Office"; Flags: uninsdeletekey; Tasks: associate
Root: HKA; Subkey: "Software\AcaudaOffice\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "{cm:Description}"; Tasks: associate
Root: HKA; Subkey: "Software\AcaudaOffice\Capabilities\FileAssociations"; ValueType: string; ValueName: ".docx"; ValueData: "AcaudaOffice.Document"; Tasks: associate
Root: HKA; Subkey: "Software\AcaudaOffice\Capabilities\FileAssociations"; ValueType: string; ValueName: ".docm"; ValueData: "AcaudaOffice.Document"; Tasks: associate
Root: HKA; Subkey: "Software\AcaudaOffice\Capabilities\FileAssociations"; ValueType: string; ValueName: ".xlsx"; ValueData: "AcaudaOffice.Workbook"; Tasks: associate
Root: HKA; Subkey: "Software\AcaudaOffice\Capabilities\FileAssociations"; ValueType: string; ValueName: ".xlsm"; ValueData: "AcaudaOffice.Workbook"; Tasks: associate
Root: HKA; Subkey: "Software\AcaudaOffice\Capabilities\FileAssociations"; ValueType: string; ValueName: ".pptx"; ValueData: "AcaudaOffice.Presentation"; Tasks: associate
Root: HKA; Subkey: "Software\AcaudaOffice\Capabilities\FileAssociations"; ValueType: string; ValueName: ".pptm"; ValueData: "AcaudaOffice.Presentation"; Tasks: associate
Root: HKA; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "AcaudaOffice"; ValueData: "Software\AcaudaOffice\Capabilities"; Flags: uninsdeletevalue; Tasks: associate

[Run]
Filename: "{app}\AcaudaOffice.exe"; Description: "{cm:LaunchProgram,Acauda Office}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; The folder as the last update left it (the files of a newer version than
; this installer's are not in its log), the update's leftovers beside it, and
; the downloads. The settings in %APPDATA%\AcaudaOffice are kept.
Type: filesandordirs; Name: "{app}"
Type: filesandordirs; Name: "{app}.update-backup"
Type: filesandordirs; Name: "{app}.update-new"
Type: filesandordirs; Name: "{localappdata}\AcaudaOffice\Update"
