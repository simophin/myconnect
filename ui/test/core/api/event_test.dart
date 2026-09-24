import 'package:flutter_test/flutter_test.dart';
import 'package:myconnect_ui/src/core/api/models/device.dart';
import 'package:myconnect_ui/src/core/api/models/event.dart';
import 'package:myconnect_ui/src/core/api/models/pairing.dart';
import 'package:myconnect_ui/src/core/api/models/settings.dart';
import 'package:myconnect_ui/src/core/api/models/transfer.dart';

Map<String, Object?> deviceJson({String reachability = 'connected'}) => {
  'deviceId': 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
  'deviceName': 'Phone',
  'deviceType': 'phone',
  'protocolVersion': 8,
  'incomingCapabilities': ['kdeconnect.ping'],
  'outgoingCapabilities': <String>[],
  'reachability': reachability,
  'paired': true,
  'pairing': false,
  'lastSeenAt': 10,
};

void main() {
  test('decodes device events, including forgotten devices', () {
    final changed = DaemonEvent.fromJson({
      'sequence': 1,
      'timestamp': 2,
      'type': 'device.connected',
      'data': deviceJson(),
    });
    expect(
      changed,
      isA<DeviceChanged>().having(
        (e) => e.device.reachability,
        'reachability',
        DeviceReachability.connected,
      ),
    );
    expect(
      DaemonEvent.fromJson({'type': 'device.forgotten', 'data': deviceJson()}),
      isA<DeviceForgotten>(),
    );
  });

  test('decodes a device battery, or its absence', () {
    expect(Device.fromJson(deviceJson()).battery, isNull);
    expect(Device.fromJson({...deviceJson(), 'battery': null}).battery, isNull);
    expect(
      Device.fromJson({
        ...deviceJson(),
        'battery': {'charge': 82, 'charging': true},
      }).battery,
      const BatteryStatus(charge: 82, charging: true),
    );
  });

  test('decodes pairing events with snake_case enums', () {
    final event = DaemonEvent.fromJson({
      'type': 'pairing.requested',
      'data': {
        'id': '00000000-0000-0000-0000-000000000001',
        'deviceId': 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
        'deviceName': 'Phone',
        'direction': 'incoming',
        'status': 'awaiting_confirmation',
        'verificationCode': 'ABCD1234',
        'createdAt': 1,
        'expiresAt': 2,
      },
    });
    final pairing = (event as PairingChanged).pairing;
    expect(pairing.status, PairingStatus.awaitingConfirmation);
    expect(pairing.needsLocalConfirmation, isTrue);
  });

  test('decodes transfer events, telling cancellation from failure', () {
    Map<String, Object?> transferJson(String status) => {
      'id': '00000000-0000-0000-0000-000000000002',
      'deviceId': 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
      'deviceName': 'Phone',
      'direction': 'incoming',
      'status': status,
      'fileName': 'photo.jpg',
      'totalBytes': 10,
      'transferredBytes': 10,
      'createdAt': 1,
      'updatedAt': 2,
      if (status == 'completed') 'savedPath': '/home/me/Downloads/photo.jpg',
    };
    final completed = DaemonEvent.fromJson({
      'type': 'transfer.completed',
      'data': transferJson('completed'),
    });
    expect(
      (completed as TransferChanged).transfer.savedPath,
      '/home/me/Downloads/photo.jpg',
    );
    final cancelled = DaemonEvent.fromJson({
      'type': 'transfer.failed',
      'data': transferJson('cancelled'),
    });
    expect(
      (cancelled as TransferChanged).transfer.status,
      TransferStatus.cancelled,
    );
  });

  test('tolerates event types and enum values from a newer daemon', () {
    expect(
      DaemonEvent.fromJson({
        'type': 'battery.changed',
        'data': <String, Object?>{},
      }),
      isA<UnhandledEvent>().having((e) => e.type, 'type', 'battery.changed'),
    );
    final device = Device.fromJson(deviceJson(reachability: 'sleeping'));
    expect(device.reachability, DeviceReachability.unknown);
  });

  test('decodes settings events', () {
    final event = DaemonEvent.fromJson({
      'type': 'settings.changed',
      'data': {
        'deviceName': 'Desk',
        'downloadDir': '/home/me/Downloads',
        'clipboardSyncEnabled': false,
        'closeToTray': true,
      },
    });
    expect(
      event,
      isA<SettingsChanged>().having(
        (e) => e.settings,
        'settings',
        const DaemonSettings(
          deviceName: 'Desk',
          downloadDir: '/home/me/Downloads',
          clipboardSyncEnabled: false,
          closeToTray: true,
        ),
      ),
    );
  });

  test('decodes ping events, with and without a message', () {
    final withMessage = DaemonEvent.fromJson({
      'type': 'ping.received',
      'data': {
        'deviceId': 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
        'deviceName': 'Phone',
        'message': 'hello',
      },
    });
    expect(
      withMessage,
      isA<PingReceived>()
          .having((e) => e.deviceName, 'deviceName', 'Phone')
          .having((e) => e.message, 'message', 'hello'),
    );
    final plain = DaemonEvent.fromJson({
      'type': 'ping.received',
      'data': {
        'deviceId': 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
        'deviceName': 'Phone',
      },
    });
    expect((plain as PingReceived).message, isNull);
  });
}
