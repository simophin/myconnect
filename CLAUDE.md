# Agent instructions

Start with [`docs/HANDOFF.md`](docs/HANDOFF.md): ground rules, the "done
means" checks, and how to verify in the real app.

## Isolate every run

Several agents may work on this repo at once, each in its own git worktree,
and the owner runs the real app on the same machine. Anything you launch
(the app, a CLI daemon, integration tests) must not touch shared state or
collide with another run. Do this every time, without being asked:

- **Data.** Give every daemon a fresh directory made with `mktemp -d` under
  your scratchpad, never the default config dir (`~/.config/MyConnect`, the
  owner's real identity and trust) and never a fixed path like `/tmp/ui`
  that another session may be using:
  - CLI and the iced app (`myconnect-gui`): `--data-dir "$dir/data"
    --download-dir "$dir/downloads"`.
  - Flutter app: `--dart-define=MYCONNECT_DATA_DIR=$dir/data` and
    `--dart-define=MYCONNECT_DOWNLOAD_DIR=$dir/downloads`.
  - Without the download dir, received files land in the owner's
    `~/Downloads`.
  - `integration_test/` already makes its own temporary directories.

  Delete the directory when you are done.
- **Ports.** Don't use the default API port 24816 or a port from the docs
  (25011). Pick a free one, e.g.
  `python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])'`.
  Both apps' embedded daemons already pick a free port.
- **Network.** Pass `--discovery-loopback` (CLI and iced app) or
  `--dart-define=MYCONNECT_DISCOVERY_LOOPBACK=true` (Flutter app) so nothing
  announces on or listens to the LAN: discovery binds `127.255.255.255:1716`
  and the control and payload ports bind `127.0.0.1`, so real devices can
  neither find nor dial the instance (`ss -lunpt` shows only loopback
  addresses for its PID). Loopback instances from other sessions can still
  see yours in a scan, so pair only with the device id you started, never by
  name alone. Don't pair with or send to real devices without asking.
- **Display and D-Bus.** Run either app under
  `dbus-run-session -- xvfb-run --auto-servernum ...` (as
  `ui/tool/integration_test.sh` does), or on an `Xvfb` display number you
  checked is free, with `WAYLAND_DISPLAY` unset. The Flutter app is a
  unique `GApplication` per D-Bus session: launched on the owner's session
  bus, it would just raise their running app and exit. A private bus also
  keeps notifications, the tray and file dialogs off the owner's desktop.
  Run `cargo test` the same way: the `plugins::clipboard::backend::system`
  tests read and write the real clipboard, so on the owner's display they
  clobber it and fail when it changes under them. Under Xvfb there is no
  GPU, so set `ICED_BACKEND=tiny-skia` for the iced app and its tests. The
  whole recipe for the iced app:

  ```sh
  dir=$(mktemp -d -p "$scratchpad")
  env -u WAYLAND_DISPLAY ICED_BACKEND=tiny-skia \
    dbus-run-session -- xvfb-run --auto-servernum \
    cargo run -p myconnect-gui -- --discovery-loopback \
      --data-dir "$dir/data" --download-dir "$dir/downloads"
  ```

  Add `--demo` for made-up devices, and `--api-port`/`--api-token` to
  drive the app's daemon from the CLI.
- **Processes.** Keep the PIDs you start and kill those, not
  `pkill -f <pattern>`, which can hit another session's processes.

## Worktrees

When asked to work in a worktree, each worktree builds its own `target/`
and `ui/build/`, so the first build is slow. Run `flutter pub get` in the
worktree's `ui/` before anything else that touches the Flutter app.
