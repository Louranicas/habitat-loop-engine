# C03 Bounded Execution — Cluster Overview

> **Cluster:** C03_BOUNDED_EXECUTION | **Layer:** L03 | **Modules:** 5 (M015-M019)
> **Error Code Range:** 2200-2299 | **Source Crate:** `crates/hle-executor/`
> **Synergy:** local runner + phase executor + timeout/retry policies make every runtime path finite and verifier-visible

---

## Purpose

C03 enforces the HLE contract that **every command execution is bounded in time, output size, and attempt count**, and produces verifier-visible evidence for each step. No execution path in L03 may proceed without a declared timeout, an output cap, and a deterministic stop condition.

This cluster is the planned topology successor to `substrate-emit::execute_local_workflow` and `run_local_command_with_timeout`. The four satellite modules (M016-M019) all depend on M015 primitives and compose through `PhaseExecutor` (M017), which is the sole entry point for phase-aware step execution.

---

## File Map

```
crates/hle-executor/src/
├── bounded.rs          # M015 — BoundedString, BoundedDuration, BoundedMemory primitives
├── local_runner.rs     # M016 — one-shot command runner (allowlist/blocklist/redaction)
├── phase_executor.rs   # M017 — phase-aware step sequencer (8 framework phases)
├── timeout_policy.rs   # M018 — TERM-then-KILL escalation policy
└── retry_policy.rs     # M019 — explicit bounded retry with BackoffStrategy
```

---

## Dependency Graph (Internal)

```
bounded.rs (M015)
    └──> local_runner.rs (M016)   [uses BoundedString for stdout/stderr caps]
              └──> phase_executor.rs (M017)  [spawns LocalRunner per step]
                        └──> timeout_policy.rs (M018)  [M017 holds TimeoutPolicy]
                        └──> retry_policy.rs (M019)    [M017 holds RetryPolicy]
```

All five modules live in the same crate. The internal dependency graph is acyclic. M018 and M019 are pure value types consumed by M017; they do not call each other.

---

## Cross-Cluster Dependencies

| Dep Direction | From | To | Reason |
|---|---|---|---|
| C03 → C01 | M016, M017 | `receipt_hash` (M001 planned), `receipts_store` (M003 planned) | M016 emits a `Receipt` per step; M017 appends to ledger via `append_jsonl_receipt` |
| C03 → C02 | M017 | `workflow_state` (M007 planned), `state_machine` (M008 planned) | M017 reads `StepState` transitions; stop-on-first-fail matches C02 authority model |
| C03 → C05 | M017 | `evidence_store` (M025 planned), `verifier_results_store` (M026 planned) | Phase receipts are persisted by C05 after each step |
| C06 → C03 | `runbook_phase_map` (M030 planned) | M017 | Runbook phases are mapped to the same 8 `ExecutionPhase` variants consumed by M017 |
| C08 → C03 | `cli_run` (M043 planned) | M017 | CLI `hle run` invokes `PhaseExecutor::run_phases` |

---

## Concurrency Architecture

| Strategy | Where | Rationale |
|---|---|---|
| Synchronous (thread::sleep polling) | M016 `LocalRunner`, M018 `TimeoutPolicy::apply` | C03 executes local M0 commands; AP29 (blocking in async) forbids async without explicit boundary isolation. All execution is foreground, one-shot. |
| No shared state | All M015-M019 | Primitives are value types or structs with owned fields; no `Arc`, `Mutex`, or `RwLock` needed |
| Process group isolation | M016, M018 | `Command::process_group(0)` ensures TERM/KILL reach the entire child tree, not just the direct child |
| Thread-sleep poll loop | M016 | `child.try_wait()` polled at 10ms intervals; never blocks the caller thread beyond `TimeoutPolicy::graceful` + `hard_kill` |

---

## Design Principles

1. **Every bound is declared at the call site, not discovered at runtime.** `LocalRunner::new` takes a `TimeoutPolicy` and `max_output_bytes`; callers cannot forget to set them.
2. **Truncation produces evidence, not silence.** `BoundedString::truncate` appends `...[truncated]` so verifier inputs always reflect the cap.
3. **UTF-8 safety is non-negotiable.** `BoundedString` truncates at `char_boundary`, never mid-codepoint.
4. **Allowlist wins over blocklist.** M016 maintains a positive allowlist of permitted binaries. The blocklist is a secondary safety net, not the primary gate.
5. **TERM before KILL.** M018 always sends SIGTERM first, waits `hard_kill` duration, then sends SIGKILL to the process group. Aggressive KILL without grace is a last resort.
6. **No infinite retry.** M019 `max_attempts` is a `NonZeroU32`; callers cannot construct `RetryPolicy` with zero or unbounded attempts.
7. **Verifier is the sole PASS authority.** M017 emits draft `StepState` values; the verifier (C01/C04) promotes or rejects them. M017 never self-certifies.
8. **Stop on first block.** M017 halts the phase sequence on the first `AwaitingHuman` or `Failed` step; remaining steps are not executed.

---

## Error Strategy (Codes 2200-2299)

| Code | Variant | Source | Retryable | Notes |
|---|---|---|---|---|
| 2200 | `BoundCapExceeded { field, cap_bytes }` | M015 | No | Output or duration exceeded declared bound |
| 2201 | `InvalidBound { reason }` | M015 | No | Zero cap or inverted duration range |
| 2210 | `CommandRejected { reason }` | M016 | No | Blocklist, metachar, URL, or allowlist miss |
| 2211 | `SpawnFailed { program, reason }` | M016 | Conditional | OS spawn failure; retryable if EAGAIN |
| 2212 | `OutputReadFailed { reason }` | M016 | No | wait_with_output failed after child exit |
| 2213 | `SecretRedacted` | M016 | No | Warning-level; output was redacted, not an error |
| 2220 | `PhaseSequenceEmpty` | M017 | No | No steps provided to executor |
| 2221 | `StepFailed { phase, step_id, message }` | M017 | No | Step returned Failed; sequence halted |
| 2222 | `AwaitingHuman { phase, step_id }` | M017 | No | Sequence halted pending human confirmation |
| 2223 | `LedgerWriteFailed { reason }` | M017 | Conditional | Receipt append to JSONL ledger failed |
| 2230 | `TimeoutElapsed { graceful_ms, hard_kill_ms }` | M018 | No | Escalation completed; child killed |
| 2231 | `InvalidTimeoutPolicy { reason }` | M018 | No | graceful is zero or hard_kill exceeds graceful |
| 2240 | `RetryExhausted { attempts, last_error }` | M019 | No | max_attempts reached without success |
| 2241 | `InvalidRetryPolicy { reason }` | M019 | No | max_attempts is zero or backoff is negative |

All variants implement `std::fmt::Display`. Error codes are embedded in the `Display` output as `[HLE-2NNN]` for grep-ability in verifier logs.

---

## Quality Gate Template

```bash
# Run from workspace root
cargo check --package hle-executor 2>&1 | tail -20
cargo clippy --package hle-executor -- -D warnings 2>&1 | tail -20
cargo clippy --package hle-executor -- -D warnings -W clippy::pedantic 2>&1 | tail -20
cargo test --package hle-executor --lib 2>&1 | tail -30

# Zero-tolerance checks
grep -rn 'unwrap\(\)\|expect\(\)\|panic!\|unsafe' crates/hle-executor/src/ && echo FAIL || echo PASS
grep -rn 'process::exit\|std::process::abort' crates/hle-executor/src/ && echo FAIL || echo PASS

# Bounded-output contract
scripts/verify-bounded-logs.sh
```

Minimum test targets: 50 tests per module (250 total for C03).

---

*C03 Bounded Execution Cluster Overview | HLE spec v1.0 | 2026-05-10*
