import 'package:flutter/foundation.dart';
import 'package:logging/logging.dart';
import 'package:tray_manager/tray_manager.dart';
import 'package:window_manager/window_manager.dart';

final _log = Logger('DesktopShell');

/// The native window and tray icon around the app.
///
/// It only reports what the user did and carries out window commands; what
/// closing or quitting means is decided by `BackgroundHost`.
abstract interface class DesktopShell {
  /// Take over the window's close button and show the tray icon.
  ///
  /// [onCloseRequested] replaces closing the window; [onShowRequested],
  /// [onSendFilesRequested] and [onQuitRequested] are the tray menu's
  /// entries.
  Future<void> start({
    required VoidCallback onCloseRequested,
    required VoidCallback onShowRequested,
    required VoidCallback onSendFilesRequested,
    required VoidCallback onQuitRequested,
  });

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
  VoidCallback? _onCloseRequested;

  @override
  Future<void> start({
    required VoidCallback onCloseRequested,
    required VoidCallback onShowRequested,
    required VoidCallback onSendFilesRequested,
    required VoidCallback onQuitRequested,
  }) async {
    _onCloseRequested = onCloseRequested;
    await windowManager.ensureInitialized();
    windowManager.addListener(this);
    await windowManager.setPreventClose(true);
    _tray = _createTray(
      onShow: onShowRequested,
      onSendFiles: onSendFilesRequested,
      onQuit: onQuitRequested,
    );
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
    windowManager.removeListener(this);
    // Closes the window even though closing is prevented; with no window
    // left, the process exits.
    await windowManager.destroy();
  }

  static TrayIcon? _createTray({
    required VoidCallback onShow,
    required VoidCallback onSendFiles,
    required VoidCallback onQuit,
  }) {
    final tray = TrayIcon.create();
    final menu = Menu.create();
    if (tray == null || menu == null) {
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
    tray.setTooltip('MyConnect');
    menu
      ..addItem(_item('Show MyConnect', onShow))
      ..addItem(_item('Send files…', onSendFiles))
      ..addSeparator()
      ..addItem(_item('Quit', onQuit));
    tray
      ..setContextMenu(menu)
      ..setVisible(true);
    return tray;
  }

  static MenuItem _item(String label, VoidCallback onClick) {
    final item = MenuItem.createWithLabelAndType(label, MenuItemType.normal)!
      ..addListener((event) {
        if (event is MenuItemClickedEvent) onClick();
      });
    return item;
  }
}
