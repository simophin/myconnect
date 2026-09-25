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

/// The capability a peer lists when it can share its own files for
/// browsing (KDE Connect for Android does).
const browseCapability = 'kdeconnect.sftp.request';

/// The capability a peer lists when it accepts pings.
const pingCapability = 'kdeconnect.ping';

/// The capability a peer lists when it can be asked to ring so it can be
/// found (KDE Connect for Android does).
const ringCapability = 'kdeconnect.findmyphone.request';

/// The capability a peer lists when it accepts clipboard text.
const clipboardCapability = 'kdeconnect.clipboard';

/// Mirror of the daemon's `BatteryStatus`: a peer's last battery report,
/// under `plugins.battery` in its snapshot.
@freezed
abstract class BatteryStatus with _$BatteryStatus {
  const factory({required int charge, required bool charging}) = _BatteryStatus;

  factory fromJson(Map<String, Object?> json) => _$BatteryStatusFromJson(json);
}

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

    /// What the daemon's plugins add to the device, keyed by plugin id.
    /// Read through getters such as [battery].
    @Default(<String, Object?>{}) Map<String, Object?> plugins,
  }) = _Device;

  const new _();

  factory fromJson(Map<String, Object?> json) => _$DeviceFromJson(json);

  bool get isConnected => reachability == DeviceReachability.connected;

  /// The battery the device last reported. Known only while it is paired
  /// and connected, once it has reported it.
  BatteryStatus? get battery => switch (plugins['battery']) {
    final Map<String, Object?> json => _decode(json, BatteryStatus.fromJson),
    _ => null,
  };

  /// Whether a file sent now would be accepted.
  bool get acceptsFiles =>
      isConnected && incomingCapabilities.contains(shareCapability);

  /// Whether the device's own files can be browsed now.
  bool get sharesFiles =>
      paired && isConnected && incomingCapabilities.contains(browseCapability);

  /// Whether a ping sent now would be accepted.
  bool get acceptsPings =>
      isConnected && incomingCapabilities.contains(pingCapability);

  /// Whether a request to ring sent now would be accepted.
  bool get canRing =>
      isConnected && incomingCapabilities.contains(ringCapability);

  /// Whether the device takes clipboard text at all, connected or not.
  bool get supportsClipboard =>
      incomingCapabilities.contains(clipboardCapability);

  /// Whether clipboard text sent now would be accepted.
  bool get acceptsClipboard => paired && isConnected && supportsClipboard;
}

/// [json] decoded by [fromJson], or null if it doesn't fit: a plugin's
/// state is optional, so a shape from another daemon version is ignored
/// rather than failing the whole device.
T? _decode<T>(
  Map<String, Object?> json,
  T Function(Map<String, Object?>) fromJson,
) {
  try {
    return fromJson(json);
  } on Object {
    return null;
  }
}
