// The analyzer evaluates `String.fromEnvironment` with no defines, sees the
// defaults, and reports the arguments below as redundant; `dart fix` would
// then delete them and silently drop every `--dart-define`.
// ignore_for_file: avoid_redundant_argument_values

import 'package:flutter/foundation.dart';
import 'package:myconnect_ui/src/core/daemon/external_daemon_host.dart';
import 'package:myconnect_ui/src/core/daemon/native_daemon_host.dart';

/// Where and how to reach a running daemon's control API.
@immutable
class DaemonEndpoint {
  const new({required this.baseUrl, this.token});

  /// Root of the daemon, e.g. `http://127.0.0.1:24816`.
  final Uri baseUrl;

  /// Bearer token, or `null` when the daemon does not require one.
  final String? token;

  @override
  String toString() => 'DaemonEndpoint($baseUrl, token: ${token != null})';
}

/// Owns the lifecycle of the daemon this UI talks to.
///
/// The UI never talks to the daemon other than over the HTTP API at the
/// endpoint [start] returns; this interface only decides which daemon that is.
abstract interface class DaemonHost {
  /// Pick the host from `--dart-define`s: an external daemon when
  /// `MYCONNECT_API_URL` is set, otherwise one embedded in this process.
  factory fromEnvironment() {
    const externalUrl = String.fromEnvironment('MYCONNECT_API_URL');
    if (externalUrl.isNotEmpty) {
      return ExternalDaemonHost(
        baseUrl: Uri.parse(externalUrl),
        token: const String.fromEnvironment('MYCONNECT_API_TOKEN'),
      );
    }
    return NativeDaemonHost(
      config: const NativeDaemonConfig(
        dataDir: String.fromEnvironment('MYCONNECT_DATA_DIR'),
        downloadDir: String.fromEnvironment('MYCONNECT_DOWNLOAD_DIR'),
        deviceName: String.fromEnvironment('MYCONNECT_DEVICE_NAME'),
        discoveryLoopback: bool.fromEnvironment('MYCONNECT_DISCOVERY_LOOPBACK'),
      ),
    );
  }

  /// Start (or locate) the daemon and return how to reach it. Idempotent.
  Future<DaemonEndpoint> start();

  /// Release the daemon if this host started it. Idempotent.
  Future<void> stop();
}
