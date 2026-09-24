# KDE Connect protocol research

Research date: 2026-09-22

## Recommendation

Implement a small, protocol-focused MyConnect core on top of established Rust
networking and cryptography crates. Do not depend on a complete third-party KDE
Connect implementation yet.

The closest reusable library is `kdeconnect-proto`, but version 0.2.1 needs more
interoperability and security work before it is suitable for MyConnect. It is a
valuable reference and possible future dependency if the issues below are fixed
upstream.

Start with LAN transport and protocol version 8. Defer Bluetooth transport until
the LAN path, pairing, clipboard, and file transfer are stable.

## Protocol shape

KDE Connect separates the system into four concepts:

1. Link providers discover peers and establish transports.
2. Devices hold peer identity, reachability, trust, and capabilities.
3. Network packets are newline-delimited JSON messages.
4. Plugins handle independent packet families such as pairing, clipboard, and
   sharing.

The current protocol reference describes versions 7 and 8; new identities must
advertise version 8. A packet has `id`, `type`, and `body`, with optional
`payloadSize` and `payloadTransferInfo` for a separate binary payload stream.
Packets are not request/response messages and handlers should be idempotent.

### LAN connection sequence

The current desktop implementation uses this sequence:

1. Listen for and broadcast identity packets over UDP port 1716. Discovery may
   also use mDNS.
2. Select a TCP listener in the 1716-1764 range and advertise it as `tcpPort`.
3. The device that received the UDP packet dials `tcpPort` and sends its own
   identity on the plain TCP connection (with `targetDeviceId` and
   `targetProtocolVersion`). The accepting device only reads it; this step is
   one-way, not an exchange.
4. Upgrade the connection to mutually authenticated TLS with self-signed device
   certificates. TLS roles are inverted: the TCP dialer is the TLS server and
   the TCP acceptor is the TLS client.
5. Exchange identity packets again inside TLS for protocol v8 and reject a
   changed device ID or protocol version.
6. Exchange newline-delimited JSON packets on the persistent TLS stream.
7. Pin the peer certificate only after explicit pairing approval.

File payloads use separate TLS/TCP connections advertised in
`payloadTransferInfo`; the control packet includes the exact `payloadSize`.

### Pairing requirements

Pair requests contain `pair: true` and a Unix timestamp in seconds. They time
out after 30 seconds. Protocol v8 presents the same eight-character verification
key on both devices:

```text
uppercase(hex(SHA-256(max(pubkey_a, pubkey_b)
                     || min(pubkey_a, pubkey_b)
                     || decimal(pair_request_timestamp))))[0..8]
```

The public keys are DER encoded and compared bytewise before hashing. Trust is
established only after the user verifies this value and approves pairing. A
trusted device must be rejected if it later presents a different certificate or
attempts a protocol downgrade.

## Rust ecosystem assessment

### `kdeconnect-proto` 0.2.1

- MIT licensed, edition 2024, and actively published.
- Provides typed packet bodies, Tokio UDP/TCP/mDNS discovery, rustls transport,
  plugin dispatch, and a pluggable trust store.
- Advertises protocol version 8 and performs the encrypted identity exchange.
- Its generic I/O layer is attractive if embedded support becomes a goal.

Blocking findings in the published 0.2.1 source:

- `NetworkPacket` names the payload length field `range` without a Serde rename
  to `payloadSize`, so file payload metadata is not wire-compatible.
- It models payload metadata but does not implement the auxiliary payload
  transfer connections needed by the MVP.
- Pairing validates timestamp skew, but does not calculate or expose the
  protocol-v8 verification key.
- The pairing handler contains a TODO instead of enforcing the 30-second
  timeout.
- Its custom rustls client and server verifiers return success from TLS 1.2 and
  TLS 1.3 handshake-signature callbacks without verifying the signatures. This
  defeats proof-of-possession and must be fixed before use.
- The published crate contains no Rust tests.

Decision: do not add it as a dependency now. Re-evaluate after fixes land, or
contribute the fixes upstream and test interoperability before adoption.

### `cosmic-utils/kdeconnect`

- An active, working Rust/COSMIC application with pairing, clipboard, file
  sharing, notifications, and many other plugins.
- Its `kdeconnect-core` crate is a useful architectural and interoperability
  reference and currently advertises protocol version 8.
- It is an application workspace rather than a small reusable protocol crate,
  has COSMIC/Linux integrations in its core dependency graph, and is GPL-3.0.

Decision: use it as an implementation reference and test peer, not as a direct
dependency or source-copy target.

### Other results

- `kdeconnect-cli` is a frontend over `kdeconnect-proto`, so it inherits the
  same limitations and is not a reusable alternative.
- `kdeconnect-embassy` supplies an embedded I/O backend for
  `kdeconnect-proto`; embedded targets are outside the current desktop MVP.
- Older Rust implementations are incomplete or platform-specific and do not
  improve on the two candidates above.

## Proposed Rust building blocks

Use focused crates whose behavior we own at the protocol boundary:

- `serde` and `serde_json`: wire packet types and tolerant decoding.
- `tokio`: tasks, UDP/TCP sockets, timers, cancellation, and async file I/O.
- `socket2`: cross-platform socket options needed for broadcast, address reuse,
  keepalive, and dual-stack behavior.
- `rustls` and `tokio-rustls`: TLS with explicit certificate pinning and real
  handshake-signature verification.
- `rcgen`: persistent self-signed device certificate generation.
- `sha2`: protocol-v8 pairing verification key.
- `uuid`: a new 32-character device ID generated from UUIDv4 without hyphens.
- `directories`: platform-specific configuration and download locations.
- `thiserror`: typed errors at protocol and transport boundaries; retain
  `anyhow` at binary/application boundaries.

Keep packet models permissive where KDE Connect is permissive: unknown packet
types and unknown body fields should survive parsing. Enforce strict bounds on
identity size, line size, payload size, pending unpaired devices, and timeouts.

## Proposed module boundaries

```text
src/
├── application.rs
├── config/                 persistent identity and trust store
├── protocol/               packet envelope and plugin packet models
├── transport/
│   ├── lan.rs              discovery and connection lifecycle
│   ├── tls.rs              certificate verification and pinning
│   └── payload.rs          bounded auxiliary transfers
├── device/                 peer state machine and event API
└── plugins/
    ├── clipboard.rs
    └── share.rs
```

The core should expose commands and events rather than UI callbacks. That keeps
the existing CLI thin and lets a future GUI observe device, pairing, transfer,
and clipboard state through the same API.

## Recommended implementation order

1. Packet envelope and identity types, newline codec, validation, and upstream
   JSON fixtures.
2. Persistent local identity: device ID, certificate, private key, and trust
   records.
3. UDP discovery plus TCP listener with bounded parsing and cancellation.
4. Protocol-v8 TLS identity exchange and certificate pinning.
5. Pairing state machine, 30-second timeout, and displayed verification key.
6. Ping as the first end-to-end plugin and interoperability test.
7. Clipboard plugin.
8. Share/file payload transport with size checks, temporary files, atomic
   destination moves, progress, and cancellation.

The next implementation slice should be steps 1 and 2. It can be fully tested
without opening sockets and establishes the formats and identity invariants that
all later transport code depends on.

## Primary references

- [KDE Connect generated protocol reference](https://github.com/KDE/kdeconnect-meta/blob/work/protocol-schemas/protocol.md)
- [KDE Connect desktop core architecture](https://github.com/KDE/kdeconnect-kde#how-does-it-work)
- [KDE Connect LAN transport](https://github.com/KDE/kdeconnect-kde/blob/master/core/backends/lan/lanlinkprovider.cpp)
- [KDE Connect pairing handler](https://github.com/KDE/kdeconnect-kde/blob/master/core/backends/pairinghandler.cpp)
- [`kdeconnect-proto` documentation](https://docs.rs/kdeconnect-proto/latest/kdeconnect_proto/)
- [`kdeconnect-proto` repository](https://codeberg.org/nifou/kdeconnect-rs)
- [COSMIC Rust implementation](https://github.com/cosmic-utils/kdeconnect)
- [rustls custom verifier contract](https://docs.rs/rustls/latest/rustls/client/danger/trait.ServerCertVerifier.html)
