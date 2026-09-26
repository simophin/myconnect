# 0007. Keep running in the tray when the window closes

- Status: Accepted
- Date: 2026-09-24

## Context

The daemon runs inside the app process ([0002](0002-embed-the-daemon-through-a-json-c-abi.md)),
and it stopped when the window closed. Peers then lost the connection, and
pairing requests or files that arrived later went unnoticed. A KDE
Connect-style app has to keep running with no window open, and a second
launch must not start a second daemon: it would fight the first for UDP
1716 and, with a different data dir, show up as a second device.

## Decision

- **Closing hides; only Quit exits.** `window_manager` takes over the close
  button (`setPreventClose`) and the window is hidden instead. Clicking the
  tray icon (`tray_manager`) shows the window; its menu has *Open
  MyConnect*, one submenu per paired device (*Send files…*, *Ping*, *Show
  details*, each enabled only when the device can take it), *Settings* and
  *Quit*. `BackgroundHost` rebuilds the menu from `pairedDevicesProvider`.
  Quit stops the daemon first, then destroys the window, which ends the
  process.
- **The policy lives in Dart, the mechanics behind interfaces.**
  `BackgroundHost` (`features/background/`) decides what close, show, quit
  and a notification click mean. It talks to `DesktopShell` (window and
  tray) and `DesktopNotifications` (`flutter_local_notifications`) through
  providers, so widget tests swap in fakes. It sits above `DaemonGate`, so
  the tray works even when the daemon failed to start, and it lives as long
  as the `ProviderScope`.
- **Notifications come from existing state.** Incoming pairing requests are
  read from `pendingIncomingPairingsProvider`, the same provider the prompt
  uses, and not from a second event subscription. A notification is shown
  only when the window doesn't have focus (the prompt is already in front
  of the user otherwise) and is withdrawn when the request is resolved.
  Clicking it shows the window, where the prompt is waiting.
- **Single instance comes from the platform.** On Linux the runner is a
  unique `GApplication` (no `G_APPLICATION_NON_UNIQUE`). A second launch
  activates the running instance over D-Bus, which presents its window,
  hidden or not, and then exits. macOS apps are single-instance through
  LaunchServices. Windows will need a named mutex when it is packaged.

## Consequences

- On Linux, `window_manager` disconnects Flutter's own `delete-event`
  handler, so `AppLifecycleListener.onExitRequested` no longer fires on
  window close. It is kept for exits the OS requests on other platforms
  (e.g. Quit from the macOS menu bar), and it also stops the daemon.
- The Linux tray icon is a StatusNotifierItem over D-Bus (no
  libappindicator build dependency). KDE Plasma and most panels show it;
  stock GNOME needs the AppIndicator extension. Without a tray host a
  closed window can still be brought back by launching the app again.
- nativeapi 0.3 (under `tray_manager` 0.7) defaults the menu trigger to
  none, which never opens it, so the shell sets `rightClicked`. On Linux,
  upstream exports the menu only for the `clicked` trigger (`ItemIsMenu`)
  and ignores the panel's `Activate`, so a left click could not show the
  window. `cnativeapi` is vendored with a patch that fixes both
  (`third_party/README.md`); drop the override once upstream has the fix.
- Launching a second copy with different `--dart-define`s (e.g. another data
  dir) only focuses the first. To run two app instances side by side, give
  the second build a different `APPLICATION_ID` in `linux/CMakeLists.txt`.
- Start on login is not built yet. It is a user preference, so it has to be
  stored by the daemon's settings API (ground rule: the UI persists
  nothing).
