#!/bin/sh
# Add MyConnect to your application menu, with its icon, pointing at this
# unpacked bundle. Only affects the current user (~/.local/share). Run it
# again if you move the bundle; run it with --uninstall to remove the entry.
set -eu

app_id=org.myconnect.myconnect_ui
bundle=$(cd "$(dirname "$0")" && pwd)
data=${XDG_DATA_HOME:-$HOME/.local/share}
desktop_file=$data/applications/$app_id.desktop
icons=$data/icons/hicolor

refresh() {
  if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q "$data/applications" || true
  fi
  if command -v gtk-update-icon-cache >/dev/null 2>&1 &&
    [ -f "$icons/index.theme" ]; then
    gtk-update-icon-cache -q -t "$icons" || true
  fi
  # Icon themes without a cache are rescanned when the directory changes.
  touch "$icons" 2>/dev/null || true
}

if [ "${1:-}" = "--uninstall" ]; then
  rm -f "$desktop_file"
  find "$icons" -name "$app_id.*" -type f -delete 2>/dev/null || true
  refresh
  echo "Removed MyConnect from the application menu."
  exit 0
fi

# The Exec line quotes the path, but these characters would need escaping
# that isn't worth getting subtly wrong.
case $bundle in
  *[\"\`\$\\]* | *"
"*)
    echo "install.sh: move the bundle to a path without \", \`, \$, \\ or newlines first: $bundle" >&2
    exit 1
    ;;
esac

mkdir -p "$data/applications" "$icons"
cp -R "$bundle/share/icons/hicolor/." "$icons/"
awk -v exec="\"$bundle/myconnect_ui\"" \
  '/^Exec=/ { print "Exec=" exec; next } /^# install.sh/ { next } { print }' \
  "$bundle/share/applications/$app_id.desktop" >"$desktop_file"
refresh
echo "Added MyConnect to the application menu ($desktop_file)."
