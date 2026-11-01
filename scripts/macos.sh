#!/bin/sh
set -eu
name=$1
program=$2
shift 2
[ $# -gt 0 ] || set -- "target/release/$program"
version=$(awk -F'"' '/^\[/ { section = $0 } section == "[workspace.package]" && /^version *=/ { print $2; exit }' Cargo.toml)
[ -n "$version" ]
app="dist/$name.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS"
if [ $# -gt 1 ]; then
  lipo -create -output "$app/Contents/MacOS/$program" "$@"
else
  cp "$1" "$app/Contents/MacOS/$program"
fi
if [ -n "${NOTICES:-}" ]; then
  mkdir -p "$app/Contents/Resources"
  cp "$NOTICES" "$app/Contents/Resources/"
fi
mkdir -p "$app/Contents/Resources" dist/icon.iconset
for size in 16 32 128 256 512; do
  rsvg-convert -w $size -h $size "apps/$program/icon.svg" -o "dist/icon.iconset/icon_${size}x${size}.png"
  rsvg-convert -w $((size * 2)) -h $((size * 2)) "apps/$program/icon.svg" -o "dist/icon.iconset/icon_${size}x${size}@2x.png"
done
iconutil -c icns dist/icon.iconset -o "$app/Contents/Resources/$program.icns"
rm -r dist/icon.iconset
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$name</string>
  <key>CFBundleIdentifier</key><string>dev.nextzlog.$program</string>
  <key>CFBundleExecutable</key><string>$program</string>
  <key>CFBundleIconFile</key><string>$program</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>NSMicrophoneUsageDescription</key><string>zLog decodes Morse code from the audio input.</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
codesign --force --deep --sign - "$app"
echo "$app"
