import 'package:flutter/foundation.dart';
import 'package:flutter_local_notifications/flutter_local_notifications.dart';

/// Desktop notifications, for events the user should see while the window
/// is hidden.
abstract interface class DesktopNotifications {
  /// Connect to the platform's notification service. [onActivated] runs when
  /// the user clicks a notification.
  Future<void> start({required VoidCallback onActivated});

  Future<void> show({
    required int id,
    required String title,
    required String body,
  });

  Future<void> cancel(int id);
}

/// [DesktopNotifications] over `flutter_local_notifications`.
class LocalDesktopNotifications implements DesktopNotifications {
  final _plugin = FlutterLocalNotificationsPlugin();

  @override
  Future<void> start({required VoidCallback onActivated}) async {
    await _plugin.initialize(
      settings: const InitializationSettings(
        linux: LinuxInitializationSettings(defaultActionName: 'Open'),
        macOS: DarwinInitializationSettings(),
        windows: WindowsInitializationSettings(
          appName: 'MyConnect',
          appUserModelId: 'org.myconnect.MyConnect',
          guid: '5f0f3c1e-8a4e-4d4b-9a51-2b7f6f3c9d10',
        ),
      ),
      onDidReceiveNotificationResponse: (_) => onActivated(),
    );
  }

  @override
  Future<void> show({
    required int id,
    required String title,
    required String body,
  }) => _plugin.show(id: id, title: title, body: body);

  @override
  Future<void> cancel(int id) => _plugin.cancel(id: id);
}
