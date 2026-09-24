import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:logging/logging.dart';
import 'package:myconnect_ui/src/core/api/models/event.dart';
import 'package:myconnect_ui/src/core/api/models/transfer.dart';
import 'package:myconnect_ui/src/core/providers.dart';

final _log = Logger('TransfersController');

/// Every transfer the daemon knows about in this session, by id.
final transfersProvider =
    AsyncNotifierProvider<TransfersController, Map<String, Transfer>>(
      TransfersController.new,
    );

/// Transfers newest first, optionally only those with one device.
final transferListProvider = Provider.family<List<Transfer>, String?>(
  (ref, deviceId) =>
      (ref.watch(transfersProvider).value?.values ?? const <Transfer>[])
          .where(
            (transfer) => deviceId == null || transfer.deviceId == deviceId,
          )
          .toList()
        ..sort((a, b) => b.createdAt.compareTo(a.createdAt)),
);

/// Holds a snapshot from `GET /transfers`, patched by transfer events and
/// refetched whenever the event stream (re)connects.
///
/// The daemon publishes progress at most ten times a second per transfer,
/// and each event is a full snapshot, so upserting every one is cheap.
class TransfersController extends AsyncNotifier<Map<String, Transfer>> {
  @override
  Future<Map<String, Transfer>> build() async {
    final subscription = ref
        .watch(daemonEventsProvider)
        .events
        .listen(_onEvent);
    ref.onDispose(subscription.cancel);
    final api = await ref.watch(apiProvider.future);
    return _byId(await api.transfers());
  }

  Future<void> refresh() async {
    try {
      final api = await ref.read(apiProvider.future);
      final transfers = await api.transfers();
      if (ref.mounted) state = AsyncData(_byId(transfers));
    } on Object catch (error) {
      _log.warning('Transfer refresh failed: $error');
    }
  }

  /// Send a file to a paired, connected device. Completes when the upload
  /// ends; the transfer shows up from its `transfer.started` event before
  /// that.
  Future<Transfer> send(String deviceId, String path) async => _upsert(
    await (await ref.read(apiProvider.future)).sendFile(deviceId, path),
  );

  /// Save a file from a device into the download folder. Completes once the
  /// download has started.
  Future<Transfer> download(String deviceId, String path) async => _upsert(
    await (await ref.read(apiProvider.future)).downloadFile(deviceId, path),
  );

  /// Upload a local file into [directory] on a device. Completes when the
  /// upload ends.
  Future<Transfer> upload(
    String deviceId,
    String directory,
    String localPath,
  ) async => _upsert(
    await (await ref.read(apiProvider.future))
        .uploadFile(deviceId, directory, localPath),
  );

  Future<Transfer> cancel(String transferId) async => _upsert(
    await (await ref.read(apiProvider.future)).cancelTransfer(transferId),
  );

  void _onEvent(DaemonEvent event) {
    switch (event) {
      case EventStreamConnected():
        unawaited(refresh());
      case TransferChanged(:final transfer):
        _upsert(transfer);
      case _:
        break;
    }
  }

  /// Store [transfer] unless a newer snapshot is already held. An HTTP
  /// response can arrive after events that superseded it: a send returns
  /// only once the upload ends, by which time `transfer.completed` may
  /// already be in.
  Transfer _upsert(Transfer transfer) {
    final current = state.value;
    if (current == null || !ref.mounted) return transfer;
    final existing = current[transfer.id];
    if (existing != null && _isNewer(existing, transfer)) return existing;
    state = AsyncData({...current, transfer.id: transfer});
    return transfer;
  }

  static bool _isNewer(Transfer a, Transfer b) =>
      (a.status.isTerminal && !b.status.isTerminal) ||
      a.updatedAt > b.updatedAt;

  static Map<String, Transfer> _byId(List<Transfer> transfers) => {
    for (final transfer in transfers) transfer.id: transfer,
  };
}
