import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:myconnect_ui/src/core/api/models/device.dart';
import 'package:myconnect_ui/src/features/devices/devices_controller.dart';
import 'package:myconnect_ui/src/shared/widgets.dart';

class DeviceDetailPage extends ConsumerWidget {
  const new({required this.deviceId, super.key});

  final String deviceId;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final device = ref.watch(deviceProvider(deviceId));
    return Scaffold(
      appBar: AppBar(title: Text(device?.deviceName ?? 'Device')),
      body: device == null
          ? const Center(child: Text('This device is no longer known.'))
          : _DeviceDetails(device),
    );
  }
}

class _DeviceDetails extends ConsumerStatefulWidget {
  const new(this.device);

  final Device device;

  @override
  ConsumerState<_DeviceDetails> createState() => _DeviceDetailsState();
}

class _DeviceDetailsState extends ConsumerState<_DeviceDetails> {
  bool _busy = false;

  Future<void> _unpair() async {
    final device = widget.device;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text('Unpair ${device.deviceName}?'),
        content: const Text(
          'The device will need to be paired again before it can exchange '
          'anything with this computer.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: const Text('Unpair'),
          ),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;
    // Forgetting removes the device from state, which unmounts this widget
    // before the call returns, so navigate through a router captured now.
    final router = GoRouter.of(context);
    setState(() => _busy = true);
    try {
      await ref.read(devicesProvider.notifier).forget(device.deviceId);
      router.go('/');
    } on Object catch (error) {
      if (mounted) showErrorSnackBar(context, error);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final device = widget.device;
    final theme = Theme.of(context);
    return ListView(
      padding: const EdgeInsets.all(16),
      children: [
        ListTile(
          leading: Icon(deviceIcon(device.deviceType), size: 40),
          title: Text(device.deviceName, style: theme.textTheme.titleLarge),
          subtitle: Text(reachabilityLabel(device)),
        ),
        const Divider(),
        _Fact('Device ID', device.deviceId),
        _Fact('Type', device.deviceType.name),
        _Fact('Protocol version', '${device.protocolVersion}'),
        const SizedBox(height: 24),
        Align(
          alignment: Alignment.centerLeft,
          child: OutlinedButton.icon(
            onPressed: _busy ? null : _unpair,
            style: OutlinedButton.styleFrom(
              foregroundColor: theme.colorScheme.error,
            ),
            icon: const Icon(Icons.link_off),
            label: const Text('Unpair'),
          ),
        ),
      ],
    );
  }
}

class _Fact extends StatelessWidget {
  const new(this.label, this.value);

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) => ListTile(
    dense: true,
    title: Text(label),
    subtitle: SelectableText(value),
  );
}
