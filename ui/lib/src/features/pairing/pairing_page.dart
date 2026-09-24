import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:myconnect_ui/src/core/api/models/pairing.dart';
import 'package:myconnect_ui/src/features/pairing/pairings_controller.dart';
import 'package:myconnect_ui/src/shared/widgets.dart';

/// Progress of an outgoing pairing request: shows the verification code while
/// the other device decides, then the outcome.
class PairingPage extends ConsumerStatefulWidget {
  const new({required this.pairingId, super.key});

  final String pairingId;

  @override
  ConsumerState<PairingPage> createState() => _PairingPageState();
}

class _PairingPageState extends ConsumerState<PairingPage> {
  bool _busy = false;

  Future<void> _run(Future<void> Function() action) async {
    setState(() => _busy = true);
    try {
      await action();
    } on Object catch (error) {
      if (mounted) showErrorSnackBar(context, error);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _cancel(Pairing pairing) => _run(() async {
    await ref.read(pairingsProvider.notifier).reject(pairing.id);
    if (mounted) context.go('/add');
  });

  Future<void> _retry(Pairing pairing) => _run(() async {
    final next = await ref
        .read(pairingsProvider.notifier)
        .start(pairing.deviceId);
    if (mounted) context.go('/add/pairing/${next.id}');
  });

  @override
  Widget build(BuildContext context) {
    final pairing = ref.watch(pairingProvider(widget.pairingId));
    return Scaffold(
      appBar: AppBar(title: const Text('Pairing')),
      body: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 420),
          child: Padding(
            padding: const EdgeInsets.all(24),
            child: pairing == null
                ? const Text('This pairing request no longer exists.')
                : _content(context, pairing),
          ),
        ),
      ),
    );
  }

  Widget _content(BuildContext context, Pairing pairing) {
    final theme = Theme.of(context);
    final name = pairing.deviceName;
    final (icon, title, detail) = switch (pairing.status) {
      PairingStatus.requested || PairingStatus.awaitingConfirmation => (
        null,
        'Waiting for $name',
        'Check that $name shows the same code, then accept the request '
            'there.',
      ),
      PairingStatus.accepted => (
        Icons.check_circle_outline,
        'Paired with $name',
        null,
      ),
      PairingStatus.rejected => (
        Icons.block,
        'Pairing declined',
        'The request was declined or cancelled.',
      ),
      PairingStatus.expired => (
        Icons.timer_off_outlined,
        'Request timed out',
        '$name did not answer in time.',
      ),
      PairingStatus.failed || PairingStatus.unknown => (
        Icons.error_outline,
        'Pairing failed',
        'The connection to $name was lost.',
      ),
    };
    final pending = !pairing.status.isTerminal;
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        if (pending)
          const CircularProgressIndicator()
        else
          Icon(icon, size: 56, color: theme.colorScheme.primary),
        const SizedBox(height: 16),
        Text(title, style: theme.textTheme.titleLarge),
        if (detail != null) ...[
          const SizedBox(height: 8),
          Text(detail, textAlign: TextAlign.center),
        ],
        if (pending && pairing.verificationCode != null) ...[
          const SizedBox(height: 24),
          VerificationCode(pairing.verificationCode!),
        ],
        const SizedBox(height: 32),
        Wrap(
          spacing: 12,
          alignment: WrapAlignment.center,
          children: switch (pairing.status) {
            _ when pending => [
              OutlinedButton(
                onPressed: _busy ? null : () => _cancel(pairing),
                child: const Text('Cancel'),
              ),
            ],
            PairingStatus.accepted => [
              FilledButton(
                onPressed: () => context.go('/devices/${pairing.deviceId}'),
                child: const Text('Done'),
              ),
            ],
            _ => [
              OutlinedButton(
                onPressed: () => context.go('/add'),
                child: const Text('Close'),
              ),
              FilledButton(
                onPressed: _busy ? null : () => _retry(pairing),
                child: const Text('Try again'),
              ),
            ],
          },
        ),
      ],
    );
  }
}
