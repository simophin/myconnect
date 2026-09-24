import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:logging/logging.dart';
import 'package:myconnect_ui/src/core/api/event_stream.dart';
import 'package:myconnect_ui/src/core/api/models/event.dart';
import 'package:myconnect_ui/src/core/api/models/settings.dart';
import 'package:myconnect_ui/src/core/providers.dart';

final _log = Logger('SettingsController');

/// The daemon's settings, which are where every user preference lives.
final settingsProvider =
    AsyncNotifierProvider<SettingsController, DaemonSettings>(
      SettingsController.new,
    );

/// Holds a snapshot from `GET /settings`, replaced by `settings.changed`
/// events and refetched whenever the event stream (re)connects.
class SettingsController extends AsyncNotifier<DaemonSettings> {
  @override
  Future<DaemonSettings> build() async {
    final subscription = ref
        .watch(daemonEventsProvider)
        .events
        .listen(_onEvent);
    ref.onDispose(subscription.cancel);
    final api = await ref.watch(apiProvider.future);
    return await _replay.fetch(api.settings);
  }

  final _replay = SnapshotReplay<DaemonSettings>(
    (current, event) => switch (event) {
      SettingsChanged(:final settings) => settings,
      _ => current,
    },
  );

  Future<void> refresh() async {
    try {
      final api = await ref.read(apiProvider.future);
      final settings = await _replay.fetch(api.settings);
      if (ref.mounted) state = AsyncData(settings);
    } on Object catch (error) {
      _log.warning('Settings refresh failed: $error');
    }
  }

  /// Rename this computer as peers see it. Throws `ApiException` with
  /// `invalid_device_name` if the daemon rejects the name.
  Future<void> rename(String deviceName) => _update({'deviceName': deviceName});

  /// Save received files in [directory], or in the default folder when it
  /// is `null`.
  Future<void> setDownloadDir(String? directory) =>
      _update({'downloadDir': directory});

  Future<void> setClipboardSyncEnabled({required bool enabled}) =>
      _update({'clipboardSyncEnabled': enabled});

  Future<void> setCloseToTray({required bool enabled}) =>
      _update({'closeToTray': enabled});

  Future<void> _update(Map<String, Object?> changes) async {
    final api = await ref.read(apiProvider.future);
    final settings = await api.updateSettings(changes);
    if (ref.mounted) state = AsyncData(settings);
  }

  void _onEvent(DaemonEvent event) {
    _replay.record(event);
    switch (event) {
      case EventStreamConnected():
        unawaited(refresh());
      case SettingsChanged(:final settings):
        if (state.hasValue) state = AsyncData(settings);
      case _:
        break;
    }
  }
}
