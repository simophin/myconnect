import 'package:freezed_annotation/freezed_annotation.dart';

part 'pairing.freezed.dart';
part 'pairing.g.dart';

@JsonEnum(fieldRename: FieldRename.snake)
enum PairingDirection { incoming, outgoing, unknown }

@JsonEnum(fieldRename: FieldRename.snake)
enum PairingStatus {
  requested,
  awaitingConfirmation,
  accepted,
  rejected,
  expired,
  failed,
  unknown;

  bool get isTerminal => switch (this) {
    requested || awaitingConfirmation => false,
    accepted || rejected || expired || failed || unknown => true,
  };
}

/// Mirror of the daemon's `PairingSnapshot`. Timestamps are Unix
/// milliseconds.
@freezed
abstract class Pairing with _$Pairing {
  const factory({
    required String id,
    required String deviceId,
    required String deviceName,
    @JsonKey(unknownEnumValue: PairingDirection.unknown)
    required PairingDirection direction,
    @JsonKey(unknownEnumValue: PairingStatus.unknown)
    required PairingStatus status,
    required int createdAt,
    required int expiresAt,
    String? verificationCode,
    String? errorCode,
  }) = _Pairing;

  const new _();

  factory fromJson(Map<String, Object?> json) => _$PairingFromJson(json);

  /// An incoming request the local user still has to accept or reject.
  bool get needsLocalConfirmation =>
      direction == PairingDirection.incoming && !status.isTerminal;

  DateTime get expiresAtTime => DateTime.fromMillisecondsSinceEpoch(expiresAt);
}
