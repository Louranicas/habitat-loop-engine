# M011 workflow_state — workflow_state.rs

> **File:** `crates/hle-core/src/state/workflow_state.rs` | **LOC:** ~230 | **Tests:** ~42
> **Role:** workflow state enum and invariants — supersedes `substrate_types::StepState` for the planned topology

---

## Types at a Glance

| Type | Kind | Copy | Hash | Const | Purpose |
|---|---|---|---|---|---|
| `WorkflowState` | enum (`#[repr(u8)]`) | Yes | Yes | Yes | Authoritative FSM state for a workflow step |
| `WorkflowStateSet` | newtype(`u8` bitmask) | Yes | Yes | Yes | Compact set of `WorkflowState` values for transition table membership |
| `StateTransitionKind` | enum | Yes | Yes | Yes | Classifies a transition as forward / rollback / human-gate |

---

## WorkflowState

```rust
/// Authoritative workflow step state.
///
/// Supersedes `substrate_types::StepState` for the planned full-codebase
/// topology.  `StepState` remains the substrate wire type; `WorkflowState`
/// adds richer predicate methods, a bitmask companion, and compile-time
/// const expressions used by the transition table (M013).
///
/// # Conversion
/// `From<WorkflowState> for substrate_types::StepState` is provided so that
/// receipts and verifier results can use the existing substrate wire format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum WorkflowState {
    /// Step is queued but not started.  No executor has claimed it.
    Pending = 0,
    /// Executor has claimed the step and is actively running it.
    Running = 1,
    /// Step is blocked waiting for a human decision before proceeding.
    AwaitingHuman = 2,
    /// Step completed successfully.  Verifier has issued PASS receipt.
    Passed = 3,
    /// Step completed with a failure.  Verifier has issued FAIL receipt.
    Failed = 4,
    /// Step was reversed as part of a compensating rollback sequence.
    RolledBack = 5,
}
```

### Predicate Methods

| Method | Signature | Returns `true` when |
|---|---|---|
| `is_terminal` | `#[must_use] pub const fn is_terminal(self) -> bool` | `Passed`, `Failed`, or `RolledBack` |
| `is_blocking_human` | `#[must_use] pub const fn is_blocking_human(self) -> bool` | `AwaitingHuman` |
| `is_active` | `#[must_use] pub const fn is_active(self) -> bool` | `Running` |
| `is_pending` | `#[must_use] pub const fn is_pending(self) -> bool` | `Pending` |
| `is_passed` | `#[must_use] pub const fn is_passed(self) -> bool` | `Passed` |
| `is_failed` | `#[must_use] pub const fn is_failed(self) -> bool` | `Failed` |
| `is_rolled_back` | `#[must_use] pub const fn is_rolled_back(self) -> bool` | `RolledBack` |
| `can_transition_to` | `#[must_use] pub const fn can_transition_to(self, target: Self) -> bool` | Consults the embedded static predicate (identical to M013 table but available without importing M013) |
| `requires_verifier_authority` | `#[must_use] pub const fn requires_verifier_authority(self) -> bool` | `Passed` — only reachable via verifier PASS receipt |

### Conversion Methods

| Method | Signature | Notes |
|---|---|---|
| `as_str` | `#[must_use] pub const fn as_str(self) -> &'static str` | Wire string (matches `StepState` wire values) |
| `bit` | `#[must_use] pub const fn bit(self) -> u8` | `1 << (self as u8)` — for `WorkflowStateSet` membership |

### Constants

```rust
pub const TERMINAL_STATES: WorkflowStateSet =
    WorkflowStateSet::empty()
        .with(WorkflowState::Passed)
        .with(WorkflowState::Failed)
        .with(WorkflowState::RolledBack);

pub const BLOCKING_STATES: WorkflowStateSet =
    WorkflowStateSet::empty().with(WorkflowState::AwaitingHuman);

pub const ACTIVE_STATES: WorkflowStateSet =
    WorkflowStateSet::empty().with(WorkflowState::Running);
```

### Trait Implementations

| Trait | Notes |
|---|---|
| `Display` | Delegates to `as_str()` — e.g., `"awaiting-human"` |
| `FromStr` | Returns `AuthorityError::Other` on unknown string |
| `From<WorkflowState> for substrate_types::StepState` | Wire-format bridge for receipt construction |
| `From<substrate_types::StepState> for WorkflowState` | Round-trip for deserialization from substrate layer |

---

## WorkflowStateSet

```rust
/// Compact bitmask representing a subset of `WorkflowState` values.
/// At most 6 bits are used (one per variant).  All operations are `const fn`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct WorkflowStateSet(u8);
```

| Method | Signature | Notes |
|---|---|---|
| `empty` | `#[must_use] pub const fn empty() -> Self` | All bits clear |
| `with` | `#[must_use] pub const fn with(self, state: WorkflowState) -> Self` | Sets corresponding bit; chainable |
| `contains` | `#[must_use] pub const fn contains(self, state: WorkflowState) -> bool` | Bit test |
| `is_empty` | `#[must_use] pub const fn is_empty(self) -> bool` | |
| `union` | `#[must_use] pub const fn union(self, other: Self) -> Self` | Bitwise OR |
| `intersection` | `#[must_use] pub const fn intersection(self, other: Self) -> Self` | Bitwise AND |

**Traits:** `Display` (`"WorkflowStateSet{Pending, Running}"`)

---

## StateTransitionKind

```rust
/// Classifies how a state transition should be interpreted by the verifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum StateTransitionKind {
    /// Normal forward progress along the happy path.
    Forward = 0,
    /// Compensating action reversing a prior step.
    Rollback = 1,
    /// Step entered a human-gate; no automated progress until human unblocks.
    HumanGate = 2,
}
```

| Method | Signature |
|---|---|
| `as_str` | `#[must_use] pub const fn as_str(self) -> &'static str` |
| `is_rollback` | `#[must_use] pub const fn is_rollback(self) -> bool` |

**Traits:** `Display`

---

## Design Notes

- `WorkflowState` supersedes `substrate_types::StepState` for the planned topology, but does
  **not** replace it in existing substrate-* crates. The `From` conversions provide a clean
  bridge without breaking existing compilation.
- `#[repr(u8)]` on `WorkflowState` enables the `bit()` method and the `WorkflowStateSet`
  bitmask to use zero-cost arithmetic rather than allocations.
- All 9 predicates are `const fn`. The transition table in M013 computes its `const` array
  entries using these predicates at compile time, guaranteeing the table cannot drift from
  the enum definition.
- `can_transition_to` embeds the same logic as M013's `is_allowed` as a convenience
  predicate on the enum itself. This redundancy is intentional: M011 is in `hle-core` (L01)
  and must be usable without importing `hle-executor` (L03).
- `requires_verifier_authority` makes the `Passed` state self-documenting: code that reads
  the predicate at a call site cannot miss that a `Passed` verdict requires external authority.
- `WorkflowStateSet` uses a u8 bitmask rather than a `HashSet` or slice so that set
  membership can be tested in `const` contexts (e.g., the static transition table).

---

## Cluster Invariants

This module enforces C02 Invariant I2:

> **I2 (Verifier-Gated Passed):** `WorkflowState::Passed` is reachable only via a verifier-issued
> PASS receipt. `requires_verifier_authority()` returning `true` for this variant is the
> machine-readable contract that `ClaimAuthorityVerifier` (M014) checks before issuing any
> `ClaimAuthority<Final>`.

See also: `../../ai_docs/use_patterns/UP_EXECUTOR_VERIFIER_SPLIT.md` (HLE-UP-001).

---

*M011 WORKFLOW_STATE Spec v1.0 | 2026-05-10*
