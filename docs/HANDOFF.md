# Handoff

For agents continuing Ferry. It has the ground rules, the checks that
define "done", what is still open, and how to verify in the real app.

The plan for the first UI milestone (items 1–11: unpairing, interop with
KDE Connect, tray, transfers, settings, clipboard, end-to-end tests, ping,
add by IP, packaging, browsing a device's files) was built in Flutter and is
finished. It is kept in
[`archive/HANDOFF_UI_MILESTONE.md`](archive/HANDOFF_UI_MILESTONE.md) for
the detail behind each feature. The Flutter app was then replaced by a
native one in Rust and iced (the finished plan is
[`archive/PLAN_ICED_UI.md`](archive/PLAN_ICED_UI.md)) and deleted; its
records are in [`archive/flutter-adr/`](archive/flutter-adr/README.md).

## Read first

1. [`ARCHITECTURE.md`](ARCHITECTURE.md): the core and its plugins, and
   the module map (§2), state machines, the full HTTP API, how the app
   embeds the daemon (§9), testing (§10), known gaps (§11), browsing a
   device's files (§12).
2. [`adr/0001`](adr/0001-native-ui-in-iced.md): why the UI is Rust and
   iced in the daemon's process, its libraries, and its desktop
   integration per platform. It says which of the Flutter app's records
   (in [`archive/flutter-adr/`](archive/flutter-adr/README.md)) still
   apply: 0003 (snapshot plus events), 0007 (tray), 0008 (browsing) and
   0009 (window placement), and what differs from the Flutter app on
   purpose.

The app is `gui/` (`ferry-gui`) and all UI code is `src/ui/`: the
shell, and each feature's UI in `src/ui/features/`, all behind the `gui`
cargo feature.

## Ground rules (set by the project owner)

- **The UI is dumb.** It persists nothing but the main window's placement
  (`window.json`); preferences are daemon settings, and its store is a
  cache of core snapshots. The one exception is starting on login, which
  only the app has: the system's login item is its state (ARCHITECTURE
  §7), so the CLI can't change it. It calls the core and the plugins' typed Rust
  functions in-process (`adr/0001`), the same ones `http.rs` calls, so
  anything it does, the CLI can do too. A UI feature that needs new
  behaviour adds it to the plugin's or the core's Rust API first, then to
  `http.rs` and `client.rs`/the CLI, then to `src/ui/features/<name>.rs`.
- **Every resource needs a snapshot and events.** The UI takes a
  snapshot, patches it from the event bus, and takes a fresh one after it
  lags (Flutter ADR 0003, carried over); the CLI does the same over
  `/events`. A resource with events but no snapshot (or the reverse)
  leaves a client unable to recover after a gap.
- **A feature is a plugin.** It lives in `src/plugins/<name>/`,
  implements `core::Plugin`, and is one line in `plugins::builtin()`
  (ARCHITECTURE §2). Its UI is `src/ui/features/<name>.rs` plus its
  lines in `src/ui/features/mod.rs`, the one place in the UI that lists
  features. The core doesn't name features, and plugins don't import each
  other: what two features share belongs in the core (or, for UI
  helpers, `src/ui/`).
- **Token auth is optional.** It is enforced only when the daemon was
  started with one. The app's API is off until Settings → Command line
  access turns it on, and always has a token, kept in `api.json` (or
  `--api-token` for one run); `ferry-cli run` defaults to none.
- **`cargo build -p ferry` has no iced in it.** UI code and its
  dependencies stay behind the `gui` feature.
- Use reputable dependencies, and record new ones in `adr/0001`'s library
  table (or write a new ADR if the choice changes an existing decision).
- Update `ARCHITECTURE.md` when the API or module map changes.

Done means all of these pass:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets     # also builds and tests the UI
cargo build -p ferry                     # ferry-cli alone, without iced
cargo tree -p ferry -e normal --prefix none | grep -c '^iced'   # prints 0
git diff --check
```

Run `cargo test` under a private display and bus with
`ICED_BACKEND=tiny-skia` (CLAUDE.md). With `SNAPSHOT_DIR` set, the UI's
snapshot tests write PNGs of each page there, in light and dark; look at
them after a UI change (the snapshot font isn't the app's, so judge
layout, not typography).

Also run the real app for anything involving windows, the tray, drops,
dialogs or notifications (see "Verifying in the real app" below):
snapshots render one frame and miss what only shows over time. Unit tests
with a fake daemon host missed two real bugs in the first milestone.

## Where things stand

Everything in the milestone works in the Linux app. It was checked live in
the Flutter app, and each step of the iced rewrite in the real app against
a CLI peer or the fake phone.
Against KDE Connect for Android (a Pixel 8a, from the CLI daemon), these
work: pairing, unpairing, clipboard, file transfer both ways, and browsing
the phone's files. Nothing has been checked against KDE Connect on a
desktop yet. Paired devices are listed even while offline (the daemon
restores them from their trust records). The tray menu lists each connected
paired device (send files, ping, ring, send clipboard, browse files,
show details), with its
battery; the device list and details
page show the battery too (`kdeconnect.battery`, read-only). Releases
(`.github/workflows/build.yml`) build a universal macOS app in a DMG, a
Windows installer, Debian packages for amd64 and arm64 holding the app and
the CLI, and for tagged builds an Arch Linux PKGBUILD. The scripts are in
`packaging/`.

## Open work

**The tray on macOS and Windows, and notifications on Windows** (the
plan's step 13b). Notifications work on Linux and macOS (ADR 0001,
"Desktop integration"). The tray works on macOS (checked by the owner) and is untested on
Windows: `tray-icon` + `muda`
in `src/ui/desktop/tray.rs` (`native`), created from `Tray::start`, which
`update` calls on the main thread once the event loop runs
(`Message::StartTray`, sent from boot). Its menu and click events are
forwarded into the `DesktopEvent` channel. macOS shows
`assets/tray_icon_template.png` as a template image and opens the menu on
any click; Windows opens the window on a left click and the menu on a
right one. It type-checks for both (below). Still to do:

- Run it on Windows: the icon shows, the menu's items (and submenus)
  work, it updates as devices come and go, and closing the window keeps
  the app in the tray. If `build()` fails, the app falls back to having
  no tray.
- On macOS the app lives in the menu bar: it has a Dock icon only while
  its window is open (`desktop::dock`, the activation policy; the bundle
  sets `LSUIElement`). A Dock icon with the window closed would do
  nothing when clicked, as winit doesn't handle
  `applicationShouldHandleReopen:`. Check that the window comes to the
  front when opened from the tray, and that no Dock icon flashes at a
  start in the tray.
- `notify-rust` for Windows' `Notifier`: clicks work; nothing is
  withdrawn (ADR 0001, "Desktop integration").
- macOS notifications were checked from an ad-hoc signed bundle with the
  app's id, driving the real `Notifier` in an iced event loop (show, a
  click reaching `NotificationClicked`, and withdrawal), not yet in the
  full app with a peer: loopback discovery doesn't work on macOS (see
  "Traps"). They need the bundle, so `cargo run` shows none; run a bundle
  from outside `/tmp` (macOS refuses those), e.g. under `target/`. Check
  that a click brings the window (and its Dock icon) back while the app
  is in the menu bar only.
- Files dropped on the menu bar icon (`drops` in `src/ui/desktop/tray.rs`)
  are untested on a Mac: only type-checked, with the app's side
  (`DesktopEvent::TrayDropped`, the chooser) covered by unit tests. Check
  that the icon highlights while files hover, that the drop pops up the
  "Send N files to:" menu from the icon without opening the window, that
  choosing a device sends them, that a folder is refused with a
  notification, and that clicking the icon still opens the tray's menu.
- macOS's menu-bar Quit and logout take the quit path. They go through
  `terminate:`, which exits after winit's `exiting`, so the daemon's
  shutdown after `program.run()` likely doesn't run.
- Confirm single instance (the `/tmp` socket file on macOS, a named pipe
  on Windows) and placement with several monitors.
- Also check what packaging couldn't: the DMG's app launches from Finder
  with its tray icon, and the installed Windows app's notifications carry
  its name and icon.

To type-check macOS or Windows code on Linux, stub out the C that `ring`
builds (it needs the platform's SDK) with a compiler and archiver that
emit empty files; nothing is linked, so clippy runs fine:

```sh
rustup target add aarch64-apple-darwin x86_64-pc-windows-msvc
# fakecc: find `-o <out>` and `--target=...`, then
#   exec clang $target -c -x c /dev/null -o "$out"
# fakear: find the archive (`$2`, or `-out:<path>`), write "!<arch>\n" to it
CC_aarch64_apple_darwin=fakecc AR_aarch64_apple_darwin=fakear \
  cargo clippy --target aarch64-apple-darwin -p ferry-gui -p ferry \
  --all-targets -- -D warnings
# Windows: the same with CC_/AR_x86_64_pc_windows_msvc and that target.
```

**The rest of the UI's open work:**

- Drag files out of the browser to the desktop (ADR 0008 lists it).
- Remembered add-by-IP addresses (see "Smaller follow-ups").
- A low-battery notification (`thresholdEvent`).
- Accessibility: check what iced 0.14 exposes to screen readers and
  record the gap against Flutter.

**Packaging.** Nothing is signed or notarized; the macOS app has only an
ad-hoc signature. The macOS and Windows apps were built and the Windows
installer installed in CI, but neither was used on a real desktop yet.

**Browsing a device's files** (ARCHITECTURE §12, ADR 0008):

- Not yet tried on a real phone: `rm`, the app's Browse files page, an
  older phone with an RSA key (only a unit test covers its host key), and
  how Android's `errorMessage` reads when "All files access" is missing.
- The 5-minute idle timeout has no test; it would need to be configurable.
- A recursive delete runs inside the 15-second request deadline, so a very
  large folder can stop partway. Running it as a background job with
  progress would fix that.
- `tests/ui_e2e.rs` covers browsing against the fake phone
  (`tests/support/fake_phone.rs`), not a real one.
- Not offered: dragging files out of the browser (ADR 0008), downloading
  whole folders, video thumbnails, and serving this machine's files (KDE
  Connect desktops don't either).

**Smaller follow-ups.**

- Ringing a device (`kdeconnect.findmyphone.request`: `ferry-cli ring`,
  the details page and the tray) was checked against the fake phone only,
  not a real one. This machine doesn't ring when a peer asks it to.

- Notifications (`kdeconnect.notification`: `ferry-cli notifications`, the
  device page's Notifications page, a desktop notification for each new
  one) were checked against the fake phone only, not a real one. Worth
  checking on the phone: that it sends what it already shows once paired
  (the plugin asks when a device becomes paired and connected), how
  messaging apps' updates read (Android posts a new message as an update
  of the same notification, which alerts only when its text changes),
  that icons arrive, and a reply from the app. Not built: the
  `conversation` history Android sends for messaging apps, showing this
  machine's notifications on the phone, and per-app muting here (the phone
  picks which apps share).

- Battery reports were checked against the fake phone only, not a real
  one. A low-battery notification (`thresholdEvent: 1`) isn't shown, and
  this machine doesn't report its own battery.

- Devices added by IP address are forgotten on restart. KDE Connect keeps
  a list of such addresses and announces to them periodically. The
  equivalent here is a daemon setting (a list of addresses in
  `settings.json`) that `LanService` announces to on its interval, plus a
  way to remove entries in the UI.
- `plugins::clipboard::backend::system::tests::clearing_the_clipboard_is_not_reported` failed
  once under a full `cargo test --workspace` run and passed on every rerun
  and on its own; it looks timing-sensitive under load.
- Snapshots carry no sequence number, so an event emitted just before a
  snapshot response can briefly be overwritten by older data (ADR 0003).
  If this shows up in practice, add the event bus sequence to list
  responses (e.g. a header) and drop older events.
- The reconnecting banner and the startup error screen have not been
  exercised in the real app, only in unit tests.
- `ferry-cli send` prints only the upload's response, which is taken once
  the last byte has been forwarded, so it ends on `transferring (N/N)`
  rather than `completed`. Waiting for the terminal state (or watching
  `/events`) would make the CLI report the real outcome.
- A transfer the sender cancels shows up on the receiver as `failed` with
  `connection_failed`, not `cancelled`, because the receiver only sees the
  payload connection close early. KDE Connect has no cancel notice in the
  share protocol either, so this probably stays; a UI could word it as
  "stopped by the sender" if it becomes confusing.

## Traps

The UI's traps. Those about driving the app under Xvfb are in
"Verifying in the real app" below.

- **Two runtimes.** iced polls futures on its own executor. Anything that
  touches the daemon's tokio I/O (russh, payload sockets, file transfers)
  must run on the daemon's runtime (`UiContext::spawn`). Symptom: a panic
  "there is no reactor running", or a hang.
- **Don't block `update`.** Core calls that take locks are fine. Anything
  that does I/O (SFTP, file reads, `RunningService::start`) goes through a
  `Task`.
- **Event ordering after subscribe-then-snapshot.** Events already
  reflected in the snapshot are replayed after it. Device events carry the
  full device, so replaying one is harmless; transfers and pairings are
  why the store's guards exist, so don't drop them.
- **Actions outlive their page.** An event can remove the device while an
  action is in flight: messages carry ids, and handlers look the device up
  again rather than holding on to it.
- **Plugins don't import each other in UI code either.** A helper two
  plugins need (say, "upload files with a summary toast") belongs in
  `src/ui/`.
- **The tray is not a view.** It can't render an `Element`, which is why
  device actions are data. Don't add widget-returning tray APIs.
- **No shadows under tiny-skia.** The software renderer paints a shadow
  again on every partial redraw, turning the widget black. Dialogs use a
  border instead; check anything new under `ICED_BACKEND=tiny-skia` in the
  real app. Snapshots render one frame and don't show it.
- **macOS loopback.** `--discovery-loopback` can't find peers on macOS
  (no `127.255.255.255`). Use `--demo` there, and do peer tests on Linux.
- **Dialog titles are fixed text.** A name someone chose (a file, a
  device, a notification's sender) can be any length, so it goes in the
  body, which wraps anywhere (`Wrapping::WordOrGlyph`): "Unpair device?"
  with "Pixel 8a will need…" under it, not "Unpair Pixel 8a?".
- **iced is pinned** (`iced = "0.14"`, `iced_fonts = "0.3"`). Upgrading it
  is its own change, never mixed into a feature.

## Verifying in the real app

Isolate every run as [`../CLAUDE.md`](../CLAUDE.md) describes: fresh
temporary data and download dirs, a free port, loopback discovery, and a
private display and D-Bus session.

```sh
dir=$(mktemp -d -p "$scratchpad")
# peer
cargo run -- --api-port "$port" run --discovery-loopback \
  --data-dir "$dir/peer" --download-dir "$dir/peer-downloads" \
  --device-name "CLI Peer"
# app (separate identity, loopback only), on a private display and bus
env -u WAYLAND_DISPLAY ICED_BACKEND=tiny-skia \
  dbus-run-session -- xvfb-run --auto-servernum \
  cargo run -p ferry-gui -- --discovery-loopback \
    --data-dir "$dir/app" --download-dir "$dir/app-downloads" \
    --device-name "UI Desktop" --api-port "$app_port"
```

The app's own daemon serves the API on `--api-port` (with `--api-token`,
or the token kept in its data dir's `api.json`, which `ferry-cli
--data-dir "$dir/app"` reads by itself), so the CLI can drive it: `pair`
with the peer, `send` to it, list its transfers. `--demo` fills the app
with made-up paired devices, for looking at the UI without a peer.

To try file browsing without a phone, run
`cargo run --example fake_phone -- <DATA_DIR> <STORAGE_DIR> <DESKTOP_ID> [NAME]`
as the peer instead. It dials the desktop once, on the first
announcement it hears, so restart it (same data dir) after the desktop
restarts. It trusts the desktop's key only after a pairing request in the
same run; a restarted phone lets the desktop in by password instead. To
pair the app with it without clicking through the UI, pair a CLI daemon
started on the app's data dir first, stop it, then start the app.

Without a display (e.g. in an agent sandbox), run the built bundle under
`Xvfb`, take screenshots with `import -display :NN -window root out.png`, and
click with XTest (`libXtst` through Python `ctypes`). Don't open windows on
the user's own session, and don't pair with or send to real devices on
their network without asking.

The README and the website share one set of screenshots,
`site/img/<page>-<light|dark>.webp`, taken at 2x
(`WINIT_X11_SCALE_FACTOR=2`) and saved 880 px wide. `ICED_THEME=Light` or
`Dark` forces the app's theme, for taking both.

- Launch the app under `env -u WAYLAND_DISPLAY DISPLAY=:NN
  GDK_BACKEND=x11 ICED_BACKEND=tiny-skia dbus-run-session -- ...`, with
  the environment *outside* `dbus-run-session`. Services the private bus starts (the file chooser
  portal, notifications) inherit the bus daemon's environment. With `env`
  inside, they get the owner's `DISPLAY`/`WAYLAND_DISPLAY` and open on the
  owner's desktop.
- There is no window manager, so a click doesn't give the app's window
  keyboard focus: call `XSetInputFocus` on it (through `libX11` with
  ctypes) before sending keys.
- Unsetting `WAYLAND_DISPLAY` isn't enough to keep a GTK or Wayland helper
  off the owner's desktop: they fall back to `$XDG_RUNTIME_DIR/wayland-0`.
  Set `GDK_BACKEND=x11` (and `XDG_SESSION_TYPE=x11` for `display-info`);
  a headless compositor needs its own short `XDG_RUNTIME_DIR` (socket
  paths are limited to 108 bytes, so not under the scratchpad).
- The "Send files" picker opens as a GTK dialog on the virtual display
  (through `xdg-desktop-portal`, which D-Bus starts on your private bus).
  Without a window manager it can be bigger than the screen and doesn't get
  keyboard focus: move it on screen with `XMoveResizeWindow` and focus it
  with `XSetInputFocus`. Then press Ctrl+L, type the absolute path, and
  press Return, all through XTest key events.
- The dialog starts in "Recent" and lists the user's real recent files. Pick
  files by typed path, and don't browse or screenshot more of it than you
  need.
- Loopback transfers in a debug build run at tens of MB/s, so use a file of
  1–2 GB (from `/dev/zero`, in the scratchpad) to catch progress mid-flight
  or to cancel it.
- "Open file" and "Open folder" launch the desktop's real default app (e.g.
  Thunar) on the virtual display, and it may start helpers (`xfconfd`,
  `tumblerd`) that outlive it. Kill them afterwards, checking
  `/proc/<pid>/environ` first to make sure they belong to the private bus.
- The portal starts `xdg-document-portal`, which mounts its FUSE file
  system at the owner's `$XDG_RUNTIME_DIR/doc` if nothing is mounted
  there. Stop every process on your bus when done, and check the mount is
  as you found it.
- Quit the app with Ctrl+Q in its focused window, or through the tray's
  Quit, which works without a tray host: read `DBUS_SESSION_BUS_ADDRESS`
  from `/proc/<app pid>/environ`, get the menu with `gdbus call --session
  --dest org.kde.StatusNotifierItem-<pid>-<n> --object-path /MenuBar
  --method com.canonical.dbusmenu.GetLayout -- 0 -1 '["label"]'` (find the
  name with `ListNames`), and send `com.canonical.dbusmenu.Event --
  <Quit's id> clicked '<"">' 0`. Closing the window only hides it.
- Don't clean up with `pkill -f <pattern>`: the pattern also matches the
  shell running the command, and kills it. Kill by PID. `setsid cmd &`
  forks, so its `$!` is a wrapper that has already exited: record the PID
  from `pgrep -f` with your run directory in the pattern.
