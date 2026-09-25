import 'package:freezed_annotation/freezed_annotation.dart';

part 'settings.freezed.dart';
part 'settings.g.dart';

/// Mirror of the daemon's `SettingsSnapshot`: the settings in effect.
@freezed
abstract class DaemonSettings with _$DaemonSettings {
  const factory({
    required String deviceName,
    required String downloadDir,

    /// Owned by the UI; the daemon only stores it.
    required bool closeToTray,

    /// The daemon's plugins' settings, keyed by plugin id. Read through
    /// getters such as [clipboardSyncEnabled].
    @Default(<String, Object?>{}) Map<String, Object?> plugins,
  }) = _DaemonSettings;

  const new _();

  factory fromJson(Map<String, Object?> json) => _$DaemonSettingsFromJson(json);

  /// Whether the clipboard is synced with paired devices
  /// (`plugins.clipboard.syncEnabled`, on unless turned off).
  bool get clipboardSyncEnabled => switch (plugins['clipboard']) {
    {'syncEnabled': final bool enabled} => enabled,
    _ => true,
  };

  /// A `PATCH /settings` body that turns clipboard sync on or off.
  static Map<String, Object?> clipboardSyncPatch({required bool enabled}) => {
    'plugins': {
      'clipboard': {'syncEnabled': enabled},
    },
  };
}
