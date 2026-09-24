import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:myconnect_ui/src/core/api/models/event.dart';
import 'package:myconnect_ui/src/core/api/models/pairing.dart';
import 'package:myconnect_ui/src/features/pairing/pairings_controller.dart';

import '../helpers.dart';

void main() {
  late TestDaemon daemon;
  late ProviderContainer container;

  setUp(() {
    daemon = TestDaemon();
    container = ProviderContainer.test(overrides: daemon.overrides)
      ..listen(pendingIncomingPairingsProvider, (_, _) {});
  });

  test('recovers requests that arrived before the UI connected', () async {
    daemon.pairings = [
      pairing(id: 'done', status: PairingStatus.accepted),
      pairing(id: 'outgoing', direction: PairingDirection.outgoing),
      pairing(id: 'waiting'),
    ];
    await container.read(pairingsProvider.future);
    expect(container.read(pendingIncomingPairingsProvider).map((p) => p.id), [
      'waiting',
    ]);
  });

  test('tracks requests and their resolution through events', () async {
    await container.read(pairingsProvider.future);

    await daemon.emit(PairingChanged(pairing(id: 'second', createdAt: 5)));
    await daemon.emit(PairingChanged(pairing(id: 'first')));
    expect(container.read(pendingIncomingPairingsProvider).map((p) => p.id), [
      'first',
      'second',
    ]);

    await daemon.emit(
      PairingChanged(pairing(id: 'first', status: PairingStatus.expired)),
    );
    expect(container.read(pendingIncomingPairingsProvider).map((p) => p.id), [
      'second',
    ]);
  });

  test('accepting applies the returned snapshot immediately', () async {
    daemon.pairings = [pairing()];
    when(() => daemon.api.acceptPairing('p1'))
        .thenAnswer((_) async => pairing(status: PairingStatus.accepted));
    await container.read(pairingsProvider.future);

    await container.read(pairingsProvider.notifier).accept('p1');

    expect(container.read(pendingIncomingPairingsProvider), isEmpty);
    expect(
      container.read(pairingProvider('p1'))!.status,
      PairingStatus.accepted,
    );
  });

  test(
    'a start response that arrives after the outcome does not undo it',
    () async {
      final started = Completer<Pairing>();
      when(() => daemon.api.startPairing(any()))
          .thenAnswer((_) => started.future);
      await container.read(pairingsProvider.future);

      final start = container.read(pairingsProvider.notifier).start('device');
      await daemon.emit(
        PairingChanged(
          pairing(
            direction: PairingDirection.outgoing,
            status: PairingStatus.accepted,
          ),
        ),
      );
      started.complete(
        pairing(
          direction: PairingDirection.outgoing,
          status: PairingStatus.requested,
        ),
      );

      expect((await start).status, PairingStatus.accepted);
      expect(
        container.read(pairingProvider('p1'))!.status,
        PairingStatus.accepted,
      );
    },
  );

  test('a request that arrives during a refetch survives it', () async {
    await container.read(pairingsProvider.future);
    // The daemon reads its pairings before the request comes in, and the
    // response arrives after the request's event.
    final fetched = Completer<List<Pairing>>();
    when(daemon.api.pairings).thenAnswer((_) => fetched.future);

    await daemon.emit(const EventStreamConnected());
    await daemon.emit(PairingChanged(pairing(id: 'waiting')));
    fetched.complete([]);
    await pumpEventQueue();

    expect(container.read(pendingIncomingPairingsProvider).map((p) => p.id), [
      'waiting',
    ]);
  });

  test('a request that arrives during the first fetch survives it', () async {
    final fetched = Completer<List<Pairing>>();
    when(daemon.api.pairings).thenAnswer((_) => fetched.future);
    final loaded = container.read(pairingsProvider.future);
    await pumpEventQueue();

    await daemon.emit(PairingChanged(pairing(id: 'waiting')));
    fetched.complete([]);
    await loaded;

    expect(container.read(pendingIncomingPairingsProvider).map((p) => p.id), [
      'waiting',
    ]);
  });
}
