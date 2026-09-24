import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:myconnect_ui/src/core/api/models/device.dart';
import 'package:myconnect_ui/src/core/routing/router.dart';
import 'package:myconnect_ui/src/features/devices/devices_controller.dart';
import 'package:myconnect_ui/src/features/transfers/transfers_controller.dart';
import 'package:myconnect_ui/src/shared/widgets.dart';

/// Paired devices that would accept a file right now.
final fileRecipientsProvider = Provider<List<Device>>(
  (ref) => [
    for (final device in ref.watch(pairedDevicesProvider).value ?? <Device>[])
      if (device.acceptsFiles) device,
  ],
);

/// The navigator's context, for dialogs opened from above the router (the
/// tray, the window-wide drop target), or `null` before the first route.
BuildContext? navigatorContext(WidgetRef ref) =>
    ref.read(routerProvider).routerDelegate.navigatorKey.currentContext;

/// Send [paths] to [to], or, when that device can't take files right now
/// (or none was given), to a device the user picks. Opens the recipient's
/// page, where the transfers show up.
///
/// [context] must be under the navigator. The uploads outlive the page
/// that started them, so errors go through a messenger captured up front.
Future<void> confirmAndSendFiles(
  BuildContext context,
  WidgetRef ref,
  List<String> paths, {
  Device? to,
}) async {
  if (paths.isEmpty) return;
  final messenger = ScaffoldMessenger.of(context);
  final router = GoRouter.of(context);
  final transfers = ref.read(transfersProvider.notifier);
  final device = to != null && to.acceptsFiles
      ? to
      : await showDialog<Device>(
          context: context,
          builder: (context) => SendFilesDialog(paths),
        );
  if (device == null) return;
  router.go('/devices/${device.deviceId}');
  await sendFiles(transfers, messenger, device, paths);
}

/// Send [paths] to [device] one at a time, and report any that failed in a
/// single snackbar.
Future<void> sendFiles(
  TransfersController transfers,
  ScaffoldMessengerState messenger,
  Device device,
  List<String> paths,
) async {
  final failures = <(String, Object)>[];
  for (final path in paths) {
    try {
      await transfers.send(device.deviceId, path);
    } on Object catch (error) {
      failures.add((path, error));
    }
  }
  final message = switch (failures) {
    [] => null,
    [(final path, final error)] =>
      "Couldn't send ${fileName(path)}: ${describeError(error)}",
    [(_, final error), ...] =>
      "Couldn't send ${failures.length} files: ${describeError(error)}",
  };
  if (message != null) {
    messenger.showSnackBar(SnackBar(content: Text(message)));
  }
}

/// The last segment of [path], on any platform's separators.
String fileName(String path) => path.split(RegExp(r'[/\\]')).last;

/// Asks which device to send files to. Pops with the chosen [Device], or
/// `null` when cancelled. The list follows devices as they connect and drop.
class SendFilesDialog extends ConsumerWidget {
  const new(this.paths, {super.key});

  final List<String> paths;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final recipients = ref.watch(fileRecipientsProvider);
    final title = paths.length == 1
        ? 'Send ${fileName(paths.single)}'
        : 'Send ${paths.length} files';
    return AlertDialog(
      title: Text(title, overflow: TextOverflow.ellipsis),
      contentPadding: const EdgeInsets.fromLTRB(0, 16, 0, 0),
      content: SizedBox(
        width: 400,
        child: recipients.isEmpty
            ? const Padding(
                padding: EdgeInsets.symmetric(horizontal: 24),
                child: Text(
                  'No paired device is connected and able to receive files.',
                ),
              )
            : ListView(
                shrinkWrap: true,
                children: [
                  const Padding(
                    padding: EdgeInsets.fromLTRB(24, 0, 24, 8),
                    child: Text('Choose the device to send to:'),
                  ),
                  for (final device in recipients)
                    ListTile(
                      contentPadding: const EdgeInsets.symmetric(
                        horizontal: 24,
                      ),
                      leading: Icon(deviceIcon(device.deviceType)),
                      title: Text(device.deviceName),
                      onTap: () => Navigator.pop(context, device),
                    ),
                ],
              ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
      ],
    );
  }
}
