# Plan: the daemon's data in SQLite

The daemon keeps its data in JSON files: `identity.json`,
`settings.json` and one `trusted-devices/<id>.json` per paired device.
Features coming next need data that grows and is queried, which files
handle badly, and plugins have nowhere to keep anything but a settings
section. This plan moves the daemon's data into one SQLite database with
a typed, reactive key/value table any part of the daemon can use,
plugins included. The decision is
[`adr/0002`](adr/0002-store-the-daemons-data-in-sqlite.md).

Ferry isn't released, so nothing is migrated: the old JSON files are
ignored, and a device paired before the change has to be paired again.

## Shape

`src/store/` owns `ferry.db` in the data directory:

| File | Holds |
| --- | --- |
| `mod.rs` | `Store`: `open`, `open_in_memory`, the schema, the connection lock, transactions |
| `config.rs` | `ConfigKey<T, S>`, scopes, get/set/remove, watching |
| `devices.rs` | The `devices` table (replaces `FilesystemTrustStore`) |

`config` keeps generating the identity and the API token; its
`settings.rs` and `trust.rs` go.

## Schema

```sql
CREATE TABLE configs (
  key        TEXT NOT NULL,             -- 'core.deviceName', 'clipboard.settings'
  scope      TEXT NOT NULL DEFAULT '',  -- '' for global keys, 'device', ...
  id         TEXT NOT NULL DEFAULT '',  -- the secondary id, e.g. a device id
  value      TEXT NOT NULL,             -- JSON
  updated_at INTEGER NOT NULL,          -- Unix milliseconds
  PRIMARY KEY (key, scope, id)
) WITHOUT ROWID;

CREATE TABLE devices (
  device_id             TEXT PRIMARY KEY,
  certificate_der       BLOB NOT NULL,
  protocol_version      INTEGER NOT NULL,
  name                  TEXT,           -- last authenticated identity; NULL until seen
  device_type           TEXT,
  incoming_capabilities TEXT,           -- JSON arrays
  outgoing_capabilities TEXT,
  paired_at             INTEGER NOT NULL,
  updated_at            INTEGER NOT NULL
);
```

- A global key's scope and id are `''`, not `NULL`: SQLite treats NULLs
  in a primary key as distinct, which would allow duplicates.
- Values are JSON text: readable with `sqlite3`, and a type can gain
  `#[serde(default)]` fields without a schema change.
- The connection runs in WAL mode with a busy timeout, so a CLI daemon and
  the app on the same data directory wait for each other instead of
  failing.
- `PRAGMA user_version` is `1`. Opening a new database (`0`) creates the
  schema; any other version is an error that says to delete `ferry.db`.
  Migrations come once there's a release to migrate from.

## Typed configs

```rust
pub struct ConfigKey<T, S: Scope = Global> { name: &'static str, .. }

pub const DEVICE_NAME: ConfigKey<String> = ConfigKey::new("core.deviceName");
pub const MUTED: ConfigKey<bool, PerDevice> = ConfigKey::new("notifications.muted");

store.get(&DEVICE_NAME)?;             // Result<Option<String>>
store.set(&DEVICE_NAME, &name)?;
store.remove(&MUTED.of(device_id))?;
store.get(&MUTED)?;                   // doesn't compile: needs .of(id)
store.transaction(|tx| { tx.set(&A, &a)?; tx.set(&B, &b) })?;
```

- **The key carries the type.** `T` is fixed where the key is declared,
  and the scope `S` makes the secondary id required (`.of(id)`) or
  impossible. `get`, `set` and `remove` take any `Entry`: a global key or
  a scoped one.
- **Keys are namespaced by owner**: `core.*`, `ui.*`, `<plugin id>.*`.
  A key two features share is declared in the core, as plugins don't
  import each other.
- **A stored value that no longer decodes** as `T` reads as `None`, with a
  warning, like an invalid settings section today.
- **Defaults stay with the owner**: `get` returns an `Option`, since some
  defaults (the host name, the download directory) are only known at run
  time.
- **Unpairing** removes the device's row and every `PerDevice` entry for it
  (`remove_scope`) in one transaction.

## Watching

```rust
let mut watch = store.watch(&clipboard::SETTINGS);  // ConfigWatch<T>
watch.get();             // the value now
watch.changed().await;   // the stored value changed

store.changes();         // broadcast of ConfigChange { key, scope, id }, untyped
```

- One `tokio::sync::watch` channel per entry, made when it's first
  watched and dropped once nobody holds it. A watch always holds the
  latest value, so a watcher has its snapshot and can't lag: the
  snapshot-plus-events rule holds by construction.
- Watchers hear about a change after its commit, and only if its JSON
  changed. A rolled-back transaction tells nobody.
- The store's lock is a leaf: nothing else is locked while it's held.
  Watchers are told before it's released, so they hear about commits in
  order; telling them only wakes their tasks and runs none of their code.
- Config changes don't reach `/events` on their own. The owner of a key
  decides what's user-facing; `settings.changed` stays `Settings`'s.

## What moves where

| Today | After |
| --- | --- |
| `identity.json` | The key `core.identity` (the certificate and key as base64); `LocalIdentity::load_or_create(&Store)` |
| `trusted-devices/*.json`, `FilesystemTrustStore` | The `devices` table: `Store::device`, `devices`, `put_device` and `remove_device`. The `TrustStore` trait goes: the core and the transport take the `Store`, and tests use `Store::open_in_memory()` instead of `MemoryTrustStore` |
| `settings.json`, `SettingsFile`, `StoredSettings` | `core.deviceName`, `core.downloadDir`, `ui.closeToTray`, and one key per plugin section, `<id>.settings`, holding the fields the user set. `PATCH` merging, `null` resets and `PluginSettings` validation are unchanged; a `PATCH` is one transaction |
| No plugin storage | `PluginContext::store()`: plugins declare their own `ConfigKey`s |
| `window.json` | Unchanged: the UI's one piece of state stays the UI's |

The HTTP API doesn't change, so neither do the CLI and the UI.

## Steps

1. ADR 0002 and `rusqlite` (`bundled`).
2. `Store`: `open`, `open_in_memory`, the schema and its version check,
   in a data directory made with `create_private_dir`.
3. `ConfigKey`, `Entry`, scopes, get/set/remove, transactions, `watch` and
   `changes`, with tests: values round-trip; a value that doesn't decode
   reads as `None`; a write of the same value tells no one; a rolled-back
   transaction tells no one; a commit of several keys tells each watcher
   once; `remove_scope` clears a device's entries.
4. The `devices` table, wired into `daemon.rs`, the core, `lan.rs` and
   the integration tests; `trust.rs`, the `TrustStore` trait and
   `MemoryTrustStore` go.
5. The identity as `core.identity`; `identity.json` handling goes. It's
   read with `get_strict`, so a stored identity that doesn't decode is an
   error rather than replaced (a new identity is a new device ID, and
   every pairing lost), and made in an `IMMEDIATE` transaction, so two
   processes starting on a new data directory can't both make one.
6. Settings on configs, and `PluginContext::store()`; `config/settings.rs`
   goes.
7. Docs: ARCHITECTURE §2, §5, §7 and §11, HANDOFF's ground rules (plugins
   keep data only under their own keys, or in a table the core defines),
   the ADR index.
8. HANDOFF's "done" checks, and the real app on loopback paired with a CLI
   peer: restart both, and the pairing and a renamed device survive.

## Later

- Tables owned by plugins, for data that grows (transfer or notification
  history): a `Plugin::schema()` run at open, its tables prefixed with the
  plugin's id. `configs` is for small values, not logs.
- Remembering devices added by IP (ARCHITECTURE §11): an address column on
  `devices`, or a `PerDevice` key.
