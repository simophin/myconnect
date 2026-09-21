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