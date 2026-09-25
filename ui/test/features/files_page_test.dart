import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:mocktail/mocktail.dart';
import 'package:myconnect_ui/src/core/api/api_exception.dart';
import 'package:myconnect_ui/src/core/api/models/device.dart';
import 'package:myconnect_ui/src/core/api/models/remote_file.dart';
import 'package:myconnect_ui/src/core/api/models/transfer.dart';

import '../helpers.dart';

const _pixelId = 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';
const _internal = '/storage/emulated/0';

RemoteFile _directory(String path, [String? name]) => RemoteFile(
  name: name ?? path.split('/').last,
  path: path,
  kind: FileKind.directory,
);

RemoteFile _file(String path, {int size = 1024}) => RemoteFile(
  name: path.split('/').last,
  path: path,
  kind: FileKind.file,
  size: size,
  modifiedAt: DateTime(2026, 9, 24, 14, 3).millisecondsSinceEpoch,
);

/// A phone sharing its internal storage and an SD card, with a few files.
TestDaemon _phoneDaemon() {
  final daemon = TestDaemon()
    ..devices = [
      device(name: 'Pixel', incomingCapabilities: [browseCapability]),
    ];
  final listings = <String?, DirectoryListing>{
    null: DirectoryListing(
      entries: [
        _directory(_internal, 'All files'),
        _directory('/storage/sdcard', 'SD card'),
      ],
    ),
    _internal: DirectoryListing(
      path: _internal,
      entries: [
        _file('$_internal/notes.txt', size: 2048),
        _directory('$_internal/DCIM'),
        _file('$_internal/.nomedia'),
      ],
    ),
    '$_internal/DCIM': DirectoryListing(
      path: '$_internal/DCIM',
      entries: [_directory('$_internal/DCIM/Camera')],
    ),
    '$_internal/DCIM/Camera': const DirectoryListing(
      path: '$_internal/DCIM/Camera',
      entries: [],
    ),
  };
  when(() => daemon.api.listFiles(any(), path: any(named: 'path'))).thenAnswer(
    (invocation) async =>
        listings[invocation.namedArguments[#path] as String?] ??
        (throw const ApiException(code: 'file_not_found', statusCode: 404)),
  );
  return daemon;
}

/// Open the phone's files from the home screen.
Future<void> _openFiles(WidgetTester tester) async {
  await tester.tap(find.text('Pixel'));
  await tester.pumpAndSettle();
  await tester.tap(find.text('Browse files'));
  await tester.pumpAndSettle();
}

/// Open the phone's internal storage.
Future<void> _openInternal(WidgetTester tester) async {
  await _openFiles(tester);
  await tester.tap(find.text('All files'));
  await tester.pumpAndSettle();
}

/// Choose [action] from [name]'s row menu.
Future<void> _rowAction(WidgetTester tester, String name, String action) async {
  final row = find.ancestor(
    of: find.text(name),
    matching: find.byType(InkWell),
  );
  await tester.tap(
    find.descendant(of: row.first, matching: find.byTooltip('More')),
  );
  await tester.pumpAndSettle();
  await tester.tap(find.text(action).last);
  await tester.pumpAndSettle();
}

void main() {
  setUpAll(() => registerFallbackValue(''));

  testWidgets('storage, then folders, with a breadcrumb back up', (
    tester,
  ) async {
    await pumpApp(tester, _phoneDaemon());

    await _openFiles(tester);
    expect(find.text('Files on Pixel'), findsOneWidget);
    expect(find.text('All files'), findsOneWidget);
    expect(find.text('SD card'), findsOneWidget);

    await tester.tap(find.text('All files'));
    await tester.pumpAndSettle();
    // Folders first; hidden files stay hidden.
    expect(
      tester.getTopLeft(find.text('DCIM')).dy,
      lessThan(tester.getTopLeft(find.text('notes.txt')).dy),
    );
    expect(find.text('2.0 KB'), findsOneWidget);
    expect(find.text('2026-09-24 14:03'), findsOneWidget);
    expect(find.text('.nomedia'), findsNothing);

    await tester.tap(find.text('DCIM'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Camera'));
    await tester.pumpAndSettle();
    expect(
      find.text('This folder is empty. Drop files here to upload them.'),
      findsOneWidget,
    );

    // The breadcrumb names the root as the device does.
    await tester.tap(find.widgetWithText(TextButton, 'All files'));
    await tester.pumpAndSettle();
    expect(find.text('notes.txt'), findsOneWidget);
    await tester.tap(find.byTooltip('Up'));
    await tester.pumpAndSettle();
    expect(find.text('SD card'), findsOneWidget);
  });

  testWidgets('hidden files can be shown', (tester) async {
    await pumpApp(tester, _phoneDaemon());
    await _openInternal(tester);

    await tester.tap(find.byType(PopupMenuButton<void>));
    await tester.pumpAndSettle();
    await tester.tap(find.byType(CheckedPopupMenuItem<void>));
    await tester.pumpAndSettle();

    expect(find.text('.nomedia'), findsOneWidget);
  });

  testWidgets('opening a file downloads it', (tester) async {
    final daemon = _phoneDaemon();
    when(() => daemon.api.downloadFile(any(), any()))
        .thenAnswer((_) async => transfer(fileName: 'notes.txt'));
    await pumpApp(tester, daemon);
    await _openInternal(tester);

    await tester.tap(find.text('notes.txt'));
    await tester.pumpAndSettle();

    verify(() => daemon.api.downloadFile(_pixelId, '$_internal/notes.txt'))
        .called(1);
    expect(find.text('Downloading notes.txt'), findsOneWidget);
  });

  testWidgets('renaming moves the file within its folder', (tester) async {
    final daemon = _phoneDaemon();
    when(() => daemon.api.moveFile(any(), any(), any()))
        .thenAnswer((_) async => _file('$_internal/todo.txt'));
    await pumpApp(tester, daemon);
    await _openInternal(tester);

    await _rowAction(tester, 'notes.txt', 'Rename');
    await tester.enterText(find.byType(TextField), 'todo.txt');
    await tester.tap(find.widgetWithText(FilledButton, 'Rename'));
    await tester.pumpAndSettle();

    verify(
      () => daemon.api.moveFile(
        _pixelId,
        '$_internal/notes.txt',
        '$_internal/todo.txt',
      ),
    ).called(1);
    // The folder is listed again to show the change.
    verify(() => daemon.api.listFiles(_pixelId, path: _internal)).called(2);
  });

  testWidgets('a name with a slash is refused before asking the device', (
    tester,
  ) async {
    final daemon = _phoneDaemon();
    await pumpApp(tester, daemon);
    await _openInternal(tester);

    await tester.tap(find.byTooltip('New folder'));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField), 'a/b');
    await tester.tap(find.widgetWithText(FilledButton, 'Create'));
    await tester.pumpAndSettle();

    expect(find.text('Names can’t contain “/”.'), findsOneWidget);
    verifyNever(() => daemon.api.createDirectory(any(), any()));
  });

  testWidgets('new folders are created in the open folder', (tester) async {
    final daemon = _phoneDaemon();
    when(() => daemon.api.createDirectory(any(), any()))
        .thenAnswer((_) async => _directory('$_internal/Trip'));
    await pumpApp(tester, daemon);
    await _openInternal(tester);

    await tester.tap(find.byTooltip('New folder'));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField), 'Trip');
    await tester.tap(find.widgetWithText(FilledButton, 'Create'));
    await tester.pumpAndSettle();

    verify(() => daemon.api.createDirectory(_pixelId, '$_internal/Trip'))
        .called(1);
  });

  testWidgets('deleting asks first, and warns about folder contents', (
    tester,
  ) async {
    final daemon = _phoneDaemon();
    when(() => daemon.api.deleteFile(any(), any())).thenAnswer((_) async {});
    await pumpApp(tester, daemon);
    await _openInternal(tester);

    await _rowAction(tester, 'DCIM', 'Delete');
    expect(find.text('Delete DCIM?'), findsOneWidget);
    expect(find.textContaining('everything in it'), findsOneWidget);
    await tester.tap(find.widgetWithText(FilledButton, 'Delete'));
    await tester.pumpAndSettle();

    verify(() => daemon.api.deleteFile(_pixelId, '$_internal/DCIM')).called(1);
  });

  testWidgets('a failed change is reported', (tester) async {
    final daemon = _phoneDaemon();
    when(() => daemon.api.createDirectory(any(), any()))
        .thenThrow(const ApiException(code: 'file_exists', statusCode: 409));
    await pumpApp(tester, daemon);
    await _openInternal(tester);

    await tester.tap(find.byTooltip('New folder'));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField), 'DCIM');
    await tester.tap(find.widgetWithText(FilledButton, 'Create'));
    await tester.pumpAndSettle();

    expect(
      find.text('There is already a file or folder with that name.'),
      findsOneWidget,
    );
  });

  testWidgets('files dropped on a folder are uploaded into it', (tester) async {
    final local = Directory.systemTemp.createTempSync('myconnect-files-test-');
    addTearDown(() => local.deleteSync(recursive: true));
    final photo = (File(
      '${local.path}/photo.jpg',
    )..writeAsStringSync('jpg')).path;
    final daemon = _phoneDaemon();
    when(
      () => daemon.api.uploadFileWithAnyId(any(), any(), any()),
    ).thenAnswer((_) async => transfer(direction: TransferDirection.outgoing));
    await pumpApp(tester, daemon);
    await _openInternal(tester);

    final position = tester.getCenter(find.text('notes.txt'));
    await _dropEvent(tester, 'entered', [position.dx, position.dy]);
    await _dropEvent(tester, 'updated', [position.dx, position.dy]);
    await _dropEvent(tester, 'performOperation', [photo]);

    verify(() => daemon.api.uploadFileWithAnyId(_pixelId, _internal, photo))
        .called(1);
    verifyNever(() => daemon.api.sendFileWithAnyId(any(), any()));
    // No dialog asks where to send it.
    expect(find.byType(AlertDialog), findsNothing);
  });

  testWidgets('a device that refuses says why and how to fix it', (
    tester,
  ) async {
    final daemon = _phoneDaemon();
    when(() => daemon.api.listFiles(any(), path: any(named: 'path'))).thenThrow(
      const ApiException(
        code: 'files_unavailable',
        statusCode: 409,
        detail: 'No storage locations configured',
      ),
    );
    await pumpApp(tester, daemon);

    await _openFiles(tester);

    expect(
      find.textContaining('(No storage locations configured)'),
      findsOneWidget,
    );
    expect(find.textContaining('Filesystem expose'), findsOneWidget);
    expect(find.text('Retry'), findsOneWidget);
  });

  testWidgets('browsing needs a device that shares its files', (tester) async {
    final daemon = TestDaemon()
      ..devices = [
        device(name: 'Pixel', incomingCapabilities: [shareCapability]),
      ];
    await pumpApp(tester, daemon);

    await tester.tap(find.text('Pixel'));
    await tester.pumpAndSettle();

    final button = tester.widget<ButtonStyleButton>(
      find.ancestor(
        of: find.text('Browse files'),
        matching: find.byWidgetPredicate(
          (widget) => widget is ButtonStyleButton,
        ),
      ),
    );
    expect(button.onPressed, isNull);
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
