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
}
