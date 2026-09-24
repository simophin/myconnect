import 'dart:async';
import 'dart:ui';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:logging/logging.dart';
import 'package:material_ui/material_ui.dart';
import 'package:myconnect_ui/src/core/api/models/pairing.dart';
import 'package:myconnect_ui/src/core/api/models/transfer.dart';
import 'package:myconnect_ui/src/core/providers.dart';
import 'package:myconnect_ui/src/features/pairing/pairings_controller.dart';
import 'package:myconnect_ui/src/features/transfers/transfers_controller.dart';

final _log = Logger('BackgroundHost');

/// Keeps the app, and with it the embedded daemon, running while the window
/// is closed.
///
/// Closing the window only hides it; the tray menu shows it again or quits.
/// Quitting is the one path that stops the daemon. While the window is
/// hidden or unfocused, incoming pairing requests and received files raise a
/// notification that brings the window back.
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

  @override
  void initState() {
    super.initState();
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
        onCloseRequested: () => unawaited(shell.hideWindow()),
        onShowRequested: () => unawaited(shell.showWindow()),
        onQuitRequested: () => unawaited(_quit()),
      );
    } on Object catch (error) {
      _log.warning('Tray and close-to-tray unavailable: $error');
    }
    try {
      await ref
          .read(desktopNotificationsProvider)
          .start(onActivated: () => unawaited(shell.showWindow()));
    } on Object catch (error) {
      _log.warning('Notifications unavailable: $error');
    }
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

  @override
  void dispose() {
    _lifecycle.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
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
