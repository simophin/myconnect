// GENERATED CODE - DO NOT MODIFY BY HAND

part of 'settings.dart';

// **************************************************************************
// JsonSerializableGenerator
// **************************************************************************

_DaemonSettings _$DaemonSettingsFromJson(Map<String, dynamic> json) =>
    _DaemonSettings(
      deviceName: json['deviceName'] as String,
      downloadDir: json['downloadDir'] as String,
      clipboardSyncEnabled: json['clipboardSyncEnabled'] as bool,
      closeToTray: json['closeToTray'] as bool,
    );

Map<String, dynamic> _$DaemonSettingsToJson(_DaemonSettings instance) =>
    <String, dynamic>{
      'deviceName': instance.deviceName,
      'downloadDir': instance.downloadDir,
      'clipboardSyncEnabled': instance.clipboardSyncEnabled,
      'closeToTray': instance.closeToTray,
    };
