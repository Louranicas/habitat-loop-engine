# M013 status_transitions — status_transitions.rs

> **File:** `crates/hle-executor/src/status_transitions.rs` | **LOC:** ~210 | **Tests:** ~38
> **Role:** static transition table and rollback affordances — guards every `StateMachine::step` call

---

## Types at a Glance

| Type | Kind | Copy | Hash | Const | Purpose |
|---|---|---|---|---|---|
| `TransitionRule` | struct | Yes | Yes | Yes | One allowed `(from, to)` pair in the static table |
| `RollbackRule` | struct | Yes | Yes | Yes | One `(from, target)` rollback mapping |
| `StatusTransitions` | unit struct | Yes | No | Yes | Namespace for the `const` table; all methods are `const fn` |

---

## Static Transition Table

```rust
/// One allowed state transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransitionRule {
    pub from: WorkflowState,
    pub to: WorkflowState,
}

impl TransitionRule {
    #[must_use]
    pub const fn new(from: WorkflowState, to: WorkflowState) -> Self {
        Self { from, to }
    }
}
```

```rust
/// The complete set of allowed forward transitions.
/// Any `(from, to)` pair NOT in this array is rejected by `is_allowed`.
pub const ALLOWED_TRANSITIONS: [TransitionRule; 9] = [
    // Pending can start (claimed by executor)
    TransitionRule::new(WorkflowState::Pending,       WorkflowState::Running),
    // Running can reach any terminal outcome
    TransitionRule::new(WorkflowState::Running,       WorkflowState::Passed),
    TransitionRule::new(WorkflowState::Running,       WorkflowState::Failed),
    TransitionRule::new(WorkflowState::Running,       WorkflowState::AwaitingHuman),
    TransitionRule::new(WorkflowState::Running,       WorkflowState::RolledBack),
    // AwaitingHuman can resume running (human unblocked) or fail outright
    TransitionRule::new(WorkflowState::AwaitingHuman, WorkflowState::Running),
    TransitionRule::new(WorkflowState::AwaitingHuman, WorkflowState::Failed),
    // Failed can be rolled back as a compensating action
    TransitionRule::new(WorkflowState::Failed,        WorkflowState::RolledBack),
    // Pending can be abandoned before it ever runs
    TransitionRule::new(WorkflowState::Pending,       WorkflowState::Failed),
];
```

**Rationale for each rule:**

| Rule | Rationale |
|---|---|
| `Pending → Running` | Normal claim: executor picks up the step |
| `Running → Passed` | Happy path; verifier must confirm via receipt |
| `Running → Failed` | Execution error observed |
| `Running → AwaitingHuman` | Step needs human decision to proceed |
| `Running → RolledBack` | Mid-run abort triggers immediate rollback |
| `AwaitingHuman → Running` | Human unblocked the gate; executor resumes |
| `AwaitingHuman → Failed` | Human rejected or timed out |
| `Failed → RolledBack` | Post-failure compensating rollback |
| `Pending → Failed` | Pre-execution cancellation / precondition failure |

---

## StatusTransitions

```rust
/// Namespace for static transition table operations.
/// All methods are `const fn` and operate on the compile-time `ALLOWED_TRANSITIONS` array.
pub struct StatusTransitions;
```

### Methods

```rust
impl StatusTransitions {
    /// Check whether a transition from `from` to `to` is in the allowed table.
    ///
    /// Iterates the 9-element `ALLOWED_TRANSITIONS` array — O(9), not a hash lookup.
    /// Suitable for `const` contexts; zero heap allocation.
    ///
    /// ```
    /// assert!(StatusTransitions::is_allowed(
    ///     WorkflowState::Pending,
    ///     WorkflowState::Running,
    /// ));
    /// assert!(!StatusTransitions::is_allowed(
    ///     WorkflowState::Passed,
    ///     WorkflowState::Running,
    /// ));
    /// ```
    #[must_use]
    pub const fn is_allowed(from: WorkflowState, to: WorkflowState) -> bool {
        // const-compatible linear scan of the static array
        let mut i = 0;
        while i < ALLOWED_TRANSITIONS.len() {
            let rule = ALLOWED_TRANSITIONS[i];
            if rule.from as u8 == from as u8 && rule.to as u8 == to as u8 {
                return true;
            }
            i += 1;
        }
        false
    }

    /// Return the rollback target for a given state, if one exists.
    ///
    /// Returns `Some(WorkflowState::RolledBack)` for states that have a defined
    /// rollback transition in `ALLOWED_TRANSITIONS`.
    /// Returns `None` for terminal states and for `Pending` (nothing to roll back).
    ///
    /// ```
    /// assert_eq!(
    ///     StatusTransitions::rollback_target(WorkflowState::Failed),
    ///     Some(WorkflowState::RolledBack),
    /// );
    /// assert_eq!(
    ///     StatusTransitions::rollback_target(WorkflowState::Passed),
    ///     None,
    /// );
    /// ```
    #[must_use]
    pub const fn rollback_target(from: WorkflowState) -> Option<WorkflowState> {
        match from {
            WorkflowState::Running | WorkflowState::Failed => {
                Some(WorkflowState::RolledBack)
            }
            WorkflowState::Pending
            | WorkflowState::AwaitingHuman
            | WorkflowState::Passed
            | WorkflowState::RolledBack => None,
        }
    }

    /// Return all states that `from` can legally transition to.
    ///
    /// The return type is a fixed-size array padded with `None`.
    /// Maximum fanout from any single state is 4 (Running has 4 successors).
    #[must_use]
    pub const fn successors(from: WorkflowState) -> [Option<WorkflowState>; 4] {
        let mut result = [None; 4];
        let mut count = 0usize;
        let mut i = 0;
        while i < ALLOWED_TRANSITIONS.len() {
            let rule = ALLOWED_TRANSITIONS[i];
            if rule.from as u8 == from as u8 && count < 4 {
                result[count] = Some(rule.to);
                count += 1;
            }
            i += 1;
        }
        result
    }

    /// Return all states that can legally transition INTO `to`.
    #[must_use]
    pub const fn predecessors(to: WorkflowState) -> [Option<WorkflowState>; 4] {
        let mut result = [None; 4];
        let mut count = 0usize;
        let mut i = 0;
        while i < ALLOWED_TRANSITIONS.len() {
            let rule = ALLOWED_TRANSITIONS[i];
            if rule.to as u8 == to as u8 && count < 4 {
                result[count] = Some(rule.from);
                count += 1;
            }
            i += 1;
        }
        result
    }
}
```

---

## RollbackRule

```rust
/// A mapping from a state that can be rolled back to its rollback target.
/// Used by the rollback orchestration layer (C06 runbook semantics).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RollbackRule {
    pub from: WorkflowState,
    pub target: WorkflowState,
}
```

```rust
/// All defined rollback transitions (a subset of `ALLOWED_TRANSITIONS`).
pub const ROLLBACK_RULES: [RollbackRule; 2] = [
    RollbackRule { from: WorkflowState::Running, target: WorkflowState::RolledBack },
    RollbackRule { from: WorkflowState::Failed,  target: WorkflowState::RolledBack },
];
```

---

## Design Notes

- The transition table is a `const` array of 9 elements. The array is exhaustive and
  authoritative: adding or removing a transition requires a source-level edit, not a
  runtime configuration change. This eliminates entire classes of configuration drift.
- `is_allowed` uses a `while` loop (not `for`) to satisfy `const fn` restrictions in
  stable Rust. The function body cannot call any non-const iterator methods.
- `successors` and `predecessors` return `[Option<WorkflowState>; 4]` rather than
  `Vec<WorkflowState>` to remain `const fn` with zero heap allocation.
- `rollback_target` returns `Option` rather than using a sentinel value so that callers
  can pattern-match exhaustively on rollback availability at a given state.
- The table does not include `Passed → anything` or `RolledBack → anything` because both
  are terminal states. `is_terminal` on M011 is the guard; the table is consistent with it.
- `ROLLBACK_RULES` is a separate array from `ALLOWED_TRANSITIONS` to make rollback
  orchestration code in C06 (runbook semantics) independent of parsing the full table.

---

## Cluster Invariants

This module enforces C02 Invariant I4:

> **I4 (Static Table Authority):** The set of valid `(from, to)` transitions is closed at
> compile time in `ALLOWED_TRANSITIONS`.  No runtime configuration, environment variable,
> or feature flag can add a transition.  `StateMachine::step` calls `is_allowed` before
> any state change; a `false` return causes `TransitionError::Authority`.

See also: `../../ai_docs/use_patterns/UP_EXECUTOR_VERIFIER_SPLIT.md` (HLE-UP-001).

---

*M013 STATUS_TRANSITIONS Spec v1.0 | 2026-05-10*
