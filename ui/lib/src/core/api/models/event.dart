import 'package:myconnect_ui/src/core/api/models/device.dart';
import 'package:myconnect_ui/src/core/api/models/pairing.dart';

/// A notification from the daemon's `/events` stream.
///
/// Events are hints that a resource changed, carrying that resource's latest
/// snapshot. They are not durable: after any gap (see
/// `EventStreamConnected`) consumers must refetch their snapshots.
sealed class DaemonEvent {
  const new();

  /// Decode the JSON `data` of one SSE frame: `{sequence, timestamp, type,
  /// data}`. Event types this build does not model decode to
  /// [UnhandledEvent] so a newer daemon never breaks an older UI.
  factory fromJson(Map<String, Object?> json) {
    final type = json['type']! as String;
    final data = json['data'];
    return switch (type) {
      'device.discovered' ||
      'device.connected' ||
      'device.updated' ||
      'device.disconnected' => DeviceChanged(
        Device.fromJson(data! as Map<String, Object?>),
      ),
      'device.forgotten' => DeviceForgotten(
        Device.fromJson(data! as Map<String, Object?>),
      ),
      'pairing.requested' || 'pairing.updated' => PairingChanged(
        Pairing.fromJson(data! as Map<String, Object?>),
      ),
      _ => UnhandledEvent(type),
    };
  }
}

/// The event stream (re)connected. Any event may have been missed before
/// this point, so snapshots must be refetched.
final class EventStreamConnected extends DaemonEvent {
  const new();
}

/// The event stream dropped; a reconnect is being attempted.
final class EventStreamDisconnected extends DaemonEvent {
  const new();
}

final class DeviceChanged extends DaemonEvent {
  const new(this.device);
  final Device device;
}

final class DeviceForgotten extends DaemonEvent {
  const new(this.device);
  final Device device;
}

final class PairingChanged extends DaemonEvent {
  const new(this.pairing);
  final Pairing pairing;
}

/// Transfer and clipboard events, which the UI does not consume yet.
final class UnhandledEvent extends DaemonEvent {
  const new(this.type);
  final String type;
}
