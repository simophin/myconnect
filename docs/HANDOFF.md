# What to build next

Handoff for agents continuing MyConnect after the first Flutter UI milestone
(2026-09-24). Each item below is self-contained: why it matters, where things
stand today (with file pointers), what to build, how to know it's done, and
the traps we already know about. Work top to bottom unless the user says
otherwise; items within a priority band are independent.

## Read first

1. [`ARCHITECTURE.md`](ARCHITECTURE.md): module map, state machines, the
   full HTTP API, the FFI embedding (§8), known gaps (§10).
2. [`../ui/README.md`](../ui/README.md): how to run the app, including two
   instances on one machine.
3. [`../ui/docs/adr/`](../ui/docs/adr/README.md): why the UI is shaped the
   way it is. ADR 0001 and 0003 are the ones you will lean on.

## Ground rules (set by the project owner)

- **The UI is dumb.** It persists nothing and reads/writes only through the
  HTTP API. If a feature needs data the API doesn't have, add the endpoint
  (and, if the data changes over time, an event) in Rust first.
- **Every resource needs a snapshot endpoint and events.** The UI loads a
  snapshot, patches it from `/events`, and refetches after any reconnect
  (ADR 0003). A resource with events but no list endpoint (or the reverse)
  leaves the UI unable to recover after a gap.
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
cargo test --workspace --all-targets
(cd ui && dart run build_runner build --delete-conflicting-outputs \
       && flutter analyze && flutter test)
git diff --check
```

Also run the real app for any UI change (see "Verifying in the real app" at
the end). Unit tests with a fake daemon host missed two real bugs in the
first milestone.

## Where things stand

Working and verified live (UI ↔ CLI daemon over loopback):

- Device list with live reachability, device details, unpair.
- Add device: scan, list nearby unpaired devices, start pairing, show the
  verification code and the outcome.
- Incoming pairing prompt on any screen, which clears itself when the request
  is resolved elsewhere or expires.
- An embedded daemon started through FFI on a free port with a per-launch
  token, and a "reconnecting" banner if the event stream drops.
- Close to tray, tray Show and Quit, pairing-request notifications, and a
  single instance on Linux (item 3).
- Sending a file from a device's page, a transfers view with progress,
  cancel, and open file/folder, and a notification for received files
  (item 4).

Only Linux bundles the native library. Nothing has been tested against a real
KDE Connect install yet.

## Priorities at a glance

| # | Item | Priority | Mostly |
| --- | --- | --- | --- |
| 1 | ~~[Tell the peer when unpairing](#1-tell-the-peer-when-unpairing)~~ **Done** | P0 | Rust |
| 2 | ~~[Interop check against real KDE Connect](#2-interop-check-against-real-kde-connect)~~ **Done** | P0 | Manual + Rust fixes |
| 3 | ~~[Keep running in the background](#3-keep-running-in-the-background-tray-and-notifications)~~ **Done** | P0 | Flutter |
| 4 | ~~[Send files and a transfers view](#4-send-files-and-a-transfers-view)~~ **Done** | P1 | Flutter (+ small API) |
| 5 | [Daemon settings API and screen](#5-daemon-settings-api-and-settings-screen) | P1 | Rust + Flutter |
| 6 | [OS clipboard integration](#6-os-clipboard-integration) | P1 | Rust |
| 7 | [Automated end-to-end test](#7-automated-end-to-end-test) | P1 | Test infra |
| 8 | [Ping: send button and receiving](#8-ping-send-button-and-receiving) | P2 | Flutter + Rust |
| 9 | [Add device by IP address](#9-add-device-by-ip-address) | P2 | Rust + Flutter |
| 10 | [macOS and Windows packaging](#10-macos-and-windows-packaging) | P2 | Build |

---

## 1. Tell the peer when unpairing

> **Done (2026-09-24).** `forget_device` queues `pair: false` before
> cancelling the connection, and the LAN connection loop now flushes queued
> packets on cancel (bounded by `CLOSE_FLUSH_TIMEOUT`). A `pair: false` from
> a paired peer outside a pairing session removes trust and publishes
> `device.updated` with `paired: false`, keeping the connection open. Covered
> by unit tests in `src/application/service.rs` and the unpair step of
> `tests/pairing_e2e.rs`. See ARCHITECTURE §4.

**Why.** Unpairing is one-sided today. If A unpairs B, B still shows A as
paired, reconnects, and has trust that A no longer honours. We saw this live:
the CLI unpaired the app, and the app kept listing the CLI as paired.

**Current state.** Both directions are broken:

- *Sending.* `ApplicationHandle::forget_device`
  (`src/application/service.rs`) removes trust, forgets the device, cancels
  the connection and emits `device.forgotten`, but never sends
  `kdeconnect.pair {"pair": false}`.
- *Receiving.* `handle_pair_body` handles `pair: false` only when a pairing
  session is in progress (it looks up `pairing_by_device` and returns early
  otherwise). A `pair: false` from an already-paired peer is ignored.

**Build.**

- In `forget_device`, if the device has a live connection, send
  `kdeconnect.pair {"pair": false}` on it *before* cancelling the
  connection. The sender is `try_send` into a bounded channel, so give the
  writer a moment to flush (or add a "send then close" path on the
  connection) rather than cancelling in the same instant.
- In `handle_pair_body`, when `pair` is false and there is no active pairing
  but the device is trusted: remove trust from the `TrustStore`, set
  `paired = false` in the registry, and publish `device.updated`. Decide
  whether to also drop the connection; KDE Connect keeps it open but unpaired.
- The UI needs no change: `device.updated` with `paired: false` moves the
  device out of the home list and into "Add device".

**Done when.** A new e2e test in `tests/pairing_e2e.rs` pairs two peers,
unpairs from one side, and asserts that the other side reports
`paired: false` and has no trust entry. In the app, unpairing from the CLI
removes the device from the app's list without a restart.

---

## 2. Interop check against real KDE Connect

> **Done (2026-09-24), against KDE Connect for Android.** Checked with the CLI
> daemon (real LAN, separate data dir) against the owner's Pixel 8a. Every
> step works both ways except ping from the phone, which is item 8. Fixed along
> the way: `ApiClient::send_file` didn't set the file part's `Content-Length`
> header, so `myconnect send` always failed with `missing_declared_size`.
> Incoming pair requests were checked against the 30 s pairing timeout
> instead of KDE Connect's 1800 s clock-skew tolerance, so a phone whose clock
> was 2 minutes off couldn't pair with us. Dropped pair requests and received
> packet types are now logged at debug level. The phone only sends its
> clipboard when the user taps "Send clipboard" (an Android 10+
> restriction). Each UDP announcement from a connected peer makes us re-dial
> and replace the session; upstream does the same (rate-limited per device),
> so this is expected. Still open: a KDE Connect *desktop* peer, and a pass
> through the Flutter app instead of the CLI. See ARCHITECTURE §10.

**Why.** Every test so far is MyConnect against MyConnect. The protocol code
follows the research in [`archive/KDECONNECT_PROTOCOL_RESEARCH.md`](archive/KDECONNECT_PROTOCOL_RESEARCH.md),
but no real KDE Connect peer has been exercised. A phone running KDE Connect
("Pixel 8a") was visible on the owner's LAN during the last session, so this
can be done now. **Ask the user before pairing with their real devices.**

**Build / do.**

- Run the app *without* `MYCONNECT_DISCOVERY_LOOPBACK` so it announces on the
  real network. Use a separate `MYCONNECT_DATA_DIR` so the session doesn't
  touch the user's identity.
- Check, in this order: discovery both ways, TLS handshake (watch for
  certificate or verification errors in the log), pairing in both
  directions with matching codes, clipboard sync, file send and receive,
  ping (the phone should show a notification), and unpair (after item 1).
- Log each failure as a gap in `ARCHITECTURE.md` §10 or fix it. Expect
  surprises in identity fields, capability names, and payload transfer
  negotiation.

**Done when.** `ARCHITECTURE.md` §10 no longer says interop is untested, or
lists precisely what doesn't work.

---

## 3. Keep running in the background (tray and notifications)

> **Done (2026-09-24), on Linux.** Closing the window hides it
> (`window_manager`). The tray icon (`tray_manager` 0.7, a D-Bus
> StatusNotifierItem, so no libappindicator) offers Show and Quit, and Quit
> stops the daemon before the process exits. Pairing requests that arrive
> while the window is unfocused raise a notification
> (`flutter_local_notifications`). The notification is withdrawn when the
> request is resolved, and clicking it brings the window back with the
> prompt. The Linux runner is a unique `GApplication`, so a second launch
> shows the running window and exits. The policy is in
> `lib/src/features/background/background_host.dart`; the plugins sit behind
> `DesktopShell` and `DesktopNotifications` (`lib/src/core/desktop/`), faked
> in widget tests. Verified live under Xvfb with a private D-Bus session, a
> stand-in notification server and a stand-in tray host: close, then a CLI
> pairing request, a notification, click, prompt, cancel from the CLI (the
> notification closes), a second launch, then tray Show and Quit (API and LAN
> ports closed, the peer sees the device unavailable). See ADR 0007. Not done:
> start on login (it needs item 5's settings), a Windows single-instance
> mutex (item 10), and a check on a real desktop panel (KDE, or GNOME with
> the extension). Items 4 and 8 can notify through `desktopNotificationsProvider`.

**Why.** The daemon lives inside the app process (ADR 0002). Closing the
window currently exits the app, which stops the daemon, so devices lose
connectivity and incoming pairing requests or files go unnoticed. For a KDE
Connect-style app, "running" has to mean "running in the tray".

**Current state.** `lib/src/app.dart` stops the daemon in
`AppLifecycleListener.onExitRequested`. There is no tray, no notifications,
and no single-instance guard.

**Build.**

- *Close to tray.* Closing the window hides it; a tray menu offers
  Show and Quit. Only Quit (or a real OS shutdown) stops the daemon and
  exits. Candidate packages: `window_manager` (intercept close and hide the
  window) and `tray_manager` (tray icon and menu), both widely used on
  desktop. Check their current Linux support (AppIndicator on GNOME needs
  the extension) and record the choice in ADR 0005.
- *Desktop notifications* for: an incoming pairing request (clicking it shows
  the window, where the prompt is already displayed), a received file (item
  4), and a received ping (item 8). Candidate: `flutter_local_notifications`,
  which supports Linux, macOS and Windows. Drive notifications from the same
  controllers the UI uses (e.g. listen to `pendingIncomingPairingsProvider`
  for new ids), not from a second event subscription.
- *Single instance.* A second launch should focus the running window rather
  than start a second daemon, which would fight for UDP 1716 and create a
  confusing second identity if the data dir differs.
- *Start on login* (optional): an autostart `.desktop` entry on Linux. It is
  a UI setting that has to live in the daemon (item 5), per ground rule 1.

**Pitfalls.** Keep daemon shutdown on the real quit path, and keep the
`ProviderScope` alive while the window is hidden, since disposing it stops the
daemon through `daemonEndpointProvider`.

**Done when.** The app can be closed to the tray, still accepts a pairing
request (notification, then prompt), and Quit stops the daemon cleanly (the
API port stops listening).

---

## 4. Send files and a transfers view

> **Done (2026-09-24).** The device page has a "Send file" button
> (`file_selector`), enabled when the peer is connected and lists
> `kdeconnect.share.request`, and shows that device's five most recent
> transfers. `/transfers` lists all of them. Each row shows direction,
> progress and outcome, with Cancel while running and Open file/Open folder
> (`url_launcher`) once received. `TransfersController` follows the pairings
> pattern and never lets a stale upload response overwrite a newer event.
> Received files raise a notification while the window is unfocused. Daemon
> changes: completed incoming snapshots carry `savedPath` (absolute), so no
> download directory is needed in `/status`. The upload route no longer
> sits under the 15 s request deadline (the "verify first" worry was real:
> a new test in `tests/api.rs` fails without the fix), and fails with
> `request_timeout` only when the upload stalls for that long.
> `transfer.progress` events are throttled to one per 100 ms per transfer,
> since one per 64 KiB chunk could overflow the 256-slot event bus and drop
> the UI's stream. Verified live under Xvfb against a CLI peer: 50 MB each
> way, byte-identical; a 2 GB upload still running after 20 s, then
> cancelled from the app (the partial file was removed on the peer); Open
> folder launched the file manager on the download directory. Not done:
> drag-and-drop onto a device (`desktop_drop`), sending several files at
> once (the API takes one file per request), and a live check of the
> received-file notification (it is covered by a widget test).

**Why.** File transfer is a core feature and the API already supports it end
to end; only the UI is missing.

**Current state (API).**

- `POST /api/v1/transfers`: streaming `multipart/form-data` with a
  `deviceId` text field **first**, then one `file` part. The `file` part
  **must carry its own `Content-Length` part header** (the declared size is
  checked against the configured maximum before anything is sent). See
  `post_transfer` in `src/api.rs`.
- `GET /transfers`, `GET /transfers/{id}`, `DELETE /transfers/{id}` (cancel).
- Events: `transfer.started/progress/completed/failed`. The UI currently
  decodes these as `UnhandledEvent` (`lib/src/core/api/models/event.dart`).
- Only paired, connected devices can send or receive.

**Build.**

- *Models and API:* a Freezed `Transfer` model mirroring `TransferSnapshot`,
  and `transfers()`, `sendFile(deviceId, path)` and `cancelTransfer(id)` in
  `MyConnectApi`. For the upload, use `MultipartFile.fromFile(path,
  headers: {'content-length': ['$length']})` and add the `deviceId` field to
  `FormData` before the file. Give this request no receive timeout, or a
  generous one; the default dio options would cut off big files.
- *Controller:* `TransfersController` following the devices/pairings pattern
  (snapshot, upsert on `transfer.*` events, refetch on reconnect). Progress
  events can be frequent; they're full snapshots, so upserting is cheap,
  but consider throttling rebuilds.
- *UI:* a "Send file" action on the device page (file picker: the official
  `file_selector` package), optional drag-and-drop onto a device
  (`desktop_drop`), and a transfers list showing direction, progress, cancel
  and final state.
- *"Open received file/folder":* the API doesn't expose where files land.
  Add the download directory to `GET /status` (or a saved path on completed
  incoming transfer snapshots), then open it with the `url_launcher` or
  `open_file` package.

**Verify first.** `enforce_request_timeout` (15 s) in `src/api.rs` appears to
wrap every route, including the streaming upload. The handler only returns
after the whole body has been forwarded to the peer, so a large or
slow-to-drain upload may be cut off at 15 s. Test with a big file to a peer
that reads slowly, and exempt the upload route if needed, the way it is
already exempted from the body-size limit.

**Done when.** A file sent from the app arrives intact on a CLI peer,
progress is visible, cancel works mid-transfer, and a file received from the
CLI shows up in the list with a working "open folder".

---

## 5. Daemon settings API and settings screen

**Why.** Ground rule 1 means user preferences can't live in the UI. Today
the device name, download directory and discovery mode come only from start
options, so the user has no way to change them.

**Current state.** `RunRequest` (`src/application.rs`) takes `device_name`,
`download_dir`, etc. at start. `ApplicationHandle::set_clipboard_sync_enabled`
exists but isn't exposed over HTTP. There is no settings file.

**Build.**

- Persist a small `settings.json` in the config dir (write atomically, as
  `config/trust.rs` does). Fields to start with: `deviceName`, `downloadDir`,
  `clipboardSyncEnabled`, plus UI-owned preferences from item 3 (e.g.
  `startOnLogin`, `closeToTray`) that the daemon stores but doesn't
  interpret.
- `GET /api/v1/settings` and `PATCH /api/v1/settings`, plus a
  `settings.changed` event.
- Decide precedence and document it: explicit start options (CLI flags or FFI
  config) override stored settings for that run, or they only seed
  defaults. The FFI currently passes the hostname as the device name on
  every start, which would always override a stored name, so change the UI to
  stop sending it once settings exist.
- Renaming must reach peers: re-announce the identity (UDP) and update
  the name sent in identity packets for new connections.
- A settings screen in the UI (route `/settings`).

**Done when.** Renaming the device in the app changes the name a CLI peer
sees after a scan, and the setting survives an app restart.

---

## 6. OS clipboard integration

**Why.** Clipboard sync works over the network, but the only backend is
`InMemoryClipboard` (`src/clipboard.rs`), so nothing reaches the real
desktop clipboard.

**Build.**

- A `ClipboardService` implementation over the `arboard` crate (the standard
  cross-platform clipboard crate). It is synchronous, so call it with
  `spawn_blocking`.
- *Local change detection:* `arboard` has no change events, so poll (e.g.
  every 500 ms) and feed changes through the same path as `PUT /clipboard`.
  The feedback-loop and duplicate guards in the application core (see
  ARCHITECTURE §6) must still hold, so text just received from a peer must
  not bounce back.
- On Linux, Wayland clipboard access from a non-focused client is
  restricted. Check `arboard`'s `wayland-data-control` feature, and fall back
  gracefully (log and keep the in-memory backend) when the session doesn't
  allow it.
- Make it selectable (`RunRequest` / FFI config), keeping `InMemoryClipboard`
  for tests and headless runs.
- UI: a clipboard sync toggle (via item 5's settings) and, optionally, the
  last synced text on the device page (`GET /clipboard` plus
  `clipboard.changed`).

**Done when.** Copying text on one machine makes it pasteable on the other,
in both directions, without loops. Never log clipboard contents, only lengths.

---

## 7. Automated end-to-end test

**Why.** The last milestone's two worst bugs (a daemon start crash from
isolate capture, and `dart fix` stripping `--dart-define`s) only showed up
in the real app. They were found manually.

**How the manual run worked** (reproduce it in CI):

- Start a CLI peer: `myconnect --api-port 25011 run --discovery-loopback
  --data-dir <tmp> --device-name "CLI Peer"`.
- Build the app with `--dart-define`s for loopback discovery, a temp data dir
  and a device name (see `ui/README.md`).
- Run it on a virtual display (`Xvfb :77`, `DISPLAY=:77 GDK_BACKEND=x11`).
- Drive the peer through its API. It has no token, so `curl
  http://127.0.0.1:25011/api/v1/pairings` works.

**Build.** Prefer Flutter's `integration_test` package, running the real app
with the real FFI library and pressing buttons through the widget tester, over
screenshot-and-click. Have the test spawn the CLI peer as a subprocess and
drive it with `dart:io` HTTP calls. Cover the flows: incoming
pairing accept and reject, outgoing pairing, unpair, and (after item 4) a
file round trip. Run on Linux in CI under `xvfb-run`.

**Done when.** `flutter test integration_test -d linux` passes locally and in
CI.

---

## 8. Ping: send button and receiving

**Why.** It's a cheap, visible "is it working?" feature, and it's what KDE
Connect users expect.

**Current state.** `POST /devices/{id}/ping` exists (optional message). It
returns `409 unsupported_by_peer` if the peer doesn't list `kdeconnect.ping`
in its incoming capabilities, and `device_not_connected` if the peer is
offline. Incoming pings are dropped and not advertised
(`src/plugins/mod.rs`).

**Build.**

- *UI:* a "Ping" button on the device page, enabled only when the device is
  connected and its `incomingCapabilities` contains `kdeconnect.ping`. Add
  a `ping()` method to `MyConnectApi` and friendly messages for the error
  codes in `ApiException.message`.
- *Rust, receiving:* handle `kdeconnect.ping` in the plugin dispatch,
  advertise it in `plugins::capabilities()`, and publish a new
  `ping.received` event (device id and name, optional message). Update the
  capability tests.
- *UI, receiving:* show it as a notification (item 3) or a snackbar.
  Decode the new event type in `DaemonEvent.fromJson`.

**Done when.** Pinging from the app to a CLI peer succeeds, and a ping
from the CLI to the app surfaces in the UI.

---

## 9. Add device by IP address

**Why.** UDP broadcast discovery fails on many networks (client isolation,
VPNs, separate subnets). KDE Connect lets users add a device by IP address
for these cases.

**Build.**

- *API:* `POST /api/v1/discovery` accepting an optional
  `{"address": "192.168.1.20"}`. Without a body, keep today's broadcast.
  With one, send the identity packet by unicast UDP to that address on port
  1716. That makes the peer dial back over TCP, which is the normal KDE
  Connect flow. Validate the address, and keep the API from becoming a
  general-purpose UDP sender.
- *Transport:* `LanService` (`src/transport/lan.rs`) currently only
  announces to configured broadcast targets; add a one-shot unicast
  announce.
- *UI:* an "Add by IP address" action on the Add device page.
- Remembering manually added addresses across restarts is a daemon setting
  (item 5), not UI state.

**Done when.** With loopback discovery off, and broadcast blocked or
unavailable, entering a peer's IP makes it appear in the Add device list.

---

## 10. macOS and Windows packaging

**Why.** The long-term goal is macOS, Linux and Windows. Only Linux bundles
`libmyconnect_ffi` today (ADR 0006).

**Build.**

- *macOS:* an Xcode build phase in `ui/macos/Runner` that runs
  `cargo build -p myconnect-ffi` and copies `libmyconnect_ffi.dylib` into
  `Contents/Frameworks`. Check the install name (`@rpath`), code signing,
  and the sandbox entitlements the app needs (network client *and*
  server, for UDP 1716 and TCP 1716–1764). `NativeBindings.open()` already
  looks for `libmyconnect_ffi.dylib`.
- *Windows:* the equivalent step in `ui/windows/CMakeLists.txt`, installing
  `myconnect_ffi.dll` next to the executable. Expect a firewall prompt on
  first run.
- Consider replacing the per-platform steps with a Flutter build hook
  (`hook/build.dart`), as ADR 0006 suggests. If so, supersede that ADR.

**Done when.** `flutter build macos` and `flutter build windows` produce apps
that start their embedded daemon. Until then, those platforms work only
against an external daemon (`--dart-define=MYCONNECT_API_URL=...`).

---

## Smaller known follow-ups

- Snapshots carry no sequence number, so an event emitted just before a
  snapshot response can briefly be overwritten by older data (ADR 0003).
  If this shows up in practice, add the event bus sequence to list
  responses (e.g. a header) and drop older events.
- The reconnecting banner and the startup error screen have not been
  exercised in the real app, only in unit tests.
- Widgets that `await` a mutation must capture the router or messenger
  beforehand, because an event can unmount them mid-await (see
  `device_detail_page.dart`). Apply the same care to new screens.
- `--dart-define` reads live only in `DaemonHost.fromEnvironment`, with an
  ignore for `avoid_redundant_argument_values`. Never run `dart fix` on
  that file without checking the diff.
- In debug builds the DEBUG banner covers the rightmost app bar action,
  which on the home screen is the Transfers button (at about x 1244–1268,
  y 14–38 in a 1280-wide window). It is there and clickable, just hidden.
  Don't mistake it for a missing widget, and use `find.byTooltip` in tests.
- `myconnect send` prints only the upload's response, which is taken once
  the last byte has been forwarded, so it ends on
  `transferring (N/N)` rather than `completed`. Waiting for the terminal
  state (or watching `/events`) would make the CLI report the real
  outcome.
- A transfer the sender cancels shows up on the receiver as `failed` with
  `connection_failed`, not `cancelled`, because the receiver only sees the
  payload connection close early. KDE Connect has no cancel notice in the
  share protocol either, so this probably stays; a UI could word it as
  "stopped by the sender" if it becomes confusing.

## Verifying in the real app

```sh
# peer
cargo run -- --api-port 25011 run --discovery-loopback \
  --data-dir /tmp/peer --device-name "CLI Peer"
# app (separate identity, loopback only)
cd ui && flutter run -d linux \
  --dart-define=MYCONNECT_DISCOVERY_LOOPBACK=true \
  --dart-define=MYCONNECT_DATA_DIR=/tmp/ui \
  --dart-define=MYCONNECT_DEVICE_NAME="UI Desktop"
```

Without a display (e.g. in an agent sandbox), run the built bundle under
`Xvfb`, take screenshots with `import -display :NN -window root out.png`, and
click with XTest (`libXtst` through Python `ctypes`). Don't open windows on
the user's own session, and don't pair with or send to real devices on
their network without asking.

Tips from the item 4 check:

- Launch the app under `dbus-run-session -- env DISPLAY=:NN
  GDK_BACKEND=x11 ...` so its D-Bus services (file chooser portal,
  notifications) stay private. The "Send file" picker then opens as a GTK
  dialog on the virtual display; press Ctrl+L, type the absolute path, and
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
- Don't clean up with `pkill -f <pattern>`: the pattern also matches the
  shell running the command, and kills it. Kill by PID.
