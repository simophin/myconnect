import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:logging/logging.dart';
import 'package:myconnect_ui/src/core/api/event_stream.dart';
import 'package:myconnect_ui/src/core/api/models/event.dart';
import 'package:myconnect_ui/src/core/api/models/pairing.dart';
import 'package:myconnect_ui/src/core/providers.dart';

final _log = Logger('PairingsController');

/// Every pairing the daemon knows about in this session, by id.
final pairingsProvider =
    AsyncNotifierProvider<PairingsController, Map<String, Pairing>>(
      PairingsController.new,
    );

/// Incoming requests waiting for the local user to accept or reject them,
/// oldest first.
final pendingIncomingPairingsProvider = Provider<List<Pairing>>(
  (ref) =>
      (ref.watch(pairingsProvider).value?.values ?? const <Pairing>[])
          .where((pairing) => pairing.needsLocalConfirmation)
          .toList()
        ..sort((a, b) => a.createdAt.compareTo(b.createdAt)),
);

final pairingProvider = Provider.family<Pairing?, String>(
  (ref, pairingId) => ref.watch(pairingsProvider).value?[pairingId],
);

/// Holds a snapshot from `GET /pairings`, patched by pairing events and
/// refetched whenever the event stream (re)connects.
class PairingsController extends AsyncNotifier<Map<String, Pairing>> {
  @override
  Future<Map<String, Pairing>> build() async {
    final subscription = ref
        .watch(daemonEventsProvider)
        .events
        .listen(_onEvent);
    ref.onDispose(subscription.cancel);
    final api = await ref.watch(apiProvider.future);
    return await _replay.fetch(() async => _byId(await api.pairings()));
  }

  final _replay = SnapshotReplay<Map<String, Pairing>>(_applied);

  Future<void> refresh() async {
    try {
      final api = await ref.read(apiProvider.future);
      final pairings = await _replay.fetch(
        () async => _byId(await api.pairings()),
      );
      if (ref.mounted) state = AsyncData(pairings);
    } on Object catch (error) {
      _log.warning('Pairing refresh failed: $error');
    }
  }

  /// Request pairing with a connected, unpaired device.
  Future<Pairing> start(String deviceId) async => _upsert(
    await (await ref.read(apiProvider.future)).startPairing(deviceId),
  );

  /// Confirm an incoming request after the user compared verification codes.
  Future<Pairing> accept(String pairingId) async => _upsert(
    await (await ref.read(apiProvider.future)).acceptPairing(pairingId),
  );

  /// Reject an incoming request or cancel an outgoing one.
  Future<Pairing> reject(String pairingId) async => _upsert(
    await (await ref.read(apiProvider.future)).rejectPairing(pairingId),
  );

  void _onEvent(DaemonEvent event) {
    _replay.record(event);
    switch (event) {
      case EventStreamConnected():
        unawaited(refresh());
      case PairingChanged(:final pairing):
        _upsert(pairing);
      case _:
        break;
    }
  }

  Pairing _upsert(Pairing pairing) {
    final current = state.value;
    if (current == null || !ref.mounted) return pairing;
    final updated = _with(current, pairing);
    if (!identical(updated, current)) state = AsyncData(updated);
    return updated[pairing.id]!;
  }

  static Map<String, Pairing> _applied(
    Map<String, Pairing> pairings,
    DaemonEvent event,
  ) => switch (event) {
    PairingChanged(:final pairing) => _with(pairings, pairing),
    _ => pairings,
  };

  /// [pairings] with [pairing] stored, unless the pairing has already
  /// ended. An HTTP response can arrive after the event that superseded it:
  /// a device that accepts at once sends `pairing.updated` before `start`
  /// returns.
  static Map<String, Pairing> _with(
    Map<String, Pairing> pairings,
    Pairing pairing,
  ) {
    final existing = pairings[pairing.id];
    if (existing != null &&
        existing.status.isTerminal &&
        !pairing.status.isTerminal) {
      return pairings;
    }
    return {...pairings, pairing.id: pairing};
  }

  static Map<String, Pairing> _byId(List<Pairing> pairings) => {
    for (final pairing in pairings) pairing.id: pairing,
  };
}
