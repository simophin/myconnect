import 'package:dio/dio.dart';

/// A failed control-API call, carrying the daemon's `problem+json` code when
/// it sent one (e.g. `device_not_connected`, `pairing_in_progress`).
class ApiException implements Exception {
  const new({required this.code, this.statusCode});

  /// Map a Dio failure to a typed exception.
  factory fromDio(DioException error) {
    final response = error.response;
    if (response == null) {
      return const ApiException(code: 'daemon_unavailable');
    }
    final body = response.data;
    final code = body is Map && body['code'] is String
        ? body['code'] as String
        : 'unknown_error';
    return ApiException(code: code, statusCode: response.statusCode);
  }

  final String code;
  final int? statusCode;

  /// A sentence suitable for showing to the user.
  String get message => switch (code) {
    'daemon_unavailable' => 'MyConnect is not responding.',
    'unauthorized' => 'MyConnect rejected this app’s access token.',
    'device_not_found' => 'That device is no longer known.',
    'device_not_connected' => 'The device is not connected right now.',
    'already_paired' => 'The device is already paired.',
    'pairing_in_progress' => 'A pairing with this device is already running.',
    'pairing_not_found' => 'That pairing request no longer exists.',
    'invalid_pairing_state' ||
    'invalid_pairing_direction' => 'That pairing request is no longer active.',
    _ => 'Something went wrong ($code).',
  };

  @override
  String toString() => 'ApiException($statusCode, $code)';
}
