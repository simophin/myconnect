// GENERATED CODE - DO NOT MODIFY BY HAND

part of 'transfer.dart';

// **************************************************************************
// JsonSerializableGenerator
// **************************************************************************

_Transfer _$TransferFromJson(Map<String, dynamic> json) => _Transfer(
  id: json['id'] as String,
  deviceId: json['deviceId'] as String,
  deviceName: json['deviceName'] as String,
  direction: $enumDecode(
    _$TransferDirectionEnumMap,
    json['direction'],
    unknownValue: TransferDirection.unknown,
  ),
  status: $enumDecode(
    _$TransferStatusEnumMap,
    json['status'],
    unknownValue: TransferStatus.unknown,
  ),
  fileName: json['fileName'] as String,
  totalBytes: (json['totalBytes'] as num).toInt(),
  transferredBytes: (json['transferredBytes'] as num).toInt(),
  createdAt: (json['createdAt'] as num).toInt(),
  updatedAt: (json['updatedAt'] as num).toInt(),
  errorCode: json['errorCode'] as String?,
  savedPath: json['savedPath'] as String?,
);

Map<String, dynamic> _$TransferToJson(_Transfer instance) => <String, dynamic>{
  'id': instance.id,
  'deviceId': instance.deviceId,
  'deviceName': instance.deviceName,
  'direction': _$TransferDirectionEnumMap[instance.direction]!,
  'status': _$TransferStatusEnumMap[instance.status]!,
  'fileName': instance.fileName,
  'totalBytes': instance.totalBytes,
  'transferredBytes': instance.transferredBytes,
  'createdAt': instance.createdAt,
  'updatedAt': instance.updatedAt,
  'errorCode': instance.errorCode,
  'savedPath': instance.savedPath,
};

const _$TransferDirectionEnumMap = {
  TransferDirection.incoming: 'incoming',
  TransferDirection.outgoing: 'outgoing',
  TransferDirection.unknown: 'unknown',
};

const _$TransferStatusEnumMap = {
  TransferStatus.queued: 'queued',
  TransferStatus.connecting: 'connecting',
  TransferStatus.transferring: 'transferring',
  TransferStatus.completed: 'completed',
  TransferStatus.cancelled: 'cancelled',
  TransferStatus.failed: 'failed',
  TransferStatus.unknown: 'unknown',
};
