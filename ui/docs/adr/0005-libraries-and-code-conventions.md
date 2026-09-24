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
| Window control | `window_manager` | The standard desktop window plugin: intercepts close, hides, shows and focuses on Linux, macOS and Windows. See [0007](0007-keep-running-in-the-tray.md). |
| Tray icon | `tray_manager` | The most used tray plugin. From 0.6 it sits on `nativeapi`, and on Linux it is a D-Bus StatusNotifierItem with no libappindicator build dependency. |
| Notifications | `flutter_local_notifications` | Widely used; covers Linux (freedesktop notifications over D-Bus), macOS and Windows, with click callbacks. |

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
