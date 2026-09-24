## MyConnect - a Rust based KDE Connect alternative

MyConnect is an open-source project written in Rust that aims to provide similar functionality to KDE Connect, allowing seamless integration and communication between your devices.

### Long term goal

Provides a desktop application for MacOS/Linux/Windows.

### Status

The MVP is implemented: LAN discovery, protocol-v8 TLS connections with
certificate pinning, user-confirmed pairing, persistent identity and trust,
text clipboard synchronization, file transfer with progress and
cancellation, and a versioned authenticated local HTTP API that the CLI (and
any future GUI) uses exclusively — no frontend touches KDE Connect sockets or
state directly.

Manual interoperability testing against a real KDE Connect (Android/desktop)
installation has not yet been performed; see
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md#9-known-gaps) for the current
list of gaps.

### CLI interface

The CLI interface is command based, allowing users to interact with MyConnect through terminal commands.

#### Example Commands

- `myconnect run [--data-dir <dir>] [--download-dir <dir>] [--device-name <name>] [--discovery-loopback]` -
  Run the authenticated local daemon in the foreground. `--discovery-loopback`
  restricts LAN discovery to loopback broadcast instead of the real network —
  useful for running multiple local instances against each other for testing
  (a physical switch never reflects a broadcast frame back to the port it
  came from, so two instances on one machine otherwise can't discover each
  other over a real NIC), at the cost of not discovering real devices.
- `myconnect devices [--watch]` - List devices and optionally follow changes.
- `myconnect scan [--timeout <seconds>] [--watch]` - Broadcast a discovery
  request and list unpaired devices that answer.
- `myconnect pair <device-id>` - Start pairing with a discovered device.
- `myconnect pair accept|reject <pairing-id>` - Resolve a pairing request.
- `myconnect unpair <device-id>` - Remove trust and forget a device.
- `myconnect send <device-id> <file> [--watch]` - Stream a file to a device.
- `myconnect clipboard get|set <text>|watch` - Control text synchronization.

Add `--json` for machine-readable output. `--api-host`/`--api-port` (global
flags, default `127.0.0.1:24816`) set the address the control API listens on
for `run` and the address every other command connects to. API clients use
the same persistent token as the daemon. For development, `MYCONNECT_API_URL`
and `MYCONNECT_API_TOKEN` override the API URL and stored token (superseded
by `--api-host`/`--api-port` when either is given).

### Project structure

MyConnect is a single Cargo package with a shared library and a thin binary
entry point:

```text
src/
├── lib.rs           # shared library
├── protocol/        # wire packet models and bounded framing
├── config/          # persistent identity, API token, peer trust
├── transport/        # UDP discovery, TCP/TLS, auxiliary payload connections
├── device.rs         # device registry and snapshots
├── plugins/           # fixed packet routing: ping, clipboard, share
├── application(.rs/*) # orchestration: pairing/transfer state machines, event bus
├── clipboard.rs        # clipboard service trait + in-memory implementation
├── api.rs               # authenticated local HTTP control plane
├── client.rs             # HTTP client used by the CLI
└── bin/
    └── myconnect/        # CLI binary
        ├── cli.rs
        └── main.rs
```

Keeping behavior in the library lets other frontends use the same application
API. A GUI can be introduced later as another binary, for example at
`src/bin/myconnect-gui/main.rs`, without coupling GUI dependencies to the CLI
entry point. See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for module
boundaries, data flow, the full HTTP API, and the pairing/transfer state
machines.

Run the CLI with:

```sh
cargo run -- run
cargo run -- devices
cargo run -- send <device-id> <file> --watch
```

Use `RUST_LOG` to control log output, for example
`RUST_LOG=myconnect=debug cargo run -- run`.

All commands except `run` communicate with the daemon through its authenticated
local HTTP API.

The current architecture — module map, connection lifecycle, state machines,
and HTTP API reference — is documented in
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md). The original protocol
research and the phase-by-phase implementation plan used to build the MVP are
preserved for historical reference in [`docs/archive/`](docs/archive/).


### Tech stack and development guidelines

The app is mostly based on async/tokio, and will leverage asynchronous programming to handle multiple device connections, file transfers, and clipboard synchronization efficiently. 

Choices for basic components and libraries in the project include:
- `tokio` for asynchronous runtime
- `clap` for command-line argument parsing
- `serde` and `serde_json` for serialization and deserialization
- `anyhow` for error handling
- `tracing` for structured logging and diagnostics
- `thiserror` for defining custom error types
- `dotenvy` for loading environment variables from a `.env` file
