// GENERATED CODE - DO NOT MODIFY BY HAND

part of 'status.dart';

// **************************************************************************
// JsonSerializableGenerator
// **************************************************************************

_LocalDevice _$LocalDeviceFromJson(Map<String, dynamic> json) => _LocalDevice(
  deviceId: json['deviceId'] as String,
  deviceName: json['deviceName'] as String,
);

Map<String, dynamic> _$LocalDeviceToJson(_LocalDevice instance) =>
    <String, dynamic>{
      'deviceId': instance.deviceId,
      'deviceName': instance.deviceName,
    };

_DaemonStatus _$DaemonStatusFromJson(Map<String, dynamic> json) =>
    _DaemonStatus(
      version: json['version'] as String,
      uptimeSeconds: (json['uptimeSeconds'] as num).toInt(),
      localDevice: LocalDevice.fromJson(
        json['localDevice'] as Map<String, dynamic>,
      ),
      protocolVersion: (json['protocolVersion'] as num).toInt(),
    );

Map<String, dynamic> _$DaemonStatusToJson(_DaemonStatus instance) =>
    <String, dynamic>{
      'version': instance.version,
      'uptimeSeconds': instance.uptimeSeconds,
      'localDevice': instance.localDevice,
      'protocolVersion': instance.protocolVersion,
    };
