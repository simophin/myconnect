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
with it. [`../archive/PLAN_ICED_UI.md`](../archive/PLAN_ICED_UI.md) is the
finished step-by-step plan; this record is the decision and the shape it
fixes.

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

The Flutter app stayed in the tree, with its records in force for it,
until plan step 16 deleted it and moved them to
[`../archive/flutter-adr/`](../archive/flutter-adr/README.md). For the new
UI:

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
| UI toolkit | `iced` 0.14 | Pure Rust, Elm-style update/view that suits a snapshot-and-events store, multi-window `daemon` programs (a window can close while the app lives in the tray), and wgpu with a tiny-skia software fallback. Pinned; upgrading is its own change. Its `advanced` feature is on, for the few custom widgets (the activity bar), and `image-without-codecs`, for the file browser's preview (`image::viewer` pans and zooms; the codecs come from the `image` row). |
| Decoding previews | `image` 0.25 (step 12) | What iced renders images with. Only the codecs a preview offers are on (BMP, GIF, JPEG, PNG, WebP), not iced's `image` feature's every format. The browser decodes a preview itself, off the UI thread, so an image that can't be decoded says so ("This image can’t be shown."): iced decodes at draw time and drops the error. |
| Local time | `chrono` 0.4 (step 12) | Already in the tree through `russh-sftp`, with `clock` for the local time zone (`iana-time-zone`). `time` reads the local offset only on a single-threaded process on Unix, and `jiff` would be a second time crate. |
| Icons | `iced_fonts` 0.3 (Lucide) | The Lucide icon font with typed helpers, the version matching iced 0.14. |
| Headless UI tests | `iced_test` 0.14 | iced's own test crate. Its simulator finds and clicks widgets and renders snapshots to PNG with no display; its emulator runs the whole program, tasks and subscriptions included, for the end-to-end tests (`tests/ui_e2e.rs`). |
| Running tasks in tests | `iced_runtime` 0.14 (dev) | Already in iced's tree; its `task::into_stream` lets a unit test see what a `Task` produces, which `iced` doesn't re-export. |
| Arguments | `clap` | Already the CLI's parser; `env` reads each flag's environment variable. |
| File and folder dialogs | `rfd` (step 9) | The standard native dialog crate: the XDG portal on Linux (zenity if there is none), AppKit, Win32. Its async dialogs need no runtime of their own. |
| Notifications | Linux: `zbus` 5 (step 13); macOS and Windows: `notify-rust` (step 13b) | On Linux the app talks to `org.freedesktop.Notifications` itself, so it can withdraw a notification and hear its click without a thread per notification (see Desktop integration). On `async-io`, as iced's theme detection already has it: its `tokio` feature would need a tokio runtime on iced's threads. `notify-rust` gives macOS and Windows toasts. |
| Opening files and folders | `opener` (step 8) | Opens with the default app and reveals in the file manager on each platform. |
| Single instance | `interprocess` (step 13), and `libc` on Unix for the uid | Cross-platform local sockets, named from the data dir, so isolated instances never collide. |
| Tray (Linux) | `ksni` 0.3 (step 13) | A StatusNotifierItem over D-Bus in pure Rust, with no libappindicator or GTK. Spawned with `assume_sni_available(true)`, so a tray host that starts, stops or restarts later is followed. Its `async-io` feature, not the default `tokio`, for the same reason as `zbus`. |
| Monitor list | `display-info` (step 13) | iced exposes only the size of the window's current monitor; the `window.json` fits-on-screen check needs every monitor's bounds. |
| Tray (macOS, Windows) | `tray-icon` (step 13b) | The Tauri team's tray crate, with `muda` menus. |
| Windows exe resources | `winresource` (step 15, build dependency of `gui` on Windows only) | Embeds the icon Explorer and the taskbar show, and the name Task Manager lists, in `myConnect.exe`. |
| Packaging | Shell scripts in `packaging/`, NSIS on Windows (step 15) | See "Packaging" below. |

## Desktop integration (plan step 10)

A throwaway spike (an iced `daemon` with `ksni`, `notify-rust`,
`interprocess` and `display-info`) was run on Linux, 2026-09-26: X11 under
Xvfb, Wayland under a headless labwc (wlroots), each on a private D-Bus
with a fake `StatusNotifierWatcher` and notification server (dbus-python),
and a GTK drag source driven with XTest. macOS and Windows weren't
available; their columns come from reading winit 0.30.13, `notify-rust`
4.18, `tray-icon` and `interprocess` 2.4 sources, and are for step 13 to
confirm on those machines.

| | Linux X11 | Linux Wayland | macOS | Windows |
| --- | --- | --- | --- | --- |
| File hover and drop | Tested: `FileHovered` per file once, on entering; `FileDropped` per file; `FilesHoveredLeft` on leaving. Folders arrive too. | No drag and drop: winit has no `wl_data_device`, so a drag over the window is never accepted and nothing reaches the app. | Source: the same three events (`draggingEntered`, `performDragOperation`, `draggingExited`). | Source: the same (`IDropTarget`). |
| Cursor position during a drag | Tested: none. The drag source holds the pointer grab, so no `CursorMoved` arrives while hovering. With a GTK source, the ungrab gave one `CursorMoved` at the drop point just *before* `FileDropped`; that order is the source's doing. | n/a | Source: none; winit ignores `draggingUpdated`. | Source: none; winit drops `DragOver`'s point. |
| Tray | Tested (`ksni`): left click reaches `activate`, the menu has submenus and disabled items, and `handle.update` rewrites labels while running. Without a watcher `spawn` fails (`ServiceUnknown`) unless `assume_sni_available(true)`; with it, `watcher_offline`/`watcher_online` report the host going and coming, and the item re-registers on its own. | Same as X11 (D-Bus only). | Source (`tray-icon` + `muda`): must be created on the main thread after the event loop starts, so from the first `update` (iced runs `update` on that thread); its menu and icon events come from global channels, forwarded into a `Subscription`. | Same as macOS. |
| Close and reopen the window | Tested: `window::close` keeps an `iced::daemon` alive; `window::open` with `Position::Specific` and the saved size puts it back where it was. `window::position`, `size` and `is_maximized` answer before closing. | Tested: works, but `window::position` is always `None` and `Position::Specific` is ignored: the compositor places windows. | Source: as X11. | Source: as X11. |
| Notifications | Tested: `notify-rust` shows it; a click on the body is the `default` action and reaches the app. Withdrawing needs the handle that `wait_for_action` consumed; by id it takes a replace-then-close that briefly shows an empty notification. | Same as X11 (D-Bus only). | Source: `NSUserNotificationCenter` (deprecated) reports the click only through a blocking wait per notification and can't withdraw; `UNUserNotificationCenter` (`preview-macos-un`) can, but needs the signed bundle. | Source: a body click is `Default` while the app runs; it needs an AUMID (the installer's, step 15); no withdraw. |
| Single instance | Tested (`interprocess`, `GenericNamespaced`): an abstract socket, so no file: after `kill -9` the next launch listens at once; a second launch sends `show` and exits. | Same as X11. | Source: a socket file under `/tmp`, left behind by a crash; `try_overwrite(true)` replaces it after `connect` is refused. | Source: a named pipe; nothing stale. |
| Monitors | Tested: `window::monitor_size` is the current monitor's size only. `display-info` lists screens with origins, but picks X11 or Wayland from `WAYLAND_DISPLAY`, *then* `XDG_SESSION_TYPE`; its `is_primary` was false for the only screen. | Tested: `monitor_size` is `None` until the surface enters an output; `display-info` lists outputs. Positions don't matter here. | Source: `display-info` (AppKit). | Source: `display-info` (Win32). |

Decisions for steps 11 and 13:

- **Drops are routed, not hit-tested.** No platform reports where a drag is
  while it hovers, so there is no per-card highlight. The hover shows the
  window border and the pill. A drop goes to the route: on a device page,
  to that device (if a plugin's `drop_target` takes it); on a browse
  folder, into that folder; anywhere else, the "Send N files" chooser.
  The X11 position at the drop is not used: it depends on the source's
  event order and exists nowhere else, so the same drop would behave
  differently by platform.
- **Folders are filtered by the shell**, since winit hands them over like
  files ("Only files can be sent, not folders.").
- **Wayland: no drag and drop**, as the owner decided. Nothing to guard:
  the app never sees the drag.
- **The window closes to the tray, it isn't hidden.** `window::close` on
  close-to-tray, `window::open` with the saved placement to show it
  again; no GPU surface while in the tray.
- **Placement:** keep `{visible, maximized, bounds}`. Restore the position
  only where it can be set (X11, macOS, Windows) and only if it fits one
  of `display-info`'s monitors, otherwise centre; on Wayland restore the
  size and maximized state and let the compositor place it. Read the
  position before closing (`window::position` answers `None` on Wayland,
  so keep the old one).
- **Tray availability** follows `watcher_online`/`watcher_offline`
  (`ksni`, `assume_sni_available(true)`); macOS and Windows always have
  one. Without a tray the window is always shown at start, and closing
  it quits whatever `closeToTray` says, since nothing could show it again
  but a second launch.
- **Notifications on Linux talk D-Bus themselves** (`zbus`, on the
  daemon's runtime): `Notify` with a `default` action, `CloseNotification`
  to withdraw the pairing request, and the `ActionInvoked` and
  `NotificationClosed` signals from one subscription. `notify-rust` is a
  macOS and Windows dependency only. **Clicking a notification shows the
  window on Linux and Windows;** on macOS it is best effort (the
  `NSUserNotificationCenter` wait, one thread per notification), and
  withdrawing is Linux-only until the macOS app is a signed bundle.
- **Single instance:** `GenericNamespaced`, named
  `myconnect-<uid>-<hash of the absolute data dir>` (short enough for
  macOS's 104-byte socket paths; the uid keeps users apart in Linux's
  shared abstract namespace). Connect first: a reply means another
  instance runs, so send `show` and exit; a refusal means listen with
  `try_overwrite(true)`.
- **Monitors:** `display-info` for the fits-on-screen check. Tests and
  Xvfb runs must set `XDG_SESSION_TYPE=x11` next to unsetting
  `WAYLAND_DISPLAY`, or it looks for a Wayland compositor. Don't use its
  `is_primary`.

## Packaging (plan step 15)

- **Hand-written scripts, not `cargo-packager`.** The app is one binary
  per platform, so a package is a few files around it:
  `packaging/linux/build_deb.sh` (the `.deb`, with dependencies from
  `dpkg-shlibdeps` plus the libraries winit loads at runtime),
  `packaging/macos/build_app.sh` (a universal `.app` from `lipo`,
  `iconutil` and an ad-hoc `codesign`, and the DMG from `hdiutil`) and
  `packaging/windows/installer.nsi` (NSIS). `cargo-packager` would be one
  more tool and config for the same result, and it can't derive the
  `.deb`'s dependencies or join two architectures into one binary.
- **The CLI is built on its own** (`cargo build -p myconnect`): built
  together with `myconnect-gui`, feature unification turns `gui` on for it
  and puts iced in it. `build_deb.sh` refuses a CLI with iced in it.
- **One app id, `org.myconnect.MyConnect`** (`ui::desktop::APP_ID`): the
  Linux window's app id and X11 class, the `.desktop` file, the icons and
  the notifications' `desktop-entry`; the macOS bundle id; the Windows
  notification id (AUMID), which the installer registers under
  `HKCU\Software\Classes\AppUserModelId`. The Flutter app's was
  `org.myconnect.myconnect_ui`.
- **Names** (owner's decision): `/usr/bin/myConnect` and the CLI as
  `/usr/bin/myconnect` in the same `.deb`; `MyConnect.app/Contents/MacOS/myConnect`
  (no CLI); `myConnect.exe` and the CLI as `cli\myconnect.exe`. The
  Windows installer is per user (`%LOCALAPPDATA%\Programs\MyConnect`, no
  administrator rights), with a Start menu shortcut.
- **Icons** come from `assets/icon/*.svg` through `assets/generate_icons.sh`,
  which writes every platform's: the hicolor theme, the macOS iconset, the
  Windows `.ico`, the window icon and the tray icons.
- The `.deb` is built on Debian 12 and checked by `packaging/linux/check_deb.sh`
  on a clean Debian 12: it installs with its Depends only (no GPU driver,
  so the app draws with tiny-skia), opens its window with its class and
  icon, and the CLI reaches its API.

## Deliberate differences from the Flutter app

These differ on purpose (owner's decisions). Don't "fix" them back.

- **No external daemon mode.** Flutter could attach to a daemon through
  `MYCONNECT_API_URL`; the app always embeds its daemon.
- **Configuration is flags and environment variables**, not compile-time
  defines (see Decision). The version in Settings is
  `CARGO_PKG_VERSION`, plus `git describe` when available.
- **No FFI.** `ffi/` existed only for Flutter and was deleted with it.
- **Retry restarts only what failed.** The "could not start" screen's
  Retry calls `RunningService::start` again; the tray keeps working.
- **No reconnecting banner.** There is no connection to lose in-process;
  a lagged receiver takes a fresh snapshot, silently.
- **Start hidden works on every platform.** The window opens at start only
  if the saved placement says it was visible, or if there is no tray.
- **Single instance works everywhere**, through one local socket rather
  than GApplication or LaunchServices.
- **Wayland has no drag and drop** (winit has none there). Don't force X11
  or XWayland to get it back; *Send files* and *Upload files* do the same
  job.
- **Not sandboxed on macOS.** Flutter's sandbox caused the download-folder
  bookmark problem; the new app is a plain, ad-hoc-signed bundle unless
  the owner later wants the App Store.
- The window title is "MyConnect", not "myconnect_ui".

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
