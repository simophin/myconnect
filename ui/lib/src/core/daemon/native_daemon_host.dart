import 'dart:isolate';

import 'package:flutter/foundation.dart';
import 'package:logging/logging.dart';
import 'package:myconnect_ui/src/core/daemon/daemon_host.dart';
import 'package:myconnect_ui/src/core/daemon/native_bindings.dart';

final _log = Logger('NativeDaemonHost');

/// Start options for the embedded daemon, mirroring `myconnect run`. Empty
/// strings mean "use the daemon's setting". A value given here overrides
/// the stored setting for this run only, so normal launches leave them
/// empty and let the settings screen decide.
@immutable
class NativeDaemonConfig {
  const new({
    this.dataDir = '',
    this.downloadDir = '',
    this.deviceName = '',
    this.discoveryLoopback = false,
  });

  final String dataDir;
  final String downloadDir;
  final String deviceName;
  final bool discoveryLoopback;

  Map<String, Object?> toJson() => {
    if (dataDir.isNotEmpty) 'dataDir': dataDir,
    if (downloadDir.isNotEmpty) 'downloadDir': downloadDir,
    if (deviceName.isNotEmpty) 'deviceName': deviceName,
    'discoveryLoopback': discoveryLoopback,
  };
}

/// Runs the daemon inside this process through the `myconnect-ffi` library.
///
/// The daemon binds its control API to a free loopback port and requires a
/// token it generates per start, so only this process can use it. Calls are
/// serialized, so a `stop` issued during `start` waits for it.
class NativeDaemonHost implements DaemonHost {
  new({this.config = const NativeDaemonConfig()});

  final NativeDaemonConfig config;

  int? _handle;
  DaemonEndpoint? _endpoint;
  Future<void> _pending = Future.value();

  @override
  Future<DaemonEndpoint> start() => _serialized(() async {
    if (_endpoint case final endpoint?) return endpoint;
    // The native call blocks while sockets are bound, so keep it off the UI
    // isolate.
    final options = config.toJson();
    _log.info('Starting embedded daemon with $options');
    final started = await _startInIsolate(options);
    _handle = started['handle']! as int;
    final endpoint = DaemonEndpoint(
      baseUrl: Uri(
        scheme: 'http',
        host: started['apiHost']! as String,
        port: started['apiPort']! as int,
      ),
      token: started['apiToken']! as String,
    );
    _log.info('Embedded daemon started at ${endpoint.baseUrl}');
    return _endpoint = endpoint;
  });

  @override
  Future<void> stop() => _serialized(() async {
    final handle = _handle;
    if (handle == null) return;
    _handle = null;
    _endpoint = null;
    await _stopInIsolate(handle);
    _log.info('Embedded daemon stopped');
  });

  Future<T> _serialized<T>(Future<T> Function() action) {
    final result = _pending.then((_) => action());
    _pending = result.then<void>((_) {}, onError: (_) {});
    return result;
  }
}

// Top-level so the closures handed to `Isolate.run` capture only their
// (sendable) arguments, never a host instance.
Future<Map<String, Object?>> _startInIsolate(Map<String, Object?> config) =>
    Isolate.run(() => NativeBindings.open().start(config));

Future<void> _stopInIsolate(int handle) =>
    Isolate.run(() => NativeBindings.open().stop(handle));
