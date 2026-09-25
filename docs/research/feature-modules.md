# Research: one module per feature

Status: proposal (2026-09-25), with phases 0 (ping, §7), 0b (find my
phone) and 1 (battery) implemented.
Once it is accepted, the target shape moves into `ARCHITECTURE.md` §2 and
this file becomes the history behind it.

Question: how should the daemon be split so that a feature (ping,
battery, clipboard, share, browsing) lives in one module that plugs into a
small core, instead of being spread over the central service?

## 1. Goal and non-goals

Set by the owner:

- **Goal: a codebase that scales.** Specifically:
  - agents working in parallel worktrees stop conflicting in the same
    central files;
  - adding a feature stops meaning edits to about eight places;
  - a feature can be read and tested on its own.
- **Non-goals:** loading plugins at runtime, a plugin marketplace, a stable
  plugin ABI, a crate per feature. It stays one crate, with boundaries kept
  by module visibility and review.
- **Scope:** the Rust daemon. `client.rs`, the CLI and the Flutter UI keep
  their shape for now, though they follow wire changes.
- **Wire contract:** free to change where that makes the design cleaner.
  The pilot didn't need any change; phase 1 moved `battery` under
  `plugins` (§5.5).

## 2. Where we are

A feature today is written in these places:

| Place | What goes there |
| --- | --- |
| `plugins/<name>.rs` | packet bodies and builders (this part is already per feature) |
| `plugins/mod.rs` | a `capabilities()` entry, an `IncomingPluginPacket` variant, a `dispatch_incoming` arm |
| `application/service.rs` | the logic, a `handle_peer_packet` arm, `ApplicationHandle` methods, `ApplicationService` trait methods and the forwarding impl |
| `application/state.rs` | its state inside the one `ApplicationState`, its snapshot types, `Query`/`QueryResult` variants |
| `application/events.rs` | `EventData` variants and their `event_type` arms |
| `ApplicationError` | its error variants |
| `api.rs` | routes, handlers, `map_error` arms, and path-sniffing in the body-limit middleware for streaming uploads |
| `device.rs` | fields it adds to `DeviceSnapshot` (battery) |
| `application/settings.rs`, `config/settings.rs` | its settings (`clipboardSyncEnabled`) |
| lifecycle code in `service.rs` | cleanup in `register_connection`, `unregister_connection`, `forget_device`, `handle_peer_unpair`, `RunningService::shutdown` |

The numbers bear this out: `service.rs` is 3,192 lines, a third of the
library, and each of the last three Rust features (#11 battery, #13
offline devices, #14 clipboard send) changed it by 84–172 lines.
`ApplicationService` has a single implementation. Even `tests/api.rs`
drives the real `ApplicationHandle`, so the trait isn't a seam, just one
more central list.

`plugins/mod.rs` and ARCHITECTURE §2 say the fixed table is deliberate,
chosen for the MVP ("not a plugin marketplace"). That reasoning still
holds against *dynamic* plugins. This proposal keeps the set fixed at
compile time and changes only where each feature's code lives.

## 3. The idea, and where I push back or go further

The proposal: expose core components, and have each feature module export
an Axum router plus a plugin-trait implementation that hooks into core
communication.

**Agree.** A `Plugin` trait with per-plugin routes is the right shape.
Capabilities and packet dispatch then follow from the list of plugins, not
from hand-kept tables.

**Further: routing is the easy part.** Most of the coupling isn't in
dispatch. The hard parts are what the trait has to answer:

1. **State and locking.** One `RwLock<ApplicationState>` holds every
   feature's state. Each plugin should own its state behind its own lock,
   with one rule: *core never calls into a plugin while holding its own
   lock*, and plugins never hold theirs while calling core.
2. **Events.** `EventData` is a closed enum that every feature edits.
   Core keeps typed variants for core resources, and plugins publish
   through one open `Plugin` variant with a typed encode/decode helper (§5.3).
3. **Errors.** `ApplicationError` and `map_error` are also closed. Core
   errors (`NotPaired`, `DeviceNotConnected`, `UnsupportedByPeer`, …) stay
   in core. A plugin's own errors map to the shared `ApiProblem` inside the
   plugin.
4. **Plugins that add to core resources.** Battery is a field on
   `DeviceSnapshot`, and `clipboardSyncEnabled` is a field in settings. A
   plugin needs a sanctioned way to add a field to a device and to own a
   settings section (§5.5, §5.6). Otherwise these leak back into core.
5. **Lifecycle.** Browsing sessions and transfers are torn down from four
   places in core, and clipboard sends a packet on connect. The trait needs
   `connected`, `disconnected`, `unpaired` and `shutdown` hooks.
6. **Shared services.** Share and browsing both create transfers. The
   transfer registry (state machine, progress throttling, tasks,
   `/transfers`) becomes a core service that both use, not something one
   plugin exports to another.

**Push back:**

- **Pairing stays in core,** as agreed. It gates every other packet and
  owns trust, so making it a plugin would mean plugins that can veto
  plugins.
- **No distributed registration** (`inventory`, `linkme`, build scripts).
  One line per plugin in a `builtin()` list is a trivial conflict and is
  easy to read. Link-time magic costs more than it saves.
- **No plugin-to-plugin dependencies.** If two features need the same
  thing, it goes into core (as transfers do). Otherwise the dependency
  graph between plugins becomes the new tangle.
- **Remove the `ApplicationService` trait, `Query`/`Command`/`QueryResult`
  and the forwarding impl** rather than modularizing them. They are central
  lists with one implementation. Core routes can take the core handle
  directly.
- **Don't expect zero central edits.** What remains is one `builtin()` line
  per plugin and, for now, `client.rs`, the CLI, the UI and ARCHITECTURE.md.
  The measure is that `service.rs`, `api.rs`, `events.rs`, `state.rs` and
  `ApplicationError` stop changing for feature work.

## 4. Options considered

| Option | Verdict |
| --- | --- |
| A. Keep the fixed table, split `service.rs` into per-feature `impl ApplicationHandle` files (as `service/browse.rs` does) | Cheapest, and it shrinks the file. But all shared lists (state, events, errors, routes, trait, dispatch) stay central, so conflicts move rather than go away. Rejected as the end state; it's roughly what phase 5 does for core-only code. |
| B. **`Plugin` trait with per-plugin state, routes and hooks, and a small core** | **Recommended.** Removes every central list except the registry line. |
| C. B plus a crate per feature | Declined by the owner. Can be done later without redesign, since B already makes plugins depend only on the core API. |
| D. Message-passing actors per plugin (each plugin a task with an inbox) | More isolation, but every call becomes async request/response and the current synchronous handlers would need rewriting. Unnecessary: per-plugin locks give the same isolation here. |

## 5. Target shape

### 5.1 Layout

```text
src/
  protocol/ transport/ config/     unchanged
  application/                     the core (rename to core/ in phase 5)
    plugin.rs                      Plugin trait, PluginContext, PluginRegistry, PluginEvent
    devices.rs connections.rs      registry and live connections (phase 5, out of service.rs)
    pairing.rs transfers.rs        core state machines (phase 5)
    events.rs settings.rs          core events and settings, open to plugins
    http.rs                        core routes (/status, /devices, /pairings, /transfers,
                                   /settings, /events) and ApiProblem
  plugins/
    mod.rs                         builtin(): the one list of plugins
    ping/     mod.rs  packet.rs  http.rs
    battery/  mod.rs  packet.rs
    clipboard/ mod.rs packet.rs  http.rs   (+ the clipboard backends, now crate::clipboard)
    share/    mod.rs  packet.rs  http.rs
    browse/   mod.rs  packet.rs  http.rs  session.rs   (today's sftp.rs + service/browse.rs + transport/sftp.rs)
  api.rs                           server shell: listener, auth, limits, SSE, merging routers
```

Dependencies point one way: `plugins/* → application (core API) →
protocol, transport, config`. Plugins never import each other. The core
reaches plugins only through `dyn Plugin`, except for the single
`plugins::builtin()` call at the composition root.

### 5.2 The trait

```rust
pub trait Plugin: Send + Sync + 'static {
    /// Stable identifier, e.g. "ping"; for logs and settings sections.
    fn id(&self) -> &'static str;
    /// Packet types this plugin receives and sends; the identity packet's
    /// capabilities are the union over all plugins.
    fn incoming(&self) -> &'static [&'static str];
    fn outgoing(&self) -> &'static [&'static str];

    /// A packet of one of `incoming()`'s types from a paired device.
    /// Pairing is checked by core before this is called.
    fn handle_packet(&self, ctx: &PluginContext, device: &DeviceSnapshot, packet: &Packet);

    /// HTTP routes, already given their state. Merged under /api/v1 with
    /// the standard body limit and request deadline.
    fn routes(self: Arc<Self>, ctx: PluginContext) -> Router { Router::new() }
    /// Streaming routes: own body limit, no overall deadline (uploads).
    fn streaming_routes(self: Arc<Self>, ctx: PluginContext) -> Router { Router::new() }

    // Lifecycle, all optional (phases 1–4 add them as plugins need them):
    fn connected(&self, ctx: &PluginContext, device: &DeviceSnapshot) {}
    fn disconnected(&self, ctx: &PluginContext, device_id: &str) {}
    fn unpaired(&self, ctx: &PluginContext, device_id: &str) {}
    fn device_state(&self, device_id: &str) -> Option<Value> { None }  // keyed by id()
    fn shutdown(&self) -> BoxFuture<'_, ()> { Box::pin(async {}) }
}
```

Handlers stay synchronous and spawn tasks when they need to, as they do
today, so the trait is dyn-compatible without `async_trait`. Core passes
the context into each call instead of plugins storing it. That way there is
no `Arc` cycle between the core handle and its plugins.

### 5.3 What core exposes: `PluginContext`

A cloneable handle over the core. It exposes only what features need:

- **Devices:** `device(id)`, and `device_changed(id)`, which republishes
  `device.updated` after a plugin's `device_state` changes.
- **Sending:** `send(device_id, packet)` checks that the device is paired
  and connected and that the peer advertised the packet type, then queues
  the packet (today's `capable_connection`). Also `broadcast(packet, except)`.
- **Events:** `publish(&T)` for any `T: PluginEventKind` (a
  `Serialize + DeserializeOwned` type with `const TYPE: &str`). On the bus
  that becomes `EventData::Plugin { type, data }`. The wire format is
  unchanged (`{"type": "ping.received", "data": {…}}`), and Rust
  consumers call `event.decode::<ReceivedPing>()`.
- **Transfers** (phase 3): `transfers().begin(device, direction, name,
  size)` returns a handle with `progress`, `complete`, `fail` and a
  cancellation token. The handle owns the state machine and the 100 ms
  progress throttle, and is cleaned up on drop.
- **Payload connections and TLS material** (phase 3): what share and
  browsing use to dial or accept pinned connections, without handing out
  the private key directly.
- **Settings** (phase 2): `settings::<T>(id)` and a watch of the plugin's
  section (§5.6).

### 5.4 HTTP

`api.rs` becomes the server shell. It merges the core routes and
`routes()` of every plugin, then applies the deadline and auth layers.
Separately it merges `streaming_routes()` with the large body limit.
This replaces `enforce_content_length`'s path-suffix check
(`/transfers`, `/files/upload`), which is the one place where a plugin's
URL is hard-coded into the shell. Overlapping routes panic when the
router is built, so a clash between plugins fails every test at once.

`ApiProblem` and `impl From<CoreError> for ApiProblem` become crate-public,
so plugin handlers can `?` core errors and build their own problem codes.

Paths are resources under the device they act on
(`/devices/{id}/ping`, `/devices/{id}/files/...`). Global plugin state has
its own top-level path (`/clipboard`). No `/plugins/<id>/` prefix: it
would change every URL to express something clients don't care about.

### 5.5 Adding to the device snapshot

`DeviceSnapshot` stops having a `battery` field. Core builds a
`plugins: {"battery": {...}}` map from each plugin's `device_state`,
keyed by plugin id; a plugin with nothing to add has no key. A plugin that
changes its state calls `ctx.device_changed(id)`. The UI keeps its
one-snapshot model (ADR 0003): still one list endpoint and one
`device.updated` event. Clearing on disconnect or unpair is the plugin's
job, via its hooks.

Decided in phase 1: a nested `plugins` map, not flattened into the
snapshot. Flattening would have kept `battery` where it was, but it puts
plugin keys in the same namespace as core fields, so a plugin id could
shadow a future core field, and a client deserializing with a catch-all
(`#[serde(flatten)]` into a map) can't tell an unknown core field from
plugin state. The nested map keeps the two apart, lets a client ignore
plugins it doesn't know as a whole, and is always present (`{}` when
empty), so clients need no null handling for it.

### 5.6 Settings sections

Core settings stay typed (`deviceName`, `downloadDir`, `closeToTray`).
Each plugin that has settings owns a typed section: it is stored under
its id in `settings.json`, has defaults and validation in the plugin,
and is exposed in `GET/PATCH /settings` under its id. A change still
publishes one `settings.changed`. `clipboardSyncEnabled` moves to
`clipboard.syncEnabled`. This is the one planned wire change, and the UI
and CLI change with it in phase 2.

### 5.7 Testing

- A plugin is unit-tested against a real core built by a small test kit
  (`application::testing`). The kit has an in-memory trust store, a fake
  paired connection whose outgoing packets are an `mpsc::Receiver`, and an
  event subscription. That is the `handle()` helper in `service.rs`'s
  tests, made shared.
- The e2e tests (`tests/*_e2e.rs`) stay as they are. They already test
  per feature.

## 6. Migration

Each phase is one PR that leaves every "done means" check green. The order
takes one new trait feature at a time.

| Phase | Moves | Proves |
| --- | --- | --- |
| 0 (done, #16) | ping | trait, registry, dispatch, capabilities, routes, open plugin events, `ApiProblem` for plugins |
| 0b (done) | find my phone (ring), added after the plan the old way | a send-only plugin: `incoming()` and `handle_packet()` got defaults |
| 1 (done) | battery | `device_state` + `device_changed`, `disconnected`/`unpaired` hooks, `DeviceSnapshot` extension (UI change) |
| 2 | clipboard | settings sections (UI + CLI change), `connected` hook, `broadcast`, plugin-owned global resource (`/clipboard`) |
| 3 | share | transfers extracted into a core service; `streaming_routes`; payload/TLS access through the context |
| 4 | browse (sftp) | plugin-owned sessions, `shutdown` hook; `service/browse.rs` and `transport/sftp.rs` move into the plugin |
| 5 | core cleanup | split what is left of `service.rs` into `devices`/`connections`/`pairing`; remove `ApplicationService`, `Query`, `Command`/`QueryResult` (the LAN command channel stays, as a core-internal type); rename `application` → `core`; move `RunningService` to a composition-root module; update ARCHITECTURE §2 and remove the "deliberately not a plugin system" notes |

Phases 1–4 are independent enough to run in parallel worktrees once
phase 0 lands. That is the goal in miniature.

## 7. Pilot: ping (phase 0)

Implemented on this branch:

- `application/plugin.rs`: `Plugin`, `PluginContext`, `PluginRegistry`,
  `PluginEvent`/`PluginEventKind`.
- `plugins/ping/`: `packet.rs` (the old `ping.rs`), `mod.rs` (the plugin,
  `send_ping`, `ReceivedPing`), `http.rs` (`POST /devices/{id}/ping`).
- Removed from core: the ping arm in `handle_peer_packet`,
  `send_ping` from `ApplicationHandle` and `ApplicationService`,
  `EventData::PingReceived`, `ReceivedPing` from `state.rs`, `post_ping`
  from `api.rs`.
- The other features still go through the fixed table. `plugins::
  capabilities()` merges both, so the identity packet doesn't change.
- Wire contract unchanged: same route, same `ping.received` JSON, same
  error codes. The UI needs no change.

Findings from the pilot are recorded in §8.

## 8. Findings from the pilot

All "done means" Rust checks pass (fmt, clippy `-D warnings`, 185 tests).
A daemon started with a token was probed live: the plugin's route returns
`401` without the token and `404 device_not_found` with it, so the route
sits behind auth and uses the shared problem format.

- **Ping now lives entirely in `plugins/ping/`,** including its unit
  tests. `service.rs` lost 150 lines net. The core files this touched
  (`api.rs`, `events.rs`, `state.rs`, `service.rs`) only lost ping code or
  gained code for all plugins; a second plugin won't touch them for the
  same reasons.
- **One `impl Plugin` per feature.** Routes are a trait method, so a
  plugin declares packet types, handler and routes in one place. Its
  `http.rs` is only where the handlers live.
- **Open events work without a wire change.** `#[serde(untagged)]` on the
  last `EventData` variant serializes plugin events exactly like core
  ones, and anything the core doesn't recognise deserializes as
  `EventData::Plugin`. Caveat: a *malformed* core event now also parses,
  as a `Plugin` event, instead of failing. Clients were already skipping
  events they couldn't use, so this seems acceptable. A custom
  `Deserialize` could close the gap if it matters.
- **Axum state composes cleanly.** Core routes call `.with_state()`
  before merging, so plugin routers (`Router<()>`, with their own state)
  merge in and share the deadline, body limit, auth and fallback layers.
- **The test kit was needed at once.** Ping's tests had used private
  helpers in `service.rs`, so `application::testing` (in-memory trust
  store, `handle()`, `make_identity`) was extracted in this phase rather
  than later. Every later plugin gets it for free.
- **Temporary seams to remove in phase 5:**
  - `ApplicationHandle::new` calls `plugins::builtin()`, an edge from
    core to features. It belongs at the composition root.
  - `plugins::capabilities()` builds a second registry just to read
    capability lists. It should come from the handle's registry.
  - `ApplicationService` gained `plugin_routes()`. That is one more sign
    the trait should go.
- **Rust callers change shape.** `handle.send_ping(..)` became
  `ping::send_ping(&handle.plugin_context(), ..)`. That's fine for tests
  and the CLI (which uses HTTP anyway), and it keeps feature functions out
  of the core's type.
- **Capability filtering is only tested through ping and ring** (as
  before). When battery or clipboard lands, move one of those tests to
  `plugin.rs` against a dummy plugin, so the core's `send` check has its
  own test.

Moving find my phone (#15), which landed the old way while the plan was
in review, showed:

- **It confirmed the problem.** #15 edited `service.rs`, `api.rs`, the
  `ApplicationService` trait and the capability table, and it conflicted
  with #16 in three of them. As a plugin it is two small files, and
  `service.rs` and `api.rs` only lost lines.
- **A send-only plugin needed defaults.** Ring handles no packets, so
  `incoming()` now defaults to none and `handle_packet()` to a no-op.
  A plugin that declares incoming types still has to override the
  handler; its tests would catch a missing one.
- **Capability order moved.** Plugins' capabilities come first, so ring's
  string moved in the identity packet. Order means nothing to peers, and
  the capabilities test now compares sorted lists so the next move won't
  break it.

Moving battery (phase 1) showed:

- **Pull, not push, for device state.** The plugin keeps each device's
  last report behind its own lock and answers `device_state(id)`; the
  core asks every plugin whenever a snapshot leaves it (the five device
  events, `GET /devices`, `GET /devices/{id}`), always after dropping its
  state lock. The registry in `device.rs` never holds plugin state, so it
  lost `set_battery` and the clearing in `mark_disconnected`/`set_paired`.
  A push model (`ctx.set_device_state`) would have put plugin data back
  into core state and made clearing core's job again.
- **`device_state` returns `Option<Value>`, keyed by `id()`,** instead of
  the planned `(&'static str, Value)`. A second key per plugin had no use
  and allowed two plugins to collide; the registry now also panics on a
  duplicate id, like it does on a duplicate packet type.
- **Hooks run before the core publishes.** `disconnected` runs before
  `device.disconnected` and `unpaired` before the `device.updated` (peer
  unpaired us) or `device.forgotten` (we forgot it). So the event already
  shows the battery gone, and a plugin clearing state in a hook needn't
  call `device_changed`. Forgetting a connected device calls both hooks.
  Without this ordering every disconnect would have published an extra
  `device.updated`.
- **Wire change, as §5.5 records.** `battery` moved to
  `plugins.battery`. The CLI and `client.rs` needed no code of their own:
  they share `DeviceSnapshot`, and read the battery through
  `BatteryStatus::of(&device)` from the plugin. The UI keeps a `battery`
  getter on `Device` that decodes `plugins['battery']`, and ignores a
  shape it doesn't know rather than failing to decode the whole device, so
  widgets didn't change.
- **Core files only lost battery code or gained shared code.** `service.rs`
  lost 44 lines net (the dispatch arm, `handle_battery`, its test) and
  gained `with_plugin_state` and the hook calls. `api.rs`, `events.rs`,
  `state.rs` and `ApplicationError` didn't change. `plugins/mod.rs` lost
  the battery arm of the fixed table and gained one `builtin()` line.
- **The first stateful plugin needs construction.** `builtin()` now holds
  `BatteryPlugin::default()`. `plugins::capabilities()` still builds a
  throwaway registry to read capability lists (a phase 5 seam), which now
  also allocates the plugin's empty map; harmless, but one more reason to
  read capabilities from the handle's registry.
- **The core's `send` check has its own test.** As §8 asked, the
  capability-filtering test moved from ping to `plugin.rs`, against a
  dummy send-only plugin: unknown device, not paired, not connected, not
  advertised, then accepted. Ping keeps a test that it sends its body.
- **Still to verify on a real phone,** as before the move: the checks
  here ran against the fake phone (unit tests, `browse_e2e`, and the app
  showing `Connected · 73%` and clearing it on disconnect).
