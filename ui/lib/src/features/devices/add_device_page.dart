import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:myconnect_ui/src/core/api/models/device.dart';
import 'package:myconnect_ui/src/features/devices/devices_controller.dart';
import 'package:myconnect_ui/src/features/pairing/pairings_controller.dart';
import 'package:myconnect_ui/src/shared/widgets.dart';

/// Scan for nearby, unpaired devices and start pairing with one.
class AddDevicePage extends ConsumerStatefulWidget {
  const new({super.key});

  /// How long the "searching" indicator stays up after a scan; devices keep
  /// appearing afterwards through events regardless.
  static const scanIndicatorDuration = Duration(seconds: 4);

  @override
  ConsumerState<AddDevicePage> createState() => _AddDevicePageState();
}

class _AddDevicePageState extends ConsumerState<AddDevicePage> {
  Timer? _scanTimer;
  String? _startingWith;

  bool get _scanning => _scanTimer?.isActive ?? false;

  @override
  void initState() {
    super.initState();
    unawaited(_scan());
  }

  @override
  void dispose() {
    _scanTimer?.cancel();
    super.dispose();
  }

  Future<void> _scan() async {
    setState(() {
      _scanTimer?.cancel();
      _scanTimer = Timer(AddDevicePage.scanIndicatorDuration, () {
        if (mounted) setState(() {});
      });
    });
    try {
      await ref.read(devicesProvider.notifier).scan();
    } on Object catch (error) {
      if (mounted) showErrorSnackBar(context, error);
    }
  }

  Future<void> _pair(Device device) async {
    setState(() => _startingWith = device.deviceId);
    try {
      final pairing = await ref
          .read(pairingsProvider.notifier)
          .start(device.deviceId);
      if (mounted) context.go('/add/pairing/${pairing.id}');
    } on Object catch (error) {
      if (mounted) showErrorSnackBar(context, error);
    } finally {
      if (mounted) setState(() => _startingWith = null);
    }
  }

  @override
  Widget build(BuildContext context) {
    final devices = ref.watch(unpairedDevicesProvider);
    return Scaffold(
      appBar: AppBar(
        title: const Text('Add device'),
        actions: [
          IconButton(
            tooltip: 'Scan again',
            onPressed: _scanning ? null : _scan,
            icon: const Icon(Icons.refresh),
          ),
        ],
        bottom: _scanning
            ? const PreferredSize(
                preferredSize: Size.fromHeight(4),
                child: LinearProgressIndicator(),
              )
            : null,
      ),
      body: switch (devices) {
        AsyncData(value: final devices) => ListView(
          children: [
            const Padding(
              padding: EdgeInsets.fromLTRB(16, 16, 16, 8),
              child: Text(
                'Open MyConnect or KDE Connect on the other device and make '
                'sure both are on the same network.',
              ),
            ),
            if (devices.isEmpty && !_scanning)
              const ListTile(
                leading: Icon(Icons.search_off),
                title: Text('No devices found'),
              ),
            for (final device in devices)
              _CandidateTile(
                device: device,
                starting: _startingWith == device.deviceId,
                onPair: _startingWith == null ? () => _pair(device) : null,
              ),
          ],
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

class _CandidateTile extends StatelessWidget {
  const new({
    required this.device,
    required this.starting,
    required this.onPair,
  });

  final Device device;
  final bool starting;
  final VoidCallback? onPair;

  @override
  Widget build(BuildContext context) {
    final blocker = switch (device) {
      Device(pairing: true) => 'Pairing in progress',
      Device(isConnected: false) => 'Not connected',
      _ => null,
    };
    return ListTile(
      leading: Icon(deviceIcon(device.deviceType)),
      title: Text(device.deviceName),
      subtitle: Text(blocker ?? reachabilityLabel(device)),
      trailing: starting
          ? const SizedBox.square(
              dimension: 24,
              child: CircularProgressIndicator(strokeWidth: 2),
            )
          : FilledButton.tonal(
              onPressed: blocker == null ? onPair : null,
              child: const Text('Pair'),
            ),
    );
  }
}
