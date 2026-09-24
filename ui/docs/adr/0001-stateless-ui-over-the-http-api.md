# 0001. The UI is a stateless client of the daemon's HTTP API

- Status: Accepted; window placement excepted by [0009](0009-remember-the-main-window-placement.md)
- Date: 2026-09-24

## Context

MyConnect's behaviour — discovery, TLS, pairing and trust, transfers,
clipboard — lives in the Rust daemon, and the CLI already drives it only
through the local `/api/v1` HTTP API. A GUI could instead link against the
Rust library and call it directly, or keep its own copy of devices and
settings. Either would create a second source of truth and a second
integration surface to keep in sync with the CLI.

## Decision

The Flutter app holds **no persisted state of its own** and reads and
writes **only through the HTTP API**:

- No `shared_preferences`, database, or files written by the UI. Anything
  worth remembering belongs in the daemon and gets an API.
- In-memory state is a cache of API snapshots (see [0003](0003-snapshot-plus-events-state-sync.md)),
  discarded on restart.
- Native code is used only to *start and stop* a daemon
  (see [0002](0002-embed-the-daemon-through-a-json-c-abi.md)), never to
  read or change application state.
- When the UI needs data the API does not expose, the API grows — e.g.
  `GET /pairings` and the `device.forgotten` event were added for this UI.

## Consequences

- The CLI and UI can drive the same daemon side by side and always agree.
- The UI can target an external daemon (`myconnect run`) with no code
  changes, which is also how it is developed and tested.
- Every UI feature needs a matching API endpoint first; UI work sometimes
  starts with a Rust change.
- UI preferences (window size, theme override) have nowhere to live yet;
  if needed they become daemon settings exposed over the API.
