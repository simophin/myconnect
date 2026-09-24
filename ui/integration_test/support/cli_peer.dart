import 'dart:async';
import 'dart:convert';
import 'dart:io';

/// The repository root, from the `ui/` directory the tests run in.
final repositoryRoot = Directory.current.parent;

/// Build the `myconnect` CLI and return its path, or use the binary named by
/// `MYCONNECT_CLI` as is.
Future<String> buildCli() async {
  final override = Platform.environment['MYCONNECT_CLI'];
  if (override != null && override.isNotEmpty) return override;
  final build = await Process.run('cargo', [
    'build',
    '--bin',
    'myconnect',
  ], workingDirectory: repositoryRoot.path);
  if (build.exitCode != 0) {
    throw StateError('cargo build failed:\n${build.stderr}');
  }
  return '${repositoryRoot.path}/target/debug/myconnect';
}

/// A `myconnect run` daemon in its own process and data directory, discovering
/// over loopback only, driven through its (tokenless) HTTP API.
class CliPeer {
  new _(this.cli, this.name, this._process, this.port, this.directory);

  final String cli;
  final String name;
  final Process _process;
  final int port;
  final Directory directory;

  final _http = HttpClient();

  Directory get downloadDir => Directory('${directory.path}/downloads');

  static Future<CliPeer> start(String cli, {required String name}) async {
    final directory = await Directory.systemTemp.createTemp('myconnect-peer-');
    final port = await _freePort();
    final process = await Process.start(cli, [
      '--api-port',
      '$port',
      'run',
      '--discovery-loopback',
      '--data-dir',
      '${directory.path}/data',
      '--download-dir',
      '${directory.path}/downloads',
      '--device-name',
      name,
    ]);
    for (final output in [process.stdout, process.stderr]) {
      output
          .transform(utf8.decoder)
          .transform(const LineSplitter())
          .listen((line) => stdout.writeln('[peer] $line'));
    }
    final peer = CliPeer._(cli, name, process, port, directory);
    await peer.waitFor('its API', () async {
      try {
        await peer.get('/status');
        return true;
      } on SocketException {
        return false;
      }
    });
    return peer;
  }

  Future<void> stop() async {
    _http.close(force: true);
    _process.kill();
    await _process.exitCode.timeout(
      const Duration(seconds: 10),
      onTimeout: () {
        _process.kill(ProcessSignal.sigkill);
        return _process.exitCode;
      },
    );
    await directory.delete(recursive: true);
  }

  Future<Object?> get(String path) => _request('GET', path);

  Future<Object?> post(String path, [Object? body]) =>
      _request('POST', path, body);

  /// Announce, and wait until [deviceName] is connected. Returns its id.
  Future<String> discover(String deviceName) async {
    String? id;
    await waitFor('$deviceName to connect', () async {
      await post('/discovery');
      final device = (await devices()).where(
        (device) =>
            device['deviceName'] == deviceName &&
            device['reachability'] == 'connected',
      );
      id = device.firstOrNull?['deviceId'] as String?;
      return id != null;
    });
    return id!;
  }

  Future<List<Map<String, Object?>>> devices() async =>
      ((await get('/devices'))! as List<Object?>).cast();

  Future<Map<String, Object?>?> device(String id) async =>
      (await devices()).where((device) => device['deviceId'] == id).firstOrNull;

  Future<List<Map<String, Object?>>> pairings() async =>
      ((await get('/pairings'))! as List<Object?>).cast();

  Future<List<Map<String, Object?>>> transfers() async =>
      ((await get('/transfers'))! as List<Object?>).cast();

  /// Send [file] to [deviceId] with `myconnect send`, the way a user would.
  Future<void> sendFile(String deviceId, File file) async {
    final result = await Process.run(cli, [
      '--api-port',
      '$port',
      'send',
      deviceId,
      file.path,
    ]);
    if (result.exitCode != 0) {
      throw StateError('myconnect send failed:\n${result.stderr}');
    }
  }

  /// Poll [condition] until it holds, failing after [timeout].
  Future<void> waitFor(
    String what,
    Future<bool> Function() condition, {
    Duration timeout = const Duration(seconds: 20),
  }) async {
    final deadline = DateTime.now().add(timeout);
    while (!await condition()) {
      if (DateTime.now().isAfter(deadline)) {
        throw TimeoutException('Timed out waiting for $what', timeout);
      }
      await Future<void>.delayed(const Duration(milliseconds: 200));
    }
  }

  Future<Object?> _request(String method, String path, [Object? body]) async {
    final request = await _http.openUrl(
      method,
      Uri.parse('http://127.0.0.1:$port/api/v1$path'),
    );
    if (body != null) {
      request.headers.contentType = ContentType.json;
      request.write(jsonEncode(body));
    }
    final response = await request.close();
    final text = await response.transform(utf8.decoder).join();
    if (response.statusCode >= 400) {
      throw HttpException('$method $path: ${response.statusCode} $text');
    }
    return text.isEmpty ? null : jsonDecode(text);
  }

  static Future<int> _freePort() async {
    final socket = await ServerSocket.bind(InternetAddress.loopbackIPv4, 0);
    final port = socket.port;
    await socket.close();
    return port;
  }
}
