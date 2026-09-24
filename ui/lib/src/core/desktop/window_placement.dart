import 'dart:convert';
import 'dart:io';
import 'dart:ui';

import 'package:flutter/foundation.dart';
import 'package:logging/logging.dart';
import 'package:myconnect_ui/src/core/daemon/daemon_host.dart';

final _log = Logger('WindowPlacement');

/// Where the main window was, and whether it was showing, when last seen.
@immutable
class WindowPlacement {
  const new({required this.visible, this.bounds, this.maximized = false});

  /// Read back from [toJson]'s output, or `null` if [json] isn't that.
  static WindowPlacement? fromJson(Object? json) {
    if (json case {'visible': final bool visible}) {
      return WindowPlacement(
        visible: visible,
        maximized: json['maximized'] == true,
        bounds: switch (json['bounds']) {
          [final num left, final num top, final num width, final num height]
              when width > 0 && height > 0 =>
            Rect.fromLTWH(
              left.toDouble(),
              top.toDouble(),
              width.toDouble(),
              height.toDouble(),
            ),
          _ => null,
        },
      );
    }
    return null;
  }

  /// Showing, rather than hidden in the tray. Minimized counts as showing.
  final bool visible;

  /// Position and size when neither maximized nor minimized, or `null`
  /// before the window was first seen.
  final Rect? bounds;

  final bool maximized;

  Map<String, Object?> toJson() => {
    'visible': visible,
    'maximized': maximized,
    if (bounds case final bounds?)
      'bounds': [bounds.left, bounds.top, bounds.width, bounds.height],
  };

  @override
  bool operator ==(Object other) =>
      other is WindowPlacement &&
      other.visible == visible &&
      other.bounds == bounds &&
      other.maximized == maximized;

  @override
  int get hashCode => Object.hash(visible, bounds, maximized);

  @override
  String toString() =>
      'WindowPlacement(visible: $visible, bounds: $bounds, '
      'maximized: $maximized)';
}

/// Whether enough of a window at [bounds] would be on one of [screens] to
/// grab and move it, e.g. not on a monitor that has since been unplugged.
bool fitsOnScreen(Rect bounds, Iterable<Rect> screens) => screens.any((screen) {
  final visible = screen.intersect(bounds);
  return visible.width >= 100 && visible.height >= 50;
});

/// Keeps the main window's [WindowPlacement] in a small JSON file.
///
/// It is the one thing the UI stores itself (ADR 0009): it is about this
/// desktop's window rather than the device, and it is needed before the
/// daemon is up.
class WindowPlacementStore {
  new(this.file);

  /// The file for this platform, or in the data directory when one is given
  /// with `MYCONNECT_DATA_DIR`, so a test or second instance doesn't share
  /// the owner's window.
  factory fromEnvironment() =>
      WindowPlacementStore(File(_defaultPath(Platform.environment)));

  final File file;

  static String _defaultPath(Map<String, String> env) {
    if (dataDirOverride.isNotEmpty) return '$dataDirOverride/window.json';
    final home = env['HOME'] ?? '';
    final String dir;
    if (Platform.isWindows) {
      dir = '${env['LOCALAPPDATA'] ?? env['APPDATA'] ?? '.'}\\MyConnect';
    } else if (Platform.isMacOS) {
      // Inside the sandbox, HOME is the app's container.
      dir = '$home/Library/Application Support/MyConnect';
    } else {
      // XDG's state directory is meant for things like window layout.
      final state = env['XDG_STATE_HOME'];
      dir =
          '${state != null && state.isNotEmpty ? state : '$home/.local/state'}'
          '/myconnect';
    }
    return '$dir/window.json';
  }

  /// The saved placement, or `null` if there is none or it can't be read.
  Future<WindowPlacement?> load() async {
    try {
      return WindowPlacement.fromJson(jsonDecode(await file.readAsString()));
    } on PathNotFoundException {
      return null;
    } on Object catch (error) {
      _log.warning('Ignoring the saved window placement: $error');
      return null;
    }
  }

  /// Replace the saved placement. Written to a temporary file first, so a
  /// crash mid-write leaves the old one.
  Future<void> save(WindowPlacement placement) async {
    try {
      await file.parent.create(recursive: true);
      final temporary = File('${file.path}.tmp');
      await temporary.writeAsString(
        jsonEncode(placement.toJson()),
        flush: true,
      );
      await temporary.rename(file.path);
    } on Object catch (error) {
      _log.warning('Could not save the window placement: $error');
    }
  }
}
