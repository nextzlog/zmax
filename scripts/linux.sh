#!/bin/sh
set -eu
name=$1
program=$2
binary=${3:-target/release/$program}
dir="dist/$program.AppDir"
rm -rf "$dir"
mkdir -p "$dir/usr/bin"
cp "$binary" "$dir/usr/bin/$program"
chmod +x "$dir/usr/bin/$program"
if [ -n "${NOTICES:-}" ]; then
  mkdir -p "$dir/usr/share/doc/$program"
  cp "$NOTICES" "$dir/usr/share/doc/$program/"
fi
cat > "$dir/AppRun" <<RUN
#!/bin/sh
here=\$(dirname "\$(readlink -f "\$0")")
exec "\$here/usr/bin/$program" "\$@"
RUN
chmod +x "$dir/AppRun"
cat > "$dir/$program.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=$name
Exec=$program
Icon=$program
Categories=Network;HamRadio;
Terminal=false
DESKTOP
rsvg-convert -w 256 -h 256 "apps/$program/icon.svg" -o "$dir/$program.png"
tool=${APPIMAGETOOL:-appimagetool}
if ! command -v "$tool" > /dev/null 2>&1; then
  echo "$dir (appimagetool not found)" >&2
  exit 1
fi
ARCH=x86_64 "$tool" "$dir" "dist/$program-x86_64.AppImage"
echo "dist/$program-x86_64.AppImage"
