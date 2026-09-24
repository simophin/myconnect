# 0005. Library choices and code conventions

- Status: Accepted
- Date: 2026-09-24

## Context

We want well-maintained, widely used dependencies and a consistent code
style, while keeping the dependency list short.

## Decision

| Concern | Choice | Why |
| --- | --- | --- |
| Design system | `material_ui` | Material now ships outside the framework; `go_router` 18 already uses it, and mixing it with `package:flutter/material.dart` would split `Theme` lookups. |
| Routing | `go_router` | Official, URL-based, nested routes mirror the screen stack. |
| HTTP | `dio` | Typed errors, base options for auth headers, streaming responses for SSE, swappable adapter for tests. |
| Models | `freezed` + `json_serializable` | Immutable value types with equality and generated JSON mapping. |
| State / DI | `flutter_riverpod` | See [0004](0004-riverpod-for-state-and-dependency-injection.md). |
| Native interop | `dart:ffi` + `ffi` | See [0002](0002-embed-the-daemon-through-a-json-c-abi.md). |
| Logging | `logging` | Hierarchical loggers, printed in debug builds. |
| Lints | `very_good_analysis` | Strict; exceptions are listed with reasons in `analysis_options.yaml`. |
| Test doubles | `mocktail` | Mocks without codegen. |
| End-to-end tests | `integration_test` | Flutter's own package for driving the real app with the widget tester; `integration_test/` runs against the real FFI daemon and a CLI peer. |
| Faking the file dialog in end-to-end tests | `file_selector_platform_interface` | The `file_selector` plugin's official platform interface: replacing `FileSelectorPlatform.instance` answers "Send file" without a GTK dialog. |
| Window control | `window_manager` | The standard desktop window plugin: intercepts close, hides, shows and focuses on Linux, macOS and Windows. See [0007](0007-keep-running-in-the-tray.md). |
| Tray icon | `tray_manager` | The most used tray plugin. From 0.6 it sits on `nativeapi`, and on Linux it is a D-Bus StatusNotifierItem with no libappindicator build dependency. |
| Notifications | `flutter_local_notifications` | Widely used; covers Linux (freedesktop notifications over D-Bus), macOS and Windows, with click callbacks. |
| File picker | `file_selector` | The Flutter team's plugin; native open dialogs on Linux (GTK, or the portal), macOS and Windows. |
| Dropping files on the window | `desktop_drop` | The widely used drop plugin (MixinNetwork) for Linux, macOS and Windows. Every `DropTarget` receives every drop inside its bounds, even on covered pages, so the app has one window-wide target (`FileDropZone`) that hit-tests for the device under the pointer. Tray icons can't take drops on Linux (StatusNotifierItem) or Windows, so the tray offers *Send files…* instead. |
| Default device name (Rust daemon) | `gethostname` | Small, widely used crate for the host name on Linux, macOS and Windows; the standard library has no API for it. |
| Opening files and folders | `url_launcher` | The Flutter team's plugin; opens `file:` URIs with the desktop's default app (on Linux through GIO, like `xdg-open`). |
| SSH and SFTP client (Rust daemon) | `russh` + `russh-sftp` | Pure Rust and async on tokio, so they build for macOS and Windows with no system libraries. `russh` is used with the `ring` backend, which rustls already uses, rather than aws-lc-rs, which needs cmake and NASM on Windows. Used to browse a device's files ([0008](0008-browse-device-files-in-the-app.md)). |
| Desktop clipboard (Rust daemon) | `arboard` | The standard cross-platform clipboard crate (maintained by 1Password), covering X11, Wayland (with the `wayland-data-control` feature), macOS and Windows. Default features are off, so it doesn't pull in image support. |

Conventions:

- Feature-first layout: `lib/src/core/` (API, daemon hosting, routing,
  shared providers), `lib/src/features/<feature>/` (controller + pages),
  `lib/src/shared/` (small reusable widgets).
- Model files mirror the daemon's JSON exactly (camelCase fields,
  snake_case enum values) and are the only place JSON shapes appear.
- Generated `*.freezed.dart` / `*.g.dart` files are committed, so the app
  builds without running `build_runner` first. Regenerate with
  `dart run build_runner build --delete-conflicting-outputs`.
- Constructors use Dart 3.13's `new` syntax, as the lint set prefers.
- `--dart-define` values are read in one place (`DaemonHost.fromEnvironment`),
  which opts out of `avoid_redundant_argument_values`: the analyzer sees
  their defaults and `dart fix` would otherwise delete them.

## Consequences

- Adding a dependency should come with a row in this table (or a new ADR
  when it changes an existing choice).
