## MyConnect - a Rust based KDE Connect alternative

MyConnect is an open-source project written in Rust that aims to provide similar functionality to KDE Connect, allowing seamless integration and communication between your devices.

### Long term goal

Provides a desktop application for MacOS/Linux/Windows.

### MVP

The Minimum Viable Product (MVP) for MyConnect includes the following features:

- CLI interface with basic commands (`run`, `send`)
- Ability to connect and communicate with multiple devices
- File transfer between devices
- Clipboard synchronization between devices

### CLI interface

The CLI interface is command based, allowing users to interact with MyConnect through terminal commands.

#### Example Commands

- `myconnect run` - Run the MyConnect service, where the clipboard is synchronized between devices, and file receiving is enabled. The file will be saved to the default download directory, or a custom directory if specified.
- `myconnect send <device> <file>` - Send a file to a specific device. This command does not handle clipboard synchronization, nor will it receive files.

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
cargo run -- send <device> <file>
```

Use `RUST_LOG` to control log output, for example
`RUST_LOG=myconnect=debug cargo run -- run`.

The commands currently parse their arguments and reach the shared application
layer; device discovery, connectivity, and transfers are the next implementation
steps.

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
