import 'dart:async';
import 'dart:math';

import 'package:logging/logging.dart';
import 'package:myconnect_ui/src/core/api/models/event.dart';

final _log = Logger('EventStream');

/// Keep a daemon event stream open for as long as it is listened to.
///
/// [connect] opens one connection whose stream starts with
/// [EventStreamConnected] (see `MyConnectApi.events`), which tells consumers
/// to refetch snapshots. [EventStreamDisconnected] is emitted when a
/// connection drops or fails to open. Reconnects back off exponentially up to
/// [maxDelay], resetting after each successful connect.
Stream<DaemonEvent> reconnectingEvents(
  Stream<DaemonEvent> Function() connect, {
  Duration initialDelay = const Duration(milliseconds: 250),
  Duration maxDelay = const Duration(seconds: 5),
}) async* {
  var delay = initialDelay;
  while (true) {
    try {
      await for (final event in connect()) {
        if (event is EventStreamConnected) delay = initialDelay;
        yield event;
      }
    } on Object catch (error) {
      _log.fine('Event stream dropped: $error');
    }
    yield const EventStreamDisconnected();
    await Future<void>.delayed(delay);
    delay = Duration(
      milliseconds: min(delay.inMilliseconds * 2, maxDelay.inMilliseconds),
    );
  }
}
