#!/bin/sh
# Pack a release bundle (build/linux/<arch>/release/bundle) into a .deb.
#
#   build_deb.sh BUNDLE VERSION OUT_DIR
#
# The bundle goes to /usr/lib/myconnect unchanged (it finds its libraries
# through $ORIGIN/lib), with the launcher on the PATH and the menu entry and
# icons in /usr/share. Runs on Debian or Ubuntu: it needs dpkg-deb, and
# dpkg-shlibdeps to work out the dependencies from the ELF files. Build on the
# oldest release you want to support, since the glibc it links against is the
# oldest one the package will install on.
set -eu

if [ $# -ne 3 ]; then
  echo "usage: $0 BUNDLE VERSION OUT_DIR" >&2
  exit 2
fi
bundle=$(cd "$1" && pwd)
version=$2
out_dir=$3

app_id=org.myconnect.myconnect_ui
packaging=$(cd "$(dirname "$0")" && pwd)
arch=$(dpkg --print-architecture)

test -x "$bundle/myconnect_ui"
test -f "$bundle/lib/libmyconnect_ffi.so"
# A RUNPATH into the build tree would load libraries from wherever that path
# happens to exist on the user's machine.
for elf in "$bundle/myconnect_ui" "$bundle"/lib/*.so; do
  if readelf -d "$elf" | grep -E 'R(UN)?PATH' | grep -v '\[\$ORIGIN[^]:]*\]'; then
    echo "build_deb.sh: $elf has a RUNPATH outside the bundle" >&2
    exit 1
  fi
done

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
root=$work/root
lib=$root/usr/lib/myconnect

mkdir -p "$lib" "$root/usr/bin" "$root/usr/share/applications" \
  "$root/usr/share/icons" "$root/DEBIAN"
cp -R "$bundle/." "$lib/"
# The bundle's own menu integration is for unpacked tarballs; the package
# installs the same files system-wide instead.
rm -rf "$lib/install.sh" "$lib/share"
ln -s ../lib/myconnect/myconnect_ui "$root/usr/bin/myconnect_ui"
grep -v '^# install.sh' "$packaging/$app_id.desktop" \
  >"$root/usr/share/applications/$app_id.desktop"
cp -R "$packaging/icons/hicolor" "$root/usr/share/icons/"

# dpkg-shlibdeps wants to run from a source tree; give it a minimal one. The
# bundled libraries (Flutter engine, plugins, the core) aren't from any
# package, so it's told where they are and not to look for their packages.
mkdir -p "$work/src/debian"
printf 'Source: myconnect\n\nPackage: myconnect\nArchitecture: any\n' \
  >"$work/src/debian/control"
depends=$(
  cd "$work/src" &&
    dpkg-shlibdeps -O --ignore-missing-info -l"$lib/lib" \
      "$lib/myconnect_ui" "$lib"/lib/*.so 2>"$work/shlibdeps.log" |
    sed -n 's/^shlibs:Depends=//p'
) || {
  cat "$work/shlibdeps.log" >&2
  exit 1
}
test -n "$depends"
# Loaded at runtime (dlopen), so dpkg-shlibdeps can't see them: the Flutter
# engine renders through EGL and GLES and aborts without them.
depends="$depends, libegl1, libgles2"

cat >"$root/DEBIAN/control" <<EOF
Package: myconnect
Version: $version
Architecture: $arch
Maintainer: fanchao <dev@fanchao.dev>
Installed-Size: $(du -sk --exclude=DEBIAN "$root" | cut -f1)
Depends: $depends
Section: net
Priority: optional
Homepage: https://github.com/simophin/myconnect
Description: Pair with your devices and share files and the clipboard
 MyConnect connects your computer with your phone and other devices on the
 local network, using the KDE Connect protocol: send files, share the
 clipboard and ping devices you have paired with.
EOF

mkdir -p "$out_dir"
package=$out_dir/myconnect_${version}_$arch.deb
dpkg-deb --root-owner-group --build "$root" "$package"
echo "$package"
