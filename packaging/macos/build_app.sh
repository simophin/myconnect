#!/bin/sh
# Assemble MyConnect.app from the app's binaries and pack it into a DMG.
#
#   build_app.sh VERSION BUILD DMG BINARY...
#
# Each BINARY is cargo's myconnect-gui for one architecture; with more than
# one, lipo joins them into a universal binary. VERSION is MAJOR.MINOR.PATCH
# (macOS accepts nothing else) and BUILD a number. Writes the DMG, and leaves
# MyConnect.app next to it. Needs macOS: lipo, iconutil, codesign and
# hdiutil.
#
# The bundle is ad-hoc signed and not sandboxed (docs/PLAN_ICED_UI.md,
# "Deliberate differences"): Gatekeeper blocks it until the user allows it in
# System Settings → Privacy & Security.
set -eu

if [ $# -lt 4 ]; then
  echo "usage: $0 VERSION BUILD DMG BINARY..." >&2
  exit 2
fi
version=$1
build=$2
dmg=$3
shift 3
out_dir=$(dirname "$dmg")

packaging=$(cd "$(dirname "$0")" && pwd)
assets=$packaging/../../assets
# Keep in step with MACOSX_DEPLOYMENT_TARGET in the Build workflow.
minimum=${MACOSX_DEPLOYMENT_TARGET:-12.0}

mkdir -p "$out_dir"
app=$out_dir/MyConnect.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
lipo -create -output "$app/Contents/MacOS/myConnect" "$@"
sed -e "s|@VERSION@|$version|" -e "s|@BUILD@|$build|" \
  -e "s|@MINIMUM_SYSTEM_VERSION@|$minimum|" \
  "$packaging/Info.plist.in" >"$app/Contents/Info.plist"
plutil -lint "$app/Contents/Info.plist"
printf 'APPL????' >"$app/Contents/PkgInfo"
iconutil -c icns -o "$app/Contents/Resources/AppIcon.icns" \
  "$assets/macos/AppIcon.iconset"
codesign --force --sign - --identifier org.myconnect.MyConnect "$app"
codesign --verify --strict --verbose=2 "$app"

staging=$(mktemp -d)
trap 'rm -rf "$staging"' EXIT
cp -R "$app" "$staging/"
ln -s /Applications "$staging/Applications"
rm -f "$dmg"
hdiutil create -volname MyConnect -srcfolder "$staging" -format UDZO "$dmg"
echo "$dmg"
