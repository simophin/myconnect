import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:myconnect_ui/src/core/api/models/device.dart';
import 'package:myconnect_ui/src/core/providers.dart';
import 'package:myconnect_ui/src/features/devices/devices_controller.dart';
import 'package:myconnect_ui/src/shared/widgets.dart';

/// Home screen: the devices this computer is paired with.
class DevicesPage extends ConsumerWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final devices = ref.watch(pairedDevicesProvider);
    final localName = ref.watch(daemonStatusProvider).value?.localDevice;
    return Scaffold(
      appBar: AppBar(
        title: const Text('Devices'),
        actions: [
          IconButton(
            tooltip: 'Transfers',
            icon: const Icon(Icons.swap_vert),
            onPressed: () => context.go('/transfers'),
          ),
        ],
        bottom: localName == null
            ? null
            : PreferredSize(
                preferredSize: const Size.fromHeight(20),
                child: Padding(
                  padding: const EdgeInsets.only(left: 16, bottom: 8),
                  child: Align(
                    alignment: Alignment.centerLeft,
                    child: Text(
                      'This computer: ${localName.deviceName}',
                      style: Theme.of(context).textTheme.bodySmall,
                    ),
                  ),
                ),
              ),
      ),
      floatingActionButton: FloatingActionButton.extended(
        onPressed: () => context.go('/add'),
        icon: const Icon(Icons.add),
        label: const Text('Add device'),
      ),
      body: switch (devices) {
        AsyncData(value: final devices) when devices.isEmpty =>
          const _NoDevices(),
        AsyncData(value: final devices) => ListView(
          padding: const EdgeInsets.only(bottom: 88),
          children: [for (final device in devices) _DeviceTile(device)],
        ),
        AsyncError(:final error) => ErrorView(
          error: error,
          onRetry: () => ref.invalidate(devicesProvider),
        ),
        _ => const Center(child: CircularProgressIndicator()),
      },
    );
  }
}

class _DeviceTile extends StatelessWidget {
  const new(this.device);

  final Device device;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    return ListTile(
      leading: Icon(
        deviceIcon(device.deviceType),
        color: device.isConnected ? colors.primary : colors.outline,
      ),
      title: Text(device.deviceName),
      subtitle: Text(reachabilityLabel(device)),
      trailing: const Icon(Icons.chevron_right),
      onTap: () => context.go('/devices/${device.deviceId}'),
    );
  }
}

class _NoDevices extends StatelessWidget {
  const new();

  @override
  Widget build(BuildContext context) => Center(
    child: Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        Icon(
          Icons.devices_outlined,
          size: 64,
          color: Theme.of(context).colorScheme.outline,
        ),
        const SizedBox(height: 16),
        const Text('No paired devices yet'),
        const SizedBox(height: 8),
        TextButton(
          onPressed: () => context.go('/add'),
          child: const Text('Find a device to pair'),
        ),
      ],
    ),
  );
}
