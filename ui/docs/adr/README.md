# Architecture decision records — Flutter UI

Each record captures one decision: the context that forced it, what was
decided, and what follows from it. Records are immutable once accepted; a
later decision that changes course supersedes an earlier one rather than
editing it.

| # | Decision | Status |
| --- | --- | --- |
| [0001](0001-stateless-ui-over-the-http-api.md) | The UI is a stateless client of the daemon's HTTP API | Accepted |
| [0002](0002-embed-the-daemon-through-a-json-c-abi.md) | Embed the daemon through a minimal JSON-over-C ABI | Accepted |
| [0003](0003-snapshot-plus-events-state-sync.md) | Sync state as REST snapshots patched by one shared SSE stream | Accepted |
| [0004](0004-riverpod-for-state-and-dependency-injection.md) | Riverpod 3 (without codegen) for state and dependency injection | Accepted |
| [0005](0005-libraries-and-code-conventions.md) | Library choices and code conventions | Accepted |
| [0006](0006-build-the-rust-core-from-the-platform-build.md) | Build and bundle the Rust core from the platform build | Accepted (Linux) |

Template for new records:

```markdown
# NNNN. Title in the imperative

- Status: Proposed | Accepted | Superseded by NNNN
- Date: YYYY-MM-DD

## Context
## Decision
## Consequences
```
