import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:logging/logging.dart';
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
    return _byId(await api.pairings());
  }

  Future<void> refresh() async {
    try {
      final api = await ref.read(apiProvider.future);
      final pairings = await api.pairings();
      if (ref.mounted) state = AsyncData(_byId(pairings));
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
    if (current != null && ref.mounted) {
      state = AsyncData({...current, pairing.id: pairing});
    }
    return pairing;
  }

  static Map<String, Pairing> _byId(List<Pairing> pairings) => {
    for (final pairing in pairings) pairing.id: pairing,
  };
}
