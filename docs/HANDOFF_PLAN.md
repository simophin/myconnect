# MyConnect MVP implementation handoff

This document is the execution plan for agents continuing the project. Follow
the phases in order unless a phase explicitly says it can be parallelized. Keep
each phase independently reviewable and do not silently widen the MVP.

Protocol findings and upstream references are in
[`KDECONNECT_PROTOCOL_RESEARCH.md`](KDECONNECT_PROTOCOL_RESEARCH.md).

## 1. Target outcome

Build a long-running MyConnect daemon that:

- discovers KDE Connect devices on the LAN;
- establishes protocol-v8 TLS connections;
- pairs devices using a user-confirmed verification code;
- persists local identity and paired-device trust;
- synchronizes text clipboard content;
- sends and receives files with progress and cancellation; and
- exposes all control and observable state through a versioned local HTTP API.

The CLI and future GUI are API clients. They must not own KDE Connect sockets,
pairing state, trust state, or transfer state.

```text
CLI ───────┐
Future GUI ├── local HTTP API ── application core ── KDE Connect transport
Automation ┘
```

## 2. Current baseline

- Commit `1beabf1` contains the multi-binary CLI skeleton.
- `src/application.rs` is a temporary frontend-to-core seam and may be replaced
  incrementally; keep the binary thin.
- `myconnect run` and `myconnect send` currently only parse and log requests.
- No protocol, persistence, HTTP, discovery, or transfer implementation exists.
- Do not add `kdeconnect-proto` as a dependency without first resolving the
  blockers recorded in the protocol research.

Before every phase:

1. Read this document and the protocol research.
2. Inspect `git status --short`; preserve unrelated user changes.
3. Confirm the preceding phase's acceptance criteria are present in the code.
4. Make the smallest complete change for the current phase.
5. Run formatting, tests, Clippy, and `git diff --check` before handoff.

Standard verification commands:

```sh
cargo fmt --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
git diff --check
```

## 3. Architectural rules

### Ownership

- `protocol`: wire types and bounded codecs; no UI or HTTP knowledge.
- `config`: local identity, settings, and trust persistence.
- `transport`: UDP, TCP, TLS, and payload streams; no CLI knowledge.
- `device`: device registry, connection state, pairing state, and capabilities.
- `plugins`: behavior for ping, clipboard, and share packet families.
- `application`: commands, queries, events, and orchestration.
- `api`: HTTP transport only; translate HTTP types to application operations.
- `src/bin/myconnect`: CLI parsing, API client, and daemon bootstrap only.

### Dependency direction

```text
binary/API → application
application → config/device/plugins/transport
device/plugins/transport → protocol
```

Do not let protocol or transport modules depend on Axum, Clap, GUI crates, or
API response types.

### Asynchronous operations

- Pairing and transfers are resources with stable UUIDs and observable state.
- Mutation endpoints enqueue work and return `202 Accepted` when completion is
  asynchronous.
- Use cancellation tokens and bounded channels. Do not create unbounded queues
  for network input, API events, or transfer work.
- Events are notifications, not the source of truth. Clients reconnect and
  fetch a current snapshot after missing events.

## 4. HTTP API contract

Use `/api/v1` from the first implementation. JSON fields use `camelCase`.
Return a consistent error body using `application/problem+json`.

### Status and discovery

| Method | Path | Result |
| --- | --- | --- |
| `GET` | `/api/v1/status` | Version, uptime, local device summary, protocol version |
| `POST` | `/api/v1/discovery` | Trigger an immediate identity announcement; return `202` |

### Devices

| Method | Path | Result |
| --- | --- | --- |
| `GET` | `/api/v1/devices` | Snapshot of known devices |
| `GET` | `/api/v1/devices/{deviceId}` | One device or `404` |
| `DELETE` | `/api/v1/devices/{deviceId}` | Unpair, remove trust, and forget the device |

Do not add `POST /devices` for ordinary discovery. KDE Connect discovers peers.
A later manual-address feature should be a separate connection resource with
explicit host and port semantics.

Every device response must distinguish:

- `discovered`: known from discovery but without a live connection;
- `connected`: live TLS link;
- `paired`: certificate is trusted;
- `pairing`: pairing is in progress; and
- `unavailable`: previously known but currently unreachable.

Include incoming/outgoing capabilities and never infer feature availability
from device type.

### Pairing

| Method | Path | Result |
| --- | --- | --- |
| `POST` | `/api/v1/pairings` | Start outgoing pairing for `deviceId`; return `202` |
| `GET` | `/api/v1/pairings/{pairingId}` | Pairing state, verification code, and expiry |
| `POST` | `/api/v1/pairings/{pairingId}/accept` | Confirm that verification codes match |
| `DELETE` | `/api/v1/pairings/{pairingId}` | Reject or cancel pairing |

Use the same pairing resource for incoming and outgoing requests. Required
states:

```text
requested
awaiting_confirmation
accepted
rejected
expired
failed
```

A pairing representation includes `id`, `deviceId`, `deviceName`, `direction`,
`status`, `verificationCode`, `createdAt`, `expiresAt`, and an optional safe
error code. Never include certificates or private keys.

### Transfers

| Method | Path | Result |
| --- | --- | --- |
| `POST` | `/api/v1/transfers` | Stream one file to a paired device; return `202` |
| `GET` | `/api/v1/transfers` | Active and recent incoming/outgoing transfers |
| `GET` | `/api/v1/transfers/{transferId}` | State, byte counts, and safe metadata |
| `DELETE` | `/api/v1/transfers/{transferId}` | Cancel an active transfer |

Use streaming `multipart/form-data` with `deviceId` and one `file` part for the
initial API. Never buffer the complete file in memory. Configure an explicit
request-size limit; Axum's default multipart limit is only 2 MB.

Required transfer states:

```text
queued
connecting
transferring
completed
cancelled
failed
```

Incoming transfers are recorded through the same resource model. Write them to
a temporary `.part` file, validate the declared size, then atomically move them
to the destination.

### Clipboard

| Method | Path | Result |
| --- | --- | --- |
| `GET` | `/api/v1/clipboard` | Current synchronized text and metadata |
| `PUT` | `/api/v1/clipboard` | Set text and send it to eligible paired devices |

Only text clipboard synchronization is in the MVP. Do not add images, rich
text, or clipboard history yet.

### Event stream

`GET /api/v1/events` returns Server-Sent Events with a keepalive. Each event has
a monotonic process-local sequence, timestamp, type, and JSON data.

Initial event types:

```text
device.discovered
device.connected
device.updated
device.disconnected
pairing.requested
pairing.updated
transfer.started
transfer.progress
transfer.completed
transfer.failed
clipboard.changed
```

The MVP does not promise durable replay. On an event gap or daemon restart, a
client must refetch device, pairing, transfer, and clipboard snapshots.

## 5. Step-by-step implementation

Each phase should normally be one commit. If a phase becomes large, split only
at the listed deliverable boundaries and keep all intermediate commits green.

### Phase 1: Wire packet foundation

**Status: complete (2026-09-22).**

Deliverables:

1. Add `serde`, `serde_json`, and `thiserror`.
2. Create `src/protocol/` with:
   - packet envelope fields `id`, `type`, `body`, `payloadSize`, and
     `payloadTransferInfo`;
   - identity and pairing body types;
   - unknown packet/body-field tolerance; and
   - a bounded newline-delimited JSON codec.
3. Enforce identity validation rules from the protocol reference.
4. Add hand-authored fixtures matching upstream JSON examples.

Acceptance criteria:

- Known and unknown packet types decode without losing the envelope.
- Optional payload fields serialize with their exact camel-case wire names.
- Fragmented reads and multiple packets in one buffer decode correctly.
- Oversized lines fail with a typed error and bounded memory use.
- Identity, pairing request, and payload-envelope round trips have tests.

Do not open sockets in this phase.

### Phase 2: Persistent identity and trust

**Status: complete (2026-09-22).**

Deliverables:

1. Add `directories`, `uuid`, `rcgen`, `sha2`, and only the certificate parsing
   support actually needed.
2. Generate a UUIDv4 device ID with hyphens removed; validate 32 characters.
3. Generate a persistent self-signed certificate whose Common Name is the
   device ID and store its private key securely.
4. Define a `TrustStore` interface and filesystem implementation containing
   peer device ID, pinned certificate, and last trusted protocol version.
5. Implement the protocol-v8 verification-code function independently of the
   pairing state machine.

Acceptance criteria:

- First load creates identity material; subsequent loads return identical
  identity and key material.
- Partial writes cannot leave a valid-looking corrupt identity.
- Private material has restrictive permissions where the platform supports it.
- Certificate Common Name and device ID must match.
- Fixed-vector tests prove public-key ordering, timestamp encoding, uppercase
  hex formatting, and eight-character output.
- No private key or full certificate appears in logs or errors.

### Phase 3: Application state and event model

Deliverables:

1. Define immutable API-facing snapshots for status, device, pairing, transfer,
   and clipboard state.
2. Implement a device registry keyed by device ID.
3. Define application commands/queries without HTTP-specific types.
4. Add a bounded broadcast event bus and monotonic event sequence.
5. Define pairing and transfer state-transition validation.

Acceptance criteria:

- Invalid state transitions return typed errors.
- Slow event subscribers cannot cause unbounded memory growth or block core
  protocol tasks.
- Snapshot tests cover JSON names without coupling core types to Axum.
- Disconnecting a device preserves trust but updates reachability.

### Phase 4: Local HTTP control plane

Deliverables:

1. Add Axum and minimal Tower middleware.
2. Implement `/api/v1/status`, device snapshot endpoints, discovery command,
   and `/api/v1/events` SSE over the application interfaces.
3. Bind to `127.0.0.1` only by default on a configurable port outside
   `1716-1764`.
4. Generate a persistent random bearer token and require it on every `/api/v1`
   endpoint.
5. Disable CORS by default; add request timeouts, body limits, request IDs, and
   redacted structured tracing.
6. Add graceful shutdown driven by a cancellation token.

Acceptance criteria:

- Unauthenticated requests receive `401` without leaking endpoint state.
- The server never binds a wildcard interface by default.
- SSE clients receive typed events and keepalives and are cleaned up on
  disconnect.
- API integration tests run on an ephemeral loopback port.
- Shutdown stops accepting requests and waits for owned tasks within a bounded
  deadline.

Do not expose the API on the LAN in the MVP.

### Phase 5: CLI as API client

Deliverables:

1. Keep `myconnect run` as the foreground daemon command.
2. Replace direct application execution for other commands with an HTTP client.
3. Add commands:

```text
myconnect devices [--watch]
myconnect pair <device-id>
myconnect pair accept <pairing-id>
myconnect pair reject <pairing-id>
myconnect unpair <device-id>
myconnect send <device-id> <file> [--watch]
myconnect clipboard get
myconnect clipboard set <text>
myconnect clipboard watch
```

4. Resolve API address and token from the same configuration location as the
   daemon, with environment overrides for development.
5. Produce stable human-readable output; reserve JSON output as an explicit
   `--json` option.

Acceptance criteria:

- CLI parser and mocked-server integration tests cover every command.
- Missing daemon, unauthorized, missing device, and failed operation messages
  are distinct and actionable.
- `--watch` uses SSE and handles reconnect by fetching a fresh snapshot.
- File bodies are streamed rather than read completely into memory.

### Phase 6: LAN discovery and connection lifecycle

Deliverables:

1. Add Tokio UDP broadcast/listen on port 1716 and TCP listen selection in
   `1716-1764` using `socket2` where required.
2. Broadcast a valid protocol-v8 identity containing the chosen `tcpPort`.
3. Parse discovery datagrams independently with strict size and validation
   limits.
4. Establish TCP connections with connect, read, and handshake timeouts.
5. Update the device registry and emit lifecycle events.
6. Handle duplicate discovery, self-discovery, simultaneous connections,
   network changes, and graceful cancellation.

Acceptance criteria:

- Two in-process peers discover one another in integration tests.
- Malformed, oversized, self, and unsupported-version identities are ignored
  without crashing or accumulating buffers.
- Repeated discovery does not create duplicate device entries or connection
  storms.
- The daemon can start and stop repeatedly without leaked tasks or sockets.

### Phase 7: Protocol-v8 TLS and pairing

Deliverables:

1. Add `rustls` and `tokio-rustls` with actual TLS 1.2/1.3 handshake-signature
   verification.
2. Exchange identity before TLS, then exchange it again inside TLS.
3. Reject device ID or protocol-version changes during the handshake.
4. For trusted devices, require the pinned certificate and reject downgrades.
5. Implement incoming/outgoing pairing sessions, timestamps, the 30-second
   timeout, verification-code display, accept, reject, cancel, and unpair.
6. Wire the pairing HTTP endpoints and events.
7. Persist trust only after local user confirmation succeeds.

Acceptance criteria:

- A valid peer can pair, reconnect using pinned trust, and unpair.
- Wrong verification flow, expired request, clock skew, changed certificate,
  invalid TLS signature, identity swap, and protocol downgrade are rejected.
- Unpaired devices cannot send plugin packets other than pairing.
- Pairing sessions always reach a terminal state and release timers/tasks.
- Interoperability is manually verified against current KDE Connect Android or
  desktop and the result is recorded.

Never implement a rustls custom verifier that returns success without verifying
the handshake signature.

### Phase 8: Ping vertical slice

Deliverables:

1. Implement plugin capability registration and packet routing only to the
   extent needed by the MVP.
2. Add the ping packet model and handler.
3. Advertise capabilities from registered handlers.
4. Add a temporary API or internal integration harness for sending a ping; do
   not expand the public MVP API unless the use case is retained.

Acceptance criteria:

- Only paired devices can exchange pings.
- Capability filtering prevents unsupported outgoing packets.
- End-to-end tests cover discovery through encrypted plugin dispatch.
- Manual KDE Connect interoperability succeeds before clipboard/file work.

### Phase 9: Text clipboard

Deliverables:

1. Implement `kdeconnect.clipboard` and `kdeconnect.clipboard.connect` packet
   handling with timestamp rules.
2. Add a platform-neutral clipboard service trait.
3. Wire `GET/PUT /api/v1/clipboard` and clipboard events.
4. Add a feedback-loop guard so remotely applied clipboard content is not
   endlessly rebroadcast.
5. Add an enable/disable setting and a conservative text-size limit.

Acceptance criteria:

- Local-to-remote and remote-to-local text synchronization works.
- Stale timestamps and duplicate content are ignored.
- Clipboard contents are never logged.
- Tests use an in-memory clipboard implementation and do not require a desktop
  session.

### Phase 10: File transfer

Deliverables:

1. Implement `kdeconnect.share.request` and
   `kdeconnect.share.request.update` packet models.
2. Implement auxiliary TLS payload listeners/connectors on allowed payload
   ports, separate from the control stream.
3. Wire transfer creation, status, list, cancel, and SSE progress events.
4. Stream API uploads to the remote device with bounded buffers.
5. Receive into a temporary file, enforce declared size and configured maximum,
   sanitize the filename, prevent traversal, and atomically finalize.
6. Clean up partial files and tasks after cancellation, disconnect, failure, or
   daemon shutdown.

Acceptance criteria:

- Zero-byte, small, and larger-than-memory files transfer without buffering the
  complete file.
- Incorrect size, oversized payload, path traversal filename, port failure,
  disconnect, cancellation, and disk-write failure are tested.
- Progress is monotonic and completion is emitted only after durable local
  finalization or confirmed outgoing completion.
- Sending and receiving interoperate with current KDE Connect.

## 6. MVP completion gate

The MVP is complete only when all of the following are true:

- A clean installation generates and reuses one valid local identity.
- The daemon is controllable only through an authenticated local API.
- CLI commands use that API rather than reaching into core state.
- Multiple devices can be discovered and represented independently.
- Pairing requires visible protocol-v8 verification and persists pinned trust.
- Trusted devices reconnect; certificate changes and downgrades fail closed.
- Text clipboard synchronization works without feedback loops.
- Files can be sent, received, monitored, cancelled, and safely finalized.
- Restart, shutdown, malformed packets, slow clients, and unavailable peers do
  not panic or leak unbounded resources.
- Automated tests pass and at least one current KDE Connect implementation has
  been exercised for discovery, pairing, clipboard, and file interoperability.

## 7. Explicit non-goals

Do not add these before the MVP completion gate:

- Bluetooth transport;
- remote/LAN exposure of the control API;
- browser CORS support;
- image or rich-text clipboard data;
- multi-file batches or directory transfer;
- notifications, SMS, MPRIS, SFTP, remote input, or command execution;
- durable event replay;
- accounts, cloud relay, or internet traversal; or
- a generic plugin marketplace or dynamic plugin ABI.

## 8. Handoff format after each phase

An agent completing a phase should report:

1. The phase and acceptance criteria completed.
2. Files and public interfaces added or changed.
3. Tests and manual interoperability checks run.
4. Any protocol ambiguity or deliberate deviation from upstream.
5. Security-sensitive decisions, especially trust, TLS, file paths, and limits.
6. Remaining work and the exact next phase.
7. The commit hash, if the user requested a commit.

If blocked by protocol behavior, capture the packet fixture or upstream source
reference without logging clipboard contents, transferred file contents,
private keys, bearer tokens, or other secrets.
