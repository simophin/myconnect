import 'package:file_selector/file_selector.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:myconnect_ui/src/core/api/models/device.dart';
import 'package:myconnect_ui/src/core/providers.dart';
import 'package:myconnect_ui/src/features/devices/devices_controller.dart';
import 'package:myconnect_ui/src/features/transfers/transfer_tile.dart';
import 'package:myconnect_ui/src/features/transfers/transfers_controller.dart';
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

/// The capability a peer lists when it accepts files.
const _shareCapability = 'kdeconnect.share.request';

/// The capability a peer lists when it accepts pings.
const _pingCapability = 'kdeconnect.ping';

/// How many of this device's transfers the page lists.
const _recentTransfers = 5;

class _DeviceDetailsState extends ConsumerState<_DeviceDetails> {
  bool _busy = false;

  Future<void> _sendFile() async {
    final file = await openFile(confirmButtonText: 'Send');
    if (file == null || !mounted) return;
    // The upload outlives this page if the user navigates away, or if the
    // device drops and unmounts it, so report errors through a messenger
    // captured now.
    final messenger = ScaffoldMessenger.of(context);
    try {
      await ref
          .read(transfersProvider.notifier)
          .send(widget.device.deviceId, file.path);
    } on Object catch (error) {
      messenger.showSnackBar(SnackBar(content: Text(describeError(error))));
    }
  }

  Future<void> _ping() async {
    // The device can drop, and unmount this page, before the call returns.
    final messenger = ScaffoldMessenger.of(context);
    final device = widget.device;
    try {
      await (await ref.read(apiProvider.future)).ping(device.deviceId);
      messenger.showSnackBar(
        SnackBar(content: Text('Pinged ${device.deviceName}.')),
      );
    } on Object catch (error) {
      messenger.showSnackBar(SnackBar(content: Text(describeError(error))));
    }
  }

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
    final canSend =
        device.isConnected &&
        device.incomingCapabilities.contains(_shareCapability);
    final canPing =
        device.isConnected &&
        device.incomingCapabilities.contains(_pingCapability);
    final transfers = ref
        .watch(transferListProvider(device.deviceId))
        .take(_recentTransfers)
        .toList();
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
        const SizedBox(height: 16),
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: [
            FilledButton.icon(
              onPressed: canSend ? _sendFile : null,
              icon: const Icon(Icons.upload_file),
              label: const Text('Send file'),
            ),
            FilledButton.tonalIcon(
              onPressed: canPing ? _ping : null,
              icon: const Icon(Icons.notifications_active_outlined),
              label: const Text('Ping'),
            ),
          ],
        ),
        if (transfers.isNotEmpty) ...[
          const SizedBox(height: 16),
          ListTile(
            title: Text('Recent transfers', style: theme.textTheme.titleSmall),
            trailing: TextButton(
              onPressed: () => context.go('/transfers'),
              child: const Text('See all'),
            ),
          ),
          for (final transfer in transfers)
            TransferTile(
              transfer,
              showDevice: false,
              key: ValueKey(transfer.id),
            ),
        ],
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
