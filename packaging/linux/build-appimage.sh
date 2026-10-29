#!/usr/bin/env bash
# Packs target/release/thejar into dist/TheJar-<version>.AppImage.
#
#   packaging/linux/build-appimage.sh 0.1.0
#
# Set APPIMAGETOOL to the appimagetool to use; it defaults to the one on PATH.
set -euo pipefail

version=${1:?Pass the version, such as 0.1.0}
tool=${APPIMAGETOOL:-appimagetool}
root=$(cd "$(dirname "$0")/../.." && pwd)
appdir=$(mktemp -d)/TheJar.AppDir

mkdir -p "$appdir/usr/bin" "$appdir/usr/share/icons/hicolor/256x256/apps" "$root/dist"
install -m 755 "$root/target/release/thejar" "$appdir/usr/bin/thejar"
install -m 644 "$root/assets/icon.png" "$appdir/thejar.png"
install -m 644 "$root/assets/icon.png" "$appdir/usr/share/icons/hicolor/256x256/apps/thejar.png"
install -m 644 "$root/packaging/linux/thejar.desktop" "$appdir/thejar.desktop"
ln -s usr/bin/thejar "$appdir/AppRun"

ARCH=x86_64 "$tool" --no-appstream "$appdir" "$root/dist/TheJar-$version.AppImage"
echo "Built dist/TheJar-$version.AppImage"
