# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (c) ibis inc.
# Additional terms under AGPL-3.0 section 7(b): see NOTICE.
# Build the desktop version for Windows: the ibisOffice folder, the release zip
# the automatic update downloads (with its SHA-256), and the installer.
#
#   pwsh scripts/bundle-windows.ps1
#
# Writes, in target/dist:
#   package/ibisOffice/                     the folder (ibisOffice.exe, ibisOfficeUpdater.exe, Resources)
#   ibisOffice-<version>-Windows.zip        that folder, as the update expects it
#   ibisOffice-<version>-Windows.zip.sha256
#   ibisOffice-<version>-Windows-Setup.exe  the installer (crates/ibis-office-app/windows/ibisOffice.iss)
#
# UI icons and fonts are embedded in the executable; the folder adds the
# third-party licences (Resources\Licenses), the art borders' zip
# (Resources\artborders) and the automatic update's ibisOfficeUpdater.exe.
# The installer needs Inno Setup 6 (ISCC.exe on PATH, or in its usual folder).
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
Set-Location (Split-Path -Parent $PSScriptRoot)
$root = (Get-Location).Path

# The paths of this machine (the home folder, the repository) are not to go
# into the executable (Rust keeps source paths for panics and the like): they
# are rewritten to short relative ones, in a target directory of its own so
# that the everyday builds (target/release) are not rebuilt by the flags.
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
$rustupHome = if ($env:RUSTUP_HOME) { $env:RUSTUP_HOME } else { Join-Path $env:USERPROFILE '.rustup' }
$env:RUSTFLAGS = @(
    "--remap-path-prefix=$cargoHome\registry\src=crates.io"
    "--remap-path-prefix=$cargoHome\git\checkouts=git"
    "--remap-path-prefix=$rustupHome=rustup"
    "--remap-path-prefix=$root=."
) -join ' '
$env:CARGO_TARGET_DIR = 'target/dist'
cargo build --release --locked -p ibis-office-app -p ibis-office-updater

# The version shown for Windows, from version.txt (a platform, a tab and its
# version, per line).
$version = Get-Content version.txt | ForEach-Object { $f = $_ -split "`t"; if ($f[0] -eq 'Windows') { $f[1].Trim() } } | Select-Object -First 1
if (-not $version) { throw 'version.txt has no Windows line' }

$dist = Join-Path $root 'target\dist'
$app = Join-Path $dist 'package\ibisOffice'
if (Test-Path $app) { Remove-Item -Recurse -Force $app }
New-Item -ItemType Directory -Force "$app\Resources\Licenses", "$app\Resources\artborders" | Out-Null
Copy-Item "$dist\release\ibisOffice.exe" "$app\ibisOffice.exe"
# The automatic update: ibisOffice starts the ibisOfficeUpdater.exe of the NEW
# folder it downloaded, which swaps the folders.
Copy-Item "$dist\release\ibisOfficeUpdater.exe" "$app\ibisOfficeUpdater.exe"
# The third-party components in the app and the notices their licences ask
# for (made afresh from the dependencies), and Carlito's licence on its own
# (OFL-1.1 asks for it with the font).
python scripts/gen-web-licenses.py windows
Copy-Item crates\ibis-office-app\windows\license.md "$app\Resources\Licenses\license.md"
Copy-Item crates\ibis-office-app\fonts\carlito\OFL.txt "$app\Resources\Licenses\Carlito-OFL.txt"
# The pictures of Word's art page borders: ECMA-376's zip, unmodified, with
# its notice (read when a document uses an art border; not in the executable).
Copy-Item crates\ibis-office-app\data\artborders\OfficeOpenXML-WordprocessingMLArtBorders.zip, crates\ibis-office-app\data\artborders\ECMA-NOTICE.txt "$app\Resources\artborders\"

# The release zip: the one ibisOffice folder at its top, as the update looks
# for it, made with the same tar that unpacks it.
$zipName = "ibisOffice-$version-Windows.zip"
$zip = Join-Path $dist $zipName
if (Test-Path $zip) { Remove-Item -Force $zip }
& "$env:SystemRoot\System32\tar.exe" -a -c -f $zip -C (Split-Path -Parent $app) ibisOffice
# "<hex>  <name>", as the update reads it from the .sha256 beside the zip.
$hash = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLowerInvariant()
[System.IO.File]::WriteAllText("$zip.sha256", "$hash  $zipName`n")

$iscc = (Get-Command ISCC.exe -ErrorAction SilentlyContinue).Source
if (-not $iscc) { $iscc = Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe' }
if (-not (Test-Path $iscc)) { throw 'Inno Setup 6 (ISCC.exe) is not installed: https://jrsoftware.org/isdl.php' }
& $iscc /Q "/DAppVersion=$version" "/DSourceDir=$app" "/O$dist" crates\ibis-office-app\windows\ibisOffice.iss

Write-Host "Built, in $dist`:"
Write-Host "  $zipName (and .sha256)"
Write-Host "  ibisOffice-$version-Windows-Setup.exe"
