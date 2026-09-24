import 'package:freezed_annotation/freezed_annotation.dart';

part 'status.freezed.dart';
part 'status.g.dart';

@freezed
abstract class LocalDevice with _$LocalDevice {
  const factory({required String deviceId, required String deviceName}) =
      _LocalDevice;

  factory fromJson(Map<String, Object?> json) => _$LocalDeviceFromJson(json);
}

/// Mirror of the daemon's `StatusSnapshot`.
@freezed
abstract class DaemonStatus with _$DaemonStatus {
  const factory({
    required String version,
    required int uptimeSeconds,
    required LocalDevice localDevice,
    required int protocolVersion,
  }) = _DaemonStatus;

  factory fromJson(Map<String, Object?> json) => _$DaemonStatusFromJson(json);
}
