# M012 state_machine — state_machine.rs

> **File:** `crates/hle-executor/src/state_machine.rs` | **LOC:** ~380 | **Tests:** ~55
> **Role:** transition executor with verifier-visible events — executor-side FSM driver

---

## Types at a Glance

| Type | Kind | Copy | Hash | Const | Purpose |
|---|---|---|---|---|---|
| `StateMachine` | struct | No | No | No | Move-on-transition FSM; owns current `WorkflowState` and emits events |
| `ExecutorEvent` | enum | No | No | No | Verifier-visible event emitted on every state transition |
| `TransitionEffect` | struct | No | No | No | Output of a successful transition: new machine + event + authority token |
| `TransitionError` | enum | No | No | No | Output of a failed transition; wraps `AuthorityError` |
| `EventSequence` | newtype(`u64`) | Yes | Yes | Yes | Monotonically increasing event counter scoped to one workflow run |

---

## StateMachine

```rust
/// Executor-side finite state machine.
///
/// `StateMachine::step` consumes `self` and returns a new machine plus an
/// `ExecutorEvent`.  There is no `&mut self` API: mutation is entirely expressed
/// through ownership transfer.  This prevents partial-update bugs where the
/// machine and its emitted event disagree.
///
/// The machine holds a `ClaimAuthority<Provisional>` issued at construction.
/// It can never hold or produce a `ClaimAuthority<Final>` — that type is
/// `pub(crate)` inside `hle-verifier`.
#[derive(Debug)]
pub struct StateMachine {
    workflow_id: String,
    step_id: String,
    state: WorkflowState,
    sequence: EventSequence,
    authority: ClaimAuthority<Provisional>,
}
```

### Methods

| Method | Signature | Notes |
|---|---|---|
| `new` | `#[must_use] pub fn new(workflow_id: impl Into<String>, step_id: impl Into<String>, class: AuthorityClass) -> Self` | Constructs with `WorkflowState::Pending`, sequence 0, fresh `ClaimAuthority<Provisional>` |
| `state` | `#[must_use] pub fn state(&self) -> WorkflowState` | Copy return |
| `workflow_id` | `#[must_use] pub fn workflow_id(&self) -> &str` | |
| `step_id` | `#[must_use] pub fn step_id(&self) -> &str` | |
| `sequence` | `#[must_use] pub fn sequence(&self) -> EventSequence` | Current event sequence number |
| `step` | `pub fn step(self, event: ExecutorEvent) -> Result<TransitionEffect, TransitionError>` | **Core transition method — consumes self** |

### `step` contract

```rust
/// Drive the FSM by one event.
///
/// # Errors
///
/// Returns `TransitionError::Authority(AuthorityError::InvalidTransition { … })`
/// when the target state derived from `event` is not in the allowed set for
/// the current state (per `StatusTransitions::is_allowed`).
///
/// Returns `TransitionError::Authority(AuthorityError::TerminalState { … })`
/// when the machine is already in a terminal state and `step` is called again.
///
/// On success, the caller receives a `TransitionEffect` containing:
/// - the new `StateMachine` (current state updated)
/// - the `ExecutorEvent` that was applied (for emission to the verifier channel)
/// - the `ClaimAuthority<Provisional>` carried through the transition
pub fn step(
    self,
    event: ExecutorEvent,
) -> Result<TransitionEffect, TransitionError>
```

**Internal logic sketch** (no full implementation — spec only):

1. Check `self.state.is_terminal()` → return `TerminalState` error if true.
2. Derive target `WorkflowState` from the event variant.
3. Call `StatusTransitions::is_allowed(self.state, target)` → return `InvalidTransition` error if false.
4. Build new `EventSequence` = `self.sequence.next()`.
5. Construct `ExecutorEvent` with current sequence, actor, and state pair.
6. Return `TransitionEffect { machine: StateMachine { state: target, sequence: new_seq, … }, event, authority: self.authority }`.

---

## ExecutorEvent

```rust
/// A verifier-visible record of a single state transition attempted by the executor.
///
/// The verifier subscribes to an `ExecutorEvent` channel and uses these records
/// to reconstruct the execution history independently of what the executor claims
/// in its receipts.  A receipt claiming PASS without a matching `ExecutorEvent`
/// stream is rejected by `ClaimAuthorityVerifier` (M014).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutorEvent {
    /// Executor claimed the step and started running it.
    StepStarted {
        workflow_id: String,
        step_id: String,
        sequence: EventSequence,
    },
    /// Executor yielded because a human decision is required.
    HumanGateReached {
        workflow_id: String,
        step_id: String,
        sequence: EventSequence,
        reason: String,
    },
    /// Executor observed step completion; draft state is `Passed`.
    /// The verifier decides whether to accept this draft.
    DraftPassed {
        workflow_id: String,
        step_id: String,
        sequence: EventSequence,
        evidence_hash: String,
    },
    /// Executor observed step failure.
    DraftFailed {
        workflow_id: String,
        step_id: String,
        sequence: EventSequence,
        reason: String,
    },
    /// Executor is executing a compensating rollback for this step.
    RollbackStarted {
        workflow_id: String,
        step_id: String,
        sequence: EventSequence,
        reason: String,
    },
    /// Executor completed the rollback.
    RollbackCompleted {
        workflow_id: String,
        step_id: String,
        sequence: EventSequence,
    },
}
```

| Method | Signature | Notes |
|---|---|---|
| `workflow_id` | `#[must_use] pub fn workflow_id(&self) -> &str` | All variants carry this |
| `step_id` | `#[must_use] pub fn step_id(&self) -> &str` | All variants carry this |
| `sequence` | `#[must_use] pub fn sequence(&self) -> EventSequence` | Monotonic per-run counter |
| `target_state` | `#[must_use] pub fn target_state(&self) -> WorkflowState` | The `WorkflowState` this event drives the FSM toward |
| `is_terminal_event` | `#[must_use] pub fn is_terminal_event(&self) -> bool` | `true` for `DraftPassed`, `DraftFailed`, `RollbackCompleted` |

**Traits:** `Display`, `Clone`, `PartialEq`, `Eq`

---

## TransitionEffect

```rust
/// Successful output of `StateMachine::step`.
/// All three components are delivered atomically — callers cannot receive a
/// machine in a new state without also receiving the event that caused it.
#[derive(Debug)]
#[must_use]
pub struct TransitionEffect {
    /// The new machine after the transition.
    pub machine: StateMachine,
    /// The event that was applied (ready for emission to the verifier channel).
    pub event: ExecutorEvent,
    /// The provisional authority token carried through the transition.
    pub authority: ClaimAuthority<Provisional>,
}
```

---

## TransitionError

```rust
/// Failure output of `StateMachine::step`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionError {
    /// Transition not permitted by the static table (M013).
    Authority(AuthorityError),
    /// The FSM was already in a terminal state; no further transitions allowed.
    Terminal { state: WorkflowState },
}
```

**Traits:** `Display`, `std::error::Error`

---

## EventSequence

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct EventSequence(u64);
```

| Method | Signature |
|---|---|
| `new` | `#[must_use] pub const fn new(n: u64) -> Self` |
| `value` | `#[must_use] pub const fn value(self) -> u64` |
| `next` | `#[must_use] pub const fn next(self) -> Self` | Saturating add(1) |

**Traits:** `Display` (`"seq:42"`), `Default` (zero)

---

## Design Notes

- The `step(self, event) -> Result<TransitionEffect, TransitionError>` signature is the
  central axiom: the old machine is consumed on entry. If the transition fails, the caller
  receives the error and the original machine is gone — it must reconstruct or restart, not
  retry silently. This prevents the executor from issuing duplicate events.
- `TransitionEffect` is `#[must_use]`. A caller that calls `step` and ignores the result
  receives a compiler warning. This is the mechanism-level equivalent of the rule "don't
  silently discard authority tokens".
- `ExecutorEvent::DraftPassed` carries an `evidence_hash` field. The executor computes this
  from the artifacts it produced. M014 independently re-hashes the same artifacts and rejects
  any `DraftPassed` whose hash does not match — this is the M012/M014 anti-FP gate.
- `EventSequence` is scoped to one workflow run (not global). Verifiers that observe a gap
  in the sequence or a repeated value flag it as `AuthorityError::StaleEvent`.
- The machine does not expose a `set_state` or `force_state` method. The only path to a
  new state is through `step`, which validates against M013's transition table.
- M012 depends on M011 (`WorkflowState`) and M013 (`StatusTransitions`), both in the same
  `hle-executor` crate or imported from `hle-core`. M012 does not import anything from
  `hle-verifier` — the dependency arrow points only one way.

---

## Cluster Invariants

This module enforces C02 Invariant I3:

> **I3 (Event Emission on Every Transition):** Every call to `StateMachine::step` that
> succeeds produces exactly one `ExecutorEvent`.  The verifier subscribes to these events
> and uses them as the ground-truth execution log.  A receipt that is not backed by a
> matching event sequence is rejected.

See also: `../../ai_docs/use_patterns/UP_EXECUTOR_VERIFIER_SPLIT.md` (HLE-UP-001).

---

*M012 STATE_MACHINE Spec v1.0 | 2026-05-10*
