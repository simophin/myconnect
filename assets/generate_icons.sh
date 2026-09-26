#!/usr/bin/env bash
# Render every platform's app icon, and the tray icons, from the SVGs in
# icon/. Run this after changing an SVG and commit the output.
#
# icon/app_icon.svg is the full drawing; icon/app_icon_small.svg is the same
# drawing simplified for 32 px and below, where the ribs and the wave would
# turn to mush.
#
# Needs rsvg-convert (librsvg) and ImageMagick 7 (`magick`).
set -euo pipefail
cd "$(dirname "$0")"

full=icon/app_icon.svg
small=icon/app_icon_small.svg
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# Drop timestamps and other metadata so unchanged icons re-render to the
# same bytes.
png_opts=(-strip -define png:exclude-chunks=date,time)

# render <svg> <size> <out.png>
render() {
  rsvg-convert --width "$2" --height "$2" "$1" --output "$3"
}

# Which drawing to use at a given pixel size.
svg_for() {
  if (($1 <= 32)); then echo "$small"; else echo "$full"; fi
}

# macOS: an iconset, which packaging/macos/build_app.sh turns into the
# bundle's AppIcon.icns with iconutil. Apple's template puts an 824 px tile
# on a 1024 px canvas, with a soft drop shadow below it. The SVG's tile is
# 240 of its 256 units, so widen the viewBox until the tile takes 824/1024
# of the canvas.
iconset=macos/AppIcon.iconset
mkdir -p "$iconset"
for size in 16 32 64 128 256 512 1024; do
  svg=$(svg_for "$size")
  sed 's/viewBox="0 0 256 256"/viewBox="-21.125 -21.125 298.25 298.25"/' \
    "$svg" >"$tmp/mac.svg"
  render "$tmp/mac.svg" "$size" "$tmp/mac.png"
  sigma=$(awk "BEGIN { printf \"%.2f\", $size / 100 }")
  offset=$(awk "BEGIN { printf \"%d\", $size / 80 + 0.5 }")
  magick "$tmp/mac.png" \
    \( +clone -background black -shadow "30x$sigma+0+$offset" \) \
    +swap -background none -layers merge +repage \
    -gravity center -extent "${size}x$size" \
    "${png_opts[@]}" "$tmp/mac_$size.png"
done
# iconutil's names: each point size at 1x and 2x.
for points in 16 32 128 256 512; do
  cp "$tmp/mac_$points.png" "$iconset/icon_${points}x$points.png"
  cp "$tmp/mac_$((points * 2)).png" "$iconset/icon_${points}x$points@2x.png"
done

# Windows: one .ico holding every size Explorer and the taskbar ask for,
# embedded in myConnect.exe (gui/build.rs) and used by the installer.
# ImageMagick stores the 256 px image as PNG and the rest as bitmaps.
mkdir -p windows
ico_parts=()
for size in 16 20 24 32 40 48 64 256; do
  render "$(svg_for "$size")" "$size" "$tmp/win_$size.png"
  ico_parts+=("$tmp/win_$size.png")
done
magick "${ico_parts[@]}" windows/app_icon.ico

# Linux: a hicolor icon theme, named after the application ID, which the
# .deb installs to /usr/share/icons for the window and the menu entry.
hicolor=linux/hicolor
app_id=org.myconnect.MyConnect
for size in 16 24 32 48 64 128 256 512; do
  mkdir -p "$hicolor/${size}x$size/apps"
  render "$(svg_for "$size")" "$size" "$tmp/linux.png"
  magick "$tmp/linux.png" "${png_opts[@]}" \
    "$hicolor/${size}x$size/apps/$app_id.png"
done
mkdir -p "$hicolor/scalable/apps"
cp "$full" "$hicolor/scalable/apps/$app_id.svg"

# The window's own icon (X11 and Windows title bars and task switchers).
render "$full" 128 "$tmp/window.png"
magick "$tmp/window.png" "${png_opts[@]}" window_icon.png

# Tray: drawn at 64 px and scaled down by the tray host, which usually
# shows it at 16 to 24 px, so it uses the small drawing.
render "$small" 64 "$tmp/tray.png"
magick "$tmp/tray.png" "${png_opts[@]}" tray_icon.png

# macOS menu bar: a template image, which macOS tints for a light or dark
# menu bar. Shown at 18 pt, so 36 px is sharp on Retina.
render icon/tray_icon_template.svg 36 "$tmp/template.png"
magick "$tmp/template.png" "${png_opts[@]}" tray_icon_template.png
