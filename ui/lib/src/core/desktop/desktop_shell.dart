import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:logging/logging.dart';
import 'package:tray_manager/tray_manager.dart';
import 'package:window_manager/window_manager.dart';

final _log = Logger('DesktopShell');

/// One entry of the tray menu.
sealed class TrayMenuEntry {
  const new();
}

/// A labelled entry. It opens [submenu] if it has one, otherwise it calls
/// [onSelected]; with neither it is shown disabled.
final class TrayMenuItem extends TrayMenuEntry {
  const new(this.label, {this.onSelected, this.submenu});

  final String label;
  final VoidCallback? onSelected;
  final List<TrayMenuEntry>? submenu;

  bool get enabled => onSelected != null || submenu != null;
}

final class TrayMenuSeparator extends TrayMenuEntry {
  const new();
}

/// The native window and tray icon around the app.
///
/// It only reports what the user did and carries out window commands; what
/// closing or quitting means, and what the tray menu holds, is decided by
/// `BackgroundHost`.
abstract interface class DesktopShell {
  /// Take over the window's close button and show the tray icon.
  ///
  /// [onCloseRequested] replaces closing the window. [onTrayClicked] is a
  /// primary click on the tray icon, where the platform reports one; the
  /// secondary click opens the menu given to [setTrayMenu].
  Future<void> start({
    required VoidCallback onCloseRequested,
    required VoidCallback onTrayClicked,
  });

  /// Replace the tray menu. Can be called before [start].
  void setTrayMenu(List<TrayMenuEntry> entries);

  /// Show, raise and focus the window.
  Future<void> showWindow();

  Future<void> hideWindow();

  Future<bool> isWindowFocused();

  /// Remove the tray icon and close the window for good, ending the process.
  Future<void> exit();
}

/// [DesktopShell] over `window_manager` and `tray_manager`.
class NativeDesktopShell with WindowListener implements DesktopShell {
  // Held for as long as the icon should show: a collected TrayIcon removes
  // itself from the tray.
  TrayIcon? _tray;
  _NativeTrayMenu? _menu;
  List<TrayMenuEntry> _entries = const [];
  VoidCallback? _onCloseRequested;

  @override
  Future<void> start({
    required VoidCallback onCloseRequested,
    required VoidCallback onTrayClicked,
  }) async {
    _onCloseRequested = onCloseRequested;
    await windowManager.ensureInitialized();
    windowManager.addListener(this);
    await windowManager.setPreventClose(true);
    _tray = _createTray(onClicked: onTrayClicked);
    _applyMenu();
  }

  @override
  void setTrayMenu(List<TrayMenuEntry> entries) {
    _entries = entries;
    _applyMenu();
  }

  @override
  void onWindowClose() => _onCloseRequested?.call();

  @override
  Future<void> showWindow() async {
    await windowManager.show();
    await windowManager.focus();
  }

  @override
  Future<void> hideWindow() => windowManager.hide();

  @override
  Future<bool> isWindowFocused() => windowManager.isFocused();

  @override
  Future<void> exit() async {
    _tray?.dispose();
    _tray = null;
    _menu?.dispose();
    _menu = null;
    windowManager.removeListener(this);
    // Closes the window even though closing is prevented; with no window
    // left, the process exits.
    await windowManager.destroy();
  }

  /// Show [_entries] in the tray. The native menu is only rebuilt when its
  /// labels or layout change; otherwise the entries' callbacks are swapped
  /// into the menu already showing.
  void _applyMenu() {
    final tray = _tray;
    if (tray == null) return;
    final current = _menu;
    if (current != null && current.update(_entries)) return;
    final menu = _NativeTrayMenu.build(_entries);
    if (menu == null) {
      _log.warning('Could not build the tray menu');
      return;
    }
    tray.setContextMenu(menu.root);
    _menu = menu;
    // The old menu may be the one whose click is being handled right now.
    if (current != null) Timer.run(current.dispose);
  }

  static TrayIcon? _createTray({required VoidCallback onClicked}) {
    final tray = TrayIcon.create();
    if (tray == null) {
      _log.warning('No tray icon on this system');
      return null;
    }
    // The macOS menu bar wants a template image, which it tints to suit a
    // light or dark menu bar. Elsewhere the tray shows the icon as drawn,
    // on its own tile so it stands out on light and dark panels alike.
    if (defaultTargetPlatform == TargetPlatform.macOS) {
      tray
        ..icon = ImageAsset.fromAsset('assets/tray_icon_template.png')
        ..isIconTemplate = true;
    } else {
      tray.icon = ImageAsset.fromAsset('assets/tray_icon.png');
    }
    tray
      ..setTooltip('MyConnect')
      // The trigger defaults to none, which never opens the menu.
      ..setContextMenuTrigger(ContextMenuTrigger.rightClicked)
      ..addListener((event) {
        if (event is TrayIconClickedEvent) onClicked();
      })
      ..setVisible(true);
    return tray;
  }
}

/// A native menu built from [TrayMenuEntry]s, holding every native object
/// it made: a collected wrapper frees its native side.
class _NativeTrayMenu {
  new _(this.root, this._layout, this._actions);

  final Menu root;
  final List<Object?> _layout;
  final _menus = <Menu>[];
  final _items = <MenuItem>[];

  /// The callback of each item without a submenu, in depth-first order.
  List<VoidCallback?> _actions;
  int _leaves = 0;

  static _NativeTrayMenu? build(List<TrayMenuEntry> entries) {
    final root = Menu.create();
    if (root == null) return null;
    final menu = _NativeTrayMenu._(
      root,
      _layoutOf(entries),
      _actionsOf(entries),
    ).._menus.add(root);
    if (menu._fill(root, entries)) return menu;
    menu.dispose();
    return null;
  }

  /// Take [entries]' callbacks if they lay out the same as this menu.
  bool update(List<TrayMenuEntry> entries) {
    if (!listEquals(_layout, _layoutOf(entries))) return false;
    _actions = _actionsOf(entries);
    return true;
  }

  bool _fill(Menu menu, List<TrayMenuEntry> entries) {
    for (final entry in entries) {
      switch (entry) {
        case TrayMenuSeparator():
          menu.addSeparator();
        case TrayMenuItem(:final label, :final submenu):
          final item = MenuItem.createWithLabelAndType(
            label,
            submenu == null ? MenuItemType.normal : MenuItemType.submenu,
          );
          if (item == null) return false;
          _items.add(item);
          item.isEnabled = entry.enabled;
          if (submenu != null) {
            final child = Menu.create();
            if (child == null) return false;
            _menus.add(child);
            if (!_fill(child, submenu)) return false;
            item.submenu = child;
          } else {
            final index = _leaves++;
            item.addListener((event) {
              if (event is MenuItemClickedEvent && index < _actions.length) {
                _actions[index]?.call();
              }
            });
          }
          menu.addItem(item);
      }
    }
    return true;
  }

  void dispose() {
    _actions = const [];
    for (final item in _items) {
      item.dispose();
    }
    for (final menu in _menus) {
      menu.dispose();
    }
  }

  /// What the native menu shows, depth first: each entry's depth, label and
  /// whether it is enabled or opens a submenu.
  static List<Object?> _layoutOf(
    List<TrayMenuEntry> entries, [
    int depth = 0,
  ]) => [
    for (final entry in entries)
      ...switch (entry) {
        TrayMenuSeparator() => [(depth, null, false, false)],
        TrayMenuItem(:final label, :final submenu) => [
          (depth, label, entry.enabled, submenu != null),
          if (submenu != null) ..._layoutOf(submenu, depth + 1),
        ],
      },
  ];

  /// Matches the order `_fill` numbers items in.
  static List<VoidCallback?> _actionsOf(List<TrayMenuEntry> entries) => [
    for (final entry in entries)
      if (entry case TrayMenuItem(:final submenu?))
        ..._actionsOf(submenu)
      else if (entry case TrayMenuItem(:final onSelected))
        onSelected,
  ];
}
