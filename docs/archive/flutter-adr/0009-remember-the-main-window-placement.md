# 0009. Remember the main window's placement in the UI

- Status: Accepted
- Date: 2026-09-24

## Context

The window opened at the default size wherever the window manager put it,
on every launch, even when the user had last left it closed in the tray.
The app should come back the way it was left:

- Opened from the tray, the window returns to where it was, at the same
  size.
- Quit from the tray with the window closed, the next launch starts
  hidden in the tray.
- Quit with the window showing, the next launch opens it where it was.

[0001](0001-stateless-ui-over-the-http-api.md) says the UI persists nothing,
and suggests UI preferences become daemon settings. Window placement is a
poor fit for that. It is needed before the daemon is up (the window must not
flash open only to hide again), it belongs to this desktop's display rather
than the device, and it changes with every drag.

## Decision

- **The UI keeps the main window's placement itself, as the one exception
  to 0001.** It saves whether the window is showing, whether it is
  maximized, and its position and size when neither maximized nor
  minimized. `WindowPlacementStore` writes it as `window.json`, replacing
  the file atomically:
  - Linux: `$XDG_STATE_HOME/myconnect` (default `~/.local/state/myconnect`).
  - macOS: `~/Library/Application Support/MyConnect` (the sandbox
    container).
  - Windows: `%LOCALAPPDATA%\MyConnect`.
  - The data directory, when `MYCONNECT_DATA_DIR` is given, so a test or a
    second instance doesn't share the owner's window.
- **`NativeDesktopShell` owns it.** It follows `window_manager`'s events and
  saves half a second after the last change, so a killed app still comes
  back close to how it was. Quit saves once more before the window is
  destroyed, so hidden or showing is recorded as the user left it.
- **The window starts hidden, and Dart shows it.** The Linux runner no
  longer shows the window on its first frame. The shell moves it into place
  while it is hidden, then shows it unless it was last left hidden. It
  always shows it when there is no tray icon, since a hidden window could
  then only come back by launching the app again.
- **A spot that is no longer on screen isn't used.** If less than a
  grabbable part of the saved rectangle is on any display (`screen_retriever`),
  the window keeps its saved size and is centred instead.
- The saved placement is applied again whenever a hidden window is shown,
  because some window managers place a window afresh each time it is mapped.

## Consequences

- Nothing else is persisted by the UI. Other preferences still go through
  the daemon's settings API.
- The macOS and Windows runners still show the window at launch, so a
  launch that should stay hidden shows the window briefly before the shell
  hides it. Only the Linux runner was changed, and only Linux was checked in
  the real app.
- On Wayland the compositor decides where windows go, so only the size and
  maximized state come back; the saved position is ignored.
- Under xfwm4, a window restored as maximized and then unmaximized can come
  back a few pixels (the frame border) from where it was.
