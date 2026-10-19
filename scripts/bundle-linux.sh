#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (c) ibis inc.
# Additional terms under AGPL-3.0 section 7(b): see NOTICE.
# Build the Linux version as an AppImage,
# target/dist/ibisOffice-<version>-Linux.AppImage, with its SHA-256 beside it
# (.sha256, which the automatic update checks).
#
#   scripts/bundle-linux.sh
#
# The AppImage is ibisOffice's own runtime (crates/ibis-appimage: no FUSE,
# no libfuse, which is LGPL) with the AppDir after it as a SquashFS image:
#
#   AppRun -> usr/bin/ibisOffice
#   ibisoffice.desktop, ibisoffice.png, .DirIcon   (what AppImage tools read)
#   usr/bin/ibisOffice
#   usr/share/icons/hicolor/<n>x<n>/apps/ibisoffice.png   (Windows' icon without its plate)
#   usr/share/ibisOffice/artborders/   (ECMA-376's art borders, unmodified, with its notice)
#   usr/share/doc/ibisOffice/          (ibisOffice's licences, the third-party ones)
#
# Built on the oldest Linux it is to run on (glibc is linked dynamically):
# built on Ubuntu 20.04 it needs glibc 2.30, so Ubuntu 20.04, Debian 11 and later.
set -euo pipefail
cd "$(dirname "$0")/.."

# The paths of this machine are not to go into the executables (see
# scripts/bundle-macos.sh), in a target directory of their own.
REMAP="--remap-path-prefix=$HOME/.cargo/registry/src=crates.io --remap-path-prefix=$HOME/.cargo/git/checkouts=git --remap-path-prefix=$HOME/.rustup=rustup --remap-path-prefix=$(pwd)=."
RUSTFLAGS="$REMAP" CARGO_TARGET_DIR=target/dist cargo build --release --locked -p ibis-office-app -p ibis-appimage

VERSION=$(awk -F'\t' '$1 == "Linux" { sub(/\r$/, "", $2); print $2 }' version.txt)
[[ -n $VERSION ]] || { echo "version.txt has no Linux line" >&2; exit 1; }

APPDIR=target/dist/linux/AppDir
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/doc/ibisOffice" "$APPDIR/usr/share/ibisOffice/artborders"
# Without the symbols (half of the file): panics still name their place.
strip -o "$APPDIR/usr/bin/ibisOffice" target/dist/release/ibisOffice
ln -s usr/bin/ibisOffice "$APPDIR/AppRun"

cp crates/ibis-office-app/linux/ibisoffice.desktop "$APPDIR/ibisoffice.desktop"
ICONS=crates/ibis-office-app/windows/png/ico
for size in 16 24 32 48 256; do
	mkdir -p "$APPDIR/usr/share/icons/hicolor/${size}x${size}/apps"
	cp "$ICONS/ibisOffice_icon_win_${size}px_transparent.png" "$APPDIR/usr/share/icons/hicolor/${size}x${size}/apps/ibisoffice.png"
done
cp "$ICONS/ibisOffice_icon_win_256px_transparent.png" "$APPDIR/ibisoffice.png"
ln -s ibisoffice.png "$APPDIR/.DirIcon"

# The pictures of Word's art page borders, read when a document uses one.
cp crates/ibis-office-app/data/artborders/OfficeOpenXML-WordprocessingMLArtBorders.zip crates/ibis-office-app/data/artborders/ECMA-NOTICE.txt "$APPDIR/usr/share/ibisOffice/artborders/"

# ibisOffice's licences, and the third-party components in the AppImage (its
# runtime's too) with the notices their licences ask for, made afresh from
# the dependencies; Carlito's licence on its own (OFL-1.1 asks for it with the font).
python3 scripts/gen-web-licenses.py linux
DOC="$APPDIR/usr/share/doc/ibisOffice"
cp publish/LICENSE "$DOC/LICENSE"
cp publish/COMMERCIAL-LICENSE.md "$DOC/COMMERCIAL-LICENSE.md"
[[ -f publish/NOTICE ]] && cp publish/NOTICE "$DOC/NOTICE"
cp crates/ibis-office-app/linux/license.md "$DOC/third-party-licenses.md"
cp crates/ibis-office-app/fonts/carlito/OFL.txt "$DOC/Carlito-OFL.txt"

OUT="target/dist/ibisOffice-$VERSION-Linux.AppImage"
rm -f "$OUT"
# The runtime is stripped too: nothing may follow its ELF part (the image is found there).
strip -o target/dist/linux/runtime target/dist/release/ibis-appimage-runtime
target/dist/release/ibis-appimage-pack target/dist/linux/runtime "$APPDIR" "$OUT"
(cd target/dist && sha256sum "$(basename "$OUT")" > "$(basename "$OUT").sha256")
ls -l "$OUT"
cat "$OUT.sha256"
