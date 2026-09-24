import 'package:flutter_test/flutter_test.dart';
import 'package:myconnect_ui/src/core/api/models/event.dart';
import 'package:myconnect_ui/src/core/api/models/pairing.dart';
import 'package:myconnect_ui/src/core/api/models/transfer.dart';

import '../helpers.dart';

void main() {
  testWidgets('closing the window hides it and keeps the daemon', (
    tester,
  ) async {
    final daemon = await pumpApp(tester, TestDaemon());

    daemon.shell.onCloseRequested!();
    await tester.pumpAndSettle();

    expect(daemon.shell.visible, isFalse);
    expect(daemon.shell.exited, isFalse);
    expect(daemon.host.stops, 0);

    daemon.shell.onShowRequested!();
    await tester.pumpAndSettle();
    expect(daemon.shell.visible, isTrue);
  });

  testWidgets('quitting from the tray stops the daemon, then exits', (
    tester,
  ) async {
    final daemon = await pumpApp(tester, TestDaemon());

    daemon.shell.onQuitRequested!();
    await tester.pumpAndSettle();

    expect(daemon.host.stops, 1);
    expect(daemon.shell.exited, isTrue);
  });

  testWidgets('a pairing request notifies while the window is hidden', (
    tester,
  ) async {
    final daemon = await pumpApp(tester, TestDaemon());
    daemon.shell.onCloseRequested!();
    await tester.pumpAndSettle();

    daemon.events.add(PairingChanged(pairing(deviceName: 'Pixel')));
    await tester.pumpAndSettle();

    expect(daemon.notifications.shown.values, [
      'Pixel wants to pair with this computer.',
    ]);

    daemon.notifications.onActivated!();
    await tester.pumpAndSettle();
    expect(daemon.shell.visible, isTrue);
    expect(find.text('Pairing request'), findsOneWidget);

    // Resolved elsewhere: the notification goes away with the prompt.
    daemon.events.add(PairingChanged(pairing(status: PairingStatus.rejected)));
    await tester.pumpAndSettle();
    expect(daemon.notifications.shown, isEmpty);
  });

  testWidgets('a pairing request does not notify over a focused window', (
    tester,
  ) async {
    final daemon = await pumpApp(tester, TestDaemon());

    daemon.events.add(PairingChanged(pairing()));
    await tester.pumpAndSettle();
    expect(find.text('Pairing request'), findsOneWidget);

    // Losing focus later doesn't bring up a stale notification.
    daemon.shell.focused = false;
    daemon.events.add(PairingChanged(pairing(id: 'p2', createdAt: 1)));
    await tester.pumpAndSettle();
    expect(daemon.notifications.shown, hasLength(1));
  });

  testWidgets('a received file notifies while the window is hidden', (
    tester,
  ) async {
    final daemon = TestDaemon()
      ..transfers = [
        transfer(id: 'earlier', status: TransferStatus.completed),
        transfer(id: 'sent', direction: TransferDirection.outgoing),
      ];
    await pumpApp(tester, daemon);
    daemon.shell.onCloseRequested!();
    await tester.pumpAndSettle();

    daemon.events
      ..add(TransferChanged(transfer()))
      ..add(
        TransferChanged(
          transfer(deviceName: 'Pixel', status: TransferStatus.completed),
        ),
      )
      ..add(
        TransferChanged(
          transfer(
            id: 'sent',
            direction: TransferDirection.outgoing,
            status: TransferStatus.completed,
          ),
        ),
      );
    await tester.pumpAndSettle();

    expect(daemon.notifications.shown.values, ['photo.jpg from Pixel']);
  });
}
