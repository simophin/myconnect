# Research: one module per feature

Status: proposal (2026-09-25), with phase 0 (ping, §7) implemented.
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
  The pilot didn't need any change.

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
    fn device_state(&self, device_id: &str) -> Option<(&'static str, Value)> { None }
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
`plugins: {"battery": {...}}` map (or flattens it, a wire detail to decide
in phase 1) from each plugin's `device_state`. A plugin that changes the
map calls `ctx.device_changed(id)`. The UI keeps its one-snapshot model
(ADR 0003): still one list endpoint and one `device.updated` event.
Clearing on disconnect or unpair is the plugin's job, via its hooks.

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
| 0 (this branch) | ping | trait, registry, dispatch, capabilities, routes, open plugin events, `ApiProblem` for plugins |
| 1 | battery | `device_state` + `device_changed`, `disconnected`/`unpaired` hooks, `DeviceSnapshot` extension (UI change) |
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
- **Capability filtering is only tested through ping** (as before). When
  battery or clipboard lands, move one of those tests to `plugin.rs`
  against a dummy plugin, so the core's `send` check has its own test.
