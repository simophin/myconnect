import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:logging/logging.dart';
import 'package:myconnect_ui/src/core/api/event_stream.dart';
import 'package:myconnect_ui/src/core/api/models/device.dart';
import 'package:myconnect_ui/src/core/api/models/event.dart';
import 'package:myconnect_ui/src/core/providers.dart';

final _log = Logger('DevicesController');

/// Every device the daemon knows about, kept current from `/events`.
final devicesProvider = AsyncNotifierProvider<DevicesController, List<Device>>(
  DevicesController.new,
);

/// Devices the user has paired, for the home list.
final pairedDevicesProvider = Provider<AsyncValue<List<Device>>>(
  (ref) => ref
      .watch(devicesProvider)
      .whenData((devices) => devices.where((d) => d.paired).toList()),
);

/// Nearby devices that could be paired, for the add-device flow.
final unpairedDevicesProvider = Provider<AsyncValue<List<Device>>>(
  (ref) => ref
      .watch(devicesProvider)
      .whenData((devices) => devices.where((d) => !d.paired).toList()),
);

final deviceProvider = Provider.family<Device?, String>(
  (ref, deviceId) => ref
      .watch(devicesProvider)
      .value
      ?.where((device) => device.deviceId == deviceId)
      .firstOrNull,
);

/// Holds a snapshot from `GET /devices`, patched by device events and
/// refetched whenever the event stream (re)connects. It holds no state of its
/// own beyond that cache.
class DevicesController extends AsyncNotifier<List<Device>> {
  @override
  Future<List<Device>> build() async {
    // Subscribe before fetching so no event between the two is lost.
    final subscription = ref
        .watch(daemonEventsProvider)
        .events
        .listen(_onEvent);
    ref.onDispose(subscription.cancel);
    final api = await ref.watch(apiProvider.future);
    return await _replay.fetch(() async => _sorted(await api.devices()));
  }

  final _replay = SnapshotReplay<List<Device>>(_applied);

  /// Refetch the snapshot. A failure keeps the current list, since the
  /// connection indicator already reports an unreachable daemon.
  Future<void> refresh() async {
    try {
      final api = await ref.read(apiProvider.future);
      final devices = await _replay.fetch(
        () async => _sorted(await api.devices()),
      );
      if (ref.mounted) state = AsyncData(devices);
    } on Object catch (error) {
      _log.warning('Device refresh failed: $error');
    }
  }

  /// Ask nearby devices to announce themselves, or only the one at
  /// [address] when broadcast discovery can't reach it.
  Future<void> scan({String? address}) async =>
      await (await ref.read(apiProvider.future)).scan(address: address);

  /// Unpair and forget a device.
  Future<void> forget(String deviceId) async {
    await (await ref.read(apiProvider.future)).forgetDevice(deviceId);
    _remove(deviceId);
  }

  void _onEvent(DaemonEvent event) {
    _replay.record(event);
    switch (event) {
      case EventStreamConnected():
        unawaited(refresh());
      case DeviceChanged() || DeviceForgotten():
        if (state.value case final current?) {
          state = AsyncData(_applied(current, event));
        }
      case _:
        break;
    }
  }

  void _remove(String deviceId) {
    final current = state.value;
    if (current == null) return;
    state = AsyncData(_without(current, deviceId));
  }

  static List<Device> _applied(List<Device> devices, DaemonEvent event) =>
      switch (event) {
        DeviceChanged(:final device) => _sorted([
          ..._without(devices, device.deviceId),
          device,
        ]),
        DeviceForgotten(:final device) => _without(devices, device.deviceId),
        _ => devices,
      };

  static List<Device> _without(List<Device> devices, String deviceId) => [
    for (final device in devices)
      if (device.deviceId != deviceId) device,
  ];

  static List<Device> _sorted(List<Device> devices) => [...devices]
    ..sort(
      (a, b) =>
          a.deviceName.toLowerCase().compareTo(b.deviceName.toLowerCase()),
    );
}
