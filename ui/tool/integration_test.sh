#!/usr/bin/env bash
# Run the end-to-end tests in integration_test/ on a private X display and
# D-Bus session, so the app's window, tray icon and file dialogs stay off the
# desktop you're working on. This is also the command CI should run.
#
# Needs Xvfb (xvfb-run), dbus-run-session and a Rust toolchain. Extra
# arguments go to `flutter test`, e.g. `--plain-name 'rejects'`.
set -euo pipefail
cd "$(dirname "$0")/.."
exec dbus-run-session -- \
  xvfb-run --auto-servernum --server-args='-screen 0 1280x800x24' \
  env GDK_BACKEND=x11 flutter test integration_test -d linux "$@"
