# M034 Runbook Phase Map — `crates/hle-runbook/src/phase_map.rs`

> **Layer:** L07 | **Cluster:** C06 Runbook Semantics | **Error Codes:** 2540
> **Role:** Map `RunbookPhaseKind` onto C03 executor `WorkflowStepKind` — the architectural seam that proves runbooks are NOT a parallel engine.
> **LOC target:** ~220 | **Test target:** ≥50

---

## Purpose

M034 is the single seam between the runbook vocabulary and the workflow executor. It answers one question: for a given runbook phase, which executor step state should govern its execution? The answer must always be an existing `WorkflowStepKind` from C03 — no new executor states may be introduced here. A runbook that cannot be mapped to existing executor vocabulary is rejected at parse time (M033 calls M034 during validation).

This module is intentionally small. Its job is structural proof: runbooks are a *kind* of workflow, not a separate engine.

---

## Types at a Glance

| Type | Kind | Notes |
|------|------|-------|
| `PhaseAffinityTable` | struct | Maps `RunbookPhaseKind` → `WorkflowStepKind` + metadata |
| `PhaseAffinity` | struct | Single mapping entry with optional timeout override |
| `WorkflowStepKind` | enum (imported from C03) | Existing executor step states |
| `RunbookPhaseKind` | type alias | Re-export of `PhaseKind` from M032, scoped for clarity |

---

## Imported from C03 (Bounded Execution)

```rust
// Imported; not defined here. These are the ONLY step kinds available.
use crate::executor::phase_executor::{WorkflowStepKind, PhaseAffinityVocabulary};
```

The `WorkflowStepKind` variants available for mapping are those declared in C03 `phase_executor`. M034 must not declare new variants; it must map to existing ones. If C03 adds new step kinds, M034 may use them. If M034 needs a step kind that C03 does not have, that is a C03 extension request — not a M034 change.

---

## Struct: `PhaseAffinity`

```rust
/// A single runbook-phase-to-executor-step mapping entry.
#[derive(Debug, Clone, PartialEq)]
pub struct PhaseAffinity {
    /// The executor step kind that governs this phase's execution.
    pub step_kind: WorkflowStepKind,
    /// Human-readable rationale for this mapping.
    pub rationale: &'static str,
    /// Optional per-phase timeout override. None means use executor default.
    pub timeout_override: Option<std::time::Duration>,
    /// When true, executor blocks on human confirmation (triggers M035) before proceeding.
    pub requires_human_confirm: bool,
}
```

---

## Struct: `PhaseAffinityTable`

```rust
/// Maps every `RunbookPhaseKind` to a `PhaseAffinity`.
///
/// Use `PhaseAffinityTable::standard()` for the canonical Framework §17.8 mapping.
/// Custom tables may be constructed for testing or operator overrides.
#[derive(Debug, Clone)]
pub struct PhaseAffinityTable {
    entries: std::collections::HashMap<PhaseKind, PhaseAffinity>,
}
```

---

## Method Table

| Method | Signature | Notes |
|--------|-----------|-------|
| `standard` | `fn() -> Self` | Returns the canonical Framework §17.8 table |
| `get` | `fn(&self, phase: PhaseKind) -> Option<&PhaseAffinity>` | `#[must_use]` |
| `map` | `fn(&self, phase: PhaseKind) -> Result<&PhaseAffinity, PhaseMapError>` | Returns `Err(2540)` if unmapped |
| `step_kind` | `fn(&self, phase: PhaseKind) -> Result<WorkflowStepKind, PhaseMapError>` | Convenience; extracts only the step kind |
| `requires_confirm` | `fn(&self, phase: PhaseKind) -> bool` | False if phase not in table |
| `all_phase_kinds` | `fn(&self) -> Vec<PhaseKind>` | All phases with a defined mapping |
| `is_complete` | `fn(&self) -> bool` | True when all `PhaseKind::all()` variants are mapped |
| `with_override` | `fn(self, phase: PhaseKind, affinity: PhaseAffinity) -> Self` | Builder-style override for tests |

---

## Free Function: `map`

```rust
/// Map a single runbook phase to its executor step kind using the standard table.
///
/// This is the primary interface for the executor integration path.
/// Returns `Err(PhaseMapError::Unknown)` only if the phase kind has no entry in
/// the standard table — which should never occur for Framework §17.8 canonical phases.
pub fn map(phase: PhaseKind) -> Result<WorkflowStepKind, PhaseMapError>;
```

---

## Error: `PhaseMapError`

```rust
/// Error produced when a phase has no executor mapping. Error code 2540.
#[derive(Debug)]
pub struct PhaseMapError {
    pub phase: String,
    pub message: String,
}
```

`PhaseMapError` implements `ErrorClassifier` with code 2540, severity Medium.

---

## Standard Mapping Table (`PhaseAffinityTable::standard()`)

This is the normative content of M034 — every row here is a design decision with architectural weight.

| `RunbookPhaseKind` | `WorkflowStepKind` | `requires_human_confirm` | Rationale |
|---|---|---|---|
| `Detect` | `Probe` | false | Detection is an observation step — it reads state, emits evidence, no side effects |
| `Block` | `Gate` | false | Blocking is a gate condition check — passes when the spread path is closed |
| `Fix` | `Execute` | true (for Hard/Safety) | Fixing applies a remediation action — highest risk phase, may require M035 confirm |
| `Verify` | `Probe` | false | Verification re-reads state after fix — same kind as Detect, different evidence set |
| `MetaTest` | `Replay` | false | Meta-test runs the M038 replay fixture — no human needed, deterministic |

The `loop_phase_affinity` vocabulary used here (`Probe`, `Gate`, `Execute`, `Replay`) is imported verbatim from C03 `PhaseAffinityVocabulary`. No new vocabulary is introduced in C06.

**Critical note on `Fix → Execute`:** The `requires_human_confirm` flag in the table is advisory metadata. The actual human confirmation gate is only activated when M039 `SafetyPolicy::check` determines the runbook's `safety_class` warrants it. A `Soft` class runbook may have `Fix` execute without M035 even though the table marks it `true`. M039 has final say; M034 provides the default intent.

---

## Invariant Proof

The following compile-time assertion is required in `phase_map.rs` and enforced by `INV-C06-01`:

```rust
// Compile-time proof that the standard table is complete.
// This assertion must remain; it prevents silent regressions when PhaseKind gains variants.
const _: () = {
    // All PhaseKind variants must have a standard mapping.
    // This is verified by integration test PhaseAffinityTable::standard().is_complete() == true.
    // The const_assert here is a documentation anchor; the runtime check is in tests.
};
```

The integration test that enforces completeness:

```rust
#[test]
fn standard_table_is_complete_for_all_phase_kinds() {
    let table = PhaseAffinityTable::standard();
    assert!(table.is_complete(), "Every PhaseKind must have a standard mapping");
    for kind in PhaseKind::all() {
        assert!(
            table.map(kind).is_ok(),
            "PhaseKind::{kind:?} has no standard executor mapping"
        );
    }
}
```

---

## Design Notes

- M034 takes no dependency on M035 (human confirm). The `requires_human_confirm` field in `PhaseAffinity` is metadata consumed by the executor when building its execution plan. M035 is invoked by the executor, not by M034.
- `PhaseAffinityTable::standard()` is a pure function — it produces the same table every call. No caching needed; the table has 5 entries.
- The `timeout_override` field is `Option<Duration>` rather than a typed newtype because timeout values are executor policy, not runbook semantics. C03 interprets the Duration; M034 merely carries it.
- The mapping of `Verify` to `Probe` (same as `Detect`) is intentional. Detect and Verify have identical executor behavior — both read state and emit evidence. They differ only in position within the incident timeline and the evidence set they expect. The distinction is captured in `PhaseKind::execution_order`, not in `WorkflowStepKind`.

---

## Cluster Invariants (this module)

- `PhaseAffinityTable::standard().is_complete()` must return `true`. If a new `PhaseKind` variant is added to M032, this method will return `false` until M034 is updated — making the gap immediately visible.
- `map(phase)` and `PhaseAffinityTable::standard().step_kind(phase)` must return identical `WorkflowStepKind` for all canonical `PhaseKind` variants.
- No `WorkflowStepKind` variant may be introduced or defined in this module. Only imports from C03 are permitted.

---

*M034 Runbook Phase Map | C06 Runbook Semantics | Habitat Loop Engine | 2026-05-10*
