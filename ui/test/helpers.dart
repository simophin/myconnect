import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_riverpod/misc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:myconnect_ui/src/app.dart';
import 'package:myconnect_ui/src/core/api/models/device.dart';
import 'package:myconnect_ui/src/core/api/models/event.dart';
import 'package:myconnect_ui/src/core/api/models/pairing.dart';
import 'package:myconnect_ui/src/core/api/models/status.dart';
import 'package:myconnect_ui/src/core/api/myconnect_api.dart';
import 'package:myconnect_ui/src/core/daemon/daemon_host.dart';
import 'package:myconnect_ui/src/core/desktop/desktop_notifications.dart';
import 'package:myconnect_ui/src/core/desktop/desktop_shell.dart';
import 'package:myconnect_ui/src/core/providers.dart';

class MockMyConnectApi extends Mock implements MyConnectApi;

class FakeDaemonHost implements DaemonHost {
  int stops = 0;

  @override
  Future<DaemonEndpoint> start() async =>
      DaemonEndpoint(baseUrl: Uri.parse('http://127.0.0.1:1'));

  @override
  Future<void> stop() async => stops++;
}

class FakeDesktopShell implements DesktopShell {
  VoidCallback? onCloseRequested;
  VoidCallback? onShowRequested;
  VoidCallback? onQuitRequested;
  bool visible = true;
  bool focused = true;
  bool exited = false;

  @override
  Future<void> start({
    required VoidCallback onCloseRequested,
    required VoidCallback onShowRequested,
    required VoidCallback onQuitRequested,
  }) async {
    this.onCloseRequested = onCloseRequested;
    this.onShowRequested = onShowRequested;
    this.onQuitRequested = onQuitRequested;
  }

  @override
  Future<void> showWindow() async => visible = focused = true;

  @override
  Future<void> hideWindow() async => visible = focused = false;

  @override
  Future<bool> isWindowFocused() async => focused;

  @override
  Future<void> exit() async => exited = true;
}

class FakeDesktopNotifications implements DesktopNotifications {
  VoidCallback? onActivated;

  /// Notifications currently shown, by id.
  final shown = <int, String>{};

  @override
  Future<void> start({required VoidCallback onActivated}) async =>
      this.onActivated = onActivated;

  @override
  Future<void> show({
    required int id,
    required String title,
    required String body,
  }) async => shown[id] = body;

  @override
  Future<void> cancel(int id) async => shown.remove(id);
}

Device device({
  String id = 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
  String name = 'Phone',
  bool paired = true,
  bool pairing = false,
  DeviceReachability reachability = DeviceReachability.connected,
}) => Device(
  deviceId: id,
  deviceName: name,
  deviceType: DeviceType.phone,
  protocolVersion: 8,
  incomingCapabilities: const [],
  outgoingCapabilities: const [],
  reachability: reachability,
  paired: paired,
  pairing: pairing,
  lastSeenAt: 0,
);

Pairing pairing({
  String id = 'p1',
  String deviceId = 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
  String deviceName = 'Phone',
  PairingDirection direction = PairingDirection.incoming,
  PairingStatus status = PairingStatus.awaitingConfirmation,
  String? code = 'ABCD1234',
  int createdAt = 0,
}) => Pairing(
  id: id,
  deviceId: deviceId,
  deviceName: deviceName,
  direction: direction,
  status: status,
  createdAt: createdAt,
  expiresAt: createdAt + 30000,
  verificationCode: code,
);

/// An API mock with empty defaults, plus a controllable event stream.
class TestDaemon {
  new() {
    when(api.devices).thenAnswer((_) async => devices);
    when(api.pairings).thenAnswer((_) async => pairings);
    when(api.status).thenAnswer(
      (_) async => const DaemonStatus(
        version: '0.1.0',
        uptimeSeconds: 1,
        localDevice: LocalDevice(deviceId: 'local', deviceName: 'Desk'),
        protocolVersion: 8,
      ),
    );
  }

  final api = MockMyConnectApi();
  final host = FakeDaemonHost();
  final shell = FakeDesktopShell();
  final notifications = FakeDesktopNotifications();
  final events = StreamController<DaemonEvent>.broadcast();
  List<Device> devices = [];
  List<Pairing> pairings = [];

  List<Override> get overrides => [
    daemonHostProvider.overrideWithValue(host),
    desktopShellProvider.overrideWithValue(shell),
    desktopNotificationsProvider.overrideWithValue(notifications),
    apiProvider.overrideWith((ref) async => api),
    daemonEventsProvider.overrideWith((ref) {
      final hub = DaemonEventHub(events.stream);
      ref.onDispose(hub.close);
      return hub;
    }),
  ];

  /// Push an event and let listeners run.
  Future<void> emit(DaemonEvent event) async {
    events.add(event);
    await pumpEventQueue();
  }
}

/// Run the whole app against [daemon].
Future<TestDaemon> pumpApp(WidgetTester tester, TestDaemon daemon) async {
  await tester.pumpWidget(
    ProviderScope(overrides: daemon.overrides, child: const MyConnectApp()),
  );
  await tester.pumpAndSettle();
  return daemon;
}
