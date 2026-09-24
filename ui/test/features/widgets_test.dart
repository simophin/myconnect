import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:mocktail/mocktail.dart';
import 'package:myconnect_ui/src/core/api/api_exception.dart';
import 'package:myconnect_ui/src/core/api/models/device.dart';
import 'package:myconnect_ui/src/core/api/models/event.dart';
import 'package:myconnect_ui/src/core/api/models/pairing.dart';
import 'package:myconnect_ui/src/core/api/models/transfer.dart';

import '../helpers.dart';

void main() {
  testWidgets('home lists paired devices only', (tester) async {
    final daemon = TestDaemon()
      ..devices = [
        device(id: 'a' * 32, name: 'Pixel'),
        device(id: 'b' * 32, name: 'Stranger', paired: false),
      ];
    await pumpApp(tester, daemon);

    expect(find.text('Pixel'), findsOneWidget);
    expect(find.text('Connected'), findsOneWidget);
    expect(find.text('Stranger'), findsNothing);
    expect(find.text('This computer: Desk'), findsOneWidget);
  });

  testWidgets('a device shows its battery once it reports one', (
    tester,
  ) async {
    final daemon = TestDaemon()..devices = [device(name: 'Pixel')];
    await pumpApp(tester, daemon);
    expect(find.text('Connected'), findsOneWidget);

    daemon.events.add(
      DeviceChanged(
        device(
          name: 'Pixel',
          battery: const BatteryStatus(charge: 82, charging: true),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('Connected · 82%, charging'), findsOneWidget);

    daemon.events.add(
      DeviceChanged(
        device(
          name: 'Pixel',
          battery: const BatteryStatus(charge: 81, charging: false),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('Connected · 81%'), findsOneWidget);
    expect(daemon.shell.trayItem(['Pixel · 81%']).enabled, isTrue);

    await tester.tap(find.text('Pixel'));
    await tester.pumpAndSettle();
    expect(find.text('Connected · 81%'), findsOneWidget);
  });

  testWidgets('an incoming request prompts on any screen until resolved', (
    tester,
  ) async {
    final daemon = await pumpApp(tester, TestDaemon());
    when(() => daemon.api.acceptPairing('p1'))
        .thenAnswer((_) async => pairing(status: PairingStatus.accepted));

    daemon.events.add(PairingChanged(pairing(deviceName: 'Pixel')));
    await tester.pumpAndSettle();

    expect(find.text('Pairing request'), findsOneWidget);
    expect(find.text('ABCD1234'), findsOneWidget);
    expect(find.textContaining('Pixel wants to pair'), findsOneWidget);

    await tester.tap(find.text('Accept'));
    await tester.pumpAndSettle();

    verify(() => daemon.api.acceptPairing('p1')).called(1);
    expect(find.text('Pairing request'), findsNothing);
  });

  testWidgets('a failed response keeps the prompt open with the reason', (
    tester,
  ) async {
    final daemon = TestDaemon()..pairings = [pairing()];
    when(() => daemon.api.rejectPairing('p1')).thenThrow(
      const ApiException(code: 'invalid_pairing_state', statusCode: 409),
    );
    await pumpApp(tester, daemon);

    await tester.tap(find.text('Reject'));
    await tester.pumpAndSettle();

    expect(find.text('Pairing request'), findsOneWidget);
    expect(
      find.text('That pairing request is no longer active.'),
      findsOneWidget,
    );
  });

  testWidgets('unpairing returns to the device list', (tester) async {
    final daemon = TestDaemon()..devices = [device(name: 'Pixel')];
    // As in the real daemon, `device.forgotten` arrives (and removes the
    // device, unmounting its details) before the DELETE response does.
    final response = Completer<void>();
    when(() => daemon.api.forgetDevice(any())).thenAnswer((_) {
      daemon.events.add(DeviceForgotten(device()));
      return response.future;
    });
    await pumpApp(tester, daemon);

    await tester.tap(find.text('Pixel'));
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(OutlinedButton, 'Unpair'));
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(FilledButton, 'Unpair'));
    await tester.pumpAndSettle();
    response.complete();
    await tester.pumpAndSettle();

    verify(() => daemon.api.forgetDevice(device().deviceId)).called(1);
    expect(find.text('No paired devices yet'), findsOneWidget);
  });

  testWidgets('files can be sent only to a connected device that takes them', (
    tester,
  ) async {
    FilledButton sendButton() =>
        tester.widget(find.widgetWithText(FilledButton, 'Send file'));
    final daemon = TestDaemon()..devices = [device(name: 'Pixel')];
    await pumpApp(tester, daemon);
    await tester.tap(find.text('Pixel'));
    await tester.pumpAndSettle();
    expect(sendButton().onPressed, isNull);

    daemon.events.add(
      DeviceChanged(
        device(
          name: 'Pixel',
          incomingCapabilities: ['kdeconnect.share.request'],
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(sendButton().onPressed, isNotNull);

    daemon.events.add(
      DeviceChanged(
        device(
          name: 'Pixel',
          incomingCapabilities: ['kdeconnect.share.request'],
          reachability: DeviceReachability.unavailable,
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(sendButton().onPressed, isNull);
  });

  testWidgets('a connected device that takes pings can be pinged', (
    tester,
  ) async {
    FilledButton pingButton() =>
        tester.widget(find.widgetWithText(FilledButton, 'Ping'));
    final daemon = TestDaemon()..devices = [device(name: 'Pixel')];
    when(() => daemon.api.ping(any())).thenAnswer((_) async {});
    await pumpApp(tester, daemon);
    await tester.tap(find.text('Pixel'));
    await tester.pumpAndSettle();
    expect(pingButton().onPressed, isNull);

    daemon.events.add(
      DeviceChanged(
        device(name: 'Pixel', incomingCapabilities: ['kdeconnect.ping']),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(FilledButton, 'Ping'));
    await tester.pumpAndSettle();

    verify(() => daemon.api.ping(device().deviceId)).called(1);
    expect(find.text('Pinged Pixel.'), findsOneWidget);
  });

  testWidgets('the transfers page shows progress and cancels', (tester) async {
    final daemon = TestDaemon()
      ..transfers = [
        transfer(id: 'running', fileName: 'movie.mkv', transferredBytes: 50),
        transfer(
          id: 'done',
          fileName: 'notes.txt',
          status: TransferStatus.completed,
          savedPath: '/home/me/Downloads/notes.txt',
        ),
      ];
    when(
      () => daemon.api.cancelTransfer('running'),
    ).thenAnswer((_) async => transfer(id: 'running', fileName: 'movie.mkv'));
    await pumpApp(tester, daemon);

    await tester.tap(find.byTooltip('Transfers'));
    await tester.pumpAndSettle();
    expect(find.text('From Phone · 50 bytes of 100 bytes'), findsOneWidget);
    expect(find.byType(LinearProgressIndicator), findsOneWidget);
    expect(find.byTooltip('Open folder'), findsOneWidget);

    await tester.tap(find.byTooltip('Cancel'));
    await tester.pumpAndSettle();
    verify(() => daemon.api.cancelTransfer('running')).called(1);

    daemon.events.add(
      TransferChanged(
        transfer(
          id: 'running',
          fileName: 'movie.mkv',
          status: TransferStatus.cancelled,
          updatedAt: 1,
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('From Phone · Cancelled'), findsOneWidget);
    expect(find.byType(LinearProgressIndicator), findsNothing);
  });

  testWidgets('a device can be added by IP address', (tester) async {
    final daemon = await pumpApp(tester, TestDaemon());
    when(daemon.api.scan).thenAnswer((_) async {});
    when(
      () => daemon.api.scan(address: 'desk.local'),
    ).thenThrow(const ApiException(code: 'invalid_address', statusCode: 400));
    when(() => daemon.api.scan(address: '192.168.1.20'))
        .thenAnswer((_) async {});

    await tester.tap(find.text('Add device'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Add by IP address'));
    await tester.pumpAndSettle();

    await tester.enterText(find.byType(TextField), 'desk.local');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pumpAndSettle();
    expect(find.textContaining('Enter an IPv4 address'), findsOneWidget);

    await tester.enterText(find.byType(TextField), ' 192.168.1.20 ');
    await tester.tap(find.text('Add'));
    await tester.pumpAndSettle();
    expect(find.byType(AlertDialog), findsNothing);
    verify(() => daemon.api.scan(address: '192.168.1.20')).called(1);

    // The device dials back and shows up like any scanned one.
    daemon.events.add(
      DeviceChanged(device(id: 'c' * 32, name: 'Laptop', paired: false)),
    );
    await tester.pumpAndSettle();
    expect(find.text('Laptop'), findsOneWidget);
  });
}
