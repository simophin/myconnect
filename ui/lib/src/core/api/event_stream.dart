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

/// Keeps snapshots fetched over HTTP consistent with the event stream.
///
/// A controller patches its cached snapshot with each event, and refetches
/// the whole snapshot when the stream (re)connects. An event that arrives
/// while a fetch is in flight is applied to the old snapshot, which the
/// fetch's result then replaces; if the daemon read its state before that
/// event, the change would be lost until the next reconnect. [fetch] replays
/// such events onto the result.
///
/// Every event the controller receives must go through [record].
class SnapshotReplay<T> {
  new(this._apply);

  /// Applies one event to a snapshot, returning it unchanged when the event
  /// doesn't concern it.
  final T Function(T snapshot, DaemonEvent event) _apply;

  final _inFlight = <List<DaemonEvent>>{};

  void record(DaemonEvent event) {
    for (final missed in _inFlight) {
      missed.add(event);
    }
  }

  /// Run [fetch] and return its snapshot with the events recorded since the
  /// fetch started applied, in order. Replaying an event the snapshot
  /// already reflects is harmless: the last event about an item is its
  /// latest state.
  Future<T> fetch(Future<T> Function() fetch) async {
    final missed = <DaemonEvent>[];
    _inFlight.add(missed);
    try {
      var snapshot = await fetch();
      for (final event in missed) {
        snapshot = _apply(snapshot, event);
      }
      return snapshot;
    } finally {
      _inFlight.remove(missed);
    }
  }
}
