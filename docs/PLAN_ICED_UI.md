# Plan: replace the Flutter UI with a native Rust UI (iced)

For agents doing this work step by step. Each step says why it exists, what
to build, what "done" means, and the traps already known. Work in order: the
early steps build the ground the later ones stand on. Steps marked
*independent* can run in parallel worktrees once their prerequisites have
landed.

Status (2026-09-25): decided by the owner. Steps 1 and 2 are done: `gui/`
is the thin composition root, the spike's device list lives in `src/ui/`,
and features plug in through the `UiPlugin` seam, with battery as the
first. Next is step 3. Each finished step says so under its heading, with what differs
from the plan.

## Read first

1. [`HANDOFF.md`](HANDOFF.md): the ground rules and the "done means" checks.
   Both still apply. Only the UI-specific rules change, as described below.
2. [`ARCHITECTURE.md`](ARCHITECTURE.md) §2: the core, the `Plugin` trait,
   and the rule that the core never names a feature. The UI copies that
   shape.
3. The Flutter app is the **spec**. [`../ui/lib/src/`](../ui/lib/src/) is
   what parity means, and [Appendix A](#appendix-a-parity-checklist) lists
   every behaviour to carry over, with file references. When this plan and
   the Flutter code disagree about a detail, the Flutter code wins, unless
   the difference is listed in [Deliberate differences](#deliberate-differences).
4. iced 0.14: <https://docs.rs/iced/0.14>, <https://book.iced.rs>, and the
   examples in the iced repo (`modal`, `toast`, `multi_window`, `todos`,
   `websocket` for subscriptions). Read an example before inventing a
   pattern.

## Decisions (made by the owner)

- **The UI is Rust and iced.** The Flutter app stays in the tree and keeps
  working until the last step deletes it. Both apps build side by side
  until then.
- **The UI runs the daemon in-process and talks to the core directly:**
  snapshots from `Core`, events from `core.subscribe()`, and actions through
  typed Rust functions. It does **not** go through HTTP.
- **The HTTP API and the CLI stay.** The UI's embedded daemon still serves
  the API, so `myconnect --api-port … devices/pair/send …` can drive and
  inspect the same instance the UI shows. That is how you test and debug.
  Every feature keeps working from the CLI.
- **A feature is one folder, UI included.** The UI half of a feature lives
  in its existing plugin module, `src/plugins/<name>/ui.rs`, next to
  `mod.rs` and `http.rs`. The UI core never names a feature, and plugins
  never import each other, in the UI too.
- **The UI does not need to look like the Flutter app.** Use what iced does
  well: the look of the spike (built-in theme palette, Lucide icons, cards)
  is the direction. Parity means the same *behaviour*, not the same
  pixels.

## Target architecture

```text
gui/ (bin myconnect-gui)           → the UI's composition root: args, start the daemon, ui::run
src/ui/        [feature "gui"]     → the UI core: shell, pages the core owns, UiPlugin seam, desktop glue
src/plugins/<name>/ui.rs  [gui]    → each feature's UI half, implementing ui::UiPlugin
src/plugins/mod.rs                 → builtin() and, behind "gui", builtin_with_ui(): the one list of each
src/plugins/<name>/mod.rs          → the feature's typed Rust API, used by both http.rs and ui.rs
```

Dependency direction, which extends ARCHITECTURE §2:

```text
gui (bin) → daemon, ui, plugins::builtin_with_ui
ui        → core (Core, snapshots, events), protocol (types only)
plugins/*/ui.rs → ui (UiPlugin, UiContext, widgets), their own plugin module, core
```

`ui` never imports `plugins`, and `core` never imports `ui`.

### The `gui` cargo feature

- `myconnect` gets `[features] gui = [...]`, which turns on the optional
  dependencies: `iced`, `iced_fonts` (Lucide), `rfd`, `notify-rust`,
  `opener`, `interprocess`, `ksni` (Linux) and `tray-icon` (macOS and
  Windows).
- `src/ui/` is `#[cfg(feature = "gui")] pub mod ui;` in `lib.rs`.
- Each `src/plugins/<name>/mod.rs` has `#[cfg(feature = "gui")] pub mod ui;`.
- `cargo build -p myconnect` (the CLI and daemon) must still build with no
  iced in its tree. CI checks this (step 15).
- `gui/Cargo.toml` depends on `myconnect = { path = "..", features = ["gui"] }`.
  A plain workspace build therefore compiles the UI too (feature
  unification). That is intended.

### `src/ui/` layout

| Module | Responsibility |
| --- | --- |
| `ui/mod.rs` | `run(service: RunningService, options: UiOptions, plugins: Vec<Box<dyn ErasedUiPlugin>>)`. Builds the iced program (`iced::daemon`, so it can live without a window), owns `App`, the top-level `Message`, `update`, `view` and `subscription`. |
| `ui/plugin.rs` | The seam: `UiPlugin`, `ErasedUiPlugin` (blanket impl), `PluginMessage`, `UiContext`, `ShellRequest`, and the slot types (`DeviceAction`, `DeviceStatus`, `PluginPage`, `DropTarget`, `SettingsSection`). |
| `ui/store.rs` | In-memory caches of what the core owns: devices, pairings, transfers, settings. Patched by events; see step 4. |
| `ui/sync.rs` | The one subscription that watches the core: snapshot, then events, with a fresh snapshot after `RecvError::Lagged`. The spike's `watch()` grows into this. |
| `ui/route.rs` | `Route` (`Devices`, `Device(id)`, `AddDevice`, `Pairing(id)`, `Transfers`, `Settings`, `Plugin { plugin, device, page }`) and back navigation (the parent route, like Flutter's nested paths). |
| `ui/pages/*.rs` | Pages the core owns: devices, device detail, add device, pairing, transfers, settings. |
| `ui/overlay/*.rs` | Toasts (the snackbar equivalent), modal dialogs, the incoming pairing prompt, the drop hint. |
| `ui/widgets.rs`, `ui/theme.rs`, `ui/icons.rs` | Shared widgets (card, page header with back, error view with Retry, empty state, verification code, `format_bytes`, `format_timestamp`), colour and style helpers, and Lucide icon helpers. |
| `ui/desktop/*.rs` | Platform glue: `tray.rs` (ksni / tray-icon), `notifications.rs` (notify-rust), `placement.rs` (window.json), `single_instance.rs` (interprocess), `open.rs` (opener), `dialogs.rs` (rfd). Each sits behind a small trait so tests swap in a fake, as the Flutter `DesktopShell` did. |
| `ui/testing.rs` | Test helpers: an `App` over `core::testing`, fake desktop services, a snapshot helper around `iced_test`. |

### The UI plugin seam (step 2 builds it)

Plugin authors write typed code. The shell stores trait objects. This is
the same arrangement as the core, where `builtin()` returns
`Vec<Arc<dyn Plugin>>`.

```rust
/// A feature's UI half. Lives in src/plugins/<name>/ui.rs.
pub trait UiPlugin: 'static {
    /// The same id as the core plugin ("ping").
    fn id(&self) -> &'static str;
    type Message: Clone + std::fmt::Debug + Send + 'static;

    /// A chip on the device card, the detail header and the tray label (battery).
    fn device_status(&self, device: &DeviceSnapshot) -> Option<DeviceStatus> { None }
    /// Actions for one device, as data. The detail page draws them as buttons
    /// and the tray as menu items, so the two can't drift apart.
    fn device_actions(&self, device: &DeviceSnapshot) -> Vec<DeviceAction<Self::Message>> { vec![] }
    /// Pages this plugin owns (browse: the file browser), keyed by a page name.
    fn view_page<'a>(&'a self, ctx: &UiContext, device: &'a DeviceSnapshot, page: &str)
        -> Option<Element<'a, Self::Message>> { None }
    /// Files dropped on a device, or on a page this plugin owns.
    fn drop_target(&self, device: &DeviceSnapshot, route: &Route) -> Option<DropTarget<Self::Message>> { None }
    /// A settings section (clipboard: "Sync clipboard").
    fn view_settings<'a>(&'a self, settings: &'a SettingsSnapshot) -> Option<Element<'a, Self::Message>> { None }
    /// Core events: plugin events (ping.received) and anything else it watches.
    fn on_event(&mut self, ctx: &UiContext, event: &CoreEvent) -> Command<Self::Message> { Command::none() }
    fn update(&mut self, ctx: &UiContext, message: Self::Message) -> Command<Self::Message>;
    fn subscription(&self) -> Subscription<Self::Message> { Subscription::none() }
}
```

- **`ErasedUiPlugin`** is dyn-compatible. A blanket
  `impl<T: UiPlugin> ErasedUiPlugin for T` maps `T::Message` into
  `PluginMessage { plugin: &'static str, message: Arc<dyn Any + Send + Sync> }`
  and downcasts on the way back. Routing by `plugin` id means the downcast
  can't fail unless there's a bug, so it may `expect`. Plugin code never
  sees `Any`.
- **`Command<M>`** wraps `iced::Task<Outcome<M>>`, where
  `Outcome = Plugin(M) | Shell(ShellRequest)`. Plugins ask the shell for
  shared things through `ShellRequest` instead of reaching into it:
  - `Toast { text, action: Option<(label, Route)> }`
  - `Notify { title, body }`: toast if the window is focused, desktop
    notification otherwise. This is Flutter's `_report`.
  - `Navigate(Route)`, `ShowWindow`
  - `PickFiles { title, confirm_label, then: fn(Vec<PathBuf>) -> M }`
  - `Confirm { title, body, confirm_label, then: M }`: the modal dialog
  - `Prompt { title, label, initial, validate, then }`: the name dialog
- **`UiContext`** gives a plugin:
  - `core()` and `plugin_context()`, so it can call its own module's typed
    API;
  - `spawn(future) -> Task`, which runs async core work **on the daemon's
    tokio runtime** (see Traps);
  - `device(id)` and `transfers()` from the store;
  - `window_focused()`.
- **`DeviceAction<M>`**: `{ id, label, icon, enabled: bool, visible_in_tray: bool, message: M }`.
  - Capability rules decide `enabled` and whether the action is listed at
    all, exactly as in Appendix A §3 and §10. Example: *Send clipboard* is
    listed only when the device supports clipboard, and enabled only when
    it accepts it.
  - The tray lists the same actions, in the same order.
  - Order the actions by where each plugin appears in `builtin_with_ui()`.
- **Registration: one instance per plugin, shared by both halves.** The
  stateful plugins expose their API as methods on the plugin instance:
  `ClipboardPlugin::send_to` and `BrowsePlugin::{list_files, download, …}`.
  The core only holds them as `Arc<dyn Plugin>`, so the UI half must get
  the **same `Arc`** when it's built. Never downcast `dyn Plugin`.
  - Add `plugins::builtin_with_ui(clipboard) -> Builtin { core: Vec<Arc<dyn Plugin>>, ui: Vec<Box<dyn ErasedUiPlugin>> }`
    behind the `gui` feature, next to `builtin()`.
  - It builds each plugin once, then its UI half from it, e.g.
    `let clipboard = Arc::new(ClipboardPlugin::new(backend)); … ClipboardUi::new(clipboard.clone())`.
    It has one line per plugin, in the same order as `builtin()`.
  - `RunningService` gets a way to start with plugins built by the caller.
    For example, `RunningService::start_with(request, |clipboard| …)`: the
    closure receives the clipboard backend the daemon chose (system or
    in-memory) and returns the core list. The UI list is kept aside by the
    caller.
  - `start()` stays and calls `start_with(request, plugins::builtin)`, so
    the CLI is unchanged.
  - A feature with no UI (none today) has no UI half.

### The typed plugin API rule

The UI can't call an axum route, so every action the UI takes must be a
Rust function or method that `http.rs` also calls. Most already exist:

| Plugin | Already there | To add |
| --- | --- | --- |
| ping | `ping::send_ping(ctx, id, message)` | — |
| findmyphone | `findmyphone::ring_device(ctx, id)` | — |
| battery | device state only | — |
| clipboard | `ClipboardPlugin::send_to(&self, ctx, id)`, `set_text`, `ClipboardSettings::sync_enabled_patch` | — |
| share | `share::send_file(ctx, id, name, size, transfer_id)`, which returns a byte `Sender` the HTTP handler streams multipart into | `share::send_path(ctx, id, path)`: opens the file, calls `send_file` with its size, and spawns the copy from disk into the sender on the daemon runtime |
| browse | `BrowsePlugin::{list_files, open_file, download, upload, create_directory, move_file, delete}` (async) | an upload from a local path, like share's |

When a step needs something that isn't there, add it to the plugin's
`mod.rs` and make `http.rs` call it, without changing behaviour. The HTTP
tests (`tests/*_e2e.rs`, `tests/api.rs`) must pass unchanged, and they
prove it.

## Owner decisions (2026-09-25)

- **Wayland: ship without drag and drop.** winit has none there. Don't
  force X11 or XWayland to get it back. Dropping works on X11, macOS and
  Windows. On Wayland the *Send files* and *Upload files* buttons do the
  same job.
- **The app's installed name is `myConnect`.** Cargo still builds it as
  `myconnect-gui` (the `gui` crate's `[[bin]]`). `myConnect` and the CLI's
  `myconnect` are the same file name on case-insensitive filesystems
  (macOS and Windows defaults), so they would overwrite each other in
  `target/`. Packaging (step 15) renames it:
  - Linux: `/usr/bin/myConnect`
  - macOS: `MyConnect.app/Contents/MacOS/myConnect`
  - Windows: `myConnect.exe`
  - The `.desktop` file's `Exec=` and the window class follow it.
- **The Linux packages ship the CLI too**, alongside the app.

## Deliberate differences

These differ from the Flutter app on purpose. Don't "fix" them back.

- **No external daemon mode.** Flutter could attach to a daemon through
  `MYCONNECT_API_URL`. The new UI always embeds its daemon. To test against
  another instance, run a CLI peer and point the CLI at the UI's API port.
- **Configuration is command-line flags and environment variables, not
  compile-time defines.** They mirror `myconnect run`: `--data-dir`,
  `--download-dir`, `--device-name`, `--discovery-loopback`,
  `--no-system-clipboard`, `--api-port`, `--api-token`.
  - Each flag also reads the env var of the same name
    (`MYCONNECT_DATA_DIR`, …), so the CLAUDE.md isolation recipe keeps its
    names.
  - The API token defaults to a random one. The UI logs the API address at
    `info`. With `--api-token` the CLI can use a known token.
  - The version shown in Settings is `env!("CARGO_PKG_VERSION")`, plus the
    git describe from a `build.rs` when one is available.
- **No FFI.** `ffi/` (`myconnect-ffi`) exists only for Flutter and is
  deleted with it in step 16.
- **Retry restarts nothing that isn't broken.** Flutter's "MyConnect could
  not start" + Retry restarted the daemon. Keep that screen: a failure from
  `RunningService::start`, shown in the window with Retry, plus the tray
  still working. It now calls `RunningService::start` again.
- **No reconnecting banner.** There is no connection to lose in-process. A
  lagged receiver just takes a fresh snapshot, silently.
- **Start hidden works on every platform.** Flutter only started hidden on
  Linux; see Appendix A §12, "Behaviour gap". The new UI opens its window
  only if the saved placement says it was visible, or if there is no tray.
- **Single instance works everywhere.** It comes from one local socket, not
  GApplication/LaunchServices. Windows gets it for free.
- **Not sandboxed on macOS.** Flutter was sandboxed, which caused the
  download-folder bookmark problem in HANDOFF. The new app is a plain,
  ad-hoc-signed bundle, unless the owner later wants the App Store.
- The window title is "MyConnect", not "myconnect_ui".

Improvements are welcome where they cost little: Escape closes dialogs,
Ctrl/Cmd+W closes the window, Ctrl/Cmd+Q quits. Anything bigger goes in
"Open work" at the end, not into a parity step.

## Ground rules for this work

Everything in HANDOFF's ground rules still applies to the daemon, **except**
that "the UI reads and writes only through the HTTP API" and "native code
only starts and stops the daemon" now describe the Flutter app only. For
the new UI:

- **The UI keeps no state of its own** beyond `window.json` (ADR 0009's
  exception). Preferences are daemon settings. The store is a cache of core
  snapshots, discarded on exit.
- **Every resource still needs a snapshot and events.** That hasn't
  changed: the CLI needs them, and so does the UI after a lag.
- **Anything the UI does, the CLI can do.** A UI feature that needs new
  core behaviour adds it to the plugin or core Rust API first, then to
  `http.rs` and `client.rs`/the CLI, then to `ui.rs`.
- **Isolate every run** (CLAUDE.md). For the new app that means:
  - `--data-dir "$dir/data" --download-dir "$dir/downloads" --discovery-loopback`;
  - on Linux, `dbus-run-session -- xvfb-run --auto-servernum`;
  - under Xvfb, `ICED_BACKEND=tiny-skia` (no GPU);
  - update CLAUDE.md in step 1 with this recipe.
  - On macOS, `127.255.255.255` doesn't exist, so loopback discovery can't
    find peers. Use `--demo` (below), or do pairing tests on Linux.
- **Record dependencies** in the new ADR's library table (step 1), as
  ADR 0005 did.

"Done" for every step:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets          # includes the gui feature via the gui crate
cargo build -p myconnect                      # CLI still builds without iced (from step 1)
git diff --check
```

Also look at the result. Render snapshots headlessly (see *Seeing the UI*)
and read the PNGs, and for anything involving windows, the tray, drops or
notifications, run the real app. Until step 16, the Flutter checks in
HANDOFF still have to pass too, because nothing may break the Flutter app
before then.

### Seeing the UI

- **Headless snapshots.** `iced_test::simulator::Simulator::with_size(settings,
  size, element).snapshot(&theme)` renders to a PNG with no display. It
  needs the Lucide font in `Settings::fonts`. The spike's
  `snapshot_device_list` test shows the pattern: it writes
  `$SNAPSHOT_DIR/{light,dark}-wgpu.png` and skips when the variable is
  unset.
  - Every page gets such a test, in light and dark, with fake data.
  - Read the PNGs after each UI change.
  - The snapshot font (Fira Sans) is not the app's font. Judge layout, not
    typography.
- **`--demo`.** Fills the embedded core with made-up paired devices through
  the core's own entry points (`discover_device`, `mark_device_connected`,
  `handle_peer_packet` for battery). The UI sees normal snapshots and
  events. Nothing is written to the trust store.
  - Keep it, move it to `src/ui/demo.rs`, and extend it as features
    arrive: fake transfers need a core-side hook, so add one only if
    needed.
  - Never ship it enabled.
- **Real peers.** Use a CLI daemon (`myconnect run --discovery-loopback`)
  or `examples/fake_phone.rs` as the peer, on Linux, as in HANDOFF's
  "Verifying in the real app". The CLI can also drive the UI's own daemon
  over its API port, for example `pair` or `send`.

## Steps

### 1. Foundation: the feature, the crates, the ADR

**Done (2026-09-25).** Where it differs from the text below:
- The `gui` feature turns on only `iced` and `iced_fonts` for now. The
  other UI crates are chosen in [`adr/0001`](adr/0001-native-ui-in-iced.md)'s
  library table, and each is added to the feature by the step that first
  uses it (rfd: 9, opener: 8, notify-rust, interprocess, ksni and
  tray-icon: 13). `iced_test` is a dev-dependency of `myconnect`.
- `ui::run(&RunningService, UiOptions, plugins)` borrows the service; the
  `gui` binary shuts it down after the UI exits. `UiOptions` carries the
  daemon's runtime handle (for `UiContext` in step 2) and `demo`.
- `ui/plugin.rs` has a placeholder `ErasedUiPlugin` (just `id()`) so
  `builtin_with_ui` has a type to return; step 2 replaces it with the full
  seam.
- `ui/testing.rs` has `snapshot(name, size, view)`, which writes
  `$SNAPSHOT_DIR/<name>-<light|dark>-<backend>.png`, replacing old images
  first (`matches_image` would otherwise compare against them).
- Boolean env vars accept `1`/`0`, `true`/`false`, `yes`/`no`, `on`/`off`.
- The `cargo tree` check is
  `cargo tree -p myconnect -e normal --prefix none | grep -c '^iced'`: a bare
  `grep iced` also matches a checkout path containing "iced".
- Checked in the real app on Linux (Xvfb, private bus, tiny-skia): `--demo`
  shows the list with live updates, only loopback sockets with
  `--discovery-loopback`, the CLI lists the app's devices through
  `--api-port`/`--api-token`, and closing the window quits and stops the
  daemon.

**Why:** everything after this assumes the layout above.

**Build:**
- The `gui` feature on `myconnect`, the optional dependencies, and
  `src/ui/mod.rs` with `run()`.
- `gui/` becomes the thin composition root:
  - parse args (flags and env, as in Deliberate differences);
  - build a tokio runtime and `RunningService::start`;
  - start through `RunningService::start_with` and
    `plugins::builtin_with_ui` (the UI list is empty until step 2), then call
    `ui::run(service, options, ui_plugins)`;
  - after the UI exits, shut the service down (as the spike does).
- Move the spike's device list into `src/ui/pages/devices.rs` and its
  watcher into `src/ui/sync.rs`, and move `demo.rs` to `src/ui/demo.rs`.
- Switch from `iced::application` to `iced::daemon` with one window opened
  at boot. That is the structure the tray needs later, even though closing
  still quits for now.
- Write `docs/adr/0001-native-ui-in-iced.md` (a new top-level ADR series,
  because `ui/docs/adr` goes away in step 16). It records:
  - the decision;
  - in-process access to the core;
  - UI halves in feature modules;
  - the `gui` feature;
  - which of `ui/docs/adr` 0001–0009 it supersedes (0001, 0002, 0004, 0005
    and 0006) and which carry over in spirit (0003, 0007, 0008, 0009);
  - a library table like ADR 0005's.
- Update CLAUDE.md's isolation section and HANDOFF's "done means" and
  "Read first" for the new app.
- Add the CI job (Linux only for now):
  - `cargo build -p myconnect` without the feature;
  - the workspace checks, which now build the UI. Install iced's Linux
    build dependencies (`libxkbcommon-dev`, `libwayland-dev`, `libvulkan1`
    or mesa, `libdbus-1-dev`) and run tests with `ICED_BACKEND=tiny-skia`.

**Done when:**
- `cargo run -p myconnect-gui -- --demo` shows the device list, as the
  spike did.
- `cargo tree -p myconnect -e normal --prefix none | grep -c '^iced'` prints 0.
- CI is green.

**Traps:**
- iced runs its own executor. Futures that touch the daemon's sockets or
  russh sessions must run on the **daemon's** runtime. Keep its
  `tokio::runtime::Handle` in `UiContext` and use `handle.spawn` +
  `Task::perform(join_handle)`.
- Don't turn on iced's `tokio` feature just to get a runtime. That would be
  a second runtime.

### 2. The UI plugin seam, with battery as the pilot

**Done (2026-09-25).** Where it differs from the text below and the seam
above:
- `UiPlugin::Message` must also be `Sync`: `PluginMessage` holds it in an
  `Arc`. `ErasedUiPlugin::update` asserts the message's plugin id before it
  downcasts.
- There are no `PluginPage` and `SettingsSection` types: `view_page` and
  `view_settings` return elements. `DropTarget` is `{ label, on_drop }`.
  Callbacks in `ShellRequest` and `DropTarget` are `Arc<dyn Fn>`
  (`Callback`), so they map through the erasure; `Prompt`'s `validate` is
  a `Validator` returning the error text, if any.
- `ui/route.rs` has the whole `Route` enum. Step 3 adds parents and the
  header. Until then `view` draws only the devices page and plugin pages;
  the other routes fall back to the devices page.
- The shell handles `Toast` (stacked at the bottom, gone after 4 s, the
  action button navigates; step 3 finishes them) and `Navigate`. `Notify`
  is a toast until step 13, `ShowWindow` focuses the window, and
  `PickFiles`, `Confirm` and `Prompt` log a warning until their steps.
- `UiContext::device` and `transfers` read the core until the store
  (step 4). Window focus is tracked from `window::events()`.
- Timers (toasts, `--demo`) run on the daemon's runtime, because iced has
  no timer without its `tokio` feature. A tokio future such as `sleep`
  needs the runtime when it is *made*, not only when polled, so
  `UiContext::spawn` takes an `async move` block. The real app caught this
  (`#[tokio::test]` hides it); `timers_work_off_the_daemon_runtime` now
  covers it.
- New slot `demo_packets(device, tick)`: `--demo` devices advertise every
  capability this build has, and each plugin makes up what its feature
  reports (battery: the phone drains, the tablet charges). That keeps
  `src/ui/demo.rs` free of feature names too.
- `builtin_with_ui` lists the core plugins itself, one line each, and a
  test checks it runs the same plugins as `builtin()` in the same order,
  with the UI halves in that order too.
- `ui::testing` gained `device(name)` and `outputs(task)`, which runs a
  `Task` and returns what it produced (through `iced_runtime`, a new
  dev-dependency).
- Snapshots: `devices` (a fake status plugin), `devices-battery` and
  `toasts`. Checked in the real app with `--demo`: the phone's battery
  drains through the slot.

**Why:** every feature step after this plugs in through the seam, so it
must exist and be proven on the smallest feature first. That is how the
daemon's module split started (research/feature-modules.md, phase 0).

**Build:**
- `ui/plugin.rs` as specified above.
- UI halves registered in `plugins::builtin_with_ui` (see Registration).
- The shell's `Message::Plugin(PluginMessage)` routing, `Command`/`Outcome`
  handling, and `ShellRequest` handling. For now only `Toast` and
  `Navigate` need to work; the rest arrive with the steps that need them,
  but the enum lists them all.
- `src/plugins/battery/ui.rs` implements `device_status`: the battery icon
  (charging / low ≤15 / medium ≤60 / full) and `N%`. The device card and
  (later) the detail header and tray label render the status slot for
  every plugin.
- Remove the battery code from the spike's page. The page must no longer
  mention battery at all.

**Done when:**
- `rg -n 'battery' src/ui` finds nothing.
- The devices page still shows battery through the slot.
- There's a unit test of the erased plugin round trip.

### 3. Shell foundations

**Why:** pages need navigation, dialogs, toasts and error handling before
they can be ported faithfully.

**Build:**
- **Routing:**
  - `Route` with a parent for back navigation;
  - a page header with a back button, a title, and trailing icon buttons
    (with tooltips);
  - a routed `view`.
- **Toasts:**
  - bottom of the window, auto-dismiss after about 4 s, an optional action
    button, stacked;
  - `ShellRequest::Toast`.
- **Modal dialogs:**
  - built with `stack` + `opaque` + `center` (the iced `modal` example);
  - `Confirm` and `Prompt`, with text input, validation, error text under
    the field, Enter submits, Escape cancels, and the confirm button
    disabled while busy;
  - one modal at a time.
- **Startup states:**
  - a "Starting MyConnect…" screen while `RunningService::start` runs (off
    the UI thread, so the window paints);
  - a "MyConnect could not start" screen with the error and Retry.
  - This means `gui/` hands `ui::run` a *start function*, not a started
    service.
- **Error wording:**
  - `describe_error(&CoreError)`, and plugin error types, should read like
    `api_exception.dart:35-75`. The messages are the same user-facing
    sentences keyed by error code, so port them.
  - Map through `OperationErrorCode` / `ApiProblem` codes so the UI and the
    CLI say the same thing.
- **Shared widgets:** see the table above (ui/widgets.rs).

**Done when:** there are snapshot tests of the header, toast, confirm
dialog, prompt with an error, and the startup error screen; and there are
unit tests of `describe_error` for every code in `api_exception.dart`.

### 4. The store: devices, pairings, transfers, settings

**Why:** all pages read from it. Getting the sync rules right once is
cheaper than per page.

**Build:**
- `ui/store.rs` holds
  `devices: BTreeMap<id, DeviceSnapshot>`, `pairings`, `transfers` and
  `settings`.
- `ui/sync.rs` is one subscription:
  - `core.subscribe()` **first**, then snapshots of all four
    (`core.devices()`, `core.pairings()`, `core.transfers().list()`,
    `core.settings()`), sent as one message;
  - then every event, forwarded to the store and to every plugin's
    `on_event`;
  - on `Lagged`, a fresh snapshot;
  - on `Closed`, stop.
  - Find or add the core getters for pairings and transfers. The API
    handlers already read them, so reuse what they call.
- **Guards** (port these; the in-process stream doesn't make them
  unnecessary, because events queued before the snapshot are replayed
  after it):
  - **Pairings:** a non-terminal snapshot never replaces a terminal one
    (`pairings_controller.dart:107-118`).
  - **Transfers:** newer wins. Keep the existing one if it is terminal and
    the new one isn't, or if its `updatedAt` is later
    (`transfers_controller.dart:166-177`).
  - **Devices:** upsert on `device.discovered/connected/updated/disconnected`,
    remove on `device.forgotten`.
  - **Settings:** replace on `settings.changed`.
  - **Mutations** apply the snapshot the core returns immediately, through
    the same guards.
- **Derived views**, as plain functions on the store:
  - paired devices, sorted by name case-insensitively (connected-first
    ordering is allowed; the spike does it);
  - unpaired and not `unavailable`, for Add device;
  - pending incoming pairings, oldest first;
  - transfers newest first, optionally for one device.

**Done when:** unit tests port the controller tests in Appendix B,
"Controllers", against `core::testing` or hand-fed events. That covers
replay across a snapshot, guards, and forget applying without waiting for
the event.

### 5. Devices page, finished

**Build:** everything in Appendix A §2 that the spike lacks:
- the header actions (Settings, Transfers);
- "This computer: {name}";
- the "Add device" primary button;
- the empty state's "Find a device to pair";
- cards that open the device;
- hover and pressed styles.
- The drop highlight on a card ("Drop to send") is wired in step 11. Leave
  the card able to show a highlighted state.

**Done when:** snapshot tests cover loading, empty and list, in light and
dark.

### 6. Device detail, with ping, ring and clipboard

**Build:**
- **The page** (Appendix A §3): header with icon, name and status (status
  slot included), selectable facts (Device ID, Type, Protocol version),
  the **action slot** rendered as buttons, Recent transfers (up to 5, "See
  all"), and Unpair with a confirm dialog that navigates home.
- **"This device is no longer known."** when the id isn't in the store.
- **Plugin UIs:**
  - `plugins/ping/ui.rs`: the *Ping* action (toast "Pinged {name}.").
    `on_event(ping.received)` gives `Notify { title: device name, body:
    message or "Ping!" }`.
  - `plugins/findmyphone/ui.rs`: *Ring*, through
    `findmyphone::ring_device(ctx, id)`.
  - `plugins/clipboard/ui.rs`: *Send clipboard*, through
    `ClipboardPlugin::send_to` on the shared instance.
  - Each follows the listed/enabled rules and the toast wording in
    Appendix A §3.
- **Unpair** calls the core's forget (the one `DELETE /devices/{id}` uses)
  and removes the device from the store right away.
- An action on a device that disconnects mid-flight must not panic or
  toast twice. Messages carry the device id and name, not a reference to
  the page.

**Done when:**
- Simulator tests click each action and check the gating (port "send /
  ping / ring / clipboard gated" from Appendix B).
- The unpair test returns to the list.
- There's a snapshot of the page.

*Independent once steps 2–4 have landed: steps 6, 7, 8, 9.*

### 7. Add device, scanning, pairing, incoming prompt

**Build:**
- **Add device** (Appendix A §4):
  - scan on open (`core.announce()`), a 4 s "searching" progress bar, and
    Scan again;
  - candidate rows with the blocker text, Pair (with a spinner while
    starting);
  - "Add by IP address" with its dialog: validation error under the field,
    Enter submits, `core.announce_to(ip)`, parse errors worded like
    `invalid_address`.
- **Pairing page** (§5): every status with its icon, title, detail and
  buttons (Cancel, Done, Close, Try again), and the `VerificationCode`
  widget.
- **Incoming prompt:**
  - an overlay on every route while pending incoming pairings exist;
  - "{n} more request(s) waiting";
  - inline error on failure;
  - Accept/Reject disabled while busy;
  - disappears on its own when the request is resolved anywhere (CLI,
    timeout).
  - Drops are disabled while it shows.
- Pairing lives in the core, so all of this is shell code, not a plugin.

**Done when:**
- Tests cover "incoming prompt on any screen until resolved" and "failed
  response keeps the prompt open with the reason", plus add by IP.
- A real check on Linux: pair the app with a CLI peer in both directions,
  with `myconnect pair` on the CLI side.

### 8. Transfers

**Build:**
- The Transfers page and a `transfer_row` widget (Appendix A §7):
  - direction icon, name, "From/To {device} · status";
  - a progress bar: determinate while `transferring`, indeterminate while
    queued or connecting;
  - Cancel;
  - Open file and Open folder on completed incoming transfers with a
    `savedPath`, through `opener` (`open` and `reveal`), with a toast
    "Couldn't open {path}" on failure.
- Status wording, including the failure reasons, as in
  `transfer_tile.dart:88-104`.
- `format_bytes` exactly as `transfer_tile.dart:107-118`.
- Transfers are core, so this is shell code.

**Done when:**
- The "transfers page shows progress and cancels" test is ported.
- A real check: send a 1–2 GB file from the CLI peer and cancel it
  mid-way from the UI.

### 9. Settings

**Build:** the page from Appendix A §9:
- **Device name:** the name dialog with max 32, a counter, the helper text,
  and the daemon's validation error shown in the field.
- **Download folder:** an `rfd` folder picker starting at the current
  folder.
- **Keep running when the window is closed:** the `closeToTray` switch.
- **Version.**
- **Plugin sections:** every plugin's `view_settings` slot, in
  `builtin_with_ui()` order.
  - `plugins/clipboard/ui.rs` renders "Sync clipboard" and patches
    `plugins.clipboard.syncEnabled` through
    `core.update_settings(SettingsPatch)`.
  - The shell must not know that section exists.

**Done when:** the settings widget tests in Appendix B are ported.

### 10. Desktop integration spike (do this before 11 and 13)

**Why:** these are the parts where Rust GUI crates are weakest, and a
surprise here changes the design of steps 11 and 13. Keep this spike
throwaway, time-boxed, and written down.

For each item below, find out on **Linux (X11 and Wayland), macOS and
Windows** where available. Write the answers into the ADR from step 1.
(Agents on macOS: the real display is the owner's. Ask before opening
windows there, or use the headless route.)

1. **File drop with a position.**
   - iced reports `window::Event::FileHovered(path)`, `FileDropped(path)`
     and `FilesHoveredLeft`. Does a cursor position arrive *during* an OS
     drag (`mouse::Event::CursorMoved`), so we can hit-test which device
     card is under the pointer?
   - winit has **no drag-and-drop on Wayland** at all. The owner decided
     to ship without it there (see Owner decisions). Only check that a
     drag over the window on Wayland does nothing harmful, and don't force
     X11 or XWayland to get it back.
2. **Tray with iced.**
   - `ksni` on Linux: its own thread and D-Bus connection; left click
     `activate`, right click menu, submenus, enabled flags; menu updates
     while running.
   - `tray-icon` + `muda` on macOS and Windows: it must be created on the
     main thread *after* the event loop starts. Create it lazily from the
     first `update`, and forward its event channel into a `Subscription`.
3. **Window hide and show vs close and reopen** under `iced::daemon`: can
   the window be closed and later re-opened (`window::open`) with the saved
   placement? That is the preferred design: no GPU surface while in the
   tray.
4. **Notifications.** `notify-rust` on each platform. Does a click reach us
   (Linux: the `default` action)? If macOS or Windows can't report the
   click, "clicking a notification shows the window" becomes Linux-only;
   write that down.
5. **Single instance.**
   - An `interprocess` local socket named from a hash of the data dir, so
     isolated test instances never collide with the owner's app.
   - First instance listens; a second connects, sends `show`, and exits.
   - Handle a stale socket file.
6. **Monitors.** How to list displays for the `window.json` fits-on-screen
   check. `iced::window::monitor_size` gives only the current monitor; if
   that isn't enough, evaluate `display-info`.

**Done when:** the ADR has a table of what works where, and a decision for
each gap.
- **DnD fallback**, if position hit-testing isn't possible: a drop on the
  device page or a file-browser folder targets that device or folder, and a
  drop anywhere else opens the "Send N files" chooser, as Flutter already
  does for drops away from a device.
- **Wayland:** no drag and drop (owner's decision). *Send files* and
  *Upload files* buttons cover it.

### 11. Share: send files, drag and drop, the chooser

**Build:**
- Add `share::send_path(ctx, device_id, path)` in the daemon first (see
  the typed API table).
- `plugins/share/ui.rs`:
  - the *Send file(s)* action (multi-file `rfd` picker, confirm label
    "Send"); files are sent one at a time, and failures are summarised once
    ("Couldn't send {name}: {reason}" / "Couldn't send N files: {first
    reason}");
  - `drop_target` for a device that `acceptsFiles`.
- **Shell drop handling:**
  - hover state (per-card highlight if step 10 made that possible);
  - the window border and the "Drop on a device, or anywhere to choose one"
    pill;
  - only regular files are accepted ("Only files can be sent, not
    folders.");
  - the **Send files chooser** dialog: eligible devices update live;
    "No paired device is connected and able to receive files.";
  - after choosing, navigate to the device and send.
- The shell asks every plugin's `drop_target` for the device and route
  under the drop. It never knows about share or browse.

**Done when:** the `send_files_test.dart` scenarios in Appendix B (except
the tray ones, which come in step 13) are ported, and a real drop has been
checked on Linux X11.

### 12. Browse: the file browser

The largest feature. It is all `plugins/browse/ui.rs`, over the shared
`BrowsePlugin` instance's methods (list, content, download, mkdir, move,
delete) plus a new upload from a local path. They are async over SFTP, so
they run with `UiContext::spawn`.

**Build:** Appendix A §8 in full:
- the *Browse files* action (enabled when `sharesFiles`) and the plugin
  page;
- the storage list with "Connecting to the device…";
- breadcrumbs, Up, and a horizontally scrolling crumb row that keeps the
  end visible;
- sortable Name / Size / Modified columns, folders first, Modified shown
  from 600 px wide;
- show hidden files;
- the per-row menu (Preview, Download, Rename, Delete);
- activate on click (folder opens, a small image previews, anything else
  downloads);
- the image preview dialog with pan and zoom (iced `image::viewer`), and
  "This image can't be shown.";
- the name dialog: pre-selects the name without its extension, runs the
  client-side validation, "Create"/"Rename";
- delete confirmation with the file or folder wording;
- toast "Downloading {name}" with a *Transfers* action;
- refetch after every change and when the device reconnects;
- "{name} doesn't share its files." and "Connect {name} to browse its
  files.";
- the `files_unavailable` hint.
- **Drop into the open folder** uploads, through `drop_target` with the
  route's folder.

**Done when:**
- The `files_page_test.dart` scenarios are ported.
- `tests/browse_e2e.rs` still passes.
- A real check against `examples/fake_phone.rs` on Linux covers list,
  preview, download, upload, rename, mkdir and delete.

### 13. Background: tray, close-to-tray, notifications, window placement, single instance

Use the decisions from step 10.

**Build:**
- **Close:** if `closeToTray` (or settings are unavailable), close the
  window and keep running; otherwise quit.
- **Quit:** save the placement, shut down the service, exit. Guard against
  running twice. OS-requested quits (macOS menu bar, logout) take the same
  path.
- **Tray menu** (Appendix A §10), rebuilt when paired devices, their status
  or their actions change:
  - Open MyConnect
  - one submenu per **connected** paired device, labelled "{name} ·
    {status}" from the status slot, holding that device's
    **`device_actions` where `visible_in_tray`**, then Show details
  - "No paired devices" / "No devices connected"
  - Settings
  - Quit
- **Tray actions from a hidden window:**
  - they don't show the window;
  - only failures are reported, through `Notify`;
  - *Send files…* re-checks the device after the picker closes ("Couldn't
    send to {name}: The device is not connected right now.").
- **Tray click:** left click shows the window. Tray icons:
  `assets/tray_icon.png` and, on macOS, `tray_icon_template.png` as a
  template.
- **Notifications** (shell): pairing request while unfocused (withdrawn
  when resolved), and file received (only completions seen after startup,
  incoming only, unfocused). Ping comes from the ping plugin's `on_event`.
- **Window placement** (ADR 0009): `window.json` in the same locations
  (`$MYCONNECT_DATA_DIR` when given):
  - `{visible, maximized, bounds}`, debounced 500 ms, written atomically;
  - bounds recorded only when normal;
  - fits-on-screen, otherwise centred;
  - start hidden when the saved state says so, but always show when there
    is no tray.
  - Port `window_placement_test.dart`.
- **Single instance** (step 10's design): a second launch shows the
  running window.

**Done when:**
- The `background_host_test.dart` scenarios are ported, against fake tray,
  notification and window services.
- A real check on Linux under a private bus: close with and without
  close-to-tray, Quit through the tray's D-Bus menu (HANDOFF explains how
  without a tray host), a second launch shows the window, and placement
  survives a restart.

### 14. End-to-end tests

**Why:** unit tests with fakes missed real bugs in the first milestone
(HANDOFF).

**Build:**
- `tests/ui_e2e.rs` (`required-features = ["gui"]`) runs the UI's `App`
  with a real `RunningService` (loopback, temporary dirs) against a second
  `RunningService` peer, headless. Drive it through `App::update` with
  messages and assert on the store and on `Simulator` finds of the view.
  - Evaluate `iced_test::Emulator` / `.ice` scripts first; they run the
    whole program, subscriptions included. Use them if they're good
    enough, and write the finding down.
- Port the seven scenarios from `integration_test/app_test.dart`:
  1. accept an incoming pairing (codes match);
  2. reject one;
  3. pair with a scanned device;
  4. unpair, and the peer forgets the app too;
  5. ping and receive a ping back;
  6. send the clipboard to a peer that missed it;
  7. send a file to the peer and receive one back.
- Add one for browsing against the fake phone. Flutter never had it.
- Linux only, under `dbus-run-session`. Loopback discovery needs
  `127.255.255.255`.

**Done when:** the suite passes in CI.

### 15. Packaging and CI

**Build:**
- Release builds of `myconnect-gui` for:
  - **Linux:**
    - `.deb` for amd64 and arm64, installing the app as `/usr/bin/myConnect`
      and the CLI as `/usr/bin/myconnect` (see Owner decisions for why
      cargo still calls it `myconnect-gui`);
    - the `.desktop` file (keep `Categories=Network;FileTransfer;`,
      `StartupWMClass`, `SingleMainWindow=true`);
    - the hicolor icons;
    - package dependencies derived with `dpkg-shlibdeps`.
    - The Arch PKGBUILD keeps repackaging the `.deb`.
  - **macOS:** a universal `MyConnect.app` (+ `.dmg`), ad-hoc signed, not
    sandboxed; bundle id `org.myconnect.MyConnect`; `LSUIElement` stays
    false.
  - **Windows:** NSIS or MSI installing `myConnect.exe`, plus the
    notification AUMID `org.myconnect.MyConnect`. If the installer also
    ships the CLI, it must not sit next to it as `myconnect.exe`: Windows
    ignores case, so put it in a `cli\` subfolder, or ship the CLI
    separately.
- Evaluate `cargo-packager` against the existing hand-written scripts
  (`ui/linux/packaging/`). Prefer fewer moving parts.
- Move the icon sources (`ui/icon/*.svg`) and `tool/generate_icons.sh` to
  a top-level `assets/` and point them at the new outputs.
- Replace the Flutter jobs in `.github/workflows/build.yml` and `ci.yml`,
  and keep a job that builds `-p myconnect` without the `gui` feature.
- The `.deb` ships the `myconnect` CLI too, in the same package (owner's
  decision). The Arch package gets it the same way, since it repackages the
  `.deb`.

**Done when:** a tagged build produces all artifacts. The `.deb` installs
and launches on Debian 12 in a container under Xvfb. The macOS app
launches from Finder with its tray icon.

### 16. Remove Flutter

**Only after** Appendix A is fully ticked and the owner has used the new
app.

**Build:**
- Delete `ui/` and `ffi/` and remove them from the workspace.
- Move `ui/docs/adr/` to `docs/archive/flutter-adr/`.
- Update ARCHITECTURE (§2 module map with `ui` and the `ui.rs` halves, §9
  embedding becomes "the UI embeds the daemon in-process", §10 testing),
  HANDOFF (read first, done means, verifying in the real app, traps),
  README and CLAUDE.md.
- Update `ARCHITECTURE.md`'s "a new feature is…" paragraph: a new feature
  is `mod.rs` + `http.rs` + `ui.rs`, one line in `builtin()` and one in
  `builtin_with_ui()`, and the CLI in `client.rs`/`cli.rs`.

## Traps

- **Two runtimes.** iced polls futures on its own executor. Anything that
  touches tokio I/O from the daemon (russh, payload sockets, file
  transfers) must be spawned on the daemon's runtime handle. Symptom: a
  panic "there is no reactor running", or a hang.
- **Don't block `update`.** Core calls that take locks are fine. Anything
  that does I/O (SFTP, file reads, `RunningService::start`) goes through a
  `Task`.
- **Event ordering after subscribe-then-snapshot.** Events already
  reflected in the snapshot are replayed after it. Device events carry the
  full device, so replaying one is harmless. Transfers and pairings are
  why the guards exist; don't drop them.
- **Actions outlive their page.** An event can remove the device while an
  action is in flight. Messages carry ids and names, and handlers look the
  device up again rather than holding a reference. This is the Rust
  version of the Flutter "capture the messenger before awaiting" trap.
- **Plugins don't import each other in UI code either.** If share and
  browse both need "upload files with a summary toast", that helper
  belongs in `src/ui/`, not in either plugin.
- **The tray is not a view.** It can't render an `Element`. That is why
  actions are data. Don't add widget-returning tray APIs.
- **`cargo test` and the clipboard.** Run under a private display and bus
  (CLAUDE.md). The system clipboard tests clobber the owner's clipboard
  otherwise.
- **macOS loopback.** `--discovery-loopback` can't find peers on macOS
  (no `127.255.255.255`). Use `--demo` there, and do peer tests on Linux.
- **iced version.** Pin `iced = "0.14"` and `iced_fonts = "0.3"` (the
  version that matches 0.14). Upgrading iced is its own change, never
  mixed into a feature step.

## Open work (not parity; after step 16)

- Drag files out of the browser to the desktop (ADR 0008 lists it).
- Start on login.
- Remembered add-by-IP addresses (HANDOFF "Smaller follow-ups").
- Low-battery notification (`thresholdEvent`).
- Accessibility: check what iced 0.14 exposes to screen readers and
  record the gap against Flutter.

---

## Appendix A: parity checklist

Tick as you go (`[x]`), in the same commit as the work. File references
are to the Flutter app under `ui/lib/src/`.

### §0 Startup and configuration
- [ ] Starting screen while the daemon starts; error screen with Retry that retries the start (`core/daemon/daemon_gate.dart`)
- [ ] Tray and close-to-tray work even when the daemon failed to start
- [ ] Flags/env: data dir, download dir, device name, discovery loopback, system clipboard (default on), API port/token; `window.json` goes to the data dir when one is given
- [ ] Version in Settings
- [ ] Light and dark themes follow the system

### §2 Devices (home) (`features/devices/devices_page.dart`)
- [ ] Title "Devices"; Settings and Transfers buttons with tooltips
- [ ] "This computer: {name}" once settings are loaded
- [ ] "Add device" button
- [ ] Loading; error with Retry; empty: icon, "No paired devices yet", "Find a device to pair"
- [ ] Paired devices only, sorted by name (case-insensitive)
- [ ] Card: type icon (primary when connected), name, status label (Connected / Nearby / Not reachable) + status slot; opens the device
- [ ] Drop on a card: "Drop to send" highlight when the device accepts files

### §3 Device detail (`features/devices/device_detail_page.dart`)
- [ ] Title = device name; "This device is no longer known." when gone
- [ ] Header: large icon, name, status (with battery)
- [ ] Selectable facts: Device ID, Type, Protocol version
- [ ] Send file: enabled when `acceptsFiles`; multi-select picker "Send"; one summary toast for failures
- [ ] Browse files: enabled when `sharesFiles`
- [ ] Ping: enabled when `acceptsPings`; toast "Pinged {name}."
- [ ] Ring: enabled when `canRing`; toast "Asked {name} to ring."
- [ ] Send clipboard: listed if `supportsClipboard`, enabled if `acceptsClipboard`; toast "Sent the clipboard to {name}."
- [ ] Errors from any action as a toast
- [ ] Recent transfers (≤5, newest first, no device name) + "See all"
- [ ] Unpair: confirm "Unpair {name}?" + body text; goes home; toast on error
- [ ] Drop anywhere on the page sends to this device

### §4 Add device (`features/devices/add_device_page.dart`)
- [ ] Scan on open; 4 s "searching" indicator; Scan again disabled while searching; toast on scan error
- [ ] Intro text
- [ ] "No devices found" when empty and not searching
- [ ] Candidates: unpaired and not unavailable; blocker text ("Pairing in progress" / "Not connected"), otherwise reachability
- [ ] Pair: disabled when blocked or while another start is in flight; spinner for the one starting; goes to the pairing page; toast on error
- [ ] "Add by IP address" row + dialog: autofocus, hint "192.168.1.20", helper text, Enter submits, error under the field, Add disabled while sending; restarts the searching indicator on success

### §5 Pairing (`features/pairing/`)
- [ ] Pairing page: every status's icon, title and detail (table in the inventory); "This pairing request no longer exists."
- [ ] Buttons: Cancel (pending), Done (accepted → device), Close / Try again (other terminal states); disabled while busy
- [ ] Verification code: large, monospace, letter-spaced, selectable, on a rounded surface
- [ ] Incoming prompt over every screen while requests are pending; modal; "{n} more request(s) waiting"; Accept/Reject; inline error keeps it open; disappears when resolved elsewhere
- [ ] Guard: a non-terminal pairing snapshot never replaces a terminal one

### §6 Send files and drop (`features/send/`)
Dropping applies on X11, macOS and Windows, not on Wayland (see Owner decisions).
- [ ] Drop on a device that accepts files: sends directly
- [ ] Drop elsewhere / on a device that can't take files: chooser dialog ("Send {file}" / "Send N files", live list, empty text, Cancel); after choosing, go to the device
- [ ] Drop on an open browser folder: uploads there
- [ ] Folders refused: "Only files can be sent, not folders."
- [ ] Drag hint: window border + "Drop on a device, or anywhere to choose one"
- [ ] Drops disabled while the pairing prompt shows
- [ ] Files sent one at a time; failures summarised once (send and upload wording)

### §7 Transfers (`features/transfers/`)
- [ ] Page: newest first; empty "No transfers yet"; loading; error + Retry
- [ ] Row: direction icon, name, "From/To {device} · status", status wording incl. failure reasons
- [ ] Progress: determinate while transferring, indeterminate before
- [ ] Cancel while active; toast on error
- [ ] Open file / Open folder on completed with `savedPath`; toast "Couldn't open {path}"
- [ ] `format_bytes` as in `transfer_tile.dart:107-118`
- [ ] Guard: newer / terminal wins

### §8 File browser (`features/files/`)
- [ ] Title "Files on {name}"; Upload files, New folder (both only inside a folder), Refresh, Show hidden files
- [ ] "no longer known" / "doesn't share its files" / "Connect {name} to browse its files."
- [ ] Storage list; "Connecting to the device…"; "The device isn't sharing any storage."
- [ ] Breadcrumbs: Up, Storage › root › segments, links, scroll to the end
- [ ] Columns Name/Size/Modified (Modified at ≥600 px), sort toggle, folders first, name tie-break
- [ ] Row: icon by extension, name, size, modified `YYYY-MM-DD HH:MM`, menu Preview/Download/Rename/Delete
- [ ] Click: folder opens, previewable image (≤32 MiB, jpg/jpeg/png/gif/webp/bmp) previews, else downloads
- [ ] Download toast with Transfers action
- [ ] Preview dialog with pan/zoom; "This image can't be shown."
- [ ] Name dialog: pre-selects the name without its extension; client validation ("Enter a name.", "That name is reserved.", "Names can't contain "/".")
- [ ] Delete confirm with file / folder wording
- [ ] Refetch after every change (success or failure), on Refresh, on reconnect
- [ ] Empty folder: "This folder is empty. Drop files here to upload them."

### §9 Settings (`features/settings/`)
- [ ] Device name dialog: max 32 with counter, helper text, error in the field from the daemon, Save disabled while saving
- [ ] Download folder picker (starts at current, "Choose")
- [ ] Sync clipboard switch (from the clipboard plugin's slot)
- [ ] Keep running when the window is closed switch
- [ ] Version
- [ ] Loading; error + Retry; toast on save error

### §10 Background (`features/background/background_host.dart`, `core/desktop/`)
- [ ] Close hides when `closeToTray` (or settings unavailable), else quits
- [ ] Quit: stop the daemon, save placement, exit; guarded; also for OS-requested exits
- [ ] Tray: left click shows the window; right click menu
- [ ] Menu: Open MyConnect · per connected paired device "{name}" / "{name} · N%" with Send files…, Ping, Ring (listed if capable), Send clipboard (listed if supported), Browse files (listed if capable), Show details · "No paired devices" / "No devices connected" · Settings · Quit
- [ ] Tray Send files…: picker without showing the window; re-check device after pick; report via Notify
- [ ] Tray Ping / Ring / Send clipboard: no window; failures only
- [ ] Notify = toast when focused, desktop notification otherwise
- [ ] Notification: pairing request (unfocused only; withdrawn when resolved)
- [ ] Notification: file received (incoming, completed after startup, unfocused)
- [ ] Notification: ping received (from the ping plugin)
- [ ] Clicking a notification shows the window (where the platform allows; see step 10)
- [ ] Window placement: saved/restored per ADR 0009, fits-on-screen, maximized, start hidden
- [ ] Single instance: second launch shows the running window
- [ ] Tray icon assets (template image on macOS)

### §12 Platform
- [ ] Linux: installed as `myConnect`, with the CLI in the same package; `.desktop` file, icons, window class matching the desktop file
- [ ] macOS: app bundle, icon, tray template icon, Downloads access works without the sandbox
- [ ] Windows: single instance, notification identity, installer

## Appendix B: tests to port

Port the scenario, not the mechanics. Use `core::testing` or a real
`RunningService` in place of the fake HTTP daemon, and `iced_test::Simulator`
in place of the widget tester.

- **Controllers** (step 4):
  - devices: sorted snapshot, paired/unpaired split, events applied,
    forgotten removed, forget without waiting, events during a refetch
    survive;
  - pairings: requests that arrived before the UI started, resolution
    through events, accept applies the response, a late start response
    doesn't undo the outcome, requests during a refetch survive;
  - transfers: newest first + per-device filter, progress and outcome, a
    send result never overwrites a newer event;
  - settings: follows `settings.changed`, a patch sends only the changed
    field.
- **Pages** (steps 5–9, 11, 12):
  - home lists paired devices only; battery shown once reported;
  - the incoming prompt on any screen until resolved; a failed response
    keeps the prompt open;
  - unpair returns to the list;
  - send / ping / ring / clipboard gating;
  - transfers progress and cancel;
  - add by IP;
  - settings: rename shows the new name, a bad name shows why, switches
    save, the version is shown;
  - files: storage → folders → breadcrumb back, hidden toggle, open
    downloads, rename within the folder, slash refused locally, new
    folder, delete confirms with folder wording, failed change reported,
    drop into folder uploads, refusing device says why, browsing needs
    `sharesFiles`;
  - send: drop on device sends, drop elsewhere asks, drop on incapable
    device asks, folder refused, failures reported once.
- **Background** (step 13):
  - close hides / quits per setting;
  - Quit stops the daemon then exits;
  - notifications while hidden (pairing, file, ping) and a toast instead
    when focused;
  - tray lists connected paired devices, then Settings and Quit;
  - tray empty states;
  - tray opens a device or Settings;
  - Browse only for sharing devices;
  - clipboard only for capable devices;
  - tray ping/ring report failures only;
  - tray send to the picked device, and to a device that dropped during
    the pick;
  - window placement: nothing saved, round trip, unreadable file,
    fits-on-screen cases.
- **End to end** (step 14): the seven `integration_test/app_test.dart`
  scenarios, plus browsing against the fake phone.
