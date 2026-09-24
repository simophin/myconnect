// GENERATED CODE - DO NOT MODIFY BY HAND

part of 'pairing.dart';

// **************************************************************************
// JsonSerializableGenerator
// **************************************************************************

_Pairing _$PairingFromJson(Map<String, dynamic> json) => _Pairing(
  id: json['id'] as String,
  deviceId: json['deviceId'] as String,
  deviceName: json['deviceName'] as String,
  direction: $enumDecode(
    _$PairingDirectionEnumMap,
    json['direction'],
    unknownValue: PairingDirection.unknown,
  ),
  status: $enumDecode(
    _$PairingStatusEnumMap,
    json['status'],
    unknownValue: PairingStatus.unknown,
  ),
  createdAt: (json['createdAt'] as num).toInt(),
  expiresAt: (json['expiresAt'] as num).toInt(),
  verificationCode: json['verificationCode'] as String?,
  errorCode: json['errorCode'] as String?,
);

Map<String, dynamic> _$PairingToJson(_Pairing instance) => <String, dynamic>{
  'id': instance.id,
  'deviceId': instance.deviceId,
  'deviceName': instance.deviceName,
  'direction': _$PairingDirectionEnumMap[instance.direction]!,
  'status': _$PairingStatusEnumMap[instance.status]!,
  'createdAt': instance.createdAt,
  'expiresAt': instance.expiresAt,
  'verificationCode': instance.verificationCode,
  'errorCode': instance.errorCode,
};

const _$PairingDirectionEnumMap = {
  PairingDirection.incoming: 'incoming',
  PairingDirection.outgoing: 'outgoing',
  PairingDirection.unknown: 'unknown',
};

const _$PairingStatusEnumMap = {
  PairingStatus.requested: 'requested',
  PairingStatus.awaitingConfirmation: 'awaiting_confirmation',
  PairingStatus.accepted: 'accepted',
  PairingStatus.rejected: 'rejected',
  PairingStatus.expired: 'expired',
  PairingStatus.failed: 'failed',
  PairingStatus.unknown: 'unknown',
};
