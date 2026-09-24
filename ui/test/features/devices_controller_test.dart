import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:myconnect_ui/src/core/api/models/device.dart';
import 'package:myconnect_ui/src/core/api/models/event.dart';
import 'package:myconnect_ui/src/features/devices/devices_controller.dart';

import '../helpers.dart';

void main() {
  late TestDaemon daemon;
  late ProviderContainer container;

  setUp(() {
    daemon = TestDaemon()
      ..devices = [
        device(id: 'b' * 32, name: 'Tablet'),
        device(id: 'a' * 32, name: 'phone', paired: false),
      ];
    container = ProviderContainer.test(overrides: daemon.overrides);
  });

  test('loads a sorted snapshot and splits paired from unpaired', () async {
    final devices = await container.read(devicesProvider.future);
    expect(devices.map((d) => d.deviceName), ['phone', 'Tablet']);
    container.listen(pairedDevicesProvider, (_, _) {});
    expect(
      container.read(pairedDevicesProvider).value!.single.deviceName,
      'Tablet',
    );
    expect(
      container.read(unpairedDevicesProvider).value!.single.deviceName,
      'phone',
    );
  });

  test('applies device events and removes forgotten devices', () async {
    await container.read(devicesProvider.future);

    await daemon.emit(
      DeviceChanged(
        device(
          id: 'b' * 32,
          name: 'Tablet',
          reachability: DeviceReachability.unavailable,
        ),
      ),
    );
    await daemon.emit(DeviceChanged(device(id: 'c' * 32, name: 'Laptop')));
    expect(
      container.read(deviceProvider('b' * 32))!.reachability,
      DeviceReachability.unavailable,
    );
    expect(container.read(devicesProvider).value, hasLength(3));

    await daemon.emit(DeviceForgotten(device(id: 'b' * 32)));
    expect(container.read(deviceProvider('b' * 32)), isNull);
  });

  test('refetches the snapshot when the event stream reconnects', () async {
    await container.read(devicesProvider.future);
    daemon.devices = [device(name: 'Only one')];

    await daemon.emit(const EventStreamConnected());
    await pumpEventQueue();

    expect(
      container.read(devicesProvider).value!.single.deviceName,
      'Only one',
    );
    verify(daemon.api.devices).called(2);
  });

  test(
    'forgetting a device removes it without waiting for the event',
    () async {
      when(() => daemon.api.forgetDevice(any())).thenAnswer((_) async {});
      await container.read(devicesProvider.future);

      await container.read(devicesProvider.notifier).forget('b' * 32);

      verify(() => daemon.api.forgetDevice('b' * 32)).called(1);
      expect(container.read(deviceProvider('b' * 32)), isNull);
    },
  );
}
