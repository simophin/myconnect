import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:myconnect_ui/src/core/api/event_stream.dart';
import 'package:myconnect_ui/src/core/api/models/event.dart';
import 'package:myconnect_ui/src/core/api/myconnect_api.dart';
import 'package:myconnect_ui/src/core/daemon/daemon_host.dart';
import 'package:myconnect_ui/src/core/desktop/desktop_notifications.dart';
import 'package:myconnect_ui/src/core/desktop/desktop_shell.dart';
import 'package:myconnect_ui/src/core/desktop/window_placement.dart';

/// Which daemon this UI drives. Overridden in tests.
final daemonHostProvider = Provider<DaemonHost>(
  (ref) => DaemonHost.fromEnvironment(),
);

/// Where the main window's placement is saved. Overridden in tests.
final windowPlacementStoreProvider = Provider<WindowPlacementStore>(
  (ref) => WindowPlacementStore.fromEnvironment(),
);

/// The native window and tray. Overridden in tests.
final desktopShellProvider = Provider<DesktopShell>((ref) {
  final shell = NativeDesktopShell(
    placements: ref.watch(windowPlacementStoreProvider),
  );
  ref.onDispose(shell.dispose);
  return shell;
});

/// Desktop notifications. Overridden in tests.
final desktopNotificationsProvider = Provider<DesktopNotifications>(
  (ref) => LocalDesktopNotifications(),
);

/// The running daemon's endpoint. Starting the app starts the daemon;
/// disposing this provider (app exit, or a retry via `ref.invalidate`) stops
/// it.
final daemonEndpointProvider = FutureProvider<DaemonEndpoint>((ref) async {
  final host = ref.watch(daemonHostProvider);
  ref.onDispose(host.stop);
  return await host.start();
});

final apiProvider = FutureProvider<MyConnectApi>(
  (ref) async =>
      MyConnectApi.forEndpoint(await ref.watch(daemonEndpointProvider.future)),
);

enum EventStreamState { connecting, connected, reconnecting }

/// The one shared, auto-reconnecting subscription to the daemon's `/events`.
///
/// Every feature listens here rather than opening its own connection.
/// [events] is a broadcast stream, so a late listener sees only later events;
/// that is fine because every consumer fetches a snapshot when it starts.

class DaemonEventHub {
  new(Stream<DaemonEvent> source) {
    _subscription = source.listen((event) {
      switch (event) {
        case EventStreamConnected():
          _state = EventStreamState.connected;
        case EventStreamDisconnected():
          _state = EventStreamState.reconnecting;
        case _:
          break;
      }
      _controller.add(event);
    });
  }

  final _controller = StreamController<DaemonEvent>.broadcast();
  late final StreamSubscription<DaemonEvent> _subscription;
  EventStreamState _state = EventStreamState.connecting;

  Stream<DaemonEvent> get events => _controller.stream;

  EventStreamState get state => _state;

  Future<void> close() async {
    await _subscription.cancel();
    await _controller.close();
  }
}

final daemonEventsProvider = Provider<DaemonEventHub>((ref) {
  final api = ref.watch(apiProvider.future);
  final hub = DaemonEventHub(
    reconnectingEvents(() async* {
      yield* (await api).events();
    }),
  );
  ref.onDispose(hub.close);
  return hub;
});

/// Health of the event stream, for a "reconnecting" indicator.
final eventStreamStateProvider =
    NotifierProvider<EventStreamStateNotifier, EventStreamState>(
      EventStreamStateNotifier.new,
    );

class EventStreamStateNotifier extends Notifier<EventStreamState> {
  @override
  EventStreamState build() {
    final hub = ref.watch(daemonEventsProvider);
    final subscription = hub.events.listen((_) => state = hub.state);
    ref.onDispose(subscription.cancel);
    return hub.state;
  }
}
