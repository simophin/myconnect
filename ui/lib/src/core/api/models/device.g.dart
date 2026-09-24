// GENERATED CODE - DO NOT MODIFY BY HAND

part of 'device.dart';

// **************************************************************************
// JsonSerializableGenerator
// **************************************************************************

_Device _$DeviceFromJson(Map<String, dynamic> json) => _Device(
  deviceId: json['deviceId'] as String,
  deviceName: json['deviceName'] as String,
  deviceType: $enumDecode(
    _$DeviceTypeEnumMap,
    json['deviceType'],
    unknownValue: DeviceType.unknown,
  ),
  protocolVersion: (json['protocolVersion'] as num).toInt(),
  incomingCapabilities: (json['incomingCapabilities'] as List<dynamic>)
      .map((e) => e as String)
      .toList(),
  outgoingCapabilities: (json['outgoingCapabilities'] as List<dynamic>)
      .map((e) => e as String)
      .toList(),
  reachability: $enumDecode(
    _$DeviceReachabilityEnumMap,
    json['reachability'],
    unknownValue: DeviceReachability.unknown,
  ),
  paired: json['paired'] as bool,
  pairing: json['pairing'] as bool,
  lastSeenAt: (json['lastSeenAt'] as num).toInt(),
);

Map<String, dynamic> _$DeviceToJson(_Device instance) => <String, dynamic>{
  'deviceId': instance.deviceId,
  'deviceName': instance.deviceName,
  'deviceType': _$DeviceTypeEnumMap[instance.deviceType]!,
  'protocolVersion': instance.protocolVersion,
  'incomingCapabilities': instance.incomingCapabilities,
  'outgoingCapabilities': instance.outgoingCapabilities,
  'reachability': _$DeviceReachabilityEnumMap[instance.reachability]!,
  'paired': instance.paired,
  'pairing': instance.pairing,
  'lastSeenAt': instance.lastSeenAt,
};

const _$DeviceTypeEnumMap = {
  DeviceType.desktop: 'desktop',
  DeviceType.laptop: 'laptop',
  DeviceType.phone: 'phone',
  DeviceType.tablet: 'tablet',
  DeviceType.tv: 'tv',
  DeviceType.unknown: 'unknown',
};

const _$DeviceReachabilityEnumMap = {
  DeviceReachability.discovered: 'discovered',
  DeviceReachability.connected: 'connected',
  DeviceReachability.unavailable: 'unavailable',
  DeviceReachability.unknown: 'unknown',
};
