# 0006. Build and bundle the Rust core from the platform build

- Status: Accepted (Linux); macOS and Windows pending
- Date: 2026-09-24

## Context

The app needs `libmyconnect_ffi` next to its executable. The Rust workspace
lives at the repository root, one level above `ui/`. Flutter's native-assets
build hooks could build it, but they add a Dart build script and toolchain
discovery for one library we already build with cargo.

## Decision

- **Linux:** `linux/CMakeLists.txt` defines a `myconnect_ffi` target that
  runs `cargo build --package myconnect-ffi` (`dev` profile for Debug
  builds, `release` otherwise), makes the runner depend on it, and installs
  the `.so` into the bundle's `lib/`, where the runner's `$ORIGIN/lib` RPATH
  finds it. `flutter run -d linux` / `flutter build linux` therefore always
  bundle a current core; cargo's incremental build keeps this cheap.
- `MYCONNECT_FFI_LIBRARY` (environment) overrides the library path at run
  time, for debugging against a different build.

## Consequences

- Building the app requires a Rust toolchain on `PATH`.
- **macOS and Windows are not wired yet**: their runners need the
  equivalent step (an Xcode build phase copying the `.dylib` into
  `Frameworks`; a CMake step for the `.dll`). Until then, those platforms
  can use an external daemon via `--dart-define=MYCONNECT_API_URL`.
- If more native pieces appear, revisit Flutter build hooks
  (`hook/build.dart`) as a cross-platform replacement.
