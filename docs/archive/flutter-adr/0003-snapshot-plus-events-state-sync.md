# 0003. Sync state as REST snapshots patched by one shared SSE stream

- Status: Accepted
- Date: 2026-09-24

## Context

The UI must reflect changes it did not cause: devices appearing, a peer
requesting pairing, a transfer progressing, the CLI unpairing a device. The
daemon exposes REST snapshots and a Server-Sent Events stream
(`GET /events`) whose events carry the changed resource's full snapshot.
Events are not durable: a client that disconnects misses whatever happened
meanwhile.

## Decision

- **One SSE connection per app**, owned by `daemonEventsProvider`
  (`DaemonEventHub`), shared by every feature. It reconnects with
  exponential backoff (250 ms → 5 s).
- Each connection first emits `EventStreamConnected` (as soon as the
  response headers arrive) and ends with `EventStreamDisconnected`.
- Each resource has a controller (`DevicesController`,
  `PairingsController`, ...) that
  1. subscribes to the hub, then fetches its REST snapshot;
  2. **upserts/removes by id** from events that carry its resource;
  3. **refetches the snapshot on every `EventStreamConnected`**, which
     repairs anything missed during a gap.
- Mutations apply the snapshot the API returns immediately rather than
  waiting for the echoing event.
- UI that reacts to server state is **derived declaratively** from these
  controllers. The incoming-pairing prompt is not a pushed dialog route
  but an overlay rendered while `pendingIncomingPairingsProvider` is
  non-empty, so it disappears by itself when the request expires or is
  resolved elsewhere.
- Event types the UI does not model decode to `UnhandledEvent`, and
  unknown enum values to `unknown`, so a newer daemon never breaks an older
  UI.

## Consequences

- Each new resource needs both a list endpoint and events; `GET /pairings`
  and `device.forgotten` were added to close gaps found this way.
- There is a small window where an event emitted just before a snapshot
  response is applied before that snapshot overwrites it; since snapshots
  are at least as new as the fetch, state converges. Snapshots carry no
  sequence number, so a stricter ordering would need an API change.
- Widgets must not assume they stay mounted across an `await` that changes
  state (an event can remove the resource they show); capture the router
  or messenger before awaiting.
