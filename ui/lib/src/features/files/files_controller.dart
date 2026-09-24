import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:myconnect_ui/src/core/api/models/remote_file.dart';
import 'package:myconnect_ui/src/core/providers.dart';
import 'package:myconnect_ui/src/features/devices/devices_controller.dart';

/// One directory on a device, or, with no path, the storage it shares.
typedef DirectoryKey = ({String deviceId, String? path});

/// A directory's entries, fetched when first shown.
///
/// Unlike other resources, a device's files have no events: nothing tells
/// the daemon when they change on the device (see ADR 0008). A listing is
/// refetched when the device reconnects, after this app changes the
/// directory, and when the user refreshes.
final directoryProvider = FutureProvider.autoDispose
    .family<DirectoryListing, DirectoryKey>((ref, key) async {
      // A listing that failed while the device was away is retried once it
      // is back.
      ref.watch(
        deviceProvider(key.deviceId).select((device) => device?.sharesFiles),
      );
      final api = await ref.watch(apiProvider.future);
      return await api.listFiles(key.deviceId, path: key.path);
    });

/// The storage root that [path] is in, among [roots], or `null` if none
/// is.
RemoteFile? rootOf(String path, List<RemoteFile> roots) {
  for (final root in roots) {
    if (path == root.path || path.startsWith('${root.path}/')) return root;
  }
  return null;
}

/// The directory holding [path], or `null` for `/`.
String? parentOf(String path) {
  final index = path.lastIndexOf('/');
  if (index < 0 || path == '/') return null;
  return index == 0 ? '/' : path.substring(0, index);
}

/// [name] inside [directory].
String childOf(String directory, String name) =>
    directory.endsWith('/') ? '$directory$name' : '$directory/$name';

/// Why [name] can't name a file, or `null` if it can.
String? invalidNameReason(String name) {
  if (name.trim().isEmpty) return 'Enter a name.';
  if (name == '.' || name == '..') return 'That name is reserved.';
  if (name.contains('/')) return 'Names can’t contain “/”.';
  return null;
}
