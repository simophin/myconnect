import 'package:freezed_annotation/freezed_annotation.dart';

part 'settings.freezed.dart';
part 'settings.g.dart';

/// Mirror of the daemon's `SettingsSnapshot`: the settings in effect.
@freezed
abstract class DaemonSettings with _$DaemonSettings {
  const factory({
    required String deviceName,
    required String downloadDir,
    required bool clipboardSyncEnabled,

    /// Owned by the UI; the daemon only stores it.
    required bool closeToTray,
  }) = _DaemonSettings;

  factory fromJson(Map<String, Object?> json) => _$DaemonSettingsFromJson(json);
}
