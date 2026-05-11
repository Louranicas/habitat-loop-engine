# C06 Runbook Semantics — Cluster Overview

> **Layer:** L07 (Runbook Semantics) | **Modules:** M032-M039 | **Error Codes:** 2500-2599
> **Crate:** `crates/hle-runbook/` | **Source prefix:** `crates/hle-runbook/src/`

---

## Purpose

C06 provides the complete incident-response runbook layer for the Habitat Loop Engine. Runbooks are typed workflow definitions for incident-response scenarios — they are **not** a parallel execution engine. Every runbook phase maps directly onto executor step states defined in C03 (Bounded Execution). The cluster's central design obligation is to prove this seam: a runbook extends the workflow engine's authority, it does not bypass it.

---

## File Map

```
crates/hle-runbook/
├── src/
│   ├── lib.rs                  # Cluster root; re-exports all public items
│   ├── schema.rs               # M032 — typed Rust mirror of §17.8 TOML schema
│   ├── parser.rs               # M033 — TOML → Runbook with full validation
│   ├── phase_map.rs            # M034 — RunbookPhaseKind → WorkflowStepKind seam
│   ├── human_confirm.rs        # M035 — AwaitingHuman trait + impls
│   ├── manual_evidence.rs      # M036 — operator evidence attachment model
│   ├── scaffold.rs             # M037 — incident runbook skeleton generator
│   ├── incident_replay.rs      # M038 — deterministic 8-fixture replay suite
│   └── safety_policy.rs        # M039 — safety gate and policy enforcement
└── tests/
    └── integration.rs          # Cross-module invariant tests (≥50 per module boundary)
```

---

## Dependency Graph (Internal)

```
schema.rs          → (leaf — no internal deps; defines all vocabulary types)
parser.rs          → schema.rs (Runbook, Phase, SafetyClass, ModeApplicability)
phase_map.rs       → schema.rs (RunbookPhaseKind), [C03] WorkflowStepKind
human_confirm.rs   → schema.rs (Runbook, Phase), [C02] ExecutionContext
manual_evidence.rs → schema.rs (EvidenceLocator), [C01] ReceiptHash
scaffold.rs        → schema.rs (Runbook, Phase, PhaseKind), [C02] WorkflowState
incident_replay.rs → schema.rs, parser.rs, phase_map.rs, manual_evidence.rs
safety_policy.rs   → schema.rs (Runbook, SafetyClass), [C02] ExecutionContext
```

---

## Cross-Cluster Dependencies

| This Module | Depends On | From Cluster | What It Needs |
|---|---|---|---|
| M034 `phase_map` | `WorkflowStepKind` | C03 Bounded Execution | Step-kind vocabulary for phase mapping |
| M034 `phase_map` | `PhaseAffinityTable` | C03 Bounded Execution | `loop_phase_affinity` vocabulary |
| M035 `human_confirm` | `ExecutionContext` | C02 Authority State | Context for authority elevation checks |
| M035 `human_confirm` | `WorkflowState` | C02 Authority State | State guard before eliciting confirmation |
| M036 `manual_evidence` | `ReceiptHash` | C01 Evidence Integrity | SHA-256 binding for operator evidence |
| M036 `manual_evidence` | `EvidenceStore` (trait) | C05 Persistence Ledger | Storage path for attached evidence |
| M037 `scaffold` | `WorkflowState` | C02 Authority State | Initial state for generated runbooks |
| M038 `incident_replay` | `FalsePassAuditor` (trait) | C04 Anti-Pattern Intelligence | Replay fixtures consumed for false-pass detection |
| M039 `safety_policy` | `ExecutionContext` | C02 Authority State | Elevation token and context for safety checks |
| M039 `safety_policy` | `ClaimAuthority` | C02 Authority State | Authority guard for hard/safety operations |

---

## Design Principles

1. **Runbook is a kind of workflow, not a parallel engine.** M034 is the architectural proof — every `RunbookPhaseKind` maps to an existing `WorkflowStepKind` with no new executor states introduced.

2. **AwaitingHuman is a first-class state, never an error.** M035 produces a typed confirmation token; the executor decides whether to proceed. The confirm module elicits — it does not authorize.

3. **Safety gates are the last line of defence.** M039 rejects any runbook attempting operations above its declared `safety_class` before a single phase executes.

4. **Evidence is hash-bound from the moment of attachment.** M036 computes SHA-256 at `ManualEvidence` construction time; the evidence is immutable thereafter.

5. **Replay fixtures are the specification for incident correctness.** M038's 8 fixtures document the system's expected behavior under known failure modes, making them regression tests and documentation simultaneously.

6. **Scaffolds are pure functions.** M037 produces deterministic TOML text from a `ScaffoldInput`; it performs no I/O. Callers persist the output.

7. **Schema is the single source of truth.** All other C06 modules operate on the typed `Runbook` struct from M032. The TOML on disk is a serialization format; the struct is canonical.

---

## Error Strategy (2500-2599)

| Code | Variant | Severity | Module | Condition |
|------|---------|----------|--------|-----------|
| 2500 | `RunbookParse` | Medium | M033 | TOML parse failure or schema mismatch |
| 2510 | `RunbookValidation` | Medium | M033 | Required field missing or constraint violated |
| 2520 | `RunbookCircularPhase` | High | M033 | Circular dependency detected in phase graph |
| 2530 | `RunbookMaxTraversalsExceeded` | High | M033 | `max_traversals` limit reached at parse time |
| 2540 | `PhaseMapUnknown` | Medium | M034 | `RunbookPhaseKind` has no executor mapping |
| 2550 | `HumanConfirmTimeout` | Medium | M035 | Confirmation token not received within deadline |
| 2560 | `HumanConfirmRefused` | High | M035 | Operator explicitly declined confirmation |
| 2570 | `EvidenceHashMismatch` | High | M036 | Recomputed SHA-256 does not match stored hash |
| 2580 | `ReplayFixtureMismatch` | High | M038 | Actual trace diverged from expected fixture trace |
| 2590 | `SafetyPolicyViolation` | Critical | M039 | Runbook operation exceeds declared safety class |
| 2591 | `SafetyElevationDenied` | Critical | M039 | Elevation requested but not granted by authority |
| 2599 | `RunbookOther` | Low | All | Residual / unclassified runbook errors |

All error variants implement `ErrorClassifier` from the foundation layer (`crates/hle-core` or `substrate-types`). Severity maps to tensor dimension D10 (error_rate) in the 12D tensor contribution.

---

## Quality Gate Template

```bash
# Run from workspace root
cargo check -p hle-runbook 2>&1 | tail -20
cargo clippy -p hle-runbook -- -D warnings 2>&1 | tail -20
cargo clippy -p hle-runbook -- -D warnings -W clippy::pedantic 2>&1 | tail -20
cargo test -p hle-runbook --lib --release 2>&1 | tail -30
# Zero-tolerance checks
rg --type rust 'unwrap\(\)|expect\(|unsafe\s*\{|panic!\(' crates/hle-runbook/src/
```

Acceptance criteria per module:
- Zero `unsafe`, `unwrap`, `expect`, `panic!` in `src/`
- Zero clippy warnings at pedantic level
- 50+ tests per module boundary
- Every public item documented with `///`
- `#[must_use]` on all pure functions and builder methods

---

## Cluster Invariants (enforced in `tests/integration.rs`)

1. **INV-C06-01** — Every `RunbookPhaseKind` variant has a defined mapping in `PhaseAffinityTable::standard()`. `phase_map::map` must not return `Err(PhaseMapUnknown)` for any canonical phase.

2. **INV-C06-02** — `RunbookHumanConfirm` impls may not write to the verifier receipt store. Confirmation tokens are passed to the executor; the executor decides what to record.

3. **INV-C06-03** — `ManualEvidence` SHA-256 is computed at construction; no mutation method exists. `evidence.sha256` always equals `sha256(evidence.content_or_path)` at the time of attachment.

4. **INV-C06-04** — `SafetyPolicy::check` returns `Ok(())` only when `runbook.safety_class` is `Soft` or when explicit elevation is present in `ExecutionContext` for `Hard` or `Safety` class runbooks.

5. **INV-C06-05** — `RunbookScaffold::scaffold` is a pure function: same `ScaffoldInput` always produces the same TOML output string. No randomness, no I/O, no side effects.

6. **INV-C06-06** — All 8 replay fixtures in M038 produce deterministic traces that pass `IncidentReplay::verify_trace`. No fixture may have an empty `expected_trace`.

7. **INV-C06-07** — A runbook with `idempotent: false` and `max_traversals: 1` must be rejected by M039 safety policy when `context.traversal_count >= 1`.

---

*C06 Runbook Semantics Cluster Overview | Habitat Loop Engine | 2026-05-10*
