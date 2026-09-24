#!/bin/sh
# Xcode build phase (Runner target): build the Rust `myconnect-ffi` cdylib
# for every architecture Xcode is building, merge them into one universal
# dylib, and put it in the app's Contents/Frameworks, where
# `NativeBindings.open()` loads it. Cargo builds incrementally, so the phase
# always runs. See ADR 0006.
set -eu

# Xcode doesn't inherit the login shell's PATH.
export PATH="$HOME/.cargo/bin:$PATH"

rust_root="$PROJECT_DIR/../.."
if [ "$CONFIGURATION" = "Debug" ]; then
  profile=dev
  out_dir=debug
else
  profile=release
  out_dir=release
fi

set --
for arch in $ARCHS; do
  case "$arch" in
    arm64) target=aarch64-apple-darwin ;;
    x86_64) target=x86_64-apple-darwin ;;
    *) echo "error: no Rust target for architecture $arch" >&2; exit 1 ;;
  esac
  cargo build --manifest-path "$rust_root/Cargo.toml" --package myconnect-ffi \
    --profile "$profile" --target "$target"
  set -- "$@" "$rust_root/target/$target/$out_dir/libmyconnect_ffi.dylib"
done

frameworks="$TARGET_BUILD_DIR/$FRAMEWORKS_FOLDER_PATH"
dylib="$frameworks/libmyconnect_ffi.dylib"
mkdir -p "$frameworks"
lipo -create -output "$dylib" "$@"
install_name_tool -id "@rpath/libmyconnect_ffi.dylib" "$dylib"

# Xcode signs the app bundle, but not files a script copies into it.
identity="${EXPANDED_CODE_SIGN_IDENTITY:-}"
if [ -z "$identity" ] || [ "$identity" = "-" ]; then
  codesign --force --sign - "$dylib"
else
  codesign --force --sign "$identity" --timestamp --options runtime "$dylib"
fi
