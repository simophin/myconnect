// GENERATED CODE - DO NOT MODIFY BY HAND

part of 'settings.dart';

// **************************************************************************
// JsonSerializableGenerator
// **************************************************************************

_DaemonSettings _$DaemonSettingsFromJson(Map<String, dynamic> json) =>
    _DaemonSettings(
      deviceName: json['deviceName'] as String,
      downloadDir: json['downloadDir'] as String,
      closeToTray: json['closeToTray'] as bool,
      plugins:
          json['plugins'] as Map<String, dynamic>? ?? const <String, Object?>{},
    );

Map<String, dynamic> _$DaemonSettingsToJson(_DaemonSettings instance) =>
    <String, dynamic>{
      'deviceName': instance.deviceName,
      'downloadDir': instance.downloadDir,
      'closeToTray': instance.closeToTray,
      'plugins': instance.plugins,
    };
