import 'package:myconnect_ui/src/core/daemon/daemon_host.dart';

/// A daemon started outside this process, e.g. `myconnect run`. Stopping the
/// UI leaves it running.
class ExternalDaemonHost implements DaemonHost {
  new({required this.baseUrl, String token = ''})
    : _token = token.isEmpty ? null : token;

  final Uri baseUrl;
  final String? _token;

  @override
  Future<DaemonEndpoint> start() async =>
      DaemonEndpoint(baseUrl: baseUrl, token: _token);

  @override
  Future<void> stop() async {}
}
