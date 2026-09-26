# 0002. Store the daemon's data in SQLite

- Status: Accepted
- Date: 2026-09-26

## Context

The daemon keeps three kinds of data, each in JSON files it writes
atomically: its identity (`identity.json`), the user's settings
(`settings.json`) and a record per paired device
(`trusted-devices/<id>.json`). That suits a handful of small values. The
features coming next need data that grows and is queried, and a plugin
can keep nothing but its settings section: each new kind of data would
need its own file format, atomic writes and parsing. Ferry isn't
released, so nothing on disk has to be carried over.

## Decision

- **One SQLite database, `ferry.db`, in the data directory**, owned by
  `src/store/`, through `rusqlite` with SQLite bundled.
- **A typed key/value table, `configs`**, keyed by a name and an optional
  secondary id (e.g. a device id). A `ConfigKey<T, S>` declared by its
  owner fixes the value's type and whether it takes an id; values are
  JSON. The core and plugins declare keys under their own namespace.
- **Configs are reactive.** Anyone can watch an entry and hear about each
  committed change.
- **Records that are lists get tables**: paired devices are the
  `devices` table.
- **No migration from the JSON files.** Devices paired before the change
  are paired again.
- `window.json` stays a file: it's the UI's, and the UI keeps nothing in
  the daemon's store (HANDOFF's ground rules).

[`../PLAN_STORE.md`](../PLAN_STORE.md) has the schema, the API and the
steps.

## Consequences

- A plugin can keep data by declaring a key, with no file handling.
- Settings, device records and the identity share one atomic write path;
  a `PATCH /settings` that changes several fields commits once.
- Store calls are synchronous (the TLS verifier needs them to be) and
  short, behind one lock. Anything bulky later runs on a blocking thread.
- SQLite is C, compiled by `cc` for each target. Every release job builds
  on a runner of its own architecture, so this needs nothing more than a
  C compiler, which the Windows and macOS builds already have.
- The HTTP API doesn't change.

## Libraries

| Concern | Choice | Why |
| --- | --- | --- |
| Database | `rusqlite` (feature `bundled`) | The standard SQLite binding: synchronous, which the trust checks in the TLS verifier need, and thin. `bundled` builds SQLite into the binary, so no platform needs a system library. `sqlx` is async and brings its own runtime integration and macros for no gain here. |
| Bytes in JSON values | `base64` 0.22 | Already in the tree. Keeps the identity's certificate and key readable in `sqlite3`, where serde would write a list of numbers. |
