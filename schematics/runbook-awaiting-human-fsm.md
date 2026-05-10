# runbook-awaiting-human-fsm

```text
[scaffold_verified]
        |
        v
[m0_waiting] -- missing begin M0 --> [blocked_on_input]
        |
        | explicit begin M0 in future
        v
[ready_for_review] -- verifier fail --> [blocked_on_verifier]
        |                               |
        | human accepts evidence         v
        v                         [waiver_requested]
[authorized_next_step] <--- explicit waiver or corrected evidence
```

## State semantics

- `m0_waiting` is the current terminal scaffold state.
- `blocked_on_input` means the missing human decision is known.
- `blocked_on_verifier` means evidence exists but fails a gate.
- `waiver_requested` requires an explicit human phrase or durable waiver receipt.
- `authorized_next_step` is future-only and does not exist in this scaffold without M0 authorization.
