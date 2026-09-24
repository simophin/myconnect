import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:myconnect_ui/src/core/api/event_stream.dart';
import 'package:myconnect_ui/src/core/api/models/event.dart';

void main() {
  test(
    'reconnects after drops and failures, reporting each transition',
    () async {
      var attempts = 0;
      Stream<DaemonEvent> connect() async* {
        attempts++;
        if (attempts == 2) throw Exception('connection refused');
        yield const EventStreamConnected();
        yield const UnhandledEvent('clipboard.changed');
      }

      final events = await reconnectingEvents(
        connect,
        initialDelay: const Duration(milliseconds: 1),
        maxDelay: const Duration(milliseconds: 2),
      ).take(7).toList();

      expect(events.map((e) => e.runtimeType), [
        EventStreamConnected,
        UnhandledEvent,
        EventStreamDisconnected,
        EventStreamDisconnected, // attempt 2 failed before connecting
        EventStreamConnected,
        UnhandledEvent,
        EventStreamDisconnected,
      ]);
      expect(attempts, 3);
    },
  );
}
