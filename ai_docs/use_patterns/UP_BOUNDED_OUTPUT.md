# UP_BOUNDED_OUTPUT

Status: scaffold use-pattern contract. Runtime log enforcement is deferred until `begin M0`; scaffold scripts and docs already enforce bounded-output expectations.

Predicate ID: `HLE-UP-003`

## Intent

Workflow loops must never create unbounded logs, runaway buffers, or infinite background output streams. Human review should receive concise evidence packets with durable paths for deeper inspection.

## Scaffold-time rules

- Verification scripts print bounded summaries and fail with actionable messages.
- Temporary files under `.deployment-work/logs/` are excluded from canonical manifest when explicitly marked as transient.
- Long-running monitors must have a hard timeout and must be read-only unless separately authorized.
- Kanban monitor loops must report state changes, not stream repetitive full board dumps forever.

## Future runtime requirements

1. Every workflow run declares maximum log bytes per phase.
2. Every spawned command declares a timeout and output retention policy.
3. Verifier inputs are hash-addressed artifacts, not terminal scrollback.
4. Human-readable summaries point to bounded files and exact hashes.
5. Exceeding an output cap produces BLOCKED, not truncated PASS.

## Negative control

A fixture that emits an unbounded stream or writes an uncapped log must be rejected by future runtime verification. Scaffold-only negative controls may model this with a static fixture and expected failure signature.

## Review checklist

- Is there a hard cap, timeout, or retention policy?
- Does a summary cite the durable artifact rather than relying on scrollback?
- Does truncation prevent PASS?
- Are logs separated from canonical manifests unless intentionally preserved?
