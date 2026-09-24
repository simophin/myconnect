import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:material_ui/material_ui.dart';
import 'package:myconnect_ui/src/core/api/models/pairing.dart';
import 'package:myconnect_ui/src/features/pairing/pairings_controller.dart';
import 'package:myconnect_ui/src/shared/widgets.dart';

/// Shows a modal prompt over [child] while an incoming pairing request waits
/// for the user, whatever screen is open.
///
/// The prompt is derived from [pendingIncomingPairingsProvider] rather than
/// pushed as a route, so it disappears by itself when the request is resolved
/// elsewhere (the CLI, a timeout, the peer cancelling).
class IncomingPairingPrompt extends ConsumerWidget {
  const new({required this.child, super.key});

  final Widget child;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final pending = ref.watch(pendingIncomingPairingsProvider);
    return Stack(
      children: [
        child,
        if (pending.isNotEmpty) ...[
          const ModalBarrier(dismissible: false, color: Colors.black54),
          Center(
            child: _IncomingPairingCard(
              key: ValueKey(pending.first.id),
              pairing: pending.first,
              queued: pending.length - 1,
            ),
          ),
        ],
      ],
    );
  }
}

class _IncomingPairingCard extends ConsumerStatefulWidget {
  const new({required this.pairing, required this.queued, super.key});

  final Pairing pairing;
  final int queued;

  @override
  ConsumerState<_IncomingPairingCard> createState() =>
      _IncomingPairingCardState();
}

class _IncomingPairingCardState extends ConsumerState<_IncomingPairingCard> {
  bool _busy = false;
  Object? _error;

  Future<void> _resolve({required bool accept}) async {
    setState(() {
      _busy = true;
      _error = null;
    });
    final controller = ref.read(pairingsProvider.notifier);
    try {
      accept
          ? await controller.accept(widget.pairing.id)
          : await controller.reject(widget.pairing.id);
    } on Object catch (error) {
      if (mounted) setState(() => _error = error);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final pairing = widget.pairing;
    return ConstrainedBox(
      constraints: const BoxConstraints(maxWidth: 420),
      child: Card(
        margin: const EdgeInsets.all(24),
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text('Pairing request', style: theme.textTheme.headlineSmall),
              const SizedBox(height: 12),
              Text(
                '${pairing.deviceName} wants to pair with this computer. '
                'Accept only if it shows the same code:',
              ),
              if (pairing.verificationCode case final code?) ...[
                const SizedBox(height: 16),
                Center(child: VerificationCode(code)),
              ],
              if (widget.queued > 0) ...[
                const SizedBox(height: 12),
                Text(
                  '${widget.queued} more request(s) waiting',
                  style: theme.textTheme.bodySmall,
                ),
              ],
              if (_error case final error?) ...[
                const SizedBox(height: 12),
                Text(
                  describeError(error),
                  style: TextStyle(color: theme.colorScheme.error),
                ),
              ],
              const SizedBox(height: 24),
              Row(
                mainAxisAlignment: MainAxisAlignment.end,
                children: [
                  TextButton(
                    onPressed: _busy ? null : () => _resolve(accept: false),
                    child: const Text('Reject'),
                  ),
                  const SizedBox(width: 8),
                  FilledButton(
                    onPressed: _busy ? null : () => _resolve(accept: true),
                    child: const Text('Accept'),
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }
}
