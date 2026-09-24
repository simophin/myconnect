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

  test('replays events recorded during each fetch onto its result', () async {
    final replay = SnapshotReplay<List<String>>(
      (names, event) => switch (event) {
        UnhandledEvent(:final type) => [...names, type],
        _ => names,
      },
    );
    final first = Completer<List<String>>();
    final second = Completer<List<String>>();

    replay.record(const UnhandledEvent('before'));
    final firstResult = replay.fetch(() => first.future);
    replay.record(const UnhandledEvent('a'));
    final secondResult = replay.fetch(() => second.future);
    replay.record(const UnhandledEvent('b'));
    second.complete(['snapshot 2']);
    expect(await secondResult, ['snapshot 2', 'b']);

    replay.record(const UnhandledEvent('c'));
    first.complete(['snapshot 1']);
    expect(await firstResult, ['snapshot 1', 'a', 'b', 'c']);
  });
}
