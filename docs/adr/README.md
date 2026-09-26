# Architecture decision records

Each record captures one decision: the context that forced it, what was
decided, and what follows from it. Records are immutable once accepted; a
later decision that changes course supersedes an earlier one rather than
editing it.

The deleted Flutter app's records are in
[`../archive/flutter-adr/`](../archive/flutter-adr/README.md); 0001 below
says which of them still apply.

| # | Decision | Status |
| --- | --- | --- |
| [0001](0001-native-ui-in-iced.md) | Build the desktop UI in Rust with iced, in the daemon's process | Accepted |

Template for new records:

```markdown
# NNNN. Title in the imperative

- Status: Proposed | Accepted | Superseded by NNNN
- Date: YYYY-MM-DD

## Context
## Decision
## Consequences
```
