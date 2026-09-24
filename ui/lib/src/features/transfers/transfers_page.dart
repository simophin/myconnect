import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:material_ui/material_ui.dart';
import 'package:myconnect_ui/src/features/transfers/transfer_tile.dart';
import 'package:myconnect_ui/src/features/transfers/transfers_controller.dart';
import 'package:myconnect_ui/src/shared/widgets.dart';

/// Every transfer in this daemon session, newest first.
class TransfersPage extends ConsumerWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final loaded = ref.watch(transfersProvider);
    final transfers = ref.watch(transferListProvider(null));
    return Scaffold(
      appBar: AppBar(title: const Text('Transfers')),
      body: switch (loaded) {
        AsyncData() when transfers.isEmpty => Center(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(
                Icons.swap_vert,
                size: 64,
                color: Theme.of(context).colorScheme.outline,
              ),
              const SizedBox(height: 16),
              const Text('No transfers yet'),
            ],
          ),
        ),
        AsyncData() => ListView(
          children: [
            for (final transfer in transfers)
              TransferTile(transfer, key: ValueKey(transfer.id)),
          ],
        ),
        AsyncError(:final error) => ErrorView(
          error: error,
          onRetry: () => ref.invalidate(transfersProvider),
        ),
        _ => const Center(child: CircularProgressIndicator()),
      },
    );
  }
}
