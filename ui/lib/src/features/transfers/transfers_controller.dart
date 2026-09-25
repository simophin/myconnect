import 'dart:async';

import 'package:dio/dio.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:logging/logging.dart';
import 'package:myconnect_ui/src/core/api/event_stream.dart';
import 'package:myconnect_ui/src/core/api/models/event.dart';
import 'package:myconnect_ui/src/core/api/models/transfer.dart';
import 'package:myconnect_ui/src/core/api/myconnect_api.dart';
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
    return await _replay.fetch(() async => _byId(await api.transfers()));
  }

  final _replay = SnapshotReplay<Map<String, Transfer>>(_applied);

  Future<void> refresh() async {
    try {
      final api = await ref.read(apiProvider.future);
      final transfers = await _replay.fetch(
        () async => _byId(await api.transfers()),
      );
      if (ref.mounted) state = AsyncData(transfers);
      transfers.values.forEach(_stopUploadIfEnded);
    } on Object catch (error) {
      _log.warning('Transfer refresh failed: $error');
    }
  }

  /// Send a file to a paired, connected device. Completes when the upload
  /// ends; the transfer shows up from its `transfer.started` event before
  /// that.
  Future<Transfer> send(String deviceId, String path) => _upload(
    (api, id, cancel) =>
        api.sendFile(deviceId, path, transferId: id, cancelToken: cancel),
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
  ) => _upload(
    (api, id, cancel) => api.uploadFile(
      deviceId,
      directory,
      localPath,
      transferId: id,
      cancelToken: cancel,
    ),
  );

  /// Uploads in flight, by the id chosen for their transfer.
  final _uploads = <String, CancelToken>{};

  /// Run an upload as a transfer with an id chosen here, and stop sending
  /// once that transfer ends before the upload does (cancelled from either
  /// end, or failed). The daemon answers at once then, but Dart's
  /// `HttpClient` reads an answer only after sending the whole body, which
  /// for a large file takes as long as the upload would have. Completes
  /// with the ended transfer rather than the error of the stopped request.
  Future<Transfer> _upload(
    Future<Transfer> Function(MyConnectApi api, String id, CancelToken cancel)
    start,
  ) async {
    final api = await ref.read(apiProvider.future);
    final id = newTransferId();
    final cancel = CancelToken();
    _uploads[id] = cancel;
    try {
      return _upsert(await start(api, id, cancel));
    } on Object {
      final ended = state.value?[id];
      if (cancel.isCancelled && ended != null) return ended;
      rethrow;
    } finally {
      _uploads.remove(id);
    }
  }

  /// Stop sending the upload of [transfer] if it ended without completing.
  void _stopUploadIfEnded(Transfer transfer) {
    if (transfer.status.isTerminal &&
        transfer.status != TransferStatus.completed) {
      _uploads[transfer.id]?.cancel();
    }
  }

  Future<Transfer> cancel(String transferId) async => _upsert(
    await (await ref.read(apiProvider.future)).cancelTransfer(transferId),
  );

  void _onEvent(DaemonEvent event) {
    _replay.record(event);
    switch (event) {
      case EventStreamConnected():
        unawaited(refresh());
      case TransferChanged(:final transfer):
        _upsert(transfer);
        _stopUploadIfEnded(transfer);
      case _:
        break;
    }
  }

  Transfer _upsert(Transfer transfer) {
    final current = state.value;
    if (current == null || !ref.mounted) return transfer;
    final updated = _with(current, transfer);
    if (!identical(updated, current)) state = AsyncData(updated);
    return updated[transfer.id]!;
  }

  static Map<String, Transfer> _applied(
    Map<String, Transfer> transfers,
    DaemonEvent event,
  ) => switch (event) {
    TransferChanged(:final transfer) => _with(transfers, transfer),
    _ => transfers,
  };

  /// [transfers] with [transfer] stored, unless a newer snapshot is already
  /// held. An HTTP response can arrive after events that superseded it: a
  /// send returns only once the upload ends, by which time
  /// `transfer.completed` may already be in.
  static Map<String, Transfer> _with(
    Map<String, Transfer> transfers,
    Transfer transfer,
  ) {
    final existing = transfers[transfer.id];
    if (existing != null && _isNewer(existing, transfer)) return transfers;
    return {...transfers, transfer.id: transfer};
  }

  static bool _isNewer(Transfer a, Transfer b) =>
      (a.status.isTerminal && !b.status.isTerminal) ||
      a.updatedAt > b.updatedAt;

  static Map<String, Transfer> _byId(List<Transfer> transfers) => {
    for (final transfer in transfers) transfer.id: transfer,
  };
}
