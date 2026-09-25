# MyConnect architecture

This document describes the system as implemented today: module boundaries,
data flow, the HTTP API surface, and the state machines that govern pairing
and transfers. It is the map an agent or contributor should read before
making a change.

Historical design work — protocol research notes and the phase-by-phase
implementation plan used to build the MVP — is preserved under
[`docs/archive/`](archive/) for reference. Both phases described there are
complete; this document supersedes them as the source of truth for current
behavior.

## 1. Shape of the system

MyConnect is a Cargo workspace: the `myconnect` package (one library,
`src/lib.rs`, plus a CLI binary that is a thin client of it), and the
`myconnect-ffi` package (`ffi/`), a C ABI that lets a GUI embed a daemon.
The Flutter desktop app lives in `ui/`.

```text
CLI (myconnect)       ─┐
Flutter UI (ui/)       ├── local HTTP API (/api/v1) ── core + plugins ── KDE Connect transport
Other automation      ─┘
```

The CLI and the Flutter UI talk to the daemon exclusively through the local
HTTP API. Neither owns sockets, pairing state, trust state, or transfer
state — that all lives inside the daemon process, behind
`core::Core`. The UI additionally uses the FFI library, but only to
start and stop an embedded daemon (§9); the UI's own design decisions are
recorded in [`ui/docs/adr/`](../ui/docs/adr/README.md).

## 2. Module map and dependency direction

The daemon is a small **core** and a fixed set of **plugins**, one per
feature. The core owns devices, connections, pairing, trust, transfers,
settings and the event bus; a plugin owns one feature's packets, state,
routes and events, and reaches the core only through the `PluginContext`
it is given. The set of plugins is fixed at compile time, listed in
`plugins::builtin()`; nothing is loaded at runtime, and there is no plugin
ABI. Boundaries are kept by module visibility and review, in one crate.

```text
binary (src/bin/myconnect) → daemon, client
ffi (myconnect-ffi) → daemon
gui (myconnect-gui) → daemon, ui, plugins::builtin_with_ui   [feature "gui"]
ui → core (snapshots, events), protocol (types only)         [feature "gui"]
daemon → core, plugins::builtin, api, transport (the composition root)
api → core (core routes, plugin routes merged in)
plugins/* → core (Plugin, PluginContext), api (ApiProblem, upload helpers), protocol
core → config, transport, protocol
transport::lan → core (it registers connections and delivers packets)
client → core (snapshot and event types), plugins/* (their types)
```

`protocol` and `transport` never depend on Axum, Clap, or API response
types. The core never names a plugin: it calls them only through
`dyn Plugin`, and `daemon` is the one place that picks them. Plugins never
import each other; what two features need (transfers, payload connections)
is a core service.

| Module | File(s) | Responsibility |
| --- | --- | --- |
| `protocol` | `src/protocol/{mod,packet,codec,verification}.rs` | Wire packet envelope, identity/pairing body types, bounded newline-delimited JSON codec, the protocol-v8 verification-code function. No I/O. |
| `config` | `src/config/{mod,identity,settings,token,trust}.rs` | Local device identity (UUID + self-signed cert), the optional API bearer token (never persisted), filesystem-backed `TrustStore` of pinned peer certificates (one `trusted-devices/<id>.json` each, with the name, type and capabilities the peer last reported over an authenticated connection), and `settings.json` (user settings, written atomically). |
| `transport` | `src/transport/{lan,tls,payload}.rs` | UDP discovery, TCP control-channel connect/accept, the real rustls TLS handshake and certificate pinning, and the auxiliary TLS payload connection used for file transfer. `LanConfig::loopback` (the daemon's `--discovery-loopback`) binds discovery to `127.255.255.255` and the control listener to `127.0.0.1`, so nothing on the LAN can discover or reach the instance. It takes `LanCommand`s (announce now, announce to one address) from the core. |
| `core` | `src/core.rs` | `Core`, the cloneable handle to everything below: its state, construction, status, settings, and running the plugins' hooks. One `RwLock` holds what must change together (devices, connections, pairings); transfers, settings and every plugin's state have their own locks. |
| | `src/core/devices.rs` | The device registry and `DeviceSnapshot` (whose `plugins` map is filled from each plugin's `device_state` when a snapshot leaves the core), discovery, forgetting a device, and keeping a paired device's trust record up to date. The registry starts with every paired device from the `TrustStore`, as `unavailable`, so paired devices are listed while offline. |
| | `src/core/connections.rs` | Registering and dropping authenticated control channels, routing each incoming packet to the plugin that claims its type (only from paired devices), sending to devices that advertised a packet type, and `LanCommand`, the channel to the LAN transport. |
| | `src/core/pairing.rs` | The pairing state machine (§4) in both directions, its timeouts, and the trust it writes or removes. |
| | `src/core/transfers.rs`, `src/core/payload.rs` | The transfers service (`Transfers`, `TransferHandle`: the state machine, progress throttling, cancellation and cleanup, for every feature that moves a file, §5), and payload connections for plugins (`PayloadPeer`: listen or dial with this device's certificate, or sign in to an SSH server on the device with its key, without handing out the key). |
| | `src/core/{plugin,events,settings,error}.rs` | The plugin API (`Plugin`, `PluginContext`, `PluginRegistry`, `Capabilities`, plugin events and settings sections), the bounded event bus (plugin events travel as `EventData::Plugin` with the same `{type, data}` shape), user settings with a section per plugin that has settings (§7), and `CoreError`. |
| | `src/core/testing.rs` | A real core for unit tests: in-memory trust store, no plugins or just the one under test, no LAN. |
| `plugins` | `src/plugins/mod.rs`, `src/plugins/{ping,findmyphone}/{mod,packet,http}.rs`, `src/plugins/battery/{mod,packet}.rs`, `src/plugins/clipboard/{mod,packet,http,backend}.rs`, `src/plugins/clipboard/backend/system.rs`, `src/plugins/share/{mod,packet,http}.rs`, `src/plugins/browse/{mod,packet,http,session,ssh,files}.rs` | The features, each a `core::Plugin`; `builtin()` lists them. Ping owns its packet handling, `ping.received` event and `POST /devices/{id}/ping`. Find my phone only sends, and owns `POST /devices/{id}/ring`. Battery adds `plugins.battery` to device snapshots and clears it through the `disconnected`/`unpaired` hooks. Clipboard owns the synced text and `/clipboard`, the `plugins.clipboard` settings section, and its backends (the `ClipboardService` trait, the desktop clipboard `SystemClipboard` over `arboard`, an in-memory one); it follows the desktop clipboard from its `started` hook and releases it in `shutdown` (§6). Share sends files through its streaming route `POST /devices/{id}/share` and saves files peers send, both as core transfers (§5). Browse owns the per-device SFTP sessions with peers' file servers and the `/devices/{id}/files` routes (the upload as a streaming route), and closes its sessions through the `disconnected`/`unpaired`/`shutdown` hooks (§12). Capabilities advertised in the identity packet are the union over the plugins: ping, clipboard and share in both directions; `kdeconnect.sftp.request` outgoing and `kdeconnect.sftp` incoming only, since this build browses peers but serves no files; `kdeconnect.battery` incoming only, since it reads peers' batteries but reports none; `kdeconnect.findmyphone.request` outgoing only, since this build asks peers to ring but doesn't ring itself. |
| `daemon` | `src/daemon.rs` | The composition root: `RunningService` builds the core with `plugins::builtin()`, applies the stored settings, starts the plugins, the LAN transport (advertising the core's capabilities) and the API, and stops them in order. Used by the CLI's `run` and by the FFI. `start_with` takes the plugin list from the caller (the desktop app, which keeps each plugin's UI half), and `core()` hands the running core to a frontend in the same process. |
| `api` | `src/api.rs`, `src/api/upload.rs` | The Axum server: the core's routes (`/status`, `/discovery`, `/devices`, `/pairings`, `/transfers`, `/settings`, `/events`), every plugin's routes merged in, `ApiProblem` (the `application/problem+json` error every handler returns, with `From<CoreError>`), optional bearer-token auth, body-size limits, request deadline and SSE. Streaming routes (every plugin's `streaming_routes`) get the transfer-sized body limit and no request deadline; `upload` has the helpers they share (idle timeout, forwarding a multipart file part into a transfer until the part or the transfer ends, and the lingering close that drains an upload a handler answered before reading to its end). |
| `client` | `src/client.rs` | Typed HTTP client used by the CLI (and any future frontend) to talk to `api`. |
| `src/bin/myconnect` | `cli.rs`, `main.rs` | Argument parsing and daemon bootstrap only. |
| `ui` | `src/ui/{mod,plugin,route,store,sync,activity,demo,error,widgets}.rs`, `src/ui/pages/*.rs`, `src/ui/overlay/*.rs`, `src/ui/desktop/*.rs` | The native desktop UI in iced, behind the `gui` feature ([`adr/0001`](adr/0001-native-ui-in-iced.md), [`PLAN_ICED_UI.md`](PLAN_ICED_UI.md)). It runs in the daemon's process and reads the core directly: `sync` subscribes to the event bus, then takes a snapshot, and takes a fresh one after a lag. It never names a feature: each feature's UI half lives in `src/plugins/<name>/ui.rs` (ping, find my phone, battery and clipboard so far), implements `ui::plugin::UiPlugin` with its own message type, and is listed by `plugins::builtin_with_ui()`. The shell stores them erased, routes their messages back by plugin id, fills its slots from them (the device card's status chips, the device page's action buttons, the settings page's sections), and does what they ask through `ShellRequest` (toast, notify, navigate, confirm). Pairing is shell code: Add device, the pairing page and the incoming pairing prompt, drawn over every page while a request waits. So are Transfers and Settings. `desktop` holds the platform glue behind small traits (opening files with `opener`, the folder picker with `rfd`), so tests swap in fakes. `demo` fills the core with made-up devices for `--demo`; what they report comes from each plugin's `demo_packets`. |
| `myconnect-gui` | `gui/src/main.rs` | The desktop app's composition root: flags (each also an environment variable), starting the daemon through `RunningService::start_with` and `plugins::builtin_with_ui()`, running `ui::run`, and shutting the daemon down after. |
| `myconnect-ffi` | `ffi/src/lib.rs` | `cdylib` exporting `myconnect_start` / `myconnect_stop` / `myconnect_free_string` (JSON in, JSON out) so a GUI process can embed a daemon. See §9. |

### The `Plugin` trait

```rust
pub trait Plugin: Send + Sync + 'static {
    fn id(&self) -> &'static str;                          // "ping"; names its settings and device state
    fn incoming(&self) -> &'static [&'static str] { &[] }  // packet types it handles
    fn outgoing(&self) -> &'static [&'static str];         // packet types it sends
    fn handle_packet(&self, ctx: &PluginContext, device: &DeviceSnapshot, packet: &Packet) {}
    fn routes(self: Arc<Self>, ctx: PluginContext) -> Router { Router::new() }
    fn streaming_routes(self: Arc<Self>, ctx: PluginContext) -> Router { Router::new() }
    fn device_state(&self, device_id: &str) -> Option<Value> { None }
    fn settings(&self) -> Option<SettingsSection> { None }
    fn connected(&self, ctx: &PluginContext, device: &DeviceSnapshot) {}
    fn disconnected(&self, ctx: &PluginContext, device_id: &str) {}
    fn unpaired(&self, ctx: &PluginContext, device_id: &str) {}
    fn started(self: Arc<Self>, ctx: &PluginContext) {}
    fn shutdown(&self) -> BoxFuture<'_, ()> { Box::pin(async {}) }
}
```

Handlers are synchronous and spawn tasks when they need to, so the trait is
dyn-compatible without `async_trait`. The core passes the context into each
call rather than plugins storing it, so there is no `Arc` cycle between the
core and its plugins. The rules the core keeps:

- **Dispatch.** Pairing packets are the core's. Any other packet goes to
  the plugin whose `incoming()` claims its type, and only from a paired
  device; a type nobody claims is dropped. Two plugins claiming one type,
  or sharing an id, panic when the registry is built, so every test fails.
- **Locks.** The core never calls into a plugin while holding its own lock,
  and a plugin doesn't hold its own while calling the core.
- **Hooks.** `connected` runs after `device.connected` is published;
  `disconnected` and `unpaired` run before the core publishes the device's
  new state, so state a plugin clears there needs no extra
  `device.updated`. `started` runs once in the daemon, inside the runtime,
  before the transport starts (never in a unit-test core); `shutdown` runs
  for every plugin concurrently after the API and transport have stopped
  and every transfer has ended.
- **HTTP.** `routes()` get the standard body limit, deadline and auth;
  `streaming_routes()` (uploads) get the transfer-sized limit and no
  overall deadline. Paths are resources under the device they act on
  (`/devices/{id}/ping`) or a top-level resource of the plugin's own
  (`/clipboard`), never a `/plugins/<id>/` prefix. Overlapping routes
  panic when the router is built.
- **Device state and settings.** A plugin adds to a device's snapshot
  under `plugins.<id>` by answering `device_state` (pulled whenever a
  snapshot leaves the core) and calls `ctx.device_changed(id)` when its
  answer changes. It owns a typed settings section, stored and exposed
  under `plugins.<id>` (§7), and reads it with `ctx.settings::<T>()`.
- **Events and errors.** A plugin publishes its own event types
  (`ctx.publish(&T)` for `T: PluginEventKind`); they look like core events
  on the wire. Its errors map to `ApiProblem` inside the plugin; core errors
  convert with `?`.

`PluginContext` offers: `device(id)` and `device_changed(id)`;
`send(device, packet)` (paired, connected, and the peer advertised the
type), `can_send` and `broadcast(packet, except)`; `publish`;
`settings::<T>()`; `transfers()`; and `payload_peer(device)` for payload
connections and SSH sign-in without the private key.

A new feature is a module under `plugins/` implementing `core::Plugin`,
with its unit tests against `core::testing::handle_with_plugin`, plus one
line in `plugins::builtin()`. `client.rs`, the CLI, the UI and this
document follow it by hand. [`research/feature-modules.md`](research/feature-modules.md)
records how the daemon was moved to this shape, feature by feature, and
what each step taught.

## 3. Connection lifecycle

1. **Discovery** (`transport::lan`): UDP broadcast/listen on port 1716.
   Each peer broadcasts a protocol-v8 identity packet containing its chosen
   TCP port (selected from `1716-1764`). Malformed, oversized, self, and
   unsupported-version identities are dropped without affecting the device
   registry. Where broadcast doesn't reach a peer, its address can be given
   (`POST /discovery` with `address`): the identity is then sent by unicast
   to that IPv4 address on port 1716, and the peer dials back as it would
   after a broadcast. Only unicast addresses are accepted, and the port and
   payload are fixed, so the endpoint can't be used as a general UDP sender.
2. **Plaintext identity, then TLS** (`transport::tls`): the peer that
   received a UDP announcement dials the announced `tcpPort` and sends its
   identity once in plaintext, carrying `targetDeviceId` and
   `targetProtocolVersion`; the accepting peer only reads it (its identity
   already arrived over UDP). TLS roles are inverted relative to TCP, as in
   KDE Connect: the dialer is the TLS *server* and the acceptor the TLS
   *client*. Only UDP announcements carry `tcpPort`. The connection then
   upgrades to a real TLS 1.2/1.3 handshake (rustls, real signature verification — no
   accept-all verifier exists in this codebase), then exchanges identity a
   second time *inside* TLS. Device ID and protocol version must match
   between the two exchanges; a mismatch or downgrade against a previously
   trusted protocol version fails the connection closed.
3. **Trust check**: if the peer's device ID has a pinned certificate in the
   `TrustStore`, the TLS verifier requires an exact match. Unknown peers are
   accepted at the TLS layer (so pairing can proceed) but cannot exchange
   any packet type other than pairing packets until paired (see §4).
4. **Steady state**: a per-connection packet read/write loop
   (`transport::lan`) hands each incoming packet to the core
   (`Core::handle_peer_packet`), which handles pairing packets itself and
   routes every other type to the plugin that claims it, only for
   already-paired devices (§2).

## 4. Pairing state machine

States: `requested → awaiting_confirmation → accepted | rejected | expired | failed`.

- The same pairing resource represents both incoming and outgoing requests,
  distinguished by `direction`.
- A pairing session always reaches a terminal state; the associated 30-second
  timeout timer is aborted on every terminal transition so no task leaks.
- Trust is written to the `TrustStore` only after local user confirmation
  (`POST /pairings/{id}/accept` for incoming, or automatic on receiving the
  peer's accept for outgoing) — never before.
- A paired peer's trust record also keeps how it last described itself
  (name, type, capabilities), written at pairing and refreshed whenever it
  connects, never from a UDP announcement. The daemon lists paired devices
  from these records at startup, as `unavailable` until they are seen.
- `DELETE /pairings/{id}` cancels an in-flight pairing or unpairs/forgets an
  already-trusted device, removing its pinned certificate.
- Unpairing (`DELETE /devices/{id}`) sends `kdeconnect.pair {pair: false}`
  to a connected peer before closing the connection; the transport writes
  out packets already queued when a connection is cancelled, so the notice
  isn't lost to the close. A `pair: false` received outside a pairing
  session from a paired peer removes its trust, sets `paired: false`, and
  publishes `device.updated`; the connection stays open (as in KDE
  Connect), so the device remains reachable and can be paired again.
- An incoming request's `timestamp` (seconds) must be within 30 minutes of
  the local clock, as in KDE Connect; requests without one, or further off,
  are dropped. This tolerance is separate from the 30-second pairing
  timeout: real devices routinely drift by more than 30 seconds.
- Verification codes, certificates, and private keys never appear in a
  pairing snapshot or in logs.

## 5. Transfer state machine

States: `queued → connecting → transferring → completed | cancelled | failed`.

- Transfers are a core service (`core::Transfers`) that every feature
  moving a file uses: sharing and browsing (§12). A feature calls
  `PluginContext::transfers().begin(..)` (or `begin_as(..)`, with an id a
  client chose) and gets a `TransferHandle` that owns
  the state machine, records progress, and ends the transfer; a handle
  dropped without ending it fails it (or cancels it if cancellation was
  asked for), so a transfer never stays running after its task is gone. The
  core lists (`GET /transfers`) and cancels (`DELETE /transfers/{id}`, a
  disconnect, shutdown) transfers whatever started them.
- Sharing (`plugins::share`) uses `kdeconnect.share.request` on the control
  channel to offer one file, then streams bytes over a **separate**
  auxiliary TLS payload connection (`transport::payload`), reusing the same
  TLS material and pinning logic as the control channel. Plugins open these
  through `PluginContext::payload_peer`, which never hands out the private
  key. `kdeconnect.share.request.update` isn't handled: one file per request.
- Uploads are streamed from the HTTP multipart body straight to the network;
  downloads are streamed from the network straight to a temporary
  `.{transfer_id}.part` file. Neither hop buffers a whole file in memory.
- Incoming files: the declared size is checked against a configured maximum
  before dialing the peer; the filename is sanitized to a bare
  `file_name()` (no directory components, no `..`, no empty name) before use;
  the temp file is atomically renamed into place only after every declared
  byte has been written.
- Progress is monotonic; `transfer.completed` is only emitted after durable
  local finalization (incoming) or full acknowledged send (outgoing).
  Every chunk updates the snapshot, but `transfer.progress` is published at
  most every 100 ms per transfer (plus the final byte), so a fast link can't
  overflow the bounded event bus.
- A completed incoming transfer's snapshot carries `savedPath`, the absolute
  path the file was saved to (a ` (n)` suffix is added when the name is
  taken), so clients can open the file or its folder.
  Cancellation, disconnect, and daemon shutdown all clean up the partial
  `.part` file and abort the associated task.
- Only paired devices can initiate or receive transfers.

## 6. Clipboard sync

- `kdeconnect.clipboard` carries `content` and applies unconditionally
  (subject to the duplicate-content guard below).
- `kdeconnect.clipboard.connect` additionally carries a millisecond
  `timestamp`; it is applied only if strictly newer than the last known
  update, so stale or replayed packets are ignored.
- Clipboard is a plugin (`src/plugins/clipboard/`): it holds the synced
  text behind its own lock, reads `plugins.clipboard.syncEnabled` from the
  core when it acts, offers its text to a device from the `connected` hook,
  and sends to all other devices with `PluginContext::broadcast`.
- A feedback-loop guard tracks the last-applied content/source so content
  just received from a peer is never rebroadcast back to that peer, and
  identical content is never resent.
- Text is capped at `MAX_CLIPBOARD_TEXT_BYTES` (32 KiB); oversized `PUT`
  requests get a typed `413` rather than silent truncation.
- Clipboard contents are never logged — only lengths.
- Backends implement `ClipboardService`. `SystemClipboard` is the desktop
  clipboard (`arboard`; on Linux the Wayland data-control protocol where the
  compositor has it, else X11/XWayland), selected by `RunRequest::
  system_clipboard` (`myconnect run --system-clipboard`, FFI
  `systemClipboard`; the app turns it on). `InMemoryClipboard` is the
  default, for tests and headless runs, and the fallback when the desktop
  clipboard can't be opened (logged as a warning).
- `SystemClipboard` owns the clipboard on its own thread: it applies writes
  as they arrive and polls every 500 ms (`POLL_INTERVAL`) for text copied by
  other applications, reporting it through a `watch` channel that
  `ClipboardPlugin::follow_local_changes` feeds into `set_text`, the
  same path as `PUT /clipboard`. Text it wrote itself (e.g. from a peer) is
  not reported, and `set_text` ignores unchanged text anyway, so
  nothing bounces back. Text already on the clipboard at start, empty text
  and non-text content (images) are not reported, and copies made while
  sync is off are dropped.
- Sending to one device on request (`POST /devices/{deviceId}/clipboard`,
  `myconnect clipboard send`, "Send clipboard" in the app and tray) covers
  what automatic sync can miss, e.g. text already on the clipboard at start
  or a peer that dropped an update. It reads the clipboard itself (falling
  back to the snapshot), sends a plain `kdeconnect.clipboard` even if the
  text is unchanged, and works while sync is off. It does not change the
  snapshot.

## 7. Settings

User preferences live in the daemon, in `settings.json` in the data
directory, never in a client. Core fields: `deviceName`, `downloadDir`,
and `closeToTray` (owned by the UI; the daemon stores it without
interpreting it). A field missing from the file uses its default: the host
name (first label, trimmed to a valid KDE Connect name, else "MyConnect"),
the platform download directory, `true`.

- **Plugin sections.** A plugin with settings owns a section under
  `plugins.<id>`, in the file and in `GET`/`PATCH /settings`; so far only
  `plugins.clipboard.syncEnabled` (default `true`). The plugin defines the
  fields, their defaults and what is valid (`PluginSettings`); the core
  stores only the fields the user set, merges a patch into them (`null`
  resets a field, a `null` section resets the section) and answers `400
  invalid_settings` for an unknown section or a value the plugin can't
  read. `GET /settings` always lists every section, defaults filled in.
  The old top-level `clipboardSyncEnabled` is not migrated: a file that
  still has it is read as if it didn't, and it is dropped on the next save.
- **Precedence.** A start option (`myconnect run --device-name` /
  `--download-dir`, or the FFI config's `deviceName` / `downloadDir`)
  overrides the stored value for that run only and is not saved. Changing
  that setting through `PATCH /settings` saves it and drops the override
  for the rest of the run. The app passes these start options only when
  given a `--dart-define`, so normal launches use the stored settings.
- **Changes** are validated (names follow the identity schema: 1–32
  characters, no reserved punctuation; download directories must be
  absolute and are created up front), saved atomically, then applied, and
  publish `settings.changed` if anything changed. An unreadable file is
  logged and ignored at start, then overwritten on the next change.
- **Renaming** takes effect at once: the LAN transport watches the name,
  re-encodes its identity for new connections, and announces it
  immediately. Peers update the name from any identity they receive, and
  KDE Connect re-dials on an announcement, so connected peers see the new
  name within a moment.
- **Download directory** is read when each incoming transfer starts, so a
  transfer in flight finishes where it began.

## 8. HTTP API (`/api/v1`)

Authentication is optional. When the daemon is started with a token
(`myconnect run --api-token`, `MYCONNECT_API_TOKEN`, or always when embedded
through the FFI), every request must carry `Authorization: Bearer <token>`
and gets a `401` otherwise. Without a token — the CLI default — any local
client may call the API. Tokens are never persisted; clients pass the same
`--api-token`/`MYCONNECT_API_TOKEN`. The server binds `127.0.0.1` by default;
CORS is disabled. Errors use `application/problem+json`. Requests must finish
within 15 seconds (`408 request_timeout`), except the file uploads (the
streaming routes) and the event stream.

| Method | Path | Notes |
| --- | --- | --- |
| `GET` | `/status` | Version, uptime, local device summary, protocol version. |
| `POST` | `/discovery` | Trigger an immediate identity announcement; `202`. An optional `{"address": "192.168.1.20"}` sends it to that unicast IPv4 address only; anything else is `400 invalid_address`. |
| `GET` | `/devices` | Snapshot of known devices. Each carries `plugins`, an object keyed by plugin id holding what that plugin adds to the device; a plugin with nothing to add has no key, so it is often `{}`. So far only `battery`: `{"charge": 0-100, "charging": bool}` from the peer's latest `kdeconnect.battery` report, present once a paired, connected peer has reported one and removed when it disconnects or is unpaired. A change publishes `device.updated`. Clients should ignore keys they don't know. |
| `GET` | `/devices/{deviceId}` | One device, or `404`. |
| `DELETE` | `/devices/{deviceId}` | Unpair, remove trust, forget the device. |
| `POST` | `/devices/{deviceId}/ping` | Send `kdeconnect.ping` to a paired, connected device that advertises receiving it; optional JSON body `{"message": "..."}`; `202`. |
| `POST` | `/devices/{deviceId}/ring` | Send `kdeconnect.findmyphone.request` (empty body) to a paired, connected device that advertises receiving it, which makes it ring until dismissed on the device; `202`. |
| `GET` | `/pairings` | Every pairing in this daemon session, including terminal ones, so a client can find requests still awaiting confirmation after (re)connecting. |
| `POST` | `/pairings` | Start outgoing pairing; `202`. |
| `GET` | `/pairings/{pairingId}` | Pairing state, verification code, expiry. |
| `POST` | `/pairings/{pairingId}/accept` | Confirm verification codes match (incoming only). |
| `DELETE` | `/pairings/{pairingId}` | Reject/cancel/unpair. |
| `POST` | `/devices/{deviceId}/share` | Send a file: streaming `multipart/form-data` with one `file` part, which must carry a `Content-Length` header; `202` with the transfer once the whole file has been forwarded, or as soon as the transfer ends if that comes first (cancelled or failed; the snapshot's `status` says which). Has its own, larger body-size limit than the rest of the API, and no overall deadline: it fails with `408 request_timeout` only if the upload stalls for longer than the request timeout. A streaming route that answers before reading the whole upload (a transfer that ended, an error) reads and discards the rest in the background, until the body ends, the client goes quiet for the request timeout, or the daemon shuts down, so a client still sending gets the answer rather than a reset connection. Clients that read the response while still sending (e.g. `reqwest`, `curl`) get it at once; ones that read it only after sending the whole body (Dart's `HttpClient`) get it once they have, so the app instead picks the transfer's id with `?transferId=<uuid>` and aborts its request once `/events` shows that transfer ended. Without `transferId` the daemon picks the id; a value that isn't a UUID is `400 invalid_transfer_id`, one already used `409 transfer_exists`. |
| `GET` | `/transfers` | Active and recent transfers. |
| `GET` | `/transfers/{transferId}` | State, byte counts, safe metadata. |
| `DELETE` | `/transfers/{transferId}` | Cancel an active transfer. |
| `GET` | `/devices/{deviceId}/files` | List a directory on a paired device (`?path=/absolute/path`), or without `path` the storage roots it shares, as `{path, entries: [{name, path, kind, size?, modifiedAt?}]}`. `kind` is `file`, `directory`, `symlink` or `other`; links are shown as what they point to. §12. |
| `GET` | `/devices/{deviceId}/files/content` | Stream a file's bytes (`?path=`), with `Content-Length` and a media type guessed from the extension. For previews; not a transfer. |
| `POST` | `/devices/{deviceId}/files/download` | `{"path": ...}`: save the file into the download directory as an incoming transfer; `202` with the transfer. |
| `POST` | `/devices/{deviceId}/files/upload` | Streaming `multipart/form-data`: a `path` field naming the directory on the device, then a `file` part with `Content-Length`. Runs as an outgoing transfer; a taken name gets a ` (n)` suffix. Same body limit, idle timeout, early answer when the transfer ends and optional `?transferId=` as `POST /devices/{id}/share`. |
| `POST` | `/devices/{deviceId}/files/directories` | `{"path": ...}`: create a directory; `201` with its entry. |
| `POST` | `/devices/{deviceId}/files/move` | `{"from": ..., "to": ...}`: move or rename; `409 file_exists` rather than replacing anything. |
| `DELETE` | `/devices/{deviceId}/files` | `?path=`: delete a file, or a directory and everything in it. Storage roots can't be moved or deleted (`400 invalid_path`). |
| `GET` | `/clipboard` | Current synchronized text and metadata. |
| `PUT` | `/clipboard` | Set text and send to eligible paired devices. |
| `POST` | `/devices/{deviceId}/clipboard` | Send this machine's clipboard text to one paired, connected device now; `202`. `409 clipboard_empty` when there is no text, `409 unsupported_by_peer` without `kdeconnect.clipboard`. §6. |
| `GET` | `/settings` | The settings in effect (§7): `deviceName`, `downloadDir`, `closeToTray`, and `plugins`, an object keyed by plugin id holding each plugin's section (so far `{"clipboard": {"syncEnabled": bool}}`). |
| `PATCH` | `/settings` | Change the fields present in the JSON body; `null` resets one to its default, unknown fields are rejected. A plugin's fields go under `plugins.<id>`, e.g. `{"plugins": {"clipboard": {"syncEnabled": false}}}`. `400 invalid_device_name` / `invalid_download_dir` / `invalid_settings` (a plugin section) for bad values. Returns the new settings. |
| `GET` | `/events` | Server-Sent Events: `device.discovered/connected/updated/disconnected/forgotten`, `pairing.requested/updated`, `transfer.started/progress/completed/failed`, `clipboard.changed`, `settings.changed`, `ping.received` (`{deviceId, deviceName, message?}` from a paired device; a one-off notification with no snapshot endpoint, so one missed during a gap is simply lost). Not durable — clients refetch a snapshot after a gap or reconnect. |

Mutation endpoints that require network round-trips return `202` and are
tracked through the resource's own state (poll the resource or watch
`/events`); events are notifications, not the source of truth.

The file endpoints fail with `409 files_unavailable` when the device won't
share its files (with the device's own reason in `detail` when it gave one),
`404 file_not_found`, `403 file_permission_denied`, `409 not_a_directory` /
`is_a_directory`, `400 invalid_path` (not absolute, or a `.`/`..` segment),
`502 files_host_key_mismatch`, `502 files_failed` or `504 files_timed_out`,
besides the device errors (`device_not_paired`, `device_not_connected`,
`unsupported_by_peer`).

## 9. Embedding (FFI)

`myconnect-ffi` exposes three C functions exchanging JSON strings:

- `myconnect_start(config)` — config mirrors `myconnect run`
  (`dataDir`, `downloadDir`, `deviceName`, `discoveryLoopback`,
  `systemClipboard`, `apiHost`, `apiPort`, `apiToken`). Defaults: loopback, port `0` (OS-chosen), and a
  freshly generated token. `deviceName` and `downloadDir` override the
  stored settings for that run (§7). Returns
  `{handle, apiHost, apiPort, apiToken}` once the LAN transport and API are
  listening.
- `myconnect_stop(handle)` — graceful shutdown via `RunningService::shutdown`,
  then the instance's Tokio runtime.
- `myconnect_free_string(ptr)` — frees any returned string.

Errors come back as `{"error": "..."}`; panics are caught at the boundary.
The embedder then uses only the HTTP API. The Linux Flutter build compiles
and bundles this library (see `ui/docs/adr/0006`). The app keeps running in
the tray with its window closed, so the embedded daemon stops only when the
user quits (see `ui/docs/adr/0007`).

## 10. Testing

Integration tests live in `tests/` and are organized by concern, not by
phase: `protocol.rs`, `tls.rs`, `lan.rs`, `pairing.rs` / `pairing_e2e.rs`,
`ping_e2e.rs`, `clipboard_e2e.rs`, `transfer_e2e.rs`, `browse_e2e.rs`,
`client.rs`, `api.rs`. `browse_e2e.rs` runs against a fake KDE Connect for
Android (`tests/support/fake_phone.rs`), which `examples/fake_phone.rs`
also runs standalone for trying the app without a phone.
The FFI crate has its own start/stop smoke test, and the Flutter app has
unit and widget tests under `ui/test/`.
Most end-to-end tests spin up two in-process peers (real UDP/TCP/TLS on
loopback, no mocked network layer) and exercise discovery through encrypted
plugin dispatch.

Standard verification before any change is considered done:

```sh
cargo fmt --all --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
git diff --check
(cd ui && flutter analyze && flutter test)
```

## 11. Known gaps

Prioritized next work, with implementation notes for each item, is in
[`HANDOFF.md`](HANDOFF.md).

- Interoperability was checked manually on 2026-09-24 against KDE Connect
  for Android (Pixel 8a, protocol v8) over a real LAN, using the CLI daemon.
  Working in both directions: discovery, TLS handshake, pairing with
  matching verification codes, unpairing, clipboard (the phone sends only
  when the user taps "Send clipboard", an Android 10+ restriction), and file
  transfer (3 MB, byte-identical). Ping to the phone works; ping from the
  phone was dropped at the time and is handled now (`ping.received`), but
  that direction has not been rechecked against the phone. Not yet checked against KDE Connect on
  desktop, and not from the Flutter app (the app embeds the same daemon).
  Two bugs found by the check are fixed: the CLI's upload omitted the file
  part's `Content-Length` header, and incoming pair requests were dropped
  when the clocks differed by more than 30 seconds.
- The desktop clipboard was checked live on X11 only (two daemons on
  separate Xvfb displays), not on a Wayland compositor. Compositors without
  data-control (e.g. GNOME) fall back to XWayland, which is untested.
- Devices added by IP address are not remembered: after a restart, a
  device only reachable that way has to be added again (or has to reach
  this one first). Adding by address has been checked between MyConnect
  instances only, not against KDE Connect.
- No Bluetooth transport, no multi-file/directory
  transfer, no durable event replay, no remote/LAN exposure of the control
  API — these are explicit non-goals for the current scope, not oversights.

## 12. Browsing a device's files

KDE Connect for Android shares its storage over SFTP; no other KDE Connect
client serves files. MyConnect is a client only: the UI's reasoning is in
[`ui/docs/adr/0008`](../ui/docs/adr/0008-browse-device-files-in-the-app.md).

- **Offer.** The first file request for a device sends
  `kdeconnect.sftp.request {"startBrowsing": true}` and waits up to 5
  seconds for `kdeconnect.sftp`. That reply carries `port`, `user`, a
  one-off `password` and the roots (`multiPaths` named by `pathNames`, else
  `path`). An `errorMessage` reply becomes `files_unavailable` with that
  message as `detail`. The `ip` field is ignored: the daemon connects to
  the address of the existing control connection, as KDE Connect does.
- **Connection** (`plugins::browse::ssh`, russh + russh-sftp, 8-second
  deadline). The peer's SSH host key must equal the public key in its
  pinned TLS certificate: Android uses its KDE Connect key pair as the host
  key. KDE Connect's own clients skip this check. A mismatch fails with
  `files_host_key_mismatch` before any credential is sent. The daemon signs
  in with its own TLS key, which Android accepts from the paired device,
  and falls back to the one-off password. The plugin never holds the key:
  the core signs in for it (`PayloadPeer::authenticate_ssh`).
- **Session.** One session per device, opened on demand, shared by
  concurrent requests (opening is serialized per device) and reused. It is
  dropped when the device disconnects, is unpaired or forgotten, when the
  peer sends `{"serverRunning": false}` (Android's plugin reloaded), when a
  request finds the SSH connection closed, at daemon shutdown, and after 5
  minutes unused. A download or upload in progress keeps it open.
- **Paths.** Every path is the peer's absolute path. The daemon rejects
  relative paths, NUL bytes and `.`/`..` segments, and strips repeated and
  trailing `/`. What a path can reach is up to the peer's server. `/` and
  the roots themselves can't be moved or deleted.
- **Copies.** Downloads are incoming transfers, saved exactly like received
  files (a `.part` file renamed into place, a ` (n)` suffix on
  collisions). Uploads are outgoing transfers into a file created with
  `EXCLUDE` under a free name. SFTP v3 reports "exists" only as a generic
  failure, so the daemon checks first. An upload that fails or is
  cancelled is removed from the peer. Moves and new folders also refuse to
  replace anything.
- **No events.** Nothing tells the daemon when files change on the device,
  so listings are fetched when needed; there is no `files.*` event.
- **Limits.** A recursive delete runs within the 15-second request
  deadline, so deleting a very large tree can stop partway.
- **Checked on Android.** A Pixel 8a (KDE Connect for Android, 2026-09)
  accepted our ECDSA key, its host key matched its certificate, and it
  offered one root, `/storage/emulated/0` ("Internal shared storage").
