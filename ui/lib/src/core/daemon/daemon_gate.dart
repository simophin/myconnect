import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:material_ui/material_ui.dart';
import 'package:myconnect_ui/src/core/providers.dart';
import 'package:myconnect_ui/src/shared/widgets.dart';

/// Shows [child] only once the daemon is running, and a thin banner whenever
/// the event stream has dropped and is reconnecting.
class DaemonGate extends ConsumerWidget {
  const new({required this.child, super.key});

  final Widget child;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return switch (ref.watch(daemonEndpointProvider)) {
      AsyncData() => Column(
        children: [
          const _ReconnectingBanner(),
          Expanded(child: child),
        ],
      ),
      AsyncError(:final error) => Material(
        child: ErrorView.message(
          'MyConnect could not start.\n$error',
          onRetry: () => ref.invalidate(daemonEndpointProvider),
        ),
      ),
      _ => const Material(
        child: Center(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              CircularProgressIndicator(),
              SizedBox(height: 16),
              Text('Starting MyConnect…'),
            ],
          ),
        ),
      ),
    };
  }
}

class _ReconnectingBanner extends ConsumerWidget {
  const new();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    if (ref.watch(eventStreamStateProvider) != EventStreamState.reconnecting) {
      return const SizedBox.shrink();
    }
    final colors = Theme.of(context).colorScheme;
    return Material(
      color: colors.errorContainer,
      child: SafeArea(
        bottom: false,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 6),
          child: Row(
            children: [
              Icon(
                Icons.sync_problem,
                size: 18,
                color: colors.onErrorContainer,
              ),
              const SizedBox(width: 8),
              Text(
                'Lost connection to MyConnect, reconnecting…',
                style: TextStyle(color: colors.onErrorContainer),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
