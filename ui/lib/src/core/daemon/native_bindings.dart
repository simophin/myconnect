import 'dart:convert';
import 'dart:ffi';
import 'dart:io';

import 'package:ffi/ffi.dart';

typedef _StartNative = Pointer<Utf8> Function(Pointer<Utf8> configJson);
typedef _StopNative = Pointer<Utf8> Function(Uint64 handle);
typedef _StopDart = Pointer<Utf8> Function(int handle);
typedef _FreeNative = Void Function(Pointer<Utf8> value);
typedef _FreeDart = void Function(Pointer<Utf8> value);

/// Raised when the native library reports `{"error": ...}`.
class NativeDaemonException implements Exception {
  const new(this.message);
  final String message;

  @override
  String toString() => 'NativeDaemonException: $message';
}

/// Thin, synchronous binding to the `myconnect-ffi` C ABI (see
/// `ffi/src/lib.rs`). Every call blocks, so callers run it off the UI isolate.
///
/// The ABI exchanges JSON strings; this class only handles marshalling and
/// freeing them.
class NativeBindings {
  new _(DynamicLibrary library)
    : _start = library.lookupFunction<_StartNative, _StartNative>(
        'myconnect_start',
      ),
      _stop = library.lookupFunction<_StopNative, _StopDart>('myconnect_stop'),
      _free = library.lookupFunction<_FreeNative, _FreeDart>(
        'myconnect_free_string',
      );

  /// Load the library bundled with the app, or the one named by the
  /// `MYCONNECT_FFI_LIBRARY` environment variable (useful during development).
  factory open() {
    final override = Platform.environment['MYCONNECT_FFI_LIBRARY'];
    final name = override != null && override.isNotEmpty
        ? override
        : switch (Platform.operatingSystem) {
            'linux' => 'libmyconnect_ffi.so',
            'macos' => 'libmyconnect_ffi.dylib',
            'windows' => 'myconnect_ffi.dll',
            final os => throw UnsupportedError('No native daemon for $os'),
          };
    return NativeBindings._(DynamicLibrary.open(name));
  }

  final _StartNative _start;
  final _StopDart _stop;
  final _FreeDart _free;

  Map<String, Object?> start(Map<String, Object?> config) {
    final configJson = jsonEncode(config).toNativeUtf8();
    try {
      return _consume(_start(configJson));
    } finally {
      malloc.free(configJson);
    }
  }

  void stop(int handle) => _consume(_stop(handle));

  Map<String, Object?> _consume(Pointer<Utf8> result) {
    try {
      final decoded = jsonDecode(result.toDartString()) as Map<String, Object?>;
      if (decoded['error'] case final String message) {
        throw NativeDaemonException(message);
      }
      return decoded;
    } finally {
      _free(result);
    }
  }
}
