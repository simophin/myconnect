import 'dart:async';
import 'dart:ui';

import 'package:file_selector/file_selector.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:logging/logging.dart';
import 'package:material_ui/material_ui.dart';
import 'package:myconnect_ui/src/core/api/models/device.dart';
import 'package:myconnect_ui/src/core/api/models/event.dart';
import 'package:myconnect_ui/src/core/api/models/pairing.dart';
import 'package:myconnect_ui/src/core/api/models/transfer.dart';
import 'package:myconnect_ui/src/core/desktop/desktop_shell.dart';
import 'package:myconnect_ui/src/core/providers.dart';
import 'package:myconnect_ui/src/core/routing/router.dart';
import 'package:myconnect_ui/src/features/devices/devices_controller.dart';
import 'package:myconnect_ui/src/features/pairing/pairings_controller.dart';
import 'package:myconnect_ui/src/features/send/send_files.dart';
import 'package:myconnect_ui/src/features/settings/settings_controller.dart';
import 'package:myconnect_ui/src/features/transfers/transfers_controller.dart';
import 'package:myconnect_ui/src/shared/widgets.dart';

final _log = Logger('BackgroundHost');

/// Keeps the app, and with it the embedded daemon, running while the window
/// is closed.
///
/// Closing the window only hides it, unless the user turned off the
/// `closeToTray` setting. Clicking the tray icon shows it again; the tray
/// menu lists the paired devices, each with its actions, then Settings and
/// Quit. Quitting is the one path that stops the daemon. While the window is
/// hidden or unfocused, incoming pairing requests, received files and pings
/// raise a notification that brings the window back. A ping over a focused
/// window shows a snackbar instead.
///
/// Sits above everything else so the tray works even if the daemon failed to
/// start, and so it lives as long as the `ProviderScope` does.
class BackgroundHost extends ConsumerStatefulWidget {
  const new({required this.child, super.key});

  final Widget child;

  @override
  ConsumerState<BackgroundHost> createState() => _BackgroundHostState();
}

class _BackgroundHostState extends ConsumerState<BackgroundHost> {
  late final AppLifecycleListener _lifecycle;
  bool _quitting = false;

  /// Notification id per pending incoming pairing, or `null` when the
  /// request arrived while the window had focus and needed no notification.
  final _pairingNotifications = <String, int?>{};
  int _nextNotificationId = 1;

  StreamSubscription<DaemonEvent>? _events;

  @override
  void initState() {
    super.initState();
    // Pings have no snapshot to watch, so follow the event stream directly.
    ref.listenManual(daemonEventsProvider, (_, hub) {
      unawaited(_events?.cancel());
      _events = hub.events.listen((event) {
        if (event is PingReceived) unawaited(_showPing(event));
      });
    }, fireImmediately: true);
    ref.listenManual(
      pairedDevicesProvider,
      (_, devices) =>
          ref.read(desktopShellProvider).setTrayMenu(_trayMenu(devices.value)),
      fireImmediately: true,
    );
    // An exit the OS asks for (e.g. quitting from the macOS menu bar) rather
    // than a window close, which the shell intercepts.
    _lifecycle = AppLifecycleListener(
      onExitRequested: () async {
        await _stopDaemon();
        return AppExitResponse.exit;
      },
    );
    unawaited(_start());
  }

  Future<void> _start() async {
    final shell = ref.read(desktopShellProvider);
    try {
      await shell.start(
        onCloseRequested: () => unawaited(_close()),
        onTrayClicked: () => unawaited(shell.showWindow()),
      );
    } on Object catch (error) {
      _log.warning('Tray and close-to-tray unavailable: $error');
      // The window starts hidden and the shell didn't get to show it.
      try {
        await shell.showWindow();
      } on Object catch (error) {
        _log.warning('Could not show the window: $error');
      }
    }
    try {
      await ref
          .read(desktopNotificationsProvider)
          .start(onActivated: () => unawaited(shell.showWindow()));
    } on Object catch (error) {
      _log.warning('Notifications unavailable: $error');
    }
  }

  /// Hide the window, or quit if the user doesn't want the app to keep
  /// running. Without settings (e.g. the daemon failed to start), hide, so
  /// the tray stays the way out.
  Future<void> _close() async {
    final closeToTray = ref.read(settingsProvider).value?.closeToTray ?? true;
    if (closeToTray) {
      await ref.read(desktopShellProvider).hideWindow();
    } else {
      await _quit();
    }
  }

  /// The tray menu: [devices] (left out while they are unknown), each with
  /// what can be done to it, then Settings and Quit.
  List<TrayMenuEntry> _trayMenu(List<Device>? devices) {
    final shell = ref.read(desktopShellProvider);
    return [
      TrayMenuItem(
        'Open MyConnect',
        onSelected: () => unawaited(shell.showWindow()),
      ),
      const TrayMenuSeparator(),
      if (devices != null) ...[
        if (devices.isEmpty) const TrayMenuItem('No paired devices'),
        for (final device in devices)
          TrayMenuItem(
            switch (device) {
              Device(isConnected: false) =>
                '${device.deviceName} (${reachabilityLabel(device)})',
              Device(battery: BatteryStatus(:final charge)) =>
                '${device.deviceName} · $charge%',
              _ => device.deviceName,
            },
            submenu: [
              TrayMenuItem(
                'Send files…',
                onSelected: device.acceptsFiles
                    ? () => unawaited(_sendFiles(device))
                    : null,
              ),
              TrayMenuItem(
                'Ping',
                onSelected: device.acceptsPings
                    ? () => unawaited(_ping(device))
                    : null,
              ),
              if (device.incomingCapabilities.contains(browseCapability))
                TrayMenuItem(
                  'Browse files',
                  onSelected: device.sharesFiles
                      ? () => unawaited(
                          _showRoute('/devices/${device.deviceId}/files'),
                        )
                      : null,
                ),
              const TrayMenuSeparator(),
              TrayMenuItem(
                'Show details',
                onSelected: () =>
                    unawaited(_showRoute('/devices/${device.deviceId}')),
              ),
            ],
          ),
        const TrayMenuSeparator(),
      ],
      TrayMenuItem(
        'Settings',
        onSelected: () => unawaited(_showRoute('/settings')),
      ),
      const TrayMenuSeparator(),
      TrayMenuItem('Quit', onSelected: () => unawaited(_quit())),
    ];
  }

  Future<void> _showRoute(String location) async {
    ref.read(routerProvider).go(location);
    await ref.read(desktopShellProvider).showWindow();
  }

  /// Ask for files and send them to [device], leaving the window as it is;
  /// the outcome is reported, and progress shows on the transfers page.
  Future<void> _sendFiles(Device device) async {
    final files = await openFiles(confirmButtonText: 'Send');
    if (files.isEmpty || !mounted) return;
    // The device may have dropped while the user picked.
    final current = ref.read(deviceProvider(device.deviceId));
    if (current == null || !current.acceptsFiles) {
      await _report(
        "Couldn't send to ${device.deviceName}",
        'The device is not connected right now.',
      );
      return;
    }
    final failure = await startTransfers(
      ref.read(transfersProvider.notifier),
      current,
      [for (final file in files) file.path],
    );
    final sending = files.length == 1
        ? 'Sending ${files.single.name}'
        : 'Sending ${files.length} files';
    await _report(current.deviceName, failure ?? '$sending.');
  }

  /// Ping [device] without showing the window; only a failure is reported.
  Future<void> _ping(Device device) async {
    try {
      await (await ref.read(apiProvider.future)).ping(device.deviceId);
    } on Object catch (error) {
      await _report("Couldn't ping ${device.deviceName}", describeError(error));
    }
  }

  /// Tell the user something: in a snackbar over a focused window,
  /// otherwise in a notification.
  Future<void> _report(String title, String body) async {
    if (await ref.read(desktopShellProvider).isWindowFocused()) {
      if (!mounted) return;
      ScaffoldMessenger.maybeOf(context)
          ?.showSnackBar(SnackBar(content: Text('$title: $body')));
      return;
    }
    await ref
        .read(desktopNotificationsProvider)
        .show(id: _nextNotificationId++, title: title, body: body);
  }

  Future<void> _stopDaemon() async {
    _quitting = true;
    await ref.read(daemonHostProvider).stop();
  }

  Future<void> _quit() async {
    if (_quitting) return;
    await _stopDaemon();
    await ref.read(desktopShellProvider).exit();
  }

  Future<void> _syncPairingNotifications(List<Pairing> pending) async {
    final notifications = ref.read(desktopNotificationsProvider);
    final pendingIds = {for (final pairing in pending) pairing.id};
    for (final id in _pairingNotifications.keys.toList()) {
      if (pendingIds.contains(id)) continue;
      if (_pairingNotifications.remove(id) case final notificationId?) {
        await notifications.cancel(notificationId);
      }
    }

    final arrived = [
      for (final pairing in pending)
        if (!_pairingNotifications.containsKey(pairing.id)) pairing,
    ];
    if (arrived.isEmpty) return;
    // Claim the ids before awaiting, so a rebuild in between doesn't notify
    // twice.
    for (final pairing in arrived) {
      _pairingNotifications[pairing.id] = null;
    }
    // The prompt is already in front of the user.
    if (await ref.read(desktopShellProvider).isWindowFocused()) return;
    for (final pairing in arrived) {
      if (!_pairingNotifications.containsKey(pairing.id)) continue;
      final notificationId = _nextNotificationId++;
      _pairingNotifications[pairing.id] = notificationId;
      await notifications.show(
        id: notificationId,
        title: 'Pairing request',
        body: '${pairing.deviceName} wants to pair with this computer.',
      );
    }
  }

  /// Notify about incoming files that completed since [previous]. A
  /// transfer [previous] didn't hold is skipped, so the first snapshot
  /// doesn't announce files received before the app started.
  Future<void> _notifyReceivedFiles(
    Map<String, Transfer>? previous,
    Map<String, Transfer> current,
  ) async {
    if (previous == null) return;
    final received = [
      for (final transfer in current.values)
        if (transfer.direction == TransferDirection.incoming &&
            transfer.status == TransferStatus.completed &&
            previous[transfer.id] != null &&
            previous[transfer.id]!.status != TransferStatus.completed)
          transfer,
    ];
    if (received.isEmpty) return;
    if (await ref.read(desktopShellProvider).isWindowFocused()) return;
    final notifications = ref.read(desktopNotificationsProvider);
    for (final transfer in received) {
      await notifications.show(
        id: _nextNotificationId++,
        title: 'File received',
        body: '${transfer.fileName} from ${transfer.deviceName}',
      );
    }
  }

  Future<void> _showPing(PingReceived ping) async {
    final text = switch (ping.message) {
      final message? when message.isNotEmpty => message,
      _ => 'Ping!',
    };
    await _report(ping.deviceName, text);
  }

  @override
  void dispose() {
    unawaited(_events?.cancel());
    _lifecycle.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    // Loaded up front so closing the window can honour `closeToTray`.
    ref.listen(settingsProvider, (_, _) {});
    ref.listen(
      pendingIncomingPairingsProvider,
      (_, pending) => unawaited(_syncPairingNotifications(pending)),
    );
    ref.listen(
      transfersProvider,
      (previous, next) => unawaited(
        _notifyReceivedFiles(previous?.value, next.value ?? const {}),
      ),
    );
    return widget.child;
  }
}
