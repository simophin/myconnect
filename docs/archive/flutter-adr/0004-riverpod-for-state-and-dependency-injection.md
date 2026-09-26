# 0004. Riverpod 3 (without codegen) for state and dependency injection

- Status: Accepted
- Date: 2026-09-24

## Context

We need dependency injection (swap the daemon host and API in tests),
async state with loading/error handling, and derived state (paired vs.
unpaired devices, pending pairings). Candidates: Riverpod, Bloc, Provider,
plain `ChangeNotifier`s.

## Decision

Use `flutter_riverpod` 3:

- `AsyncNotifier`s own each resource cache ([0003](0003-snapshot-plus-events-state-sync.md));
  plain `Provider`s derive views from them.
- The object graph is a chain of providers — `daemonHostProvider →
  daemonEndpointProvider → apiProvider → daemonEventsProvider` — so tests
  override one link (`TestDaemon.overrides`) and everything above it runs
  for real.
- **No `riverpod_generator`.** The provider set is small, and hand-written
  declarations avoid a second codegen pipeline beside Freezed.
- **Automatic retry is disabled** (`ProviderScope(retry: ...)`). Failures
  surface immediately with an explicit Retry button instead of silently
  re-running daemon start-up.

## Consequences

- Riverpod's family provider types are not public API, so the
  `specify_nonobvious_property_types` lint is disabled.
- Tests use `ProviderContainer.test` for controllers and `ProviderScope`
  overrides for widgets, with `mocktail` for the API.
