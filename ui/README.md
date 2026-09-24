# MyConnect UI

Flutter desktop app for MyConnect. It is a thin client: all state lives in
the Rust daemon, and the app reads and writes it only through the daemon's
HTTP API (see [docs/adr](docs/adr/README.md) for why and how).

By default the app starts its own daemon in-process via the `myconnect-ffi`
library, on a free loopback port with a per-launch token.

## Features

- Paired device list with live reachability
- Device details and unpairing
- Add device: scan for nearby devices and start pairing, with the
  verification code and outcome
- Incoming pairing requests prompt on any screen
- Send a file from a device's page; a transfers page (and each device's
  recent transfers) shows progress, cancels running transfers, and opens
  received files or their folder
- Keeps running in the tray when the window is closed (Quit from the tray
  menu stops it), with a desktop notification for pairing requests and
  received files that arrive while the window is hidden. Launching it again shows the running
  instance. On GNOME the tray icon needs the AppIndicator extension.
- Settings: this computer's name as other devices see it, where received
  files go, clipboard sync, and whether closing the window keeps the app
  running. The daemon stores them, so they survive restarts.

## Running

Requires Flutter and a Rust toolchain (the Linux build runs `cargo` to build
the core, see [ADR 0006](docs/adr/0006-build-the-rust-core-from-the-platform-build.md)).

```sh
flutter run -d linux
```

### Options (`--dart-define`)

| Define | Effect |
| --- | --- |
| `MYCONNECT_API_URL` | Use an already-running daemon (e.g. `http://127.0.0.1:24816`) instead of starting one. |
| `MYCONNECT_API_TOKEN` | Token for that external daemon, if it was started with `--api-token`. |
| `MYCONNECT_DATA_DIR` | Identity/trust directory for the embedded daemon. |
| `MYCONNECT_DOWNLOAD_DIR` | Where the embedded daemon saves received files. Overrides the saved setting for this run. |
| `MYCONNECT_DEVICE_NAME` | Name advertised to peers. Overrides the saved setting for this run; without either, the host name. |
| `MYCONNECT_DISCOVERY_LOOPBACK` | `true` to discover only instances on this machine. |
| `MYCONNECT_SYSTEM_CLIPBOARD` | `false` to keep the embedded daemon's clipboard in memory instead of syncing the desktop clipboard. |
| `MYCONNECT_VERSION` | Version shown in Settings; CI sets it from the release tag. Without it, `dev`. |

### Two instances on one machine

Pair the app with a CLI daemon without a second computer:

```sh
# terminal 1: a CLI peer
cargo run -- --api-port 25011 run --discovery-loopback \
  --data-dir /tmp/peer --device-name "CLI Peer"

# terminal 2: the app, isolated from your real identity
cd ui && flutter run -d linux \
  --dart-define=MYCONNECT_DISCOVERY_LOOPBACK=true \
  --dart-define=MYCONNECT_DATA_DIR=/tmp/ui \
  --dart-define=MYCONNECT_DEVICE_NAME="UI Desktop"

# then, e.g., request pairing from the CLI and accept it in the app
cargo run -- --api-port 25011 scan
cargo run -- --api-port 25011 pair <ui-device-id>
```

## Development

```sh
dart run build_runner build --delete-conflicting-outputs  # after model changes
flutter analyze
flutter test
tool/integration_test.sh  # end-to-end, see below
```

`integration_test/` drives the real app, with its daemon embedded through
the real FFI library, against a `myconnect run` peer it spawns (built with
`cargo build --bin myconnect`, or set `MYCONNECT_CLI`). Both discover over
loopback only, on fresh identities in temporary directories. It covers
incoming pairing accept and reject, outgoing pairing, unpair, and a file each
way. `tool/integration_test.sh` runs it under `xvfb-run` and
`dbus-run-session` so nothing opens on your desktop; `flutter test
integration_test -d linux` also works on a desktop session.

Layout:

```text
lib/
├── main.dart
└── src/
    ├── app.dart                 # MyConnectRoot (provider scope), MaterialApp, theme
    ├── core/
    │   ├── api/                 # dio client, SSE parsing, reconnecting stream, models
    │   ├── daemon/              # DaemonHost: FFI-embedded or external daemon
    │   ├── desktop/             # window + tray, desktop notifications
    │   ├── routing/router.dart
    │   └── providers.dart       # host → endpoint → api → event hub
    ├── features/
    │   ├── background/          # close to tray, quit, pairing and file notifications
    │   ├── devices/             # list, details (send file), add device (scan)
    │   ├── pairing/             # outgoing pairing page, incoming prompt
    │   ├── settings/            # settings controller and page
    │   └── transfers/           # transfers controller, page and tile
    └── shared/widgets.dart
```
