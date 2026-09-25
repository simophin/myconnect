import 'package:dio/dio.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:myconnect_ui/src/core/api/api_exception.dart';
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
    when(() => daemon.api.sendFileWithAnyId('a' * 32, '/tmp/photo.jpg'))
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

  test('an upload whose transfer ends early stops sending', () async {
    await container.read(transfersProvider.future);
    CancelToken? token;
    String? chosenId;
    // Like Dart's HttpClient, the fake reads no answer until it has sent
    // the whole file, which here never happens unless it is stopped.
    when(() => daemon.api.sendFileWithAnyId('a' * 32, '/tmp/big.iso'))
        .thenAnswer((invocation) async {
          chosenId = invocation.namedArguments[#transferId] as String;
          token = invocation.namedArguments[#cancelToken] as CancelToken;
          await daemon.emit(
            TransferChanged(
              transfer(id: chosenId!, direction: TransferDirection.outgoing),
            ),
          );
          await token!.whenCancel;
          throw const ApiException(code: 'daemon_unavailable');
        });

    final sending = container
        .read(transfersProvider.notifier)
        .send('a' * 32, '/tmp/big.iso');
    await pumpEventQueue();
    expect(token!.isCancelled, isFalse);

    final cancelled = transfer(
      id: chosenId!,
      direction: TransferDirection.outgoing,
      status: TransferStatus.cancelled,
      updatedAt: 5,
    );
    await daemon.emit(TransferChanged(cancelled));

    expect(await sending, cancelled);
    expect(token!.isCancelled, isTrue);
  });

  test('a completed upload is left to finish', () async {
    await container.read(transfersProvider.future);
    when(() => daemon.api.sendFileWithAnyId('a' * 32, '/tmp/photo.jpg'))
        .thenAnswer((invocation) async {
          final id = invocation.namedArguments[#transferId] as String;
          final token = invocation.namedArguments[#cancelToken] as CancelToken;
          final completed = transfer(
            id: id,
            direction: TransferDirection.outgoing,
            status: TransferStatus.completed,
            transferredBytes: 100,
          );
          await daemon.emit(TransferChanged(completed));
          expect(token.isCancelled, isFalse);
          return completed;
        });

    final result = await container
        .read(transfersProvider.notifier)
        .send('a' * 32, '/tmp/photo.jpg');
    expect(result.status, TransferStatus.completed);
  });
}
