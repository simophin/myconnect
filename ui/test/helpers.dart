import 'dart:async';

import 'package:dio/dio.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_riverpod/misc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:myconnect_ui/src/app.dart';
import 'package:myconnect_ui/src/core/api/models/device.dart';
import 'package:myconnect_ui/src/core/api/models/event.dart';
import 'package:myconnect_ui/src/core/api/models/pairing.dart';
import 'package:myconnect_ui/src/core/api/models/settings.dart';
import 'package:myconnect_ui/src/core/api/models/transfer.dart';
import 'package:myconnect_ui/src/core/api/myconnect_api.dart';
import 'package:myconnect_ui/src/core/daemon/daemon_host.dart';
import 'package:myconnect_ui/src/core/desktop/desktop_notifications.dart';
import 'package:myconnect_ui/src/core/desktop/desktop_shell.dart';
import 'package:myconnect_ui/src/core/providers.dart';

class MockMyConnectApi extends Mock implements MyConnectApi;

/// Uploads as the transfers controller makes them, with whatever transfer
/// id and cancel token it chose, for stubbing and verifying.
extension AnyUploadId on MyConnectApi {
  Future<Transfer> sendFileWithAnyId(String deviceId, String path) => sendFile(
    deviceId,
    path,
    transferId: any(named: 'transferId'),
    cancelToken: any(named: 'cancelToken'),
  );

  Future<Transfer> uploadFileWithAnyId(
    String deviceId,
    String directory,
    String path,
  ) => uploadFile(
    deviceId,
    directory,
    path,
    transferId: any(named: 'transferId'),
    cancelToken: any(named: 'cancelToken'),
  );
}

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
  VoidCallback? onTrayClicked;
  List<TrayMenuEntry> trayMenu = const [];
  bool visible = true;
  bool focused = true;
  bool exited = false;

  @override
  Future<void> start({
    required VoidCallback onCloseRequested,
    required VoidCallback onTrayClicked,
  }) async {
    this.onCloseRequested = onCloseRequested;
    this.onTrayClicked = onTrayClicked;
  }

  @override
  void setTrayMenu(List<TrayMenuEntry> entries) => trayMenu = entries;

  /// The tray menu item found by following [labels] through submenus.
  TrayMenuItem trayItem(List<String> labels) {
    var entries = trayMenu;
    late TrayMenuItem item;
    for (final label in labels) {
      item = entries.whereType<TrayMenuItem>().singleWhere(
        (item) => item.label == label,
      );
      entries = item.submenu ?? const [];
    }
    return item;
  }

  /// Pick the tray menu item at [labels], as the user would.
  void selectTrayItem(List<String> labels) {
    final item = trayItem(labels);
    expect(item.enabled, isTrue, reason: '${labels.join(' > ')} is disabled');
    item.onSelected!();
  }

  /// The top-level tray menu's labels, with `-` for a separator.
  List<String> get trayLabels => [
    for (final entry in trayMenu)
      switch (entry) {
        TrayMenuItem(:final label) => label,
        TrayMenuSeparator() => '-',
      },
  ];

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
  List<String> incomingCapabilities = const [],
  BatteryStatus? battery,
}) => Device(
  deviceId: id,
  deviceName: name,
  deviceType: DeviceType.phone,
  protocolVersion: 8,
  incomingCapabilities: incomingCapabilities,
  outgoingCapabilities: const [],
  reachability: reachability,
  paired: paired,
  pairing: pairing,
  lastSeenAt: 0,
  plugins: {'battery': ?battery?.toJson()},
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

Transfer transfer({
  String id = 't1',
  String deviceId = 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
  String deviceName = 'Phone',
  TransferDirection direction = TransferDirection.incoming,
  TransferStatus status = TransferStatus.transferring,
  String fileName = 'photo.jpg',
  int transferredBytes = 0,
  int createdAt = 0,
  int? updatedAt,
  String? savedPath,
}) => Transfer(
  id: id,
  deviceId: deviceId,
  deviceName: deviceName,
  direction: direction,
  status: status,
  fileName: fileName,
  totalBytes: 100,
  transferredBytes: transferredBytes,
  createdAt: createdAt,
  updatedAt: updatedAt ?? createdAt,
  savedPath: savedPath,
);

/// An API mock with empty defaults, plus a controllable event stream.
class TestDaemon {
  new() {
    registerFallbackValue(CancelToken());
    when(api.devices).thenAnswer((_) async => devices);
    when(api.pairings).thenAnswer((_) async => pairings);
    when(api.transfers).thenAnswer((_) async => transfers);
    when(api.settings).thenAnswer((_) async => settings);
  }

  final api = MockMyConnectApi();
  final host = FakeDaemonHost();
  final shell = FakeDesktopShell();
  final notifications = FakeDesktopNotifications();
  final events = StreamController<DaemonEvent>.broadcast();
  List<Device> devices = [];
  List<Pairing> pairings = [];
  List<Transfer> transfers = [];
  DaemonSettings settings = const DaemonSettings(
    deviceName: 'Desk',
    downloadDir: '/home/me/Downloads',
    closeToTray: true,
    plugins: {
      'clipboard': {'syncEnabled': true},
    },
  );

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
