import 'dart:async';
import 'dart:typed_data';

import 'package:file_selector/file_selector.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:myconnect_ui/src/core/api/models/device.dart';
import 'package:myconnect_ui/src/core/api/models/remote_file.dart';
import 'package:myconnect_ui/src/core/api/myconnect_api.dart';
import 'package:myconnect_ui/src/core/providers.dart';
import 'package:myconnect_ui/src/features/devices/devices_controller.dart';
import 'package:myconnect_ui/src/features/files/files_controller.dart';
import 'package:myconnect_ui/src/features/send/file_drop_zone.dart';
import 'package:myconnect_ui/src/features/send/send_files.dart';
import 'package:myconnect_ui/src/features/transfers/transfer_tile.dart';
import 'package:myconnect_ui/src/features/transfers/transfers_controller.dart';
import 'package:myconnect_ui/src/shared/widgets.dart';

/// Images this page previews rather than downloads when opened, up to
/// [_maxPreviewBytes].
const _previewExtensions = {'jpg', 'jpeg', 'png', 'gif', 'webp', 'bmp'};
const _maxPreviewBytes = 32 * 1024 * 1024;

/// Browse a paired device's files: its shared storage, then the folders
/// in it. Files can be downloaded, previewed, renamed and deleted, and
/// files picked or dropped on the page are uploaded to the open folder.
class FilesPage extends ConsumerStatefulWidget {
  const new({required this.deviceId, super.key});

  final String deviceId;

  @override
  ConsumerState<FilesPage> createState() => _FilesPageState();
}

enum _SortBy { name, size, modified }

class _FilesPageState extends ConsumerState<FilesPage> {
  /// The open folder, or `null` for the list of storage.
  String? _path;
  bool _showHidden = false;
  _SortBy _sortBy = _SortBy.name;
  bool _ascending = true;

  DirectoryKey get _key => (deviceId: widget.deviceId, path: _path);

  void _open(String? path) => setState(() => _path = path);

  /// Up one folder; from a storage root, back to the list of storage.
  void _up(List<RemoteFile> roots) {
    final path = _path;
    if (path == null) return;
    _open(roots.any((root) => root.path == path) ? null : parentOf(path));
  }

  void _refresh() => ref.invalidate(directoryProvider(_key));

  void _sort(_SortBy by) => setState(() {
    _ascending = _sortBy != by || !_ascending;
    _sortBy = by;
  });

  Future<void> _upload() async {
    final directory = _path;
    if (directory == null) return;
    final files = await openFiles(confirmButtonText: 'Upload');
    if (files.isEmpty || !mounted) return;
    await uploadFiles(
      ProviderScope.containerOf(context),
      ScaffoldMessenger.of(context),
      widget.deviceId,
      directory,
      [for (final file in files) file.path],
    );
  }

  Future<void> _createFolder() async {
    final directory = _path;
    if (directory == null) return;
    final name = await showDialog<String>(
      context: context,
      builder: (context) =>
          const _NameDialog(title: 'New folder', action: 'Create'),
    );
    if (name == null || !mounted) return;
    await _change(
      (api) => api.createDirectory(widget.deviceId, childOf(directory, name)),
    );
  }

  Future<void> _rename(RemoteFile file) async {
    final name = await showDialog<String>(
      context: context,
      builder: (context) =>
          _NameDialog(title: 'Rename', action: 'Rename', initial: file.name),
    );
    final parent = parentOf(file.path);
    if (name == null || name == file.name || parent == null || !mounted) {
      return;
    }
    await _change(
      (api) => api.moveFile(widget.deviceId, file.path, childOf(parent, name)),
    );
  }

  Future<void> _delete(RemoteFile file) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text('Delete ${file.name}?'),
        content: Text(
          file.isDirectory
              ? 'The folder and everything in it will be deleted from the '
                    'device. This can’t be undone.'
              : 'The file will be deleted from the device. This can’t be '
                    'undone.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: const Text('Delete'),
          ),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;
    await _change((api) => api.deleteFile(widget.deviceId, file.path));
  }

  /// Run a change to the open folder, then show its new contents.
  Future<void> _change(Future<void> Function(MyConnectApi api) change) async {
    final messenger = ScaffoldMessenger.of(context);
    final key = _key;
    try {
      await change(await ref.read(apiProvider.future));
    } on Object catch (error) {
      messenger.showSnackBar(SnackBar(content: Text(describeError(error))));
    }
    if (mounted) ref.invalidate(directoryProvider(key));
  }

  Future<void> _download(RemoteFile file) async {
    final messenger = ScaffoldMessenger.of(context);
    final router = GoRouter.of(context);
    try {
      await ref
          .read(transfersProvider.notifier)
          .download(widget.deviceId, file.path);
      messenger.showSnackBar(
        SnackBar(
          content: Text('Downloading ${file.name}'),
          // A snackbar with an action otherwise stays until dismissed.
          persist: false,
          action: SnackBarAction(
            label: 'Transfers',
            onPressed: () => router.go('/transfers'),
          ),
        ),
      );
    } on Object catch (error) {
      messenger.showSnackBar(SnackBar(content: Text(describeError(error))));
    }
  }

  Future<void> _preview(RemoteFile file) => showDialog<void>(
    context: context,
    builder: (context) => _PreviewDialog(deviceId: widget.deviceId, file: file),
  );

  void _activate(RemoteFile file) {
    if (file.isDirectory) {
      _open(file.path);
    } else if (_canPreview(file)) {
      unawaited(_preview(file));
    } else {
      unawaited(_download(file));
    }
  }

  @override
  Widget build(BuildContext context) {
    final device = ref.watch(deviceProvider(widget.deviceId));
    final roots =
        ref
            .watch(directoryProvider((deviceId: widget.deviceId, path: null)))
            .value
            ?.entries ??
        const <RemoteFile>[];
    final inFolder = _path != null;
    final available = device?.sharesFiles ?? false;
    return Scaffold(
      appBar: AppBar(
        title: Text(device == null ? 'Files' : 'Files on ${device.deviceName}'),
        actions: [
          IconButton(
            tooltip: 'Upload files',
            onPressed: available && inFolder ? _upload : null,
            icon: const Icon(Icons.upload_file),
          ),
          IconButton(
            tooltip: 'New folder',
            onPressed: available && inFolder ? _createFolder : null,
            icon: const Icon(Icons.create_new_folder_outlined),
          ),
          IconButton(
            tooltip: 'Refresh',
            onPressed: available ? _refresh : null,
            icon: const Icon(Icons.refresh),
          ),
          PopupMenuButton<void>(
            itemBuilder: (context) => [
              CheckedPopupMenuItem(
                checked: _showHidden,
                onTap: () => setState(() => _showHidden = !_showHidden),
                child: const Text('Show hidden files'),
              ),
            ],
          ),
        ],
      ),
      body: switch (device) {
        null => const Center(child: Text('This device is no longer known.')),
        Device(sharesFiles: false) => ErrorView.message(
          device.isConnected
              ? '${device.deviceName} doesn’t share its files.'
              : 'Connect ${device.deviceName} to browse its files.',
        ),
        _ => Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            _Breadcrumbs(
              path: _path,
              roots: roots,
              onOpen: _open,
              onUp: inFolder ? () => _up(roots) : null,
            ),
            const Divider(height: 1),
            Expanded(
              child: FileDropTarget(
                deviceId: widget.deviceId,
                directory: _path,
                child: _listing(context),
              ),
            ),
          ],
        ),
      },
    );
  }

  Widget _listing(BuildContext context) {
    final listing = ref.watch(directoryProvider(_key));
    return switch (listing) {
      AsyncData(:final value) => _entries(context, value),
      AsyncError(:final error) => ErrorView(error: error, onRetry: _refresh),
      _ => Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const CircularProgressIndicator(),
            if (_path == null) ...[
              const SizedBox(height: 16),
              const Text('Connecting to the device…'),
            ],
          ],
        ),
      ),
    };
  }

  Widget _entries(BuildContext context, DirectoryListing listing) {
    final entries = [
      for (final entry in listing.entries)
        if (_showHidden || !entry.name.startsWith('.')) entry,
    ];
    if (listing.path == null) {
      return entries.isEmpty
          ? const _Empty('The device isn’t sharing any storage.')
          : ListView(
              children: [
                for (final root in entries)
                  ListTile(
                    leading: const Icon(Icons.sd_storage_outlined),
                    title: Text(root.name),
                    subtitle: Text(root.path),
                    onTap: () => _open(root.path),
                  ),
              ],
            );
    }
    entries.sort(_compare);
    return LayoutBuilder(
      builder: (context, constraints) {
        final wide = constraints.maxWidth >= 600;
        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            _HeaderRow(
              wide: wide,
              sortBy: _sortBy,
              ascending: _ascending,
              onSort: _sort,
            ),
            const Divider(height: 1),
            Expanded(
              child: entries.isEmpty
                  ? const _Empty(
                      'This folder is empty. Drop files here to upload them.',
                    )
                  : ListView.builder(
                      itemCount: entries.length,
                      itemBuilder: (context, index) {
                        final file = entries[index];
                        return _FileRow(
                          file,
                          wide: wide,
                          key: ValueKey(file.path),
                          onOpen: () => _activate(file),
                          onDownload: file.isDirectory
                              ? null
                              : () => unawaited(_download(file)),
                          onPreview: _canPreview(file)
                              ? () => unawaited(_preview(file))
                              : null,
                          onRename: () => unawaited(_rename(file)),
                          onDelete: () => unawaited(_delete(file)),
                        );
                      },
                    ),
            ),
          ],
        );
      },
    );
  }

  /// Folders first, then by the chosen column.
  int _compare(RemoteFile a, RemoteFile b) {
    if (a.isDirectory != b.isDirectory) return a.isDirectory ? -1 : 1;
    final byName = a.name.toLowerCase().compareTo(b.name.toLowerCase());
    final order = switch (_sortBy) {
      _SortBy.name => byName,
      _SortBy.size => (a.size ?? 0).compareTo(b.size ?? 0),
      _SortBy.modified => (a.modifiedAt ?? 0).compareTo(b.modifiedAt ?? 0),
    };
    final result = order == 0 ? byName : order;
    return _ascending ? result : -result;
  }
}

bool _canPreview(RemoteFile file) {
  final dot = file.name.lastIndexOf('.');
  return !file.isDirectory &&
      dot > 0 &&
      _previewExtensions.contains(file.name.substring(dot + 1).toLowerCase()) &&
      (file.size ?? 0) <= _maxPreviewBytes;
}

/// Where the open folder is: the device's storage, the root, then each
/// folder, each a link back up.
class _Breadcrumbs extends StatelessWidget {
  const new({
    required this.path,
    required this.roots,
    required this.onOpen,
    required this.onUp,
  });

  final String? path;
  final List<RemoteFile> roots;
  final ValueChanged<String?> onOpen;
  final VoidCallback? onUp;

  @override
  Widget build(BuildContext context) {
    final crumbs = <(String, String?)>[('Storage', null)];
    final path = this.path;
    if (path != null) {
      final root = rootOf(path, roots);
      var current = root?.path ?? '';
      if (root != null) crumbs.add((root.name, root.path));
      final rest = root == null ? path : path.substring(root.path.length);
      for (final segment in rest.split('/').where((s) => s.isNotEmpty)) {
        current = childOf(current.isEmpty ? '/' : current, segment);
        crumbs.add((segment, current));
      }
    }
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
      child: Row(
        children: [
          IconButton(
            tooltip: 'Up',
            onPressed: onUp,
            icon: const Icon(Icons.arrow_upward),
          ),
          Expanded(
            // Deep paths scroll, showing the end; short ones sit at the
            // start.
            child: LayoutBuilder(
              builder: (context, constraints) => SingleChildScrollView(
                scrollDirection: Axis.horizontal,
                reverse: true,
                child: ConstrainedBox(
                  constraints: BoxConstraints(minWidth: constraints.maxWidth),
                  child: Row(
                    children: [
                      for (final (index, (label, target))
                          in crumbs.indexed) ...[
                        if (index > 0)
                          const Icon(Icons.chevron_right, size: 18),
                        TextButton(
                          onPressed: index == crumbs.length - 1
                              ? null
                              : () => onOpen(target),
                          child: Text(label),
                        ),
                      ],
                    ],
                  ),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _HeaderRow extends StatelessWidget {
  const new({
    required this.wide,
    required this.sortBy,
    required this.ascending,
    required this.onSort,
  });

  final bool wide;
  final _SortBy sortBy;
  final bool ascending;
  final ValueChanged<_SortBy> onSort;

  Widget _column(BuildContext context, String label, _SortBy by) {
    final style = Theme.of(context).textTheme.labelLarge;
    return InkWell(
      onTap: () => onSort(by),
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 8),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(label, style: style),
            if (sortBy == by)
              Icon(
                ascending ? Icons.arrow_upward : Icons.arrow_downward,
                size: 16,
              ),
          ],
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.symmetric(horizontal: 16),
    child: Row(
      children: [
        const SizedBox(width: 40),
        Expanded(
          child: Align(
            alignment: Alignment.centerLeft,
            child: _column(context, 'Name', _SortBy.name),
          ),
        ),
        SizedBox(
          width: 96,
          child: Align(
            alignment: Alignment.centerRight,
            child: _column(context, 'Size', _SortBy.size),
          ),
        ),
        if (wide)
          SizedBox(
            width: 168,
            child: Align(
              alignment: Alignment.centerRight,
              child: _column(context, 'Modified', _SortBy.modified),
            ),
          ),
        const SizedBox(width: 48),
      ],
    ),
  );
}

enum _FileAction { download, preview, rename, delete }

class _FileRow extends StatelessWidget {
  const new(
    this.file, {
    required this.wide,
    required this.onOpen,
    required this.onDownload,
    required this.onPreview,
    required this.onRename,
    required this.onDelete,
    super.key,
  });

  final RemoteFile file;
  final bool wide;
  final VoidCallback onOpen;
  final VoidCallback? onDownload;
  final VoidCallback? onPreview;
  final VoidCallback onRename;
  final VoidCallback onDelete;

  @override
  Widget build(BuildContext context) {
    final muted = Theme.of(context).textTheme.bodyMedium
        ?.copyWith(color: Theme.of(context).colorScheme.onSurfaceVariant);
    return InkWell(
      onTap: onOpen,
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 16),
        child: Row(
          children: [
            SizedBox(width: 40, child: Icon(fileIcon(file))),
            Expanded(
              child: Text(
                file.name,
                overflow: TextOverflow.ellipsis,
                semanticsLabel: file.isDirectory
                    ? 'Folder ${file.name}'
                    : file.name,
              ),
            ),
            SizedBox(
              width: 96,
              child: Text(
                file.size == null ? '' : formatBytes(file.size!),
                textAlign: TextAlign.right,
                style: muted,
              ),
            ),
            if (wide)
              SizedBox(
                width: 168,
                child: Text(
                  switch (file.modifiedAt) {
                    final at? => formatTimestamp(at),
                    null => '',
                  },
                  textAlign: TextAlign.right,
                  style: muted,
                ),
              ),
            SizedBox(
              width: 48,
              child: PopupMenuButton<_FileAction>(
                tooltip: 'More',
                onSelected: (action) => switch (action) {
                  _FileAction.download => onDownload?.call(),
                  _FileAction.preview => onPreview?.call(),
                  _FileAction.rename => onRename(),
                  _FileAction.delete => onDelete(),
                },
                itemBuilder: (context) => [
                  if (onPreview != null)
                    const PopupMenuItem(
                      value: _FileAction.preview,
                      child: Text('Preview'),
                    ),
                  if (onDownload != null)
                    const PopupMenuItem(
                      value: _FileAction.download,
                      child: Text('Download'),
                    ),
                  const PopupMenuItem(
                    value: _FileAction.rename,
                    child: Text('Rename'),
                  ),
                  const PopupMenuItem(
                    value: _FileAction.delete,
                    child: Text('Delete'),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

IconData fileIcon(RemoteFile file) {
  if (file.isDirectory) return Icons.folder_outlined;
  final dot = file.name.lastIndexOf('.');
  final extension = dot > 0 ? file.name.substring(dot + 1).toLowerCase() : '';
  return switch (extension) {
    'jpg' ||
    'jpeg' ||
    'png' ||
    'gif' ||
    'webp' ||
    'bmp' ||
    'heic' => Icons.image_outlined,
    'mp4' || 'mkv' || 'mov' || 'webm' || '3gp' => Icons.movie_outlined,
    'mp3' ||
    'm4a' ||
    'ogg' ||
    'opus' ||
    'flac' ||
    'wav' => Icons.audio_file_outlined,
    'pdf' => Icons.picture_as_pdf_outlined,
    'zip' || 'tar' || 'gz' || '7z' || 'rar' => Icons.folder_zip_outlined,
    'apk' => Icons.android,
    _ => Icons.insert_drive_file_outlined,
  };
}

/// Unix milliseconds as local `YYYY-MM-DD HH:MM`.
String formatTimestamp(int millis) {
  final time = DateTime.fromMillisecondsSinceEpoch(millis);
  String two(int value) => value.toString().padLeft(2, '0');
  return '${time.year}-${two(time.month)}-${two(time.day)} '
      '${two(time.hour)}:${two(time.minute)}';
}

class _Empty extends StatelessWidget {
  const new(this.message);

  final String message;

  @override
  Widget build(BuildContext context) => Center(
    child: Padding(
      padding: const EdgeInsets.all(24),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(
            Icons.folder_open,
            size: 64,
            color: Theme.of(context).colorScheme.outline,
          ),
          const SizedBox(height: 16),
          Text(message, textAlign: TextAlign.center),
        ],
      ),
    ),
  );
}

/// Asks for a file or folder name. Pops with the name, or `null` when
/// cancelled.
class _NameDialog extends StatefulWidget {
  const new({required this.title, required this.action, this.initial = ''});

  final String title;
  final String action;
  final String initial;

  @override
  State<_NameDialog> createState() => _NameDialogState();
}

class _NameDialogState extends State<_NameDialog> {
  late final _controller = TextEditingController(text: widget.initial);
  String? _error;

  @override
  void initState() {
    super.initState();
    // Select the name without its extension, as file managers do.
    final dot = widget.initial.lastIndexOf('.');
    _controller.selection = TextSelection(
      baseOffset: 0,
      extentOffset: dot > 0 ? dot : widget.initial.length,
    );
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _submit() {
    final name = _controller.text;
    final error = invalidNameReason(name);
    if (error != null) {
      setState(() => _error = error);
      return;
    }
    Navigator.pop(context, name);
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: Text(widget.title),
    content: TextField(
      controller: _controller,
      autofocus: true,
      decoration: InputDecoration(labelText: 'Name', errorText: _error),
      onSubmitted: (_) => _submit(),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('Cancel'),
      ),
      FilledButton(onPressed: _submit, child: Text(widget.action)),
    ],
  );
}

/// Shows an image from the device, loaded when opened.
class _PreviewDialog extends ConsumerStatefulWidget {
  const new({required this.deviceId, required this.file});

  final String deviceId;
  final RemoteFile file;

  @override
  ConsumerState<_PreviewDialog> createState() => _PreviewDialogState();
}

class _PreviewDialogState extends ConsumerState<_PreviewDialog> {
  late final Future<Uint8List> _content = () async {
    final api = await ref.read(apiProvider.future);
    return await api.fileContent(widget.deviceId, widget.file.path);
  }();

  @override
  Widget build(BuildContext context) => Dialog(
    clipBehavior: Clip.antiAlias,
    child: Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        ListTile(
          title: Text(widget.file.name, overflow: TextOverflow.ellipsis),
          trailing: IconButton(
            tooltip: 'Close',
            onPressed: () => Navigator.pop(context),
            icon: const Icon(Icons.close),
          ),
        ),
        Flexible(
          child: FutureBuilder(
            future: _content,
            builder: (context, snapshot) => switch (snapshot) {
              AsyncSnapshot(:final data?) => InteractiveViewer(
                child: Image.memory(
                  data,
                  errorBuilder: (context, error, stackTrace) =>
                      const ErrorView.message('This image can’t be shown.'),
                ),
              ),
              AsyncSnapshot(:final error?) => ErrorView(error: error),
              _ => const Padding(
                padding: EdgeInsets.all(48),
                child: CircularProgressIndicator(),
              ),
            },
          ),
        ),
      ],
    ),
  );
}
