import 'dart:io';

import 'package:file_selector_platform_interface/file_selector_platform_interface.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:mocktail/mocktail.dart';
import 'package:myconnect_ui/src/core/api/api_exception.dart';
import 'package:myconnect_ui/src/core/api/models/device.dart';
import 'package:myconnect_ui/src/core/api/models/transfer.dart';

import '../helpers.dart';

const _pixelId = 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';
const _laptopId = 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb';

void main() {
  late Directory files;
  late String photo;
  late String notes;

  setUp(() {
    files = Directory.systemTemp.createTempSync('myconnect-send-test-');
    photo = (File('${files.path}/photo.jpg')..writeAsStringSync('jpg')).path;
    notes = (File('${files.path}/notes.txt')..writeAsStringSync('txt')).path;
  });
  tearDown(() => files.deleteSync(recursive: true));

  /// Paired devices that take files, with uploads that succeed.
  TestDaemon daemonWithRecipients() {
    final daemon = TestDaemon()
      ..devices = [
        device(name: 'Pixel', incomingCapabilities: [shareCapability]),
        device(
          id: _laptopId,
          name: 'Laptop',
          incomingCapabilities: [shareCapability],
        ),
      ];
    when(() => daemon.api.sendFile(any(), any())).thenAnswer(
      (invocation) async => transfer(
        deviceId: invocation.positionalArguments[0] as String,
        direction: TransferDirection.outgoing,
      ),
    );
    return daemon;
  }

  testWidgets('files dropped on a device go straight to it', (tester) async {
    final daemon = await pumpApp(tester, daemonWithRecipients());
    final pixel = tester.getCenter(find.text('Pixel'));

    await _dragOver(tester, pixel);
    expect(find.text('Drop to send'), findsOneWidget);
    await _drop(tester, [photo, notes]);

    verifyInOrder([
      () => daemon.api.sendFile(_pixelId, photo),
      () => daemon.api.sendFile(_pixelId, notes),
    ]);
    verifyNever(() => daemon.api.sendFile(_laptopId, any()));
    expect(find.byType(AlertDialog), findsNothing);
    // The device's page, where the transfers show.
    expect(find.text('Device ID'), findsOneWidget);
    expect(find.text('Drop to send'), findsNothing);
  });

  testWidgets('files dropped away from a device ask where to go', (
    tester,
  ) async {
    final daemon = await pumpApp(tester, daemonWithRecipients());

    await _dragOver(tester, tester.getCenter(find.text('Devices')));
    expect(
      find.text('Drop on a device, or anywhere to choose one'),
      findsOneWidget,
    );
    await _drop(tester, [photo]);

    expect(find.text('Send photo.jpg'), findsOneWidget);
    verifyNever(() => daemon.api.sendFile(any(), any()));
    await tester.tap(
      find.descendant(
        of: find.byType(AlertDialog),
        matching: find.text('Laptop'),
      ),
    );
    await tester.pumpAndSettle();

    verify(() => daemon.api.sendFile(_laptopId, photo)).called(1);
    expect(find.text('Device ID'), findsOneWidget);
  });

  testWidgets('a drop on a device that cannot take files asks instead', (
    tester,
  ) async {
    final daemon = daemonWithRecipients();
    daemon.devices = [device(name: 'Pixel'), ...daemon.devices.skip(1)];
    await pumpApp(tester, daemon);

    await _dragOver(tester, tester.getCenter(find.text('Pixel')));
    expect(find.text('Drop to send'), findsNothing);
    await _drop(tester, [photo]);

    expect(find.text('Send photo.jpg'), findsOneWidget);
    expect(
      find.descendant(
        of: find.byType(AlertDialog),
        matching: find.text('Pixel'),
      ),
      findsNothing,
    );
    await tester.tap(find.text('Cancel'));
    await tester.pumpAndSettle();
    verifyNever(() => daemon.api.sendFile(any(), any()));
  });

  testWidgets('a dropped folder is refused', (tester) async {
    final daemon = await pumpApp(tester, daemonWithRecipients());

    await _dragOver(tester, tester.getCenter(find.text('Pixel')));
    await _drop(tester, [files.path]);

    expect(find.text('Only files can be sent, not folders.'), findsOneWidget);
    verifyNever(() => daemon.api.sendFile(any(), any()));
  });

  testWidgets('the tray sends files to the device the user picks', (
    tester,
  ) async {
    final picker = FileSelectorPlatform.instance;
    addTearDown(() => FileSelectorPlatform.instance = picker);
    FileSelectorPlatform.instance = _PickFiles([photo, notes]);
    final daemon = await pumpApp(tester, daemonWithRecipients());
    daemon.shell.onCloseRequested!();
    await tester.pumpAndSettle();

    daemon.shell.onSendFilesRequested!();
    await tester.pumpAndSettle();

    expect(daemon.shell.visible, isTrue);
    expect(find.text('Send 2 files'), findsOneWidget);
    await tester.tap(
      find.descendant(
        of: find.byType(AlertDialog),
        matching: find.text('Pixel'),
      ),
    );
    await tester.pumpAndSettle();

    verify(() => daemon.api.sendFile(_pixelId, photo)).called(1);
    verify(() => daemon.api.sendFile(_pixelId, notes)).called(1);
  });

  testWidgets('without a device to send to, the dialog says so', (
    tester,
  ) async {
    final picker = FileSelectorPlatform.instance;
    addTearDown(() => FileSelectorPlatform.instance = picker);
    FileSelectorPlatform.instance = _PickFiles([photo]);
    final daemon = TestDaemon()..devices = [device(name: 'Pixel')];
    await pumpApp(tester, daemon);

    daemon.shell.onSendFilesRequested!();
    await tester.pumpAndSettle();

    expect(
      find.text('No paired device is connected and able to receive files.'),
      findsOneWidget,
    );
  });

  testWidgets('failed uploads are reported once', (tester) async {
    final daemon = daemonWithRecipients();
    when(() => daemon.api.sendFile(any(), any())).thenThrow(
      const ApiException(code: 'device_not_connected', statusCode: 409),
    );
    await pumpApp(tester, daemon);

    await _dragOver(tester, tester.getCenter(find.text('Pixel')));
    await _drop(tester, [photo, notes]);

    expect(find.textContaining("Couldn't send 2 files: "), findsOneWidget);
  });
}

/// Deliver [method] from `desktop_drop`'s native side.
Future<void> _dropEvent(
  WidgetTester tester,
  String method,
  Object arguments,
) async {
  await tester.binding.defaultBinaryMessenger.handlePlatformMessage(
    'desktop_drop',
    const StandardMethodCodec().encodeMethodCall(MethodCall(method, arguments)),
    (_) {},
  );
  await tester.pumpAndSettle();
}

/// Drag files from outside the window to [position].
Future<void> _dragOver(WidgetTester tester, Offset position) async {
  await _dropEvent(tester, 'entered', [position.dx, position.dy]);
  await _dropEvent(tester, 'updated', [position.dx, position.dy]);
}

/// Drop [paths] where the drag is.
Future<void> _drop(WidgetTester tester, List<String> paths) =>
    _dropEvent(tester, 'performOperation', paths);

/// The file picker, answered with [paths].
class _PickFiles extends FileSelectorPlatform {
  new(this.paths);

  final List<String> paths;

  @override
  Future<List<XFile>> openFiles({
    List<XTypeGroup>? acceptedTypeGroups,
    String? initialDirectory,
    String? confirmButtonText,
  }) async => [for (final path in paths) XFile(path)];
}
