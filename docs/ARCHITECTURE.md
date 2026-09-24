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
Flutter UI (ui/)       ├── local HTTP API (/api/v1) ── application core ── KDE Connect transport
Other automation      ─┘
```

The CLI and the Flutter UI talk to the daemon exclusively through the local
HTTP API. Neither owns sockets, pairing state, trust state, or transfer
state — that all lives inside the daemon process, behind
`ApplicationHandle`. The UI additionally uses the FFI library, but only to
start and stop an embedded daemon (§8); the UI's own design decisions are
recorded in [`ui/docs/adr/`](../ui/docs/adr/README.md).

## 2. Module map and dependency direction

```text
binary (src/bin/myconnect) → client → api
ffi (myconnect-ffi) → application::RunningService
api → application
application → config, device, plugins, transport, clipboard
device, plugins, transport → protocol
```

`protocol` and `transport` never depend on Axum, Clap, or API response
types — they know nothing about HTTP. `plugins` depends only on `protocol`,
so packet handling can be exercised without a live connection.

| Module | File(s) | Responsibility |
| --- | --- | --- |
| `protocol` | `src/protocol/{mod,packet,codec,verification}.rs` | Wire packet envelope, identity/pairing body types, bounded newline-delimited JSON codec, the protocol-v8 verification-code function. No I/O. |
| `config` | `src/config/{mod,identity,token,trust}.rs` | Local device identity (UUID + self-signed cert), the optional API bearer token (never persisted), filesystem-backed `TrustStore` of pinned peer certificates. |
| `transport` | `src/transport/{lan,tls,payload}.rs` | UDP discovery, TCP control-channel connect/accept, the real rustls TLS handshake and certificate pinning, and the auxiliary TLS payload connection used for file transfer. |
| `device` | `src/device.rs` | `DeviceSnapshot`, `DeviceReachability`, and the in-memory device registry keyed by device ID. |
| `plugins` | `src/plugins/{mod,ping,clipboard,share}.rs` | Fixed (non-dynamic) packet-type routing table for the packet families this build understands: ping, clipboard, share. Advertises capability strings for the identity packet; ping is advertised as outgoing only, since incoming pings are not handled yet. |
| `application` | `src/application.rs`, `src/application/{state,events,service,transfer}.rs` | Orchestration: connection registry, pairing state machine, transfer state machine, clipboard sync, bounded event bus. Everything HTTP-facing is a snapshot type defined here. `RunningService` starts/stops a whole daemon (LAN + API) for the CLI and embedders. |
| `clipboard` | `src/clipboard.rs` | `ClipboardService` trait plus an in-memory implementation (no OS clipboard integration yet). |
| `api` | `src/api.rs` | Axum HTTP transport only — translates HTTP requests to `ApplicationService` calls and snapshots back to JSON. Optional bearer-token auth, body-size limits, SSE. |
| `client` | `src/client.rs` | Typed HTTP client used by the CLI (and any future frontend) to talk to `api`. |
| `src/bin/myconnect` | `cli.rs`, `main.rs` | Argument parsing and daemon bootstrap only. |
| `myconnect-ffi` | `ffi/src/lib.rs` | `cdylib` exporting `myconnect_start` / `myconnect_stop` / `myconnect_free_string` (JSON in, JSON out) so a GUI process can embed a daemon. See §8. |

Adding a new packet family means adding a match arm in `plugins::mod::dispatch_incoming`
and an entry in `plugins::capabilities()` — not registering a trait object at
runtime. This is deliberate: the MVP has a small, fixed plugin set, not a
plugin marketplace.

## 3. Connection lifecycle

1. **Discovery** (`transport::lan`): UDP broadcast/listen on port 1716.
   Each peer broadcasts a protocol-v8 identity packet containing its chosen
   TCP port (selected from `1716-1764`). Malformed, oversized, self, and
   unsupported-version identities are dropped without affecting the device
   registry.
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
   (`application::service::ApplicationHandle`) dispatches incoming packets
   by type — pairing packets always; ping/clipboard/share packets only for
   already-paired devices.

## 4. Pairing state machine

States: `requested → awaiting_confirmation → accepted | rejected | expired | failed`.

- The same pairing resource represents both incoming and outgoing requests,
  distinguished by `direction`.
- A pairing session always reaches a terminal state; the associated 30-second
  timeout timer is aborted on every terminal transition so no task leaks.
- Trust is written to the `TrustStore` only after local user confirmation
  (`POST /pairings/{id}/accept` for incoming, or automatic on receiving the
  peer's accept for outgoing) — never before.
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

- Transfers use `kdeconnect.share.request` / `kdeconnect.share.request.update`
  on the control channel to negotiate, then stream bytes over a **separate**
  auxiliary TLS payload connection (`transport::payload`), reusing the same
  TLS material and pinning logic as the control channel.
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
  Cancellation, disconnect, and daemon shutdown all clean up the partial
  `.part` file and abort the associated task.
- Only paired devices can initiate or receive transfers.

## 6. Clipboard sync

- `kdeconnect.clipboard` carries `content` and applies unconditionally
  (subject to the duplicate-content guard below).
- `kdeconnect.clipboard.connect` additionally carries a millisecond
  `timestamp`; it is applied only if strictly newer than the last known
  update, so stale or replayed packets are ignored.
- A feedback-loop guard tracks the last-applied content/source so content
  just received from a peer is never rebroadcast back to that peer, and
  identical content is never resent.
- Text is capped at `MAX_CLIPBOARD_TEXT_BYTES` (32 KiB); oversized `PUT`
  requests get a typed `413` rather than silent truncation.
- Clipboard contents are never logged — only lengths.
- The only implementation today is `InMemoryClipboard`; there is no OS
  clipboard integration yet.

## 7. HTTP API (`/api/v1`)

Authentication is optional. When the daemon is started with a token
(`myconnect run --api-token`, `MYCONNECT_API_TOKEN`, or always when embedded
through the FFI), every request must carry `Authorization: Bearer <token>`
and gets a `401` otherwise. Without a token — the CLI default — any local
client may call the API. Tokens are never persisted; clients pass the same
`--api-token`/`MYCONNECT_API_TOKEN`. The server binds `127.0.0.1` by default;
CORS is disabled. Errors use `application/problem+json`.

| Method | Path | Notes |
| --- | --- | --- |
| `GET` | `/status` | Version, uptime, local device summary, protocol version. |
| `POST` | `/discovery` | Trigger an immediate identity announcement; `202`. |
| `GET` | `/devices` | Snapshot of known devices. |
| `GET` | `/devices/{deviceId}` | One device, or `404`. |
| `DELETE` | `/devices/{deviceId}` | Unpair, remove trust, forget the device. |
| `POST` | `/devices/{deviceId}/ping` | Send `kdeconnect.ping` to a paired, connected device that advertises receiving it; optional JSON body `{"message": "..."}`; `202`. |
| `GET` | `/pairings` | Every pairing in this daemon session, including terminal ones, so a client can find requests still awaiting confirmation after (re)connecting. |
| `POST` | `/pairings` | Start outgoing pairing; `202`. |
| `GET` | `/pairings/{pairingId}` | Pairing state, verification code, expiry. |
| `POST` | `/pairings/{pairingId}/accept` | Confirm verification codes match (incoming only). |
| `DELETE` | `/pairings/{pairingId}` | Reject/cancel/unpair. |
| `POST` | `/transfers` | Streaming `multipart/form-data` (`deviceId` + `file`); `202`. Has its own, larger body-size limit than the rest of the API. |
| `GET` | `/transfers` | Active and recent transfers. |
| `GET` | `/transfers/{transferId}` | State, byte counts, safe metadata. |
| `DELETE` | `/transfers/{transferId}` | Cancel an active transfer. |
| `GET` | `/clipboard` | Current synchronized text and metadata. |
| `PUT` | `/clipboard` | Set text and send to eligible paired devices. |
| `GET` | `/events` | Server-Sent Events: `device.discovered/connected/updated/disconnected/forgotten`, `pairing.requested/updated`, `transfer.started/progress/completed/failed`, `clipboard.changed`. Not durable — clients refetch a snapshot after a gap or reconnect. |

Mutation endpoints that require network round-trips return `202` and are
tracked through the resource's own state (poll the resource or watch
`/events`); events are notifications, not the source of truth.

## 8. Embedding (FFI)

`myconnect-ffi` exposes three C functions exchanging JSON strings:

- `myconnect_start(config)` — config mirrors `myconnect run`
  (`dataDir`, `downloadDir`, `deviceName`, `discoveryLoopback`, `apiHost`,
  `apiPort`, `apiToken`). Defaults: loopback, port `0` (OS-chosen), and a
  freshly generated token. Returns `{handle, apiHost, apiPort, apiToken}`
  once the LAN transport and API are listening.
- `myconnect_stop(handle)` — graceful shutdown via `RunningService::shutdown`,
  then the instance's Tokio runtime.
- `myconnect_free_string(ptr)` — frees any returned string.

Errors come back as `{"error": "..."}`; panics are caught at the boundary.
The embedder then uses only the HTTP API. The Linux Flutter build compiles
and bundles this library (see `ui/docs/adr/0006`). The app keeps running in
the tray with its window closed, so the embedded daemon stops only when the
user quits (see `ui/docs/adr/0007`).

## 9. Testing

Integration tests live in `tests/` and are organized by concern, not by
phase: `protocol.rs`, `tls.rs`, `lan.rs`, `pairing.rs` / `pairing_e2e.rs`,
`ping_e2e.rs`, `clipboard_e2e.rs`, `transfer_e2e.rs`, `client.rs`, `api.rs`.
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

## 10. Known gaps

Prioritized next work, with implementation notes for each item, is in
[`HANDOFF.md`](HANDOFF.md).

- Interoperability was checked manually on 2026-09-24 against KDE Connect
  for Android (Pixel 8a, protocol v8) over a real LAN, using the CLI daemon.
  Working in both directions: discovery, TLS handshake, pairing with
  matching verification codes, unpairing, clipboard (the phone sends only
  when the user taps "Send clipboard", an Android 10+ restriction), and file
  transfer (3 MB, byte-identical). Ping to the phone works; ping from the
  phone is dropped (next item). Not yet checked against KDE Connect on
  desktop, and not from the Flutter app (the app embeds the same daemon).
  Two bugs found by the check are fixed: the CLI's upload omitted the file
  part's `Content-Length` header, and incoming pair requests were dropped
  when the clocks differed by more than 30 seconds.
- Ping is outgoing only: incoming `kdeconnect.ping` packets are dropped and
  not advertised in `incomingCapabilities`.
- No OS clipboard backend, no Bluetooth transport, no multi-file/directory
  transfer, no durable event replay, no remote/LAN exposure of the control
  API — these are explicit non-goals for the current scope, not oversights.
