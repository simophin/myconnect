# Handoff

For agents continuing MyConnect. It has the ground rules, the checks that
define "done", what is still open, and how to verify in the real app.

The plan for the first Flutter UI milestone (items 1–11: unpairing,
interop with KDE Connect, tray, transfers, settings, clipboard, end-to-end
tests, ping, add by IP, packaging, browsing a device's files) is finished.
It is kept in
[`archive/HANDOFF_UI_MILESTONE.md`](archive/HANDOFF_UI_MILESTONE.md) for
the detail behind each feature and the traps found along the way.

## Read first

1. [`ARCHITECTURE.md`](ARCHITECTURE.md): the core and its plugins, and
   the module map (§2), state machines, the full HTTP API, the FFI
   embedding (§9), known gaps (§11), browsing a device's files (§12).
2. [`../ui/README.md`](../ui/README.md): how to run the app, including two
   instances on one machine.
3. [`../ui/docs/adr/`](../ui/docs/adr/README.md): why the Flutter UI is
   shaped the way it is. ADR 0001 and 0003 are the ones you will lean on.
4. For the native UI that replaces it: [`PLAN_ICED_UI.md`](PLAN_ICED_UI.md)
   (the steps, its own ground rules, and how to see the UI) and
   [`adr/`](adr/README.md), whose 0001 is the decision and says which
   Flutter records still apply. The app is `gui/` (`myconnect-gui`), the UI
   core is `src/ui/`, both behind the `gui` cargo feature.

## Ground rules (set by the project owner)

- **The UI is dumb.** It persists nothing but the main window's placement
  (ADR 0009). The Flutter UI reads and writes only through the
  HTTP API; the native UI calls the core and the plugins' typed Rust
  functions in-process instead (`adr/0001`), and anything it does, the
  CLI can do too. If a feature needs data the API doesn't have, add the endpoint
  (and, if the data changes over time, an event) in Rust first.
- **Every resource needs a snapshot endpoint and events.** The UI loads a
  snapshot, patches it from `/events`, and refetches after any reconnect
  (ADR 0003). A resource with events but no list endpoint (or the reverse)
  leaves the UI unable to recover after a gap.
- **A feature is a plugin.** It lives in `src/plugins/<name>/`,
  implements `core::Plugin`, and is one line in `plugins::builtin()`
  (ARCHITECTURE §2). The core doesn't name features, and plugins don't
  import each other: what two features share belongs in the core.
- **Native code only starts and stops the daemon** (`ffi/`, ADR 0002). Don't
  add FFI functions for application features.
- **Token auth is optional.** It is enforced only when the daemon was
  started with one. The FFI always sets one; the CLI defaults to none.
- Use reputable dependencies, and record new ones in ADR 0005's table (or
  write a new ADR if the choice changes an existing decision).
- Update `ARCHITECTURE.md` when the API or module map changes.

Done means all of these pass:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets     # also builds and tests the native UI
cargo build -p myconnect                 # the CLI alone, without iced
cargo tree -p myconnect -e normal --prefix none | grep -c '^iced'   # prints 0
git diff --check
```

Run `cargo test` under a private display and bus with
`ICED_BACKEND=tiny-skia` (CLAUDE.md). With `SNAPSHOT_DIR` set, the native
UI's snapshot tests write PNGs of each page there; look at them after a UI
change.

Also run the real app for any UI change (see "Verifying in the real app"
below). Unit tests with a fake daemon host missed two real bugs in the
first milestone.

## Where things stand

Everything in the milestone works in the Linux app and was checked live.
Against KDE Connect for Android (a Pixel 8a, from the CLI daemon), these
work: pairing, unpairing, clipboard, file transfer both ways, and browsing
the phone's files. Nothing has been checked against KDE Connect on a
desktop yet. Paired devices are listed even while offline (the daemon
restores them from their trust records). The tray menu lists each connected
paired device (send files, ping, ring, send clipboard, browse files,
show details), with its
battery; the device list and details
page show the battery too (`kdeconnect.battery`, read-only); on Linux it needs a patched `cnativeapi`, vendored in
`ui/third_party/`. Releases (`.github/workflows/build.yml`) build the
native (iced) app, not the Flutter one: a universal macOS app in a DMG, a
Windows installer, Debian packages for amd64 and arm64 holding the app and
the CLI, and for tagged builds an Arch Linux PKGBUILD. The scripts are in
`packaging/`.

## Open work

**Replacing the Flutter UI with a native Rust UI (iced).** Decided by the
owner on 2026-09-25. Follow [`PLAN_ICED_UI.md`](PLAN_ICED_UI.md) step by
step. The Flutter app is no longer maintained (owner, 2026-09-25): its
checks aren't part of "done", and it stays only as the spec until the
plan's last step deletes it.

Other items still open:

**Packaging.** The macOS and Windows builds bundle the daemon, but their
first launch was never recorded here. On macOS, check that a download
folder chosen in Settings still works after a restart: the sandbox forgets
it without a security-scoped bookmark. Nothing is signed or notarized; the
macOS app has only an ad-hoc signature. A Flutter build hook
(`hook/build.dart`) could replace the per-platform build steps (ADR 0006).

**Browsing a device's files** (ARCHITECTURE §12, ADR 0008):

- Not yet tried on a real phone: `rm`, the app's Browse files page, an
  older phone with an RSA key (only a unit test covers its host key), and
  how Android's `errorMessage` reads when "All files access" is missing.
- The 5-minute idle timeout has no test; it would need to be configurable.
- A recursive delete runs inside the 15-second request deadline, so a very
  large folder can stop partway. Running it as a background job with
  progress would fix that.
- `ui/integration_test/` doesn't cover browsing, because its peer is the
  CLI, which serves no files. The iced UI's `tests/ui_e2e.rs` does,
  against the fake phone (`tests/support/fake_phone.rs`).
- Not offered: dragging files out of the browser (ADR 0008), downloading
  whole folders, video thumbnails, and serving this machine's files (KDE
  Connect desktops don't either).

**Smaller follow-ups.**

- Ringing a device (`kdeconnect.findmyphone.request`: `myconnect ring`,
  the details page and the tray) was checked against the fake phone only,
  not a real one. This machine doesn't ring when a peer asks it to.

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
- `myconnect send` prints only the upload's response, which is taken once
  the last byte has been forwarded, so it ends on `transferring (N/N)`
  rather than `completed`. Waiting for the terminal state (or watching
  `/events`) would make the CLI report the real outcome.
- A transfer the sender cancels shows up on the receiver as `failed` with
  `connection_failed`, not `cancelled`, because the receiver only sees the
  payload connection close early. KDE Connect has no cancel notice in the
  share protocol either, so this probably stays; a UI could word it as
  "stopped by the sender" if it becomes confusing.

## Traps

- Widgets that `await` a mutation must capture the router or messenger
  beforehand, because an event can unmount them mid-await (see
  `device_detail_page.dart`). Apply the same care to new screens.
- `--dart-define` reads live only in `daemon_host.dart`
  (`DaemonHost.fromEnvironment` and `dataDirOverride`), with an ignore for
  `avoid_redundant_argument_values`. Never run `dart fix` on
  that file without checking the diff.
- In debug builds the DEBUG banner covers the rightmost app bar action,
  which on the home screen is the Transfers button (at about x 1244–1268,
  y 14–38 in a 1280-wide window). It is there and clickable, just hidden.
  Don't mistake it for a missing widget, and use `find.byTooltip` in tests.

## Verifying in the real app

Isolate every run as [`../CLAUDE.md`](../CLAUDE.md) describes: fresh
temporary data and download dirs, a free port, loopback discovery, and a
private display and D-Bus session.

```sh
dir=$(mktemp -d)
# peer
cargo run -- --api-port "$port" run --discovery-loopback \
  --data-dir "$dir/peer" --download-dir "$dir/peer-downloads" \
  --device-name "CLI Peer"
# app (separate identity, loopback only)
cd ui && flutter run -d linux \
  --dart-define=MYCONNECT_DISCOVERY_LOOPBACK=true \
  --dart-define=MYCONNECT_DATA_DIR="$dir/ui" \
  --dart-define=MYCONNECT_DOWNLOAD_DIR="$dir/ui-downloads" \
  --dart-define=MYCONNECT_DEVICE_NAME="UI Desktop"
```

To try file browsing without a phone, run
`cargo run --example fake_phone -- <DATA_DIR> <STORAGE_DIR> <DESKTOP_ID>`
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

- Launch the app under `env -u WAYLAND_DISPLAY DISPLAY=:NN
  GDK_BACKEND=x11 dbus-run-session -- ...`, with the environment *outside*
  `dbus-run-session`. Services the private bus starts (the file chooser
  portal, notifications) inherit the bus daemon's environment. With `env`
  inside, they get the owner's `DISPLAY`/`WAYLAND_DISPLAY` and open on the
  owner's desktop.
- The "Send file" picker opens as a GTK dialog on the virtual display.
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
- Quitting the app (the tray's Quit, the one path that stops the
  daemon) works without a tray host: read `DBUS_SESSION_BUS_ADDRESS`
  from `/proc/<app pid>/environ`, get the menu with `gdbus call --session
  --dest org.kde.StatusNotifierItem-<pid>-1 --object-path
  /StatusNotifierItem/Menu --method com.canonical.dbusmenu.GetLayout --
  0 -1 '["label"]'`, and send `com.canonical.dbusmenu.Event -- <Quit's
  id> clicked '<"">' 0`.
- Don't clean up with `pkill -f <pattern>`: the pattern also matches the
  shell running the command, and kills it. Kill by PID.
