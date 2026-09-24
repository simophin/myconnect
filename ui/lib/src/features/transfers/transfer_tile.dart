import 'dart:io';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:material_ui/material_ui.dart';
import 'package:myconnect_ui/src/core/api/models/transfer.dart';
import 'package:myconnect_ui/src/features/transfers/transfers_controller.dart';
import 'package:myconnect_ui/src/shared/widgets.dart';
import 'package:url_launcher/url_launcher.dart';

/// One transfer: direction, file, progress or outcome, and the actions that
/// apply to it (cancel while running, open once received).
class TransferTile extends ConsumerWidget {
  const new(this.transfer, {this.showDevice = true, super.key});

  final Transfer transfer;

  /// Name the peer device, for lists that mix devices.
  final bool showDevice;

  Future<void> _cancel(BuildContext context, WidgetRef ref) async {
    final messenger = ScaffoldMessenger.of(context);
    try {
      await ref.read(transfersProvider.notifier).cancel(transfer.id);
    } on Object catch (error) {
      messenger.showSnackBar(SnackBar(content: Text(describeError(error))));
    }
  }

  Future<void> _open(BuildContext context, String path) async {
    final messenger = ScaffoldMessenger.of(context);
    if (!await launchUrl(Uri.file(path))) {
      messenger.showSnackBar(SnackBar(content: Text('Couldn’t open $path')));
    }
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final incoming = transfer.direction == TransferDirection.incoming;
    final savedPath = transfer.savedPath;
    final peer = showDevice
        ? '${incoming ? 'From' : 'To'} ${transfer.deviceName} · '
        : '';
    return ListTile(
      leading: Icon(incoming ? Icons.download : Icons.upload),
      title: Text(transfer.fileName, overflow: TextOverflow.ellipsis),
      subtitle: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('$peer${transferStatusLabel(transfer)}'),
          if (!transfer.status.isTerminal)
            Padding(
              padding: const EdgeInsets.only(top: 4),
              child: LinearProgressIndicator(
                value: transfer.status == TransferStatus.transferring
                    ? transfer.progress
                    : null,
              ),
            ),
        ],
      ),
      trailing: switch (transfer.status) {
        _ when !transfer.status.isTerminal => IconButton(
          tooltip: 'Cancel',
          icon: const Icon(Icons.close),
          onPressed: () => _cancel(context, ref),
        ),
        TransferStatus.completed when savedPath != null => Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            IconButton(
              tooltip: 'Open file',
              icon: const Icon(Icons.open_in_new),
              onPressed: () => _open(context, savedPath),
            ),
            IconButton(
              tooltip: 'Open folder',
              icon: const Icon(Icons.folder_open),
              onPressed: () => _open(context, File(savedPath).parent.path),
            ),
          ],
        ),
        _ => null,
      },
    );
  }
}

String transferStatusLabel(Transfer transfer) => switch (transfer.status) {
  TransferStatus.queued => 'Waiting',
  TransferStatus.connecting => 'Connecting',
  TransferStatus.transferring =>
    '${formatBytes(transfer.transferredBytes)} of '
        '${formatBytes(transfer.totalBytes)}',
  TransferStatus.completed => formatBytes(transfer.totalBytes),
  TransferStatus.cancelled => 'Cancelled',
  TransferStatus.failed => switch (transfer.errorCode) {
    'connection_failed' => 'Failed: connection lost',
    'timed_out' => 'Failed: timed out',
    'unavailable' => 'Failed: refused by the receiver',
    'protocol_error' => 'Failed: the device sent something unexpected',
    _ => 'Failed',
  },
  TransferStatus.unknown => 'Unknown',
};

/// A byte count in the largest unit that keeps it at or above 1.
String formatBytes(int bytes) {
  const units = ['bytes', 'KB', 'MB', 'GB', 'TB'];
  var value = bytes.toDouble();
  var unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return unit == 0
      ? '$bytes bytes'
      : '${value.toStringAsFixed(value < 10 ? 1 : 0)} ${units[unit]}';
}
