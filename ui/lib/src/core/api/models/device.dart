import 'package:freezed_annotation/freezed_annotation.dart';

part 'device.freezed.dart';
part 'device.g.dart';

/// Icon category a peer advertises. Unrecognized values from a newer daemon
/// map to [unknown] instead of failing to decode.
@JsonEnum()
enum DeviceType { desktop, laptop, phone, tablet, tv, unknown }

@JsonEnum(fieldRename: FieldRename.snake)
enum DeviceReachability { discovered, connected, unavailable, unknown }

/// The capability a peer lists when it accepts files.
const shareCapability = 'kdeconnect.share.request';

/// Mirror of the daemon's `DeviceSnapshot`.
@freezed
abstract class Device with _$Device {
  const factory({
    required String deviceId,
    required String deviceName,
    @JsonKey(unknownEnumValue: DeviceType.unknown)
    required DeviceType deviceType,
    required int protocolVersion,
    required List<String> incomingCapabilities,
    required List<String> outgoingCapabilities,
    @JsonKey(unknownEnumValue: DeviceReachability.unknown)
    required DeviceReachability reachability,
    required bool paired,
    required bool pairing,
    required int lastSeenAt,
  }) = _Device;

  const new _();

  factory fromJson(Map<String, Object?> json) => _$DeviceFromJson(json);

  bool get isConnected => reachability == DeviceReachability.connected;

  /// Whether a file sent now would be accepted.
  bool get acceptsFiles =>
      isConnected && incomingCapabilities.contains(shareCapability);
}
