import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:myconnect_ui/src/core/api/api_exception.dart';
import 'package:myconnect_ui/src/core/api/models/device.dart';
import 'package:myconnect_ui/src/core/api/models/event.dart';
import 'package:myconnect_ui/src/core/api/models/pairing.dart';
import 'package:myconnect_ui/src/core/api/models/remote_file.dart';
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

    daemon.shell.onTrayClicked!();
    await tester.pumpAndSettle();
    expect(daemon.shell.visible, isTrue);
  });

  testWidgets('closing the window quits when close-to-tray is off', (
    tester,
  ) async {
    final daemon = TestDaemon();
    daemon.settings = daemon.settings.copyWith(closeToTray: false);
    await pumpApp(tester, daemon);

    daemon.shell.onCloseRequested!();
    await tester.pumpAndSettle();

    expect(daemon.host.stops, 1);
    expect(daemon.shell.exited, isTrue);
  });

  testWidgets('quitting from the tray stops the daemon, then exits', (
    tester,
  ) async {
    final daemon = await pumpApp(tester, TestDaemon());

    daemon.shell.selectTrayItem(['Quit']);
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

  testWidgets('a ping notifies while hidden and shows a snackbar otherwise', (
    tester,
  ) async {
    final daemon = await pumpApp(tester, TestDaemon());

    daemon.events.add(
      const PingReceived(deviceId: 'a', deviceName: 'Pixel', message: 'hi'),
    );
    await tester.pumpAndSettle();
    expect(find.text('Pixel: hi'), findsOneWidget);
    expect(daemon.notifications.shown, isEmpty);

    daemon.shell.onCloseRequested!();
    await tester.pumpAndSettle();
    daemon.events.add(const PingReceived(deviceId: 'a', deviceName: 'Pixel'));
    await tester.pumpAndSettle();
    expect(daemon.notifications.shown.values, ['Ping!']);
  });

  testWidgets('the tray menu lists connected paired devices, then Settings '
      'and Quit', (tester) async {
    final daemon = TestDaemon()
      ..devices = [
        device(
          name: 'Pixel',
          incomingCapabilities: [shareCapability, pingCapability],
        ),
        device(
          id: 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
          name: 'Laptop',
          reachability: DeviceReachability.unavailable,
        ),
        device(id: 'cccccccccccccccccccccccccccccccc', paired: false),
      ];
    await pumpApp(tester, daemon);
    final shell = daemon.shell;

    expect(shell.trayLabels, [
      'Open MyConnect',
      '-',
      'Pixel',
      '-',
      'Settings',
      '-',
      'Quit',
    ]);
    expect(shell.trayItem(['Pixel', 'Send files…']).enabled, isTrue);
    expect(shell.trayItem(['Pixel', 'Ping']).enabled, isTrue);

    daemon.events.add(
      DeviceChanged(
        device(
          id: 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
          name: 'Laptop',
          incomingCapabilities: [pingCapability],
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(shell.trayItem(['Laptop', 'Ping']).enabled, isTrue);
    expect(shell.trayItem(['Laptop', 'Send files…']).enabled, isFalse);
  });

  testWidgets('the tray menu says when nothing is paired', (tester) async {
    final daemon = await pumpApp(tester, TestDaemon());

    expect(daemon.shell.trayItem(['No paired devices']).enabled, isFalse);
  });

  testWidgets('the tray menu says when no paired device is connected', (
    tester,
  ) async {
    final daemon = TestDaemon()
      ..devices = [
        device(name: 'Pixel', reachability: DeviceReachability.unavailable),
      ];
    await pumpApp(tester, daemon);

    expect(daemon.shell.trayItem(['No devices connected']).enabled, isFalse);
    expect(() => daemon.shell.trayItem(['Pixel']), throwsStateError);
  });

  testWidgets('the tray opens a device, or settings, in the window', (
    tester,
  ) async {
    final daemon = TestDaemon()..devices = [device(name: 'Pixel')];
    await pumpApp(tester, daemon);
    daemon.shell.onCloseRequested!();
    await tester.pumpAndSettle();

    daemon.shell.selectTrayItem(['Pixel', 'Show details']);
    await tester.pumpAndSettle();
    expect(daemon.shell.visible, isTrue);
    expect(find.text('Device ID'), findsOneWidget);

    daemon.shell.selectTrayItem(['Settings']);
    await tester.pumpAndSettle();
    expect(find.text('Keep running when the window is closed'), findsOneWidget);
  });

  testWidgets('the tray offers browsing only on devices that share files', (
    tester,
  ) async {
    final daemon = TestDaemon()
      ..devices = [
        device(name: 'Pixel', incomingCapabilities: [browseCapability]),
        device(
          id: 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
          name: 'Laptop',
          incomingCapabilities: [pingCapability],
        ),
      ];
    when(() => daemon.api.listFiles(any(), path: any(named: 'path')))
        .thenAnswer((_) async => const DirectoryListing(entries: []));
    await pumpApp(tester, daemon);
    final shell = daemon.shell;

    expect(() => shell.trayItem(['Laptop', 'Browse files']), throwsStateError);
    daemon.events.add(
      DeviceChanged(
        device(
          name: 'Pixel',
          incomingCapabilities: [browseCapability],
          reachability: DeviceReachability.unavailable,
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(() => shell.trayItem(['Pixel']), throwsStateError);

    daemon.events.add(
      DeviceChanged(
        device(name: 'Pixel', incomingCapabilities: [browseCapability]),
      ),
    );
    await tester.pumpAndSettle();
    shell.onCloseRequested!();
    await tester.pumpAndSettle();
    shell.selectTrayItem(['Pixel', 'Browse files']);
    await tester.pumpAndSettle();
    expect(shell.visible, isTrue);
    expect(find.text('Files on Pixel'), findsOneWidget);
  });

  testWidgets('the tray sends the clipboard only to devices that take it', (
    tester,
  ) async {
    final daemon = TestDaemon()
      ..devices = [
        device(name: 'Pixel', incomingCapabilities: [clipboardCapability]),
        device(
          id: 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
          name: 'Laptop',
          incomingCapabilities: [pingCapability],
        ),
      ];
    when(() => daemon.api.sendClipboard(any())).thenAnswer((_) async {});
    await pumpApp(tester, daemon);
    final shell = daemon.shell;
    shell.onCloseRequested!();
    await tester.pumpAndSettle();

    expect(
      () => shell.trayItem(['Laptop', 'Send clipboard']),
      throwsStateError,
    );
    shell.selectTrayItem(['Pixel', 'Send clipboard']);
    await tester.pumpAndSettle();
    verify(() => daemon.api.sendClipboard('aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'))
        .called(1);
    expect(shell.visible, isFalse);
    expect(daemon.notifications.shown, isEmpty);

    when(
      () => daemon.api.sendClipboard(any()),
    ).thenThrow(const ApiException(code: 'clipboard_empty', statusCode: 409));
    shell.selectTrayItem(['Pixel', 'Send clipboard']);
    await tester.pumpAndSettle();
    expect(daemon.notifications.shown.values, [
      'There is no text on the clipboard to send.',
    ]);
  });

  testWidgets('pinging from the tray reports only a failure', (tester) async {
    final daemon = TestDaemon()
      ..devices = [
        device(name: 'Pixel', incomingCapabilities: [pingCapability]),
      ];
    when(() => daemon.api.ping(any())).thenAnswer((_) async {});
    await pumpApp(tester, daemon);
    daemon.shell.onCloseRequested!();
    await tester.pumpAndSettle();

    daemon.shell.selectTrayItem(['Pixel', 'Ping']);
    await tester.pumpAndSettle();
    verify(() => daemon.api.ping('aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa')).called(1);
    expect(daemon.shell.visible, isFalse);
    expect(daemon.notifications.shown, isEmpty);

    when(() => daemon.api.ping(any())).thenThrow(
      const ApiException(code: 'device_not_connected', statusCode: 409),
    );
    daemon.shell.selectTrayItem(['Pixel', 'Ping']);
    await tester.pumpAndSettle();
    expect(daemon.notifications.shown.values, [
      'The device is not connected right now.',
    ]);
  });
}
