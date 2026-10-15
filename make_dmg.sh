#!/usr/bin/env bash
# Package Ralfred.app into a compressed DMG (UDZO) ready to hand to users.
# Usage: bash make_dmg.sh [output-path]
# Notes: ad-hoc signature (no Apple Developer ID); first launch after
# download requires right-click > Open (Gatekeeper quarantine).
set -euo pipefail
cd "$(dirname "$0")"

VERSION=$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)"/\1/')
SRC=${SRC:-../Ralfred.app}                # built by make_app.sh (lives one level up)
OUT=${1:-dist/Ralfred-$VERSION.dmg}

if [ ! -d "$SRC" ]; then
  echo "missing $SRC — run make_app.sh first" >&2; exit 1
fi

STAGE=$(mktemp -d /tmp/rf-dmg-stage.XXXXXX)
trap 'rm -rf "$STAGE"' EXIT

cp -R "$SRC" "$STAGE/Ralfred.app"
ln -s /Applications "$STAGE/Applications"
cat > "$STAGE/README.txt" <<TXT
Ralfred $VERSION — a clean-room launcher for macOS
(React front end + Rust back end; inspired by Alfred-style launchers.
Not affiliated with or derived from any other product.)

Install: drag Ralfred.app into Applications, launch it, press
Option+Space (configurable in Settings, ⌘, ) to open the panel.
The app lives in the menu bar ("R!"); Settings > General chooses
whether it opens the panel or starts silently on launch.
TXT

codesign --force --deep --sign - "$STAGE/Ralfred.app" >/dev/null 2>&1 || true

mkdir -p "$(dirname "$OUT")"
hdiutil create -volname "Ralfred" -srcfolder "$STAGE" -ov -format UDZO "$OUT" >/dev/null
hdiutil verify "$OUT" >/dev/null && echo "dmg verified: $OUT"
ls -lh "$OUT" | awk '{print $NF, $5}'
