# 0006. Build and bundle the Rust core from the platform build

- Status: Accepted
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
- **macOS:** a "Build MyConnect Core" run-script phase on the Runner target
  calls `macos/build_myconnect_ffi.sh`, which runs `cargo build --target`
  for each of Xcode's `ARCHS` (`aarch64-apple-darwin`,
  `x86_64-apple-darwin`), merges them with `lipo`, sets the install name to
  `@rpath/libmyconnect_ffi.dylib`, puts it in `Contents/Frameworks` and signs
  it (Xcode only signs what it copies itself). `NativeBindings.open()` loads
  it by full path from there, since `dlopen` doesn't search `Frameworks` for
  a bare name. Release builds are universal, so both Rust targets must be
  installed (`rustup target add`).
- **Windows:** `windows/CMakeLists.txt` has the same `myconnect_ffi` target
  as Linux, with the profile picked per configuration by generator
  expression, and installs `myconnect_ffi.dll` next to the executable.
- `MYCONNECT_FFI_LIBRARY` (environment) overrides the library path at run
  time, for debugging against a different build.

## Consequences

- Building the app requires a Rust toolchain on `PATH`.
- The macOS and Windows steps are built in CI (`.github/workflows/
  build.yml`) but not yet run on a real machine.
- If more native pieces appear, revisit Flutter build hooks
  (`hook/build.dart`) as a cross-platform replacement.
