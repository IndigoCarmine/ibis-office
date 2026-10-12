#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (c) ibis inc.
# Additional terms under AGPL-3.0 section 7(b): see NOTICE.
# Build the macOS app icon, crates/ibis-office-app/macos/AppIcon.icns, from AppIcon.png.
#
#   scripts/gen-macos-icon.sh
#
# AppIcon.png is the master 1024x1024 macOS app icon design (squircle with shadow).
# Each size is downscaled with sips and packed into AppIcon.icns with iconutil.
set -euo pipefail
cd "$(dirname "$0")/.."

DIR=crates/ibis-office-app/macos
SRC="$DIR/AppIcon.png"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
ICONSET="$TMP/AppIcon.iconset"
mkdir -p "$ICONSET"

# sips -z <height> <width>
sips -z 16 16     "$SRC" --out "$ICONSET/icon_16x16.png" >/dev/null
sips -z 32 32     "$SRC" --out "$ICONSET/icon_16x16@2x.png" >/dev/null
sips -z 32 32     "$SRC" --out "$ICONSET/icon_32x32.png" >/dev/null
sips -z 64 64     "$SRC" --out "$ICONSET/icon_32x32@2x.png" >/dev/null
sips -z 128 128   "$SRC" --out "$ICONSET/icon_128x128.png" >/dev/null
sips -z 256 256   "$SRC" --out "$ICONSET/icon_128x128@2x.png" >/dev/null
sips -z 256 256   "$SRC" --out "$ICONSET/icon_256x256.png" >/dev/null
sips -z 512 512   "$SRC" --out "$ICONSET/icon_256x256@2x.png" >/dev/null
sips -z 512 512   "$SRC" --out "$ICONSET/icon_512x512.png" >/dev/null
cp "$SRC" "$ICONSET/icon_512x512@2x.png"

iconutil -c icns -o "$DIR/AppIcon.icns" "$ICONSET"
echo "Wrote $DIR/AppIcon.icns"
