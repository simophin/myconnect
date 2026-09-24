import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:mocktail/mocktail.dart';
import 'package:myconnect_ui/src/core/api/api_exception.dart';
import 'package:myconnect_ui/src/core/api/models/event.dart';
import 'package:myconnect_ui/src/core/api/models/pairing.dart';

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
}
