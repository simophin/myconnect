# 0002. Embed the daemon through a minimal JSON-over-C ABI

- Status: Accepted
- Date: 2026-09-24

## Context

A desktop app should not ask users to start a separate daemon first. The
app therefore starts one in-process, from the Rust core built as a shared
library. We need a way for Dart to start and stop it, and to learn where
its HTTP API is and how to authenticate — while keeping [0001](0001-stateless-ui-over-the-http-api.md):
native calls never become a second API.

Options considered:

- **flutter_rust_bridge**: generates bindings for arbitrary Rust APIs.
  Powerful, but brings codegen, a runtime, and an incentive to expose more
  Rust functions than start/stop.
- **Spawning the `myconnect` binary** as a child process: simple, but
  requires shipping and locating a second executable and parsing its output
  to discover the port.
- **A hand-written C ABI over `dart:ffi`** with three functions.

## Decision

The `ffi/` crate (`myconnect-ffi`, a `cdylib`) exports exactly:

| Function | Purpose |
| --- | --- |
| `myconnect_start(config_json) -> json` | Start an instance. Config mirrors `myconnect run` (`dataDir`, `downloadDir`, `deviceName`, `discoveryLoopback`, `apiHost`, `apiPort`, `apiToken`). Returns `{handle, apiHost, apiPort, apiToken}`. |
| `myconnect_stop(handle) -> json` | Shut down the API, LAN transport and transfers, then the runtime. |
| `myconnect_free_string(ptr)` | Free any string returned above. |

- Parameters and results are **JSON strings**, so adding a start option
  never changes the ABI. Errors are `{"error": "..."}`; panics are caught
  and never unwind into Dart.
- The embedded API binds **port 0 on loopback** by default (no clash with a
  CLI daemon on 24816) and requires a **token generated per start**, so only
  the process that started the instance can use it.
- Each instance owns its Tokio runtime; handles allow more than one
  instance per process (used by tests).
- On the Dart side, `DaemonHost` hides where the daemon comes from:
  `NativeDaemonHost` (FFI, default) or `ExternalDaemonHost` (selected by
  `--dart-define=MYCONNECT_API_URL=...`). Blocking native calls run in
  `Isolate.run`, and the isolate entry points are top-level functions so
  they capture only sendable values.

## Consequences

- The native surface is tiny and stable; everything else is HTTP.
- Bindings are hand-written (`native_bindings.dart`); three functions do
  not justify `ffigen`.
- The daemon's lifetime is tied to the app: it stops when the app exits
  (`AppLifecycleListener.onExitRequested`) or when the `ProviderScope` is
  disposed. (Since [0007](0007-keep-running-in-the-tray.md), closing the
  window no longer exits the app; the tray's Quit does.) A hard kill skips graceful shutdown; the OS still reclaims
  sockets, but partial `.part` downloads may be left behind.
- The HTTP API gained optional authentication for this: a token is
  enforced only when the daemon is started with one (the CLI defaults to
  none; the FFI always sets one).
