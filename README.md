## MyConnect - a Rust based KDE Connect alternative

MyConnect is an open-source project written in Rust that aims to provide similar functionality to KDE Connect, allowing seamless integration and communication between your devices.

### Long term goal

Provides a desktop application for MacOS/Linux/Windows.

### MVP

The Minimum Viable Product (MVP) for MyConnect includes the following features:

- CLI interface for daemon control, devices, pairing, clipboard, and transfers
- Ability to connect and communicate with multiple devices
- File transfer between devices
- Clipboard synchronization between devices

### CLI interface

The CLI interface is command based, allowing users to interact with MyConnect through terminal commands.

#### Example Commands

- `myconnect run` - Run the authenticated local daemon in the foreground.
- `myconnect devices [--watch]` - List devices and optionally follow changes.
- `myconnect pair <device-id>` - Start pairing with a discovered device.
- `myconnect pair accept|reject <pairing-id>` - Resolve a pairing request.
- `myconnect unpair <device-id>` - Remove trust and forget a device.
- `myconnect send <device-id> <file> [--watch]` - Stream a file to a device.
- `myconnect clipboard get|set <text>|watch` - Control text synchronization.

Add `--json` for machine-readable output. API clients use the same persistent
token as the daemon. For development, `MYCONNECT_API_URL` and
`MYCONNECT_API_TOKEN` override the loopback URL and stored token.

### Project structure

MyConnect is a single Cargo package with a shared library and separate binary
entry points:

```text
src/
├── application.rs          # frontend-independent application API
├── lib.rs                  # shared library
└── bin/
    └── myconnect/          # CLI binary
        ├── cli.rs
        └── main.rs
```

Keeping behavior in the library lets other frontends use the same application
API. A GUI can be introduced later as another binary, for example at
`src/bin/myconnect-gui/main.rs`, without coupling GUI dependencies to the CLI
entry point.

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

Protocol and Rust ecosystem research is recorded in
[`docs/KDECONNECT_PROTOCOL_RESEARCH.md`](docs/KDECONNECT_PROTOCOL_RESEARCH.md).
The ordered implementation handoff is in
[`docs/HANDOFF_PLAN.md`](docs/HANDOFF_PLAN.md).


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
