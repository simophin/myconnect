#!/bin/sh
# Pack the release binaries into a .deb.
#
#   build_deb.sh APP CLI VERSION OUT_DIR
#
# APP is cargo's ferry-gui, installed under that name as /usr/bin/ferry-gui;
# CLI is the ferry command line, installed next to it as /usr/bin/ferry
# (docs/adr/0001, "Packaging").
# The menu entry and icons go to /usr/share. Runs on Debian or
# Ubuntu: it needs dpkg-deb, and dpkg-shlibdeps to work out the dependencies
# from the ELF files. Build on the oldest release you want to support, since
# the glibc it links against is the oldest one the package will install on.
set -eu

if [ $# -ne 4 ]; then
  echo "usage: $0 APP CLI VERSION OUT_DIR" >&2
  exit 2
fi
app=$1
cli=$2
version=$3
out_dir=$4

app_id=dev.fanchao.Ferry
packaging=$(cd "$(dirname "$0")" && pwd)
assets=$packaging/../../assets
arch=$(dpkg --print-architecture)

# The CLI must be built on its own: built together with the app, cargo
# turns the gui feature on for it too.
if LC_ALL=C grep -aq iced_winit "$cli"; then
  echo "build_deb.sh: $cli has the UI in it; build it with cargo build -p ferry alone" >&2
  exit 1
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
root=$work/root

mkdir -p "$root/usr/bin" "$root/usr/share/applications" \
  "$root/usr/share/icons" "$root/DEBIAN"
install -m 755 -s "$app" "$root/usr/bin/ferry-gui"
install -m 755 -s "$cli" "$root/usr/bin/ferry"
install -m 644 "$packaging/$app_id.desktop" "$root/usr/share/applications/"
cp -R "$assets/linux/hicolor" "$root/usr/share/icons/"
chmod -R u=rwX,go=rX "$root/usr/share"

# dpkg-shlibdeps wants to run from a source tree; give it a minimal one.
mkdir -p "$work/src/debian"
printf 'Source: ferry\n\nPackage: ferry\nArchitecture: any\n' \
  >"$work/src/debian/control"
depends=$(
  cd "$work/src" &&
    dpkg-shlibdeps -O "$root/usr/bin/ferry-gui" "$root/usr/bin/ferry" \
      2>"$work/shlibdeps.log" |
    sed -n 's/^shlibs:Depends=//p'
) || {
  cat "$work/shlibdeps.log" >&2
  exit 1
}
test -n "$depends"
# Loaded at runtime (dlopen), so dpkg-shlibdeps can't see them: winit's
# keyboard, Wayland and X11 libraries. Without a GPU driver the app draws in
# software, so the GPU's are only recommended; a font is needed to draw text.
depends="$depends, libxkbcommon0, libxkbcommon-x11-0, libwayland-client0, libx11-6, libx11-xcb1, libxcursor1, libxi6, libxrandr2, fontconfig, fonts-dejavu-core | fonts-freefont-ttf | fonts-liberation"
recommends="libvulkan1, mesa-vulkan-drivers | vulkan-icd, libegl1, xdg-desktop-portal"

cat >"$root/DEBIAN/control" <<CONTROL
Package: ferry
Version: $version
Architecture: $arch
Maintainer: fanchao <dev@fanchao.dev>
Installed-Size: $(du -sk --exclude=DEBIAN "$root" | cut -f1)
Depends: $depends
Recommends: $recommends
Section: net
Priority: optional
Homepage: https://github.com/simophin/ferryapp
Description: Pair with your devices and share files and the clipboard
 Ferry connects your computer with your phone and other devices on the
 local network, using the KDE Connect protocol: send files, share the
 clipboard and ping devices you have paired with. It comes with the
 ferry command line, which drives the running app.
CONTROL

mkdir -p "$out_dir"
package=$out_dir/ferry_${version}_$arch.deb
dpkg-deb --root-owner-group --build "$root" "$package"
echo "$package"
