import 'dart:async';
import 'dart:io';

import 'package:desktop_drop/desktop_drop.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:material_ui/material_ui.dart';
import 'package:myconnect_ui/src/core/api/models/device.dart';
import 'package:myconnect_ui/src/features/devices/devices_controller.dart';
import 'package:myconnect_ui/src/features/pairing/pairings_controller.dart';
import 'package:myconnect_ui/src/features/send/send_files.dart';

/// Files being dragged over the window: `null` when there are none, else
/// the device under the pointer (see [FileDropTarget]) if it would take
/// them.
final fileDragProvider = NotifierProvider<FileDragNotifier, FileDrag?>(
  FileDragNotifier.new,
);

class FileDrag {
  const new({this.deviceId});

  final String? deviceId;
}

class FileDragNotifier extends Notifier<FileDrag?> {
  @override
  FileDrag? build() => null;

  /// Files are over the window, and over [deviceId] if it would take them.
  void hover(String? deviceId) => state = FileDrag(deviceId: deviceId);

  /// The files were dropped, or left the window.
  void end() => state = null;
}

/// Marks [child] as the place to drop files for [deviceId]: sent to it, or,
/// with a [directory], uploaded into that folder on it.
///
/// Only [FileDropZone] receives the drop; it finds the device under the
/// pointer by hit testing, so a covered page never takes a drop meant for
/// the one on top.
class FileDropTarget extends StatelessWidget {
  const new({
    required this.deviceId,
    required this.child,
    this.directory,
    super.key,
  });

  final String deviceId;
  final String? directory;
  final Widget child;

  @override
  Widget build(BuildContext context) => MetaData(
    metaData: _DropDestination(deviceId, directory),
    behavior: HitTestBehavior.translucent,
    child: child,
  );
}

class _DropDestination {
  const new(this.deviceId, this.directory);

  final String deviceId;
  final String? directory;

  /// Whether [device] would take the files now.
  bool accepts(Device device) =>
      directory == null ? device.acceptsFiles : device.sharesFiles;
}

/// Accepts files dropped anywhere on the window and sends them: straight to
/// the device they were dropped on (a [FileDropTarget]), or else to one the
/// user picks.
///
/// It is the only drop target: every `DropTarget` receives every drop within
/// its bounds, even on pages hidden under the current one.
class FileDropZone extends ConsumerStatefulWidget {
  const new({required this.child, super.key});

  final Widget child;

  @override
  ConsumerState<FileDropZone> createState() => _FileDropZoneState();
}

class _FileDropZoneState extends ConsumerState<FileDropZone> {
  void _hover(Offset position) {
    final destination = _destinationAt(position);
    final device = switch (destination) {
      _DropDestination(:final deviceId) => ref.read(deviceProvider(deviceId)),
      null => null,
    };
    ref
        .read(fileDragProvider.notifier)
        .hover(
          device != null && destination!.accepts(device)
              ? device.deviceId
              : null,
        );
  }

  _DropDestination? _destinationAt(Offset position) {
    final result = HitTestResult();
    WidgetsBinding.instance.hitTestInView(
      result,
      position,
      View.of(context).viewId,
    );
    for (final entry in result.path) {
      if (entry.target case RenderMetaData(
        metaData: final _DropDestination destination,
      )) {
        return destination;
      }
    }
    return null;
  }

  Future<void> _drop(DropDoneDetails details) async {
    ref.read(fileDragProvider.notifier).end();
    final context = navigatorContext(ref);
    if (context == null) return;
    final dropped = [for (final item in details.files) item.path];
    // Folders can't be sent, and some drops (e.g. links) aren't local files.
    final files = [
      for (final path in dropped)
        if (FileSystemEntity.isFileSync(path)) path,
    ];
    if (files.isEmpty) {
      if (dropped.isNotEmpty) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('Only files can be sent, not folders.')),
        );
      }
      return;
    }
    final destination = _destinationAt(details.globalPosition);
    final device = switch (destination) {
      _DropDestination(:final deviceId) => ref.read(deviceProvider(deviceId)),
      null => null,
    };
    if (destination?.directory case final directory?
        when device != null && device.sharesFiles) {
      await uploadFiles(
        ProviderScope.containerOf(context),
        ScaffoldMessenger.of(context),
        device.deviceId,
        directory,
        files,
      );
      return;
    }
    await confirmAndSendFiles(context, ref, files, to: device);
  }

  @override
  Widget build(BuildContext context) {
    final drag = ref.watch(fileDragProvider);
    // The pairing prompt is modal; a drop would open a dialog under it.
    final prompting = ref.watch(pendingIncomingPairingsProvider).isNotEmpty;
    return DropTarget(
      enable: !prompting,
      onDragEntered: (details) => _hover(details.globalPosition),
      onDragUpdated: (details) => _hover(details.globalPosition),
      onDragExited: (_) => ref.read(fileDragProvider.notifier).end(),
      onDragDone: (details) => unawaited(_drop(details)),
      child: Stack(
        children: [
          widget.child,
          if (drag != null && drag.deviceId == null)
            const Positioned.fill(child: IgnorePointer(child: _DropHint())),
        ],
      ),
    );
  }
}

/// Outlines the window while files are dragged over it, away from any
/// device.
class _DropHint extends StatelessWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    return DecoratedBox(
      decoration: BoxDecoration(
        border: Border.all(color: colors.primary, width: 3),
      ),
      child: Align(
        alignment: Alignment.bottomCenter,
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Material(
            color: colors.primaryContainer,
            borderRadius: BorderRadius.circular(24),
            elevation: 3,
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 12),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Icon(Icons.upload_file, color: colors.onPrimaryContainer),
                  const SizedBox(width: 8),
                  Text(
                    'Drop on a device, or anywhere to choose one',
                    style: TextStyle(color: colors.onPrimaryContainer),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
