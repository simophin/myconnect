import 'package:material_ui/material_ui.dart';
import 'package:myconnect_ui/src/core/api/api_exception.dart';
import 'package:myconnect_ui/src/core/api/models/device.dart';

IconData deviceIcon(DeviceType type) => switch (type) {
  DeviceType.desktop => Icons.desktop_windows_outlined,
  DeviceType.laptop => Icons.laptop_outlined,
  DeviceType.phone => Icons.smartphone_outlined,
  DeviceType.tablet => Icons.tablet_outlined,
  DeviceType.tv => Icons.tv_outlined,
  DeviceType.unknown => Icons.devices_other_outlined,
};

String reachabilityLabel(Device device) => switch (device.reachability) {
  DeviceReachability.connected => 'Connected',
  DeviceReachability.discovered => 'Nearby',
  DeviceReachability.unavailable => 'Not reachable',
  DeviceReachability.unknown => 'Unknown',
};

/// Whether [device] is reachable, with its battery when it reported one,
/// e.g. `Connected · 82%, charging`.
String deviceStatusLabel(Device device) => switch (device.battery) {
  null => reachabilityLabel(device),
  BatteryStatus(:final charge, charging: true) =>
    '${reachabilityLabel(device)} · $charge%, charging',
  BatteryStatus(:final charge) => '${reachabilityLabel(device)} · $charge%',
};

/// A user-facing sentence for any error the UI may catch.
String describeError(Object error) =>
    error is ApiException ? error.message : 'Something went wrong: $error';

void showErrorSnackBar(BuildContext context, Object error) {
  ScaffoldMessenger.of(context)
      .showSnackBar(SnackBar(content: Text(describeError(error))));
}

/// Centered error with an optional retry.
class ErrorView extends StatelessWidget {
  new({required Object error, this.onRetry, super.key})
    : message = describeError(error);

  const new message(this.message, {this.onRetry, super.key});

  final String message;
  final VoidCallback? onRetry;

  @override
  Widget build(BuildContext context) => Center(
    child: Padding(
      padding: const EdgeInsets.all(24),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(
            Icons.error_outline,
            size: 48,
            color: Theme.of(context).colorScheme.error,
          ),
          const SizedBox(height: 16),
          Text(message, textAlign: TextAlign.center),
          if (onRetry != null) ...[
            const SizedBox(height: 16),
            FilledButton.tonal(onPressed: onRetry, child: const Text('Retry')),
          ],
        ],
      ),
    ),
  );
}

/// A pairing verification code, large and letter-spaced so it is easy to
/// compare with the code on the other device.
class VerificationCode extends StatelessWidget {
  const new(this.code, {super.key});

  final String code;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 12),
      decoration: BoxDecoration(
        color: theme.colorScheme.surfaceContainerHighest,
        borderRadius: BorderRadius.circular(12),
      ),
      child: SelectableText(
        code,
        style: theme.textTheme.headlineMedium?.copyWith(
          fontFamily: 'monospace',
          letterSpacing: 4,
          fontWeight: FontWeight.w600,
        ),
      ),
    );
  }
}
