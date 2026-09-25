import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:mocktail/mocktail.dart';
import 'package:myconnect_ui/src/core/api/api_exception.dart';
import 'package:myconnect_ui/src/core/api/models/event.dart';
import 'package:myconnect_ui/src/features/settings/settings_controller.dart';

import '../helpers.dart';

void main() {
  group('SettingsController', () {
    late TestDaemon daemon;
    late ProviderContainer container;

    setUp(() {
      daemon = TestDaemon();
      container = ProviderContainer.test(overrides: daemon.overrides)
        ..listen(settingsProvider, (_, _) {});
    });

    test('follows settings.changed and refetches on reconnect', () async {
      await container.read(settingsProvider.future);

      final renamed = daemon.settings.copyWith(deviceName: 'Elsewhere');
      await daemon.emit(SettingsChanged(renamed));
      expect(container.read(settingsProvider).value, renamed);

      daemon.settings = daemon.settings.copyWith(
        plugins: {
          'clipboard': {'syncEnabled': false},
        },
      );
      await daemon.emit(const EventStreamConnected());
      expect(container.read(settingsProvider).value, daemon.settings);
    });

    test('sends only the changed field and keeps the answer', () async {
      await container.read(settingsProvider.future);
      final answer = daemon.settings.copyWith(downloadDir: '/tmp/in');
      when(() => daemon.api.updateSettings({'downloadDir': '/tmp/in'}))
          .thenAnswer((_) async => answer);

      await container.read(settingsProvider.notifier).setDownloadDir('/tmp/in');

      expect(container.read(settingsProvider).value, answer);
    });
  });

  testWidgets('renaming shows the new name, and a bad one says why', (
    tester,
  ) async {
    final daemon = await pumpApp(tester, TestDaemon());
    when(() => daemon.api.updateSettings({'deviceName': 'Bad.Name'})).thenThrow(
      const ApiException(code: 'invalid_device_name', statusCode: 400),
    );
    when(
      () => daemon.api.updateSettings({'deviceName': 'Studio'}),
    ).thenAnswer((_) async => daemon.settings.copyWith(deviceName: 'Studio'));

    await tester.tap(find.byTooltip('Settings'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Device name'));
    await tester.pumpAndSettle();

    await tester.enterText(find.byType(TextField), 'Bad.Name');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pumpAndSettle();
    expect(find.textContaining('1 to 32 characters'), findsOneWidget);
    // Enter keeps the field focused, so the name can be fixed right away.
    expect(
      tester.widget<EditableText>(find.byType(EditableText)).focusNode.hasFocus,
      isTrue,
    );

    await tester.enterText(find.byType(TextField), 'Studio');
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    expect(find.byType(AlertDialog), findsNothing);
    expect(find.text('Studio'), findsOneWidget);

    await tester.tap(find.byType(BackButton));
    await tester.pumpAndSettle();
    expect(find.text('This computer: Studio'), findsOneWidget);
  });

  testWidgets('switches save their setting', (tester) async {
    final daemon = await pumpApp(tester, TestDaemon());
    when(
      () => daemon.api.updateSettings({
        'plugins': {
          'clipboard': {'syncEnabled': false},
        },
      }),
    ).thenAnswer(
      (_) async => daemon.settings.copyWith(
        plugins: {
          'clipboard': {'syncEnabled': false},
        },
      ),
    );

    await tester.tap(find.byTooltip('Settings'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Sync clipboard'));
    await tester.pumpAndSettle();

    final toggle = tester.widget<SwitchListTile>(
      find.widgetWithText(SwitchListTile, 'Sync clipboard'),
    );
    expect(toggle.value, isFalse);
  });

  testWidgets('shows the version, dev without a MYCONNECT_VERSION', (
    tester,
  ) async {
    await pumpApp(tester, TestDaemon());

    await tester.tap(find.byTooltip('Settings'));
    await tester.pumpAndSettle();

    expect(find.widgetWithText(ListTile, 'Version'), findsOneWidget);
    expect(find.text('dev'), findsOneWidget);
  });
}
