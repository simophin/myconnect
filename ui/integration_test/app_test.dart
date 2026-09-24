// End-to-end flows: the real app, with its daemon embedded through the real
// FFI library, against a `myconnect run` peer in another process. Both
// discover over loopback only and keep their state in temporary directories.
//
// Run with `tool/integration_test.sh` (a private X display and D-Bus
// session), or `flutter test integration_test -d linux` on a desktop.

import 'dart:io';
import 'dart:math';

import 'package:file_selector_platform_interface/file_selector_platform_interface.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:myconnect_ui/src/app.dart';
import 'package:myconnect_ui/src/core/daemon/native_daemon_host.dart';
import 'package:myconnect_ui/src/core/desktop/desktop_notifications.dart';
import 'package:myconnect_ui/src/core/desktop/window_placement.dart';
import 'package:myconnect_ui/src/core/providers.dart';
import 'package:myconnect_ui/src/shared/widgets.dart';

import 'support/cli_peer.dart';

const appName = 'E2E Desktop';
const peerName = 'CLI Peer';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  late String cli;
  late CliPeer peer;
  late Directory appDir;

  setUpAll(() async => cli = await buildCli());

  setUp(() async {
    peer = await CliPeer.start(cli, name: peerName);
    appDir = await Directory.systemTemp.createTemp('myconnect-app-');
  });

  tearDown(() async {
    await peer.stop();
    await appDir.delete(recursive: true);
  });

  /// Start the app on a fresh identity and wait for its empty home screen.
  Future<void> launchApp(
    WidgetTester tester, {
    DesktopNotifications? notifications,
  }) async {
    final host = NativeDaemonHost(
      config: NativeDaemonConfig(
        dataDir: '${appDir.path}/data',
        downloadDir: '${appDir.path}/downloads',
        deviceName: appName,
        discoveryLoopback: true,
        systemClipboard: false,
      ),
    );
    addTearDown(() async {
      await tester.pumpWidget(const SizedBox.shrink());
      await host.stop();
    });
    await tester.pumpWidget(
      MyConnectRoot(
        overrides: [
          daemonHostProvider.overrideWithValue(host),
          // Keep the owner's own window placement out of it.
          windowPlacementStoreProvider.overrideWithValue(
            WindowPlacementStore(File('${appDir.path}/window.json')),
          ),
          // Keep test runs off the desktop's notification area.
          desktopNotificationsProvider.overrideWithValue(
            notifications ?? _SilentNotifications(),
          ),
        ],
      ),
    );
    await pumpUntil(tester, find.text('No paired devices yet'));
  }

  Future<String?> peerVerificationCode(String pairingId) async {
    String? code;
    await peer.waitFor('the verification code', () async {
      final pairing = (await peer.get('/pairings/$pairingId'))! as Map;
      code = pairing['verificationCode'] as String?;
      return code != null;
    });
    return code;
  }

  Future<String?> peerPairingStatus(String pairingId) async =>
      ((await peer.get('/pairings/$pairingId'))! as Map)['status'] as String?;

  /// Have the peer request pairing, and wait for the app's prompt. Returns
  /// the app's device id and the peer's pairing id.
  Future<(String, String)> requestPairingFromPeer(WidgetTester tester) async {
    final appId = await peer.discover(appName);
    final pairing =
        (await peer.post('/pairings', {'deviceId': appId}))!
            as Map<String, Object?>;
    final pairingId = pairing['id']! as String;
    await pumpUntil(tester, find.text('Pairing request'));
    final shown = tester
        .widget<VerificationCode>(find.byType(VerificationCode))
        .code;
    expect(await peerVerificationCode(pairingId), shown);
    return (appId, pairingId);
  }

  /// Pair through an incoming request accepted in the app. Returns the app's
  /// device id.
  Future<String> pairWithPeer(WidgetTester tester) async {
    final (appId, _) = await requestPairingFromPeer(tester);
    await tester.tap(find.text('Accept'));
    await pumpUntil(tester, find.text(peerName));
    await peer.waitFor(
      'the peer to trust the app',
      () async => (await peer.device(appId))?['paired'] == true,
    );
    return appId;
  }

  testWidgets('accepts an incoming pairing request', (tester) async {
    await launchApp(tester);

    final appId = await pairWithPeer(tester);

    expect(find.text('Pairing request'), findsNothing);
    expect(find.text('Connected'), findsOneWidget);
    expect((await peer.device(appId))!['paired'], isTrue);
  });

  testWidgets('rejects an incoming pairing request', (tester) async {
    await launchApp(tester);
    final (appId, pairingId) = await requestPairingFromPeer(tester);

    await tester.tap(find.text('Reject'));

    await peer.waitFor(
      'the peer to see the rejection',
      () async => await peerPairingStatus(pairingId) == 'rejected',
    );
    await pumpUntilGone(tester, find.text('Pairing request'));
    expect(find.text('No paired devices yet'), findsOneWidget);
    expect((await peer.device(appId))!['paired'], isFalse);
  });

  testWidgets('pairs with a device found by scanning', (tester) async {
    await launchApp(tester);

    await tester.tap(find.text('Add device'));
    final pair = find.descendant(
      of: find.widgetWithText(ListTile, peerName),
      matching: find.widgetWithText(FilledButton, 'Pair'),
    );
    await pumpUntilEnabled(tester, pair);
    await tester.tap(pair);
    await pumpUntil(tester, find.text('Waiting for $peerName'));
    final shown = tester
        .widget<VerificationCode>(find.byType(VerificationCode))
        .code;

    Map<String, Object?>? request;
    await peer.waitFor('the request to reach the peer', () async {
      request = (await peer.pairings())
          .where((pairing) => pairing['status'] == 'awaiting_confirmation')
          .firstOrNull;
      return request != null;
    });
    expect(request!['direction'], 'incoming');
    expect(request!['verificationCode'], shown);
    await peer.post('/pairings/${request!['id']}/accept');

    await pumpUntil(tester, find.text('Paired with $peerName'));
    await tester.tap(find.text('Done'));
    await pumpUntil(tester, find.text('Send file'));
    expect(
      (await peer.device(request!['deviceId']! as String))!['paired'],
      isTrue,
    );
  });

  testWidgets('unpairs, and the peer forgets the app too', (tester) async {
    await launchApp(tester);
    final appId = await pairWithPeer(tester);

    await tester.tap(find.text(peerName));
    await pumpUntil(tester, find.text('Unpair'));
    await tester.tap(find.text('Unpair'));
    await pumpUntil(tester, find.widgetWithText(FilledButton, 'Unpair'));
    await tester.tap(find.widgetWithText(FilledButton, 'Unpair'));

    await pumpUntil(tester, find.text('No paired devices yet'));
    await peer.waitFor(
      'the peer to drop its trust',
      () async => (await peer.device(appId))?['paired'] == false,
    );
  });

  testWidgets('pings the peer and shows its ping back', (tester) async {
    final notifications = _SilentNotifications();
    await launchApp(tester, notifications: notifications);
    final appId = await pairWithPeer(tester);

    final received = (await peer.events()).firstWhere(
      (event) => event['type'] == 'ping.received',
    );
    await tester.tap(find.text(peerName));
    final ping = find.ancestor(
      of: find.text('Ping'),
      matching: find.bySubtype<ButtonStyleButton>(),
    );
    await pumpUntilEnabled(tester, ping);
    await tester.tap(ping);
    final data =
        (await received.timeout(const Duration(seconds: 20)))['data']! as Map;
    expect(data['deviceName'], appName);
    await pumpUntil(tester, find.text('Pinged $peerName.'));

    await peer.post('/devices/$appId/ping', {'message': 'hello app'});
    // A snackbar if the window has focus, a notification otherwise; which
    // one depends on the display the test runs on.
    await _pumpWhile(
      tester,
      () =>
          find.text('$peerName: hello app').evaluate().isEmpty &&
          !notifications.shown.contains('hello app'),
      'Timed out waiting for the ping from the peer',
      const Duration(seconds: 20),
    );
  });

  testWidgets('sends a file to the peer and receives one back', (tester) async {
    await launchApp(tester);
    final appId = await pairWithPeer(tester);
    final outgoing = await _randomFile(appDir, 'to-peer.bin');
    final incoming = await _randomFile(peer.directory, 'from-peer.bin');
    final picker = FileSelectorPlatform.instance;
    addTearDown(() => FileSelectorPlatform.instance = picker);
    FileSelectorPlatform.instance = _PickFile(outgoing.path);

    await tester.tap(find.text(peerName));
    final sendFile = find.ancestor(
      of: find.text('Send file'),
      matching: find.bySubtype<ButtonStyleButton>(),
    );
    await pumpUntilEnabled(tester, sendFile);
    await tester.tap(sendFile);

    Map<String, Object?>? received;
    await peer.waitFor('the peer to receive the file', () async {
      received = (await peer.transfers())
          .where((transfer) => transfer['status'] == 'completed')
          .firstOrNull;
      return received != null;
    });
    expect(
      await File(received!['savedPath']! as String).readAsBytes(),
      await outgoing.readAsBytes(),
    );
    await pumpUntil(tester, find.text('to-peer.bin'));

    await peer.sendFile(appId, incoming);
    await tester.tap(find.text('See all'));
    await pumpUntil(
      tester,
      find.descendant(
        of: find.widgetWithText(ListTile, 'from-peer.bin'),
        matching: find.byTooltip('Open file'),
      ),
    );
    expect(
      await File('${appDir.path}/downloads/from-peer.bin').readAsBytes(),
      await incoming.readAsBytes(),
    );
  });
}

/// Pump frames until [finder] matches, failing after [timeout]. The app talks
/// to real daemons, so this waits in real time rather than faking it.
Future<void> pumpUntil(
  WidgetTester tester,
  Finder finder, {
  Duration timeout = const Duration(seconds: 20),
}) => _pumpWhile(
  tester,
  () => finder.evaluate().isEmpty,
  'Timed out waiting for $finder',
  timeout,
);

Future<void> pumpUntilGone(
  WidgetTester tester,
  Finder finder, {
  Duration timeout = const Duration(seconds: 20),
}) => _pumpWhile(
  tester,
  () => finder.evaluate().isNotEmpty,
  'Timed out waiting for $finder to go away',
  timeout,
);

/// Pump until the button [finder] matches is enabled.
Future<void> pumpUntilEnabled(
  WidgetTester tester,
  Finder finder, {
  Duration timeout = const Duration(seconds: 20),
}) => _pumpWhile(
  tester,
  () =>
      finder.evaluate().isEmpty ||
      !tester.widget<ButtonStyleButton>(finder).enabled,
  'Timed out waiting for $finder to be enabled',
  timeout,
);

Future<void> _pumpWhile(
  WidgetTester tester,
  bool Function() waiting,
  String failure,
  Duration timeout,
) async {
  final deadline = DateTime.now().add(timeout);
  await tester.pump();
  while (waiting()) {
    if (DateTime.now().isAfter(deadline)) fail(failure);
    await Future<void>.delayed(const Duration(milliseconds: 100));
    await tester.pump();
  }
}

/// A file of random bytes, larger than one payload chunk.
Future<File> _randomFile(Directory directory, String name) {
  final random = Random(name.hashCode);
  final bytes = Uint8List.fromList([
    for (var i = 0; i < 300 * 1024; i++) random.nextInt(256),
  ]);
  return File('${directory.path}/$name').writeAsBytes(bytes);
}

/// The "Send file" dialog, answered with [path].
class _PickFile extends FileSelectorPlatform {
  new(this.path);

  final String path;

  @override
  Future<List<XFile>> openFiles({
    List<XTypeGroup>? acceptedTypeGroups,
    String? initialDirectory,
    String? confirmButtonText,
  }) async => [XFile(path)];
}

/// Notifications recorded instead of shown.
class _SilentNotifications implements DesktopNotifications {
  final shown = <String>[];

  @override
  Future<void> start({required VoidCallback onActivated}) async {}

  @override
  Future<void> show({
    required int id,
    required String title,
    required String body,
  }) async => shown.add(body);

  @override
  Future<void> cancel(int id) async {}
}
