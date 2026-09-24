import 'dart:convert';
import 'dart:io';

import 'package:dio/dio.dart';
import 'package:flutter/foundation.dart';
import 'package:myconnect_ui/src/core/api/api_exception.dart';
import 'package:myconnect_ui/src/core/api/models/device.dart';
import 'package:myconnect_ui/src/core/api/models/event.dart';
import 'package:myconnect_ui/src/core/api/models/pairing.dart';
import 'package:myconnect_ui/src/core/api/models/settings.dart';
import 'package:myconnect_ui/src/core/api/models/status.dart';
import 'package:myconnect_ui/src/core/api/models/transfer.dart';
import 'package:myconnect_ui/src/core/api/sse.dart';
import 'package:myconnect_ui/src/core/daemon/daemon_host.dart';

typedef _Json = Map<String, Object?>;

/// Typed client for the daemon's `/api/v1` control API — the only way the
/// UI reads or changes state. Every failure surfaces as [ApiException].
class MyConnectApi {
  new(this._dio);

  factory forEndpoint(DaemonEndpoint endpoint) => MyConnectApi(
    Dio(
      BaseOptions(
        baseUrl: endpoint.baseUrl.resolve('/api/v1/').toString(),
        headers: {
          if (endpoint.token case final token?)
            'Authorization': 'Bearer $token',
        },
        connectTimeout: const Duration(seconds: 3),
        // The daemon's own request deadline is 15 seconds.
        receiveTimeout: const Duration(seconds: 20),
      ),
    ),
  );

  final Dio _dio;

  /// The underlying client, for tests that substitute its transport.
  @visibleForTesting
  Dio get debugDio => _dio;

  Future<DaemonStatus> status() async =>
      DaemonStatus.fromJson(await _get<_Json>('status'));

  Future<List<Device>> devices() async =>
      (await _get<List<Object?>>('devices'))
          .map((json) => Device.fromJson(json! as _Json))
          .toList();

  /// Broadcast a discovery request so nearby devices answer promptly. With
  /// an [address], announce to that IPv4 address only, for networks where
  /// broadcast doesn't reach the device.
  Future<void> scan({String? address}) => _send(
    () => _dio.post<void>(
      'discovery',
      data: address == null ? null : {'address': address},
    ),
  );

  /// Unpair and forget a device.
  Future<void> forgetDevice(String deviceId) => _send(
    () => _dio.delete<void>('devices/${Uri.encodeComponent(deviceId)}'),
  );

  /// Ping a paired, connected device that accepts pings.
  Future<void> ping(String deviceId) => _send(
    () => _dio.post<void>('devices/${Uri.encodeComponent(deviceId)}/ping'),
  );

  Future<List<Pairing>> pairings() async =>
      (await _get<List<Object?>>('pairings'))
          .map((json) => Pairing.fromJson(json! as _Json))
          .toList();

  Future<Pairing> startPairing(String deviceId) async => Pairing.fromJson(
    await _send(
      () => _dio.post<_Json>('pairings', data: {'deviceId': deviceId}),
    ),
  );

  Future<Pairing> acceptPairing(String pairingId) async => Pairing.fromJson(
    await _send(() => _dio.post<_Json>('pairings/$pairingId/accept')),
  );

  /// Reject an incoming request or cancel an outgoing one.
  Future<Pairing> rejectPairing(String pairingId) async => Pairing.fromJson(
    await _send(() => _dio.delete<_Json>('pairings/$pairingId')),
  );

  Future<List<Transfer>> transfers() async =>
      (await _get<List<Object?>>('transfers'))
          .map((json) => Transfer.fromJson(json! as _Json))
          .toList();

  /// Send the file at [path] to a paired, connected device.
  ///
  /// The daemon answers only once the whole file has been streamed to the
  /// peer, so this completes at the end of the upload; follow progress
  /// through `transfer.*` events meanwhile.
  Future<Transfer> sendFile(String deviceId, String path) async {
    final file = File(path);
    final length = await file.length();
    // The daemon reads `deviceId` before the file, and takes the file's
    // declared size from its part's Content-Length header.
    final form = FormData()
      ..fields.add(MapEntry('deviceId', deviceId))
      ..files.add(
        MapEntry(
          'file',
          await MultipartFile.fromFile(
            path,
            filename: file.uri.pathSegments.last,
            headers: {
              'content-length': ['$length'],
            },
          ),
        ),
      );
    return Transfer.fromJson(
      await _send(
        () => _dio.post<_Json>(
          'transfers',
          data: form,
          // The daemon fails an upload that stalls, but a large one may
          // legitimately take far longer than the default deadline.
          options: Options(receiveTimeout: Duration.zero),
        ),
      ),
    );
  }

  Future<Transfer> cancelTransfer(String transferId) async => Transfer.fromJson(
    await _send(() => _dio.delete<_Json>('transfers/$transferId')),
  );

  Future<DaemonSettings> settings() async =>
      DaemonSettings.fromJson(await _get<_Json>('settings'));

  /// Change the settings named in [changes], a JSON merge patch keyed by
  /// [DaemonSettings] field names: absent fields stay as they are, and a
  /// `null` value resets one to its default.
  Future<DaemonSettings> updateSettings(Map<String, Object?> changes) async =>
      DaemonSettings.fromJson(
        await _send(() => _dio.patch<_Json>('settings', data: changes)),
      );

  /// One connection to `/events`, starting with [EventStreamConnected] once
  /// the daemon accepts it. The stream ends (or errors) when the connection
  /// drops; reconnecting is the caller's job.
  Stream<DaemonEvent> events() async* {
    final response = await _send(
      () => _dio.get<ResponseBody>(
        'events',
        options: Options(
          responseType: ResponseType.stream,
          // The daemon sends a keepalive every 15 seconds; a longer silence
          // means the connection is dead.
          receiveTimeout: const Duration(seconds: 40),
        ),
      ),
    );
    yield const EventStreamConnected();
    yield* response.stream
        .transform(sseDataTransformer<Uint8List>())
        .map((data) => DaemonEvent.fromJson(jsonDecode(data) as _Json));
  }

  Future<T> _get<T>(String path) => _send(() => _dio.get<T>(path));

  Future<T> _send<T>(Future<Response<T>> Function() request) async {
    try {
      return (await request()).data as T;
    } on DioException catch (error) {
      throw ApiException.fromDio(error);
    }
  }
}
