# Vendored packages

## cnativeapi 0.3.0

The C and C++ core under `tray_manager` 0.7 (through `nativeapi`), from
[pub.dev](https://pub.dev/packages/cnativeapi/versions/0.3.0) (MIT, see
`cnativeapi/LICENSE`). `pubspec.yaml` points `dependency_overrides` here.

Changes from the published package:

- `example/` is left out.
- `resolution: workspace` is removed from its `pubspec.yaml`, since this
  repo is not its workspace.
- [`patches/cnativeapi-linux-tray-clicks.patch`](patches/cnativeapi-linux-tray-clicks.patch):
  on Linux the StatusNotifierItem ignored the panel's `Activate` and
  exported its menu only for the `clicked` trigger, so a left click could
  not be told from a right click. With the patch, `Activate` reports a
  click, `ContextMenu` a right click, and the menu is exported for any
  trigger. Upstream `main` had the same code when this was vendored.

To upgrade: copy the new version from `~/.pub-cache/hosted/pub.dev/`
without `example/`, drop `resolution: workspace`, and apply the patch with
`patch -p1 -d cnativeapi < patches/cnativeapi-linux-tray-clicks.patch`, or
remove the override if upstream has fixed it. Keep the version in step
with the `nativeapi` that `tray_manager` resolves to (see `pubspec.lock`).
