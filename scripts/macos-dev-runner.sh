#!/bin/sh
# Cargo runner for `make dev` on macOS: starts the debug binary from inside a minimal
# "Forge Dev.app", so macOS knows it as the handler of forge:// links (the scheme is
# registered through Info.plist; the deep-link plugin cannot register it at runtime on macOS).
set -eu

bin="$1"
shift
app="$(dirname "$bin")/Forge Dev.app"
macos="$app/Contents/MacOS"

mkdir -p "$macos"
ln -sf "$bin" "$macos/forge"
cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>dev.forge.ide.dev</string>
  <key>CFBundleName</key><string>Forge Dev</string>
  <key>CFBundleExecutable</key><string>forge</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleURLTypes</key>
  <array>
    <dict>
      <key>CFBundleURLName</key><string>Forge</string>
      <key>CFBundleURLSchemes</key><array><string>forge</string></array>
    </dict>
  </array>
</dict>
</plist>
PLIST

lsregister=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
"$lsregister" -f "$app" >/dev/null 2>&1 || true

exec "$macos/forge" "$@"
