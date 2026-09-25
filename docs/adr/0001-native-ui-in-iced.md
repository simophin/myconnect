# 0001. Build the desktop UI in Rust with iced, in the daemon's process

- Status: Accepted
- Date: 2026-09-25

## Context

The desktop app is a Flutter UI that embeds the Rust daemon through a JSON
C ABI (`ffi/`) and then talks to it only over the local HTTP API
(`ui/docs/adr/`). That costs a second language and toolchain, generated
model code that mirrors the API's JSON, a Flutter build hook per platform,
and a platform plugin (often patched) for each desktop feature: tray,
notifications, drops, window placement. A feature is written twice: once
in its Rust plugin and once in Dart.

A spike (`gui/`, PR #24) showed the device list in iced 0.14, running the
daemon in-process and reading the core directly, with headless snapshot
tests through `iced_test`. The owner decided to replace the Flutter app
with it. [`../PLAN_ICED_UI.md`](../PLAN_ICED_UI.md) is the step-by-step
plan; this record is the decision and the shape it fixes.

## Decision

- **The UI is Rust and iced 0.14**, with the Lucide icon font
  (`iced_fonts`). The look follows iced's built-in theme palette, not the
  Flutter app's pixels; parity means the same behaviour.
- **In-process access to the core.** The app (`gui/`, binary
  `myconnect-gui`) starts a `RunningService` and hands its `Core` to the UI:
  snapshots from `Core`, events from `core.subscribe()` (a fresh snapshot
  after the receiver lags), and actions through typed Rust functions. The
  UI never calls the HTTP API. The embedded daemon still serves it, so the
  CLI can drive and inspect the instance the UI shows; every feature keeps
  working from the CLI.
- **The UI never blocks on the daemon's I/O.** iced runs its own executor;
  anything that touches the daemon's sockets runs on the daemon's tokio
  runtime (`UiOptions::runtime`), and iced's `tokio` feature stays off so
  there is only one runtime.
- **UI halves live in the feature modules.** A feature's UI is
  `src/plugins/<name>/ui.rs`, next to its `mod.rs` and `http.rs`, and plugs
  in through the seam in `src/ui/plugin.rs`. `src/ui/` is the UI core:
  shell, the pages the core owns (devices, pairing, transfers, settings),
  sync and desktop glue. It never names a feature, and plugins never import
  each other, in the UI too. `plugins::builtin_with_ui()` builds each
  plugin once and its UI half from the same instance;
  `RunningService::start_with` lets the app start the daemon with that
  list.
- **A `gui` cargo feature.** `src/ui/` and every `ui.rs` are behind
  `feature = "gui"`, which turns on the UI's optional dependencies. `gui/`
  depends on `myconnect` with the feature; `cargo build -p myconnect` (the
  CLI and daemon) has no iced in its tree, which CI checks.
- **Configuration is flags and environment variables**, mirroring
  `myconnect run`: `--data-dir`, `--download-dir`, `--device-name`,
  `--discovery-loopback`, `--no-system-clipboard`, `--api-port`,
  `--api-token`, each also read from `MYCONNECT_<NAME>`. The API token
  defaults to a random one; the API address is logged at `info`.

### What happens to the Flutter UI's records

The Flutter app stays in the tree, and its records stay in force for it,
until it is deleted (plan step 16, which moves them to
`docs/archive/flutter-adr/`). For the new UI:

| Flutter record | For the new UI |
| --- | --- |
| 0001 Stateless UI over the HTTP API | Superseded: in-process access to the core. What carries over: the UI keeps no state of its own; its store is a cache of core snapshots. |
| 0002 Embed the daemon through a JSON C ABI | Superseded: no FFI; the daemon runs in the UI's process as a library. |
| 0003 Snapshot plus events | Carries over in spirit: subscribe, then snapshot, then events, a fresh snapshot after a lag, and the same guards against stale snapshots. |
| 0004 Riverpod | Superseded: iced's update/view and a store in `src/ui/`. |
| 0005 Libraries and conventions | Superseded by the table below. |
| 0006 Build the Rust core from the platform build | Superseded: the app is one cargo binary. |
| 0007 Keep running in the tray | Carries over. |
| 0008 Browse device files in the app | Carries over; the browser calls `BrowsePlugin`'s methods instead of HTTP. |
| 0009 Remember the main window placement | Carries over: `window.json`, the UI's one piece of state. |

## Libraries

Rows marked with a step are chosen but not yet in the tree; the step that
first needs one adds it to the `gui` feature.

| Concern | Choice | Why |
| --- | --- | --- |
| UI toolkit | `iced` 0.14 | Pure Rust, Elm-style update/view that suits a snapshot-and-events store, multi-window `daemon` programs (a window can close while the app lives in the tray), and wgpu with a tiny-skia software fallback. Pinned; upgrading is its own change. |
| Icons | `iced_fonts` 0.3 (Lucide) | The Lucide icon font with typed helpers, the version matching iced 0.14. |
| Headless UI tests | `iced_test` 0.14 | iced's own simulator: finds and clicks widgets, and renders snapshots to PNG with no display. |
| Running tasks in tests | `iced_runtime` 0.14 (dev) | Already in iced's tree; its `task::into_stream` lets a unit test see what a `Task` produces, which `iced` doesn't re-export. |
| Arguments | `clap` | Already the CLI's parser; `env` reads each flag's environment variable. |
| File and folder dialogs | `rfd` (step 9) | The standard native dialog crate: GTK or the XDG portal on Linux, AppKit, Win32. |
| Notifications | `notify-rust` (step 13) | freedesktop notifications over D-Bus, macOS and Windows toasts. |
| Opening files and folders | `opener` (step 8) | Opens with the default app and reveals in the file manager on each platform. |
| Single instance | `interprocess` (step 13) | Cross-platform local sockets, named from the data dir, so isolated instances never collide. |
| Tray (Linux) | `ksni` (step 13) | A StatusNotifierItem over D-Bus in pure Rust, with no libappindicator or GTK. |
| Tray (macOS, Windows) | `tray-icon` (step 13) | The Tauri team's tray crate, with `muda` menus. |

## Consequences

- A feature is one folder, UI included, in one language. Adding one means
  `mod.rs`, `http.rs` and `ui.rs`, a line in `builtin()` and one in
  `builtin_with_ui()`, and the CLI in `client.rs`/`cli.rs`.
- Anything the UI does must be a Rust function that `http.rs` calls too,
  so the UI and the CLI can't drift apart.
- A plain workspace build compiles the UI (feature unification); the CLI
  alone still builds without it.
- Linux builds need iced's system libraries (xkbcommon, Wayland, Vulkan or
  Mesa); tests run headless with `ICED_BACKEND=tiny-skia`.
- There is no external-daemon mode: the app always embeds its daemon. To
  test against another instance, run a CLI peer.
