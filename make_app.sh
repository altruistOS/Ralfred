#!/bin/bash
# Package ralfred as a standalone .app — clean-room, no Alfred assets.
# Usage: bash make_app.sh
set -euo pipefail
cd "$(dirname "$0")"
export PATH=/opt/homebrew/bin:$PATH

APP=/Volumes/Elements-Mini/GitHub/Ralfred.app
BIN=target/release/ralfred

# React front end first (its bundle is embedded by include_bytes!)
if command -v npm >/dev/null 2>&1; then
  ( cd web && npm install --no-audit --no-fund && npm run build ) \
    || echo "WARN: web build failed — embedding whatever dist/ already has"
fi

# binary
cargo build --release

# fresh bundle
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

# binary (release, universal-friendly: just the local arch for now)
cp "$BIN" "$APP/Contents/MacOS/ralfred"
chmod +x "$APP/Contents/MacOS/ralfred"

# icon
if [ ! -f assets/ralfred.icns ]; then
  swift make_icon.swift >/dev/null
  iconutil -c icns AppIcon.iconset -o assets/ralfred.icns
fi
cp assets/ralfred.icns "$APP/Contents/Resources/ralfred.icns"

# Info.plist — LSUIElement: no Dock icon, the panel is summoned by hotkey
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Ralfred</string>
  <key>CFBundleDisplayName</key><string>Ralfred</string>
  <key>CFBundleIdentifier</key><string>com.kleo.ralfred</string>
  <key>CFBundleVersion</key><string>0.14</string>
  <key>CFBundleShortVersionString</key><string>0.14</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleExecutable</key><string>ralfred</string>
  <key>CFBundleIconFile</key><string>ralfred</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
  <key>LSUIElement</key><true/>
  <key>NSSupportsAutomaticTermination</key><false/>
  <key>NSSupportsSuddenTermination</key><false/>
  <key>NSAppleEventsUsageDescription</key>
  <string>Ralfred searches Safari bookmarks and controls Music on your behalf.</string>
  <key>NSContactsUsageDescription</key>
  <string>Ralfred searches your contacts so you can call or email them.</string>
</dict>
</plist>
PLIST

# ad-hoc signature so TCC stable codesigning works
codesign --force --deep -s - "$APP" 2>/dev/null || codesign --force -s - "$APP"

# register it at a predictable place + open it now
if [ "${INSTALL:-0}" = "1" ]; then
  rm -rf /Applications/Ralfred.app
  cp -R "$APP" /Applications/Ralfred.app
  echo "installed to /Applications/Ralfred.app"
  open /Applications/Ralfred.app
else
  open "$APP"
fi
echo "bundled: $APP"
