import 'dart:async';

import 'package:file_selector/file_selector.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:material_ui/material_ui.dart';
import 'package:myconnect_ui/src/core/api/models/settings.dart';
import 'package:myconnect_ui/src/features/settings/settings_controller.dart';
import 'package:myconnect_ui/src/shared/widgets.dart';

/// The version users see: the release tag CI builds with
/// (`--dart-define=MYCONNECT_VERSION`), or `dev` for a local build. The
/// `const` matters: outside a constant context `fromEnvironment` always
/// returns the default.
const _appVersion = String.fromEnvironment(
  'MYCONNECT_VERSION',
  defaultValue: 'dev',
);

/// The daemon's settings. Every change is saved by the daemon right away.
class SettingsPage extends ConsumerWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final settings = ref.watch(settingsProvider);
    return Scaffold(
      appBar: AppBar(title: const Text('Settings')),
      body: switch (settings) {
        AsyncData(value: final settings) => _SettingsList(settings),
        AsyncError(:final error) => ErrorView(
          error: error,
          onRetry: () => ref.invalidate(settingsProvider),
        ),
        _ => const Center(child: CircularProgressIndicator()),
      },
    );
  }
}

class _SettingsList extends ConsumerWidget {
  const new(this.settings);

  final DaemonSettings settings;

  /// Run [change], reporting a failure through a messenger captured first,
  /// since the resulting event can rebuild this page mid-await.
  Future<void> _apply(
    BuildContext context,
    WidgetRef ref,
    Future<void> Function(SettingsController controller) change,
  ) async {
    final messenger = ScaffoldMessenger.of(context);
    final controller = ref.read(settingsProvider.notifier);
    try {
      await change(controller);
    } on Object catch (error) {
      messenger.showSnackBar(SnackBar(content: Text(describeError(error))));
    }
  }

  Future<void> _chooseDownloadDir(BuildContext context, WidgetRef ref) async {
    final directory = await getDirectoryPath(
      initialDirectory: settings.downloadDir,
      confirmButtonText: 'Choose',
    );
    if (directory == null || !context.mounted) return;
    await _apply(
      context,
      ref,
      (controller) => controller.setDownloadDir(directory),
    );
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) => ListView(
    children: [
      ListTile(
        leading: const Icon(Icons.badge_outlined),
        title: const Text('Device name'),
        subtitle: Text(settings.deviceName),
        trailing: const Icon(Icons.edit_outlined),
        onTap: () => showDialog<void>(
          context: context,
          builder: (_) => _RenameDialog(settings.deviceName),
        ),
      ),
      ListTile(
        leading: const Icon(Icons.folder_outlined),
        title: const Text('Save received files in'),
        subtitle: Text(settings.downloadDir),
        trailing: const Icon(Icons.edit_outlined),
        onTap: () => unawaited(_chooseDownloadDir(context, ref)),
      ),
      SwitchListTile(
        secondary: const Icon(Icons.content_paste_outlined),
        title: const Text('Sync clipboard'),
        subtitle: const Text('Share copied text with paired devices'),
        value: settings.clipboardSyncEnabled,
        onChanged: (enabled) => unawaited(
          _apply(
            context,
            ref,
            (controller) =>
                controller.setClipboardSyncEnabled(enabled: enabled),
          ),
        ),
      ),
      SwitchListTile(
        secondary: const Icon(Icons.close_fullscreen_outlined),
        title: const Text('Keep running when the window is closed'),
        subtitle: const Text(
          'Stay in the tray so devices can still reach this computer',
        ),
        value: settings.closeToTray,
        onChanged: (enabled) => unawaited(
          _apply(
            context,
            ref,
            (controller) => controller.setCloseToTray(enabled: enabled),
          ),
        ),
      ),
      const ListTile(
        leading: Icon(Icons.info_outline),
        title: Text('Version'),
        subtitle: Text(_appVersion),
      ),
    ],
  );
}

/// Asks for a new device name and keeps the daemon's objection, if any, on
/// screen until the name is fixed.
class _RenameDialog extends ConsumerStatefulWidget {
  const new(this.currentName);

  final String currentName;

  @override
  ConsumerState<_RenameDialog> createState() => _RenameDialogState();
}

class _RenameDialogState extends ConsumerState<_RenameDialog> {
  late final _name = TextEditingController(text: widget.currentName);
  String? _error;
  bool _saving = false;

  Future<void> _save() async {
    final navigator = Navigator.of(context);
    setState(() {
      _saving = true;
      _error = null;
    });
    try {
      await ref.read(settingsProvider.notifier).rename(_name.text);
      navigator.pop();
    } on Object catch (error) {
      if (mounted) setState(() => _error = describeError(error));
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }

  @override
  void dispose() {
    _name.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: const Text('Device name'),
    content: TextField(
      controller: _name,
      autofocus: true,
      maxLength: 32,
      decoration: InputDecoration(
        helperText: 'How this computer appears on your other devices',
        errorText: _error,
        errorMaxLines: 3,
      ),
      // Replaces the default unfocus on Enter, so the field keeps focus to
      // fix a rejected name.
      onEditingComplete: _saving ? null : () => unawaited(_save()),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.of(context).pop(),
        child: const Text('Cancel'),
      ),
      FilledButton(
        onPressed: _saving ? null : () => unawaited(_save()),
        child: const Text('Save'),
      ),
    ],
  );
}
