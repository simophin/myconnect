import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:myconnect_ui/src/core/api/models/event.dart';
import 'package:myconnect_ui/src/core/api/models/transfer.dart';
import 'package:myconnect_ui/src/features/transfers/transfers_controller.dart';

import '../helpers.dart';

void main() {
  late TestDaemon daemon;
  late ProviderContainer container;

  setUp(() {
    daemon = TestDaemon();
    container = ProviderContainer.test(overrides: daemon.overrides)
      ..listen(transferListProvider(null), (_, _) {});
  });

  List<String> ids([String? deviceId]) =>
      container.read(transferListProvider(deviceId)).map((t) => t.id).toList();

  test('lists newest first, optionally for one device', () async {
    daemon.transfers = [
      transfer(id: 'old'),
      transfer(id: 'other', deviceId: 'b' * 32, createdAt: 1),
      transfer(id: 'new', createdAt: 2),
    ];
    await container.read(transfersProvider.future);

    expect(ids(), ['new', 'other', 'old']);
    expect(ids('a' * 32), ['new', 'old']);
  });

  test(
    'applies progress and outcome events, and refetches on reconnect',
    () async {
      await container.read(transfersProvider.future);

      await daemon.emit(TransferChanged(transfer(transferredBytes: 40)));
      expect(container.read(transfersProvider).value!['t1']!.progress, 0.4);

      daemon.transfers = [transfer(id: 'missed')];
      await daemon.emit(const EventStreamConnected());
      expect(ids(), ['missed']);
    },
  );

  test('a send response never overwrites a newer event', () async {
    await container.read(transfersProvider.future);
    final completed = transfer(
      direction: TransferDirection.outgoing,
      status: TransferStatus.completed,
      transferredBytes: 100,
      updatedAt: 5,
    );
    // As in the real daemon, `transfer.completed` arrives before the
    // upload's response, which describes the transfer mid-flight.
    when(() => daemon.api.sendFile('a' * 32, '/tmp/photo.jpg'))
        .thenAnswer((_) async {
          await daemon.emit(TransferChanged(completed));
          return transfer(direction: TransferDirection.outgoing, updatedAt: 4);
        });

    final result = await container
        .read(transfersProvider.notifier)
        .send('a' * 32, '/tmp/photo.jpg');

    expect(result, completed);
    expect(container.read(transfersProvider).value!['t1'], completed);
  });
}
