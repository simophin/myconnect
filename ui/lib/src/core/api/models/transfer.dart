import 'package:freezed_annotation/freezed_annotation.dart';

part 'transfer.freezed.dart';
part 'transfer.g.dart';

@JsonEnum(fieldRename: FieldRename.snake)
enum TransferDirection { incoming, outgoing, unknown }

@JsonEnum(fieldRename: FieldRename.snake)
enum TransferStatus {
  queued,
  connecting,
  transferring,
  completed,
  cancelled,
  failed,
  unknown;

  bool get isTerminal => switch (this) {
    queued || connecting || transferring => false,
    completed || cancelled || failed || unknown => true,
  };
}

/// Mirror of the daemon's `TransferSnapshot`. Timestamps are Unix
/// milliseconds.
@freezed
abstract class Transfer with _$Transfer {
  const factory({
    required String id,
    required String deviceId,
    required String deviceName,
    @JsonKey(unknownEnumValue: TransferDirection.unknown)
    required TransferDirection direction,
    @JsonKey(unknownEnumValue: TransferStatus.unknown)
    required TransferStatus status,
    required String fileName,
    required int totalBytes,
    required int transferredBytes,
    required int createdAt,
    required int updatedAt,
    String? errorCode,

    /// Where a completed incoming file was saved.
    String? savedPath,
  }) = _Transfer;

  const new _();

  factory fromJson(Map<String, Object?> json) => _$TransferFromJson(json);

  /// Fraction done, from 0 to 1. An empty file counts as done once
  /// completed.
  double get progress => totalBytes == 0
      ? (status == TransferStatus.completed ? 1 : 0)
      : transferredBytes / totalBytes;
}
