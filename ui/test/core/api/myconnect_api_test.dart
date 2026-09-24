import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:dio/dio.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:myconnect_ui/src/core/api/api_exception.dart';
import 'package:myconnect_ui/src/core/api/models/event.dart';
import 'package:myconnect_ui/src/core/api/models/remote_file.dart';
import 'package:myconnect_ui/src/core/api/myconnect_api.dart';
import 'package:myconnect_ui/src/core/daemon/daemon_host.dart';

/// Answers each request from [handler] and records it.
class FakeAdapter implements HttpClientAdapter {
  new(this.handler);

  final ResponseBody Function(RequestOptions options) handler;
  final requests = <RequestOptions>[];

  @override
  Future<ResponseBody> fetch(
    RequestOptions options,
    Stream<Uint8List>? requestStream,
    Future<void>? cancelFuture,
  ) async {
    requests.add(options);
    return handler(options);
  }

  @override
  void close({bool force = false}) {}
}

ResponseBody json(
  Object? body, {
  int status = 200,
  String type = 'application/json',
}) => ResponseBody.fromString(
  jsonEncode(body),
  status,
  headers: {
    Headers.contentTypeHeader: [type],
  },
);

MyConnectApi apiWith(FakeAdapter adapter, {String? token = 'secret'}) {
  final api = MyConnectApi.forEndpoint(
    DaemonEndpoint(baseUrl: Uri.parse('http://127.0.0.1:4000'), token: token),
  );
  // Reach the Dio instance through a fresh client sharing its options.
  return api..debugDio.httpClientAdapter = adapter;
}

void main() {
  test('sends the bearer token and targets /api/v1', () async {
    final adapter = FakeAdapter((_) => json(<Object?>[]));
    await apiWith(adapter).devices();
    final request = adapter.requests.single;
    expect(request.uri.toString(), 'http://127.0.0.1:4000/api/v1/devices');
    expect(request.headers['Authorization'], 'Bearer secret');
  });

  test('omits authentication when the daemon has no token', () async {
    final adapter = FakeAdapter((_) => json(<Object?>[]));
    await apiWith(adapter, token: null).pairings();
    expect(adapter.requests.single.headers, isNot(contains('Authorization')));
  });

  test('maps problem+json responses to typed errors', () async {
    final adapter = FakeAdapter(
      (_) => json(
        {
          'type': 'about:blank',
          'title': 'Conflict',
          'status': 409,
          'code': 'device_not_connected',
        },
        status: 409,
        type: 'application/problem+json',
      ),
    );
    await expectLater(
      apiWith(adapter).startPairing('device'),
      throwsA(
        isA<ApiException>()
            .having((e) => e.code, 'code', 'device_not_connected')
            .having((e) => e.statusCode, 'statusCode', 409),
      ),
    );
  });

  test('reports an unreachable daemon', () async {
    final adapter = FakeAdapter(
      (options) => throw DioException.connectionError(
        requestOptions: options,
        reason: 'refused',
      ),
    );
    await expectLater(
      apiWith(adapter).devices(),
      throwsA(
        isA<ApiException>().having((e) => e.code, 'code', 'daemon_unavailable'),
      ),
    );
  });

  test('uploads a file as deviceId, then a file part with its size', () async {
    final directory = await Directory.systemTemp.createTemp('myconnect_test');
    addTearDown(() => directory.delete(recursive: true));
    final file = File('${directory.path}/notes.txt')
      ..writeAsStringSync('hello');
    final adapter = FakeAdapter(
      (_) => json({
        'id': 't1',
        'deviceId': 'device',
        'deviceName': 'Phone',
        'direction': 'outgoing',
        'status': 'completed',
        'fileName': 'notes.txt',
        'totalBytes': 5,
        'transferredBytes': 5,
        'createdAt': 1,
        'updatedAt': 2,
      }, status: 202),
    );

    final transfer = await apiWith(adapter).sendFile('device', file.path);

    expect(transfer.fileName, 'notes.txt');
    final request = adapter.requests.single;
    expect(request.path, 'transfers');
    expect(request.receiveTimeout, Duration.zero);
    final form = request.data as FormData;
    expect(form.fields.single, isA<MapEntry<String, String>>());
    expect(form.fields.single.key, 'deviceId');
    expect(form.fields.single.value, 'device');
    final part = form.files.single;
    expect(part.key, 'file');
    expect(part.value.filename, 'notes.txt');
    expect(part.value.headers?['content-length'], ['5']);
  });

  test('lists storage, then a folder by its path', () async {
    final adapter = FakeAdapter(
      (options) => json({
        'path': options.queryParameters['path'],
        'entries': [
          {
            'name': 'notes.txt',
            'path': '/storage/emulated/0/notes.txt',
            'kind': 'file',
            'size': 5,
            'modifiedAt': 1,
          },
          {'name': 'Link', 'path': '/l', 'kind': 'a_new_kind'},
        ],
      }),
    );
    final api = apiWith(adapter);

    await api.listFiles('dev ice');
    final listing = await api.listFiles('dev ice', path: '/storage/emulated/0');

    final [roots, folder] = adapter.requests;
    expect(roots.uri.path, '/api/v1/devices/dev%20ice/files');
    expect(roots.uri.queryParameters, isEmpty);
    expect(folder.uri.queryParameters, {'path': '/storage/emulated/0'});
    expect(listing.path, '/storage/emulated/0');
    expect(listing.entries.first.size, 5);
    expect(listing.entries.last.kind, FileKind.unknown);
  });

  test('uploads into a folder as path, then the file part', () async {
    final directory = await Directory.systemTemp.createTemp('myconnect_test');
    addTearDown(() => directory.delete(recursive: true));
    final file = File('${directory.path}/photo.jpg')..writeAsStringSync('jpg');
    final adapter = FakeAdapter(
      (_) => json({
        'id': 't1',
        'deviceId': 'device',
        'deviceName': 'Phone',
        'direction': 'outgoing',
        'status': 'completed',
        'fileName': 'photo.jpg',
        'totalBytes': 3,
        'transferredBytes': 3,
        'createdAt': 1,
        'updatedAt': 2,
      }, status: 202),
    );

    await apiWith(adapter).uploadFile('device', '/sdcard/DCIM', file.path);

    final request = adapter.requests.single;
    expect(request.path, 'devices/device/files/upload');
    final form = request.data as FormData;
    expect(form.fields.single.key, 'path');
    expect(form.fields.single.value, '/sdcard/DCIM');
    expect(form.files.single.value.headers?['content-length'], ['3']);
  });

  test('changes files by path', () async {
    final adapter = FakeAdapter(
      (options) => switch (options.method) {
        'DELETE' => ResponseBody.fromString('', 204),
        _ when options.path.endsWith('/download') => json({
          'id': 't1',
          'deviceId': 'device',
          'deviceName': 'Phone',
          'direction': 'incoming',
          'status': 'queued',
          'fileName': 'b.txt',
          'totalBytes': 3,
          'transferredBytes': 0,
          'createdAt': 1,
          'updatedAt': 1,
        }, status: 202),
        _ => json({'name': 'b', 'path': '/a/b', 'kind': 'directory'}),
      },
    );
    final api = apiWith(adapter);

    await api.createDirectory('device', '/a/b');
    await api.moveFile('device', '/a/c', '/a/b');
    await api.deleteFile('device', '/a/b');
    await api.downloadFile('device', '/a/b.txt');

    final [create, move, delete, download] = adapter.requests;
    expect(create.uri.path, '/api/v1/devices/device/files/directories');
    expect(create.data, {'path': '/a/b'});
    expect(move.uri.path, '/api/v1/devices/device/files/move');
    expect(move.data, {'from': '/a/c', 'to': '/a/b'});
    expect(delete.method, 'DELETE');
    expect(delete.uri.queryParameters, {'path': '/a/b'});
    expect(download.uri.path, '/api/v1/devices/device/files/download');
    expect(download.data, {'path': '/a/b.txt'});
  });

  test("keeps the peer's explanation from a problem", () async {
    final adapter = FakeAdapter(
      (_) => json(
        {
          'type': 'about:blank',
          'title': 'Conflict',
          'status': 409,
          'code': 'files_unavailable',
          'detail': 'No permission',
        },
        status: 409,
        type: 'application/problem+json',
      ),
    );
    await expectLater(
      apiWith(adapter).listFiles('device'),
      throwsA(
        isA<ApiException>()
            .having((e) => e.detail, 'detail', 'No permission')
            .having((e) => e.message, 'message', contains('(No permission)')),
      ),
    );
  });

  test('pings a device by id', () async {
    final adapter = FakeAdapter((_) => ResponseBody.fromString('', 202));
    await apiWith(adapter).ping('device');

    final request = adapter.requests.single;
    expect(request.method, 'POST');
    expect(request.uri.path, '/api/v1/devices/device/ping');
  });

  test('broadcasts discovery, or announces to one address', () async {
    final adapter = FakeAdapter((_) => ResponseBody.fromString('', 202));
    final api = apiWith(adapter);
    await api.scan();
    await api.scan(address: '192.168.1.20');

    final [broadcast, unicast] = adapter.requests;
    expect(broadcast.method, 'POST');
    expect(broadcast.uri.path, '/api/v1/discovery');
    expect(broadcast.data, isNull);
    expect(unicast.uri.path, '/api/v1/discovery');
    expect(unicast.data, {'address': '192.168.1.20'});
  });

  test('patches settings with only the given fields', () async {
    final adapter = FakeAdapter(
      (_) => json({
        'deviceName': 'Desk',
        'downloadDir': '/home/me/Downloads',
        'clipboardSyncEnabled': true,
        'closeToTray': true,
      }),
    );
    final settings = await apiWith(adapter)
        .updateSettings({'deviceName': 'Desk', 'downloadDir': null});

    final request = adapter.requests.single;
    expect(request.method, 'PATCH');
    expect(request.uri.path, '/api/v1/settings');
    expect(request.data, {'deviceName': 'Desk', 'downloadDir': null});
    expect(settings.deviceName, 'Desk');
  });

  test('streams events, announcing the connection first', () async {
    final adapter = FakeAdapter(
      (_) => ResponseBody.fromString(
        ':keepalive\n\n'
        'id: 1\nevent: clipboard.changed\n'
        'data: {"sequence":1,"timestamp":1,"type":"clipboard.changed",'
        ' "data":{}}\n\n',
        200,
        headers: {
          Headers.contentTypeHeader: ['text/event-stream'],
        },
      ),
    );
    final events = await apiWith(adapter).events().toList();
    expect(events, [isA<EventStreamConnected>(), isA<UnhandledEvent>()]);
  });
}
