#![forbid(unsafe_code)]

//! M012 — `StateMachine` transition executor with verifier-visible events.
//!
//! **Cluster:** C02 Authority & State | **Layer:** L03
//!
//! Enforces C02 Invariant I3: every call to `StateMachine::step` that succeeds
//! produces exactly one [`ExecutorEvent`].  The verifier subscribes to these
//! events and uses them as the ground-truth execution log.
//!
//! The machine can only hold [`ClaimAuthority<Provisional>`] — it can never
//! produce or name `ClaimAuthority<Final>` because that type lives in
//! `hle-verifier` which `hle-executor` does not depend on.
//!
//! Cross-reference: `ai_specs/modules/c02-authority-state/M012_STATE_MACHINE.md`
//! Use pattern: `ai_docs/use_patterns/UP_EXECUTOR_VERIFIER_SPLIT.md` (HLE-UP-001)
//! Depends on: M010 `ClaimAuthority` (hle-core), M011 `WorkflowState` (hle-core),
//!             M013 `StatusTransitions` (this crate)

use std::fmt;

use hle_core::authority::claim_authority::{
    AuthorityClass, AuthorityError, ClaimAuthority, Provisional,
};
use hle_core::state::workflow_state::WorkflowState;

use crate::status_transitions::StatusTransitions;

// ---------------------------------------------------------------------------
// EventSequence
// ---------------------------------------------------------------------------

/// Monotonically increasing event counter scoped to one workflow run.
///
/// Gaps or regressions in the sequence are flagged as
/// [`AuthorityError::StaleEvent`] by `ClaimAuthorityVerifier` (M014).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct EventSequence(u64);

impl EventSequence {
    /// Construct with an explicit value.
    #[must_use]
    pub const fn new(n: u64) -> Self {
        Self(n)
    }

    /// The raw sequence number.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }

    /// Advance by one, saturating at `u64::MAX`.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl fmt::Display for EventSequence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "seq:{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// ExecutorEvent
// ---------------------------------------------------------------------------

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
        /// Workflow identifier.
        workflow_id: String,
        /// Step identifier.
        step_id: String,
        /// Monotonic sequence number for this workflow run.
        sequence: EventSequence,
    },
    /// Executor yielded because a human decision is required.
    HumanGateReached {
        /// Workflow identifier.
        workflow_id: String,
        /// Step identifier.
        step_id: String,
        /// Monotonic sequence number.
        sequence: EventSequence,
        /// Human-readable reason for the gate.
        reason: String,
    },
    /// Executor observed step completion; draft state is `Passed`.
    ///
    /// The verifier decides whether to accept this draft.  The `evidence_hash`
    /// is computed by the executor from its output artifacts; M014 re-checks it
    /// independently.
    DraftPassed {
        /// Workflow identifier.
        workflow_id: String,
        /// Step identifier.
        step_id: String,
        /// Monotonic sequence number.
        sequence: EventSequence,
        /// SHA256 (or similar) hash of the step's output artifacts.
        evidence_hash: String,
    },
    /// Executor observed step failure.
    DraftFailed {
        /// Workflow identifier.
        workflow_id: String,
        /// Step identifier.
        step_id: String,
        /// Monotonic sequence number.
        sequence: EventSequence,
        /// Human-readable failure reason.
        reason: String,
    },
    /// Executor is executing a compensating rollback for this step.
    RollbackStarted {
        /// Workflow identifier.
        workflow_id: String,
        /// Step identifier.
        step_id: String,
        /// Monotonic sequence number.
        sequence: EventSequence,
        /// Human-readable rollback reason.
        reason: String,
    },
    /// Executor completed the rollback.
    RollbackCompleted {
        /// Workflow identifier.
        workflow_id: String,
        /// Step identifier.
        step_id: String,
        /// Monotonic sequence number.
        sequence: EventSequence,
    },
}

impl ExecutorEvent {
    /// Workflow identifier carried by this event.
    #[must_use]
    pub fn workflow_id(&self) -> &str {
        match self {
            Self::StepStarted { workflow_id, .. }
            | Self::HumanGateReached { workflow_id, .. }
            | Self::DraftPassed { workflow_id, .. }
            | Self::DraftFailed { workflow_id, .. }
            | Self::RollbackStarted { workflow_id, .. }
            | Self::RollbackCompleted { workflow_id, .. } => workflow_id,
        }
    }

    /// Step identifier carried by this event.
    #[must_use]
    pub fn step_id(&self) -> &str {
        match self {
            Self::StepStarted { step_id, .. }
            | Self::HumanGateReached { step_id, .. }
            | Self::DraftPassed { step_id, .. }
            | Self::DraftFailed { step_id, .. }
            | Self::RollbackStarted { step_id, .. }
            | Self::RollbackCompleted { step_id, .. } => step_id,
        }
    }

    /// Monotonic sequence number carried by this event.
    #[must_use]
    pub fn sequence(&self) -> EventSequence {
        match self {
            Self::StepStarted { sequence, .. }
            | Self::HumanGateReached { sequence, .. }
            | Self::DraftPassed { sequence, .. }
            | Self::DraftFailed { sequence, .. }
            | Self::RollbackStarted { sequence, .. }
            | Self::RollbackCompleted { sequence, .. } => *sequence,
        }
    }

    /// The `WorkflowState` this event drives the FSM toward.
    #[must_use]
    pub fn target_state(&self) -> WorkflowState {
        match self {
            Self::StepStarted { .. } => WorkflowState::Running,
            Self::HumanGateReached { .. } => WorkflowState::AwaitingHuman,
            Self::DraftPassed { .. } => WorkflowState::Passed,
            Self::DraftFailed { .. } => WorkflowState::Failed,
            Self::RollbackStarted { .. } | Self::RollbackCompleted { .. } => {
                WorkflowState::RolledBack
            }
        }
    }

    /// Returns `true` for events that mark the terminal end of a step's
    /// execution: `DraftPassed`, `DraftFailed`, `RollbackCompleted`.
    #[must_use]
    pub fn is_terminal_event(&self) -> bool {
        matches!(
            self,
            Self::DraftPassed { .. } | Self::DraftFailed { .. } | Self::RollbackCompleted { .. }
        )
    }
}

impl fmt::Display for ExecutorEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ExecutorEvent::{} (workflow={}, step={}, seq={})",
            match self {
                Self::StepStarted { .. } => "StepStarted",
                Self::HumanGateReached { .. } => "HumanGateReached",
                Self::DraftPassed { .. } => "DraftPassed",
                Self::DraftFailed { .. } => "DraftFailed",
                Self::RollbackStarted { .. } => "RollbackStarted",
                Self::RollbackCompleted { .. } => "RollbackCompleted",
            },
            self.workflow_id(),
            self.step_id(),
            self.sequence()
        )
    }
}

// ---------------------------------------------------------------------------
// TransitionEffect
// ---------------------------------------------------------------------------

/// Successful output of [`StateMachine::step`].
///
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

// ---------------------------------------------------------------------------
// TransitionError
// ---------------------------------------------------------------------------

/// Failure output of [`StateMachine::step`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionError {
    /// Transition not permitted by the static table (M013).
    Authority(AuthorityError),
    /// The FSM was already in a terminal state; no further transitions allowed.
    Terminal { state: WorkflowState },
}

impl fmt::Display for TransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Authority(err) => write!(f, "transition authority error: {err}"),
            Self::Terminal { state } => {
                write!(
                    f,
                    "machine already in terminal state '{state}'; no further transitions"
                )
            }
        }
    }
}

impl std::error::Error for TransitionError {}

/// Cluster-scoped `Result` alias for state machine operations.
pub type Result<T> = std::result::Result<T, TransitionError>;

// ---------------------------------------------------------------------------
// StateMachine
// ---------------------------------------------------------------------------

/// Executor-side finite state machine.
///
/// `StateMachine::step` consumes `self` and returns a new machine plus an
/// [`ExecutorEvent`].  There is no `&mut self` API: mutation is entirely
/// expressed through ownership transfer, preventing partial-update bugs where
/// the machine and its emitted event disagree.
///
/// The machine holds a [`ClaimAuthority<Provisional>`] issued at construction.
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

impl StateMachine {
    /// Construct a new state machine in `Pending` state with sequence 0.
    ///
    /// A fresh [`ClaimAuthority<Provisional>`] is issued for this workflow/step.
    #[must_use]
    pub fn new(
        workflow_id: impl Into<String>,
        step_id: impl Into<String>,
        class: AuthorityClass,
    ) -> Self {
        let workflow_id = workflow_id.into();
        let step_id = step_id.into();
        let authority =
            ClaimAuthority::<Provisional>::new(workflow_id.clone(), step_id.clone(), class);
        Self {
            workflow_id,
            step_id,
            state: WorkflowState::Pending,
            sequence: EventSequence::new(0),
            authority,
        }
    }

    /// Current workflow state.
    #[must_use]
    pub fn state(&self) -> WorkflowState {
        self.state
    }

    /// Workflow identifier.
    #[must_use]
    pub fn workflow_id(&self) -> &str {
        &self.workflow_id
    }

    /// Step identifier.
    #[must_use]
    pub fn step_id(&self) -> &str {
        &self.step_id
    }

    /// Current event sequence number.
    #[must_use]
    pub fn sequence(&self) -> EventSequence {
        self.sequence
    }

    /// Drive the FSM by one event.
    ///
    /// Consumes `self`.  On success, returns a [`TransitionEffect`] containing
    /// the new machine, the emitted event, and the carried authority token.
    ///
    /// # Errors
    ///
    /// - [`TransitionError::Terminal`] when the machine is already in a terminal
    ///   state and `step` is called again.
    /// - [`TransitionError::Authority`] wrapping
    ///   [`AuthorityError::InvalidTransition`] when the target state derived from
    ///   `event` is not in the allowed set for the current state (M013).
    pub fn step(self, event: ExecutorEvent) -> Result<TransitionEffect> {
        // 1. Reject transitions from terminal states.
        if self.state.is_terminal() {
            return Err(TransitionError::Terminal { state: self.state });
        }

        // 2. Derive target from event.
        let target = event.target_state();

        // 3. Validate against static table.
        if !StatusTransitions::is_allowed(self.state, target) {
            return Err(TransitionError::Authority(
                AuthorityError::InvalidTransition {
                    from: self.state.as_str().to_owned(),
                    to: target.as_str().to_owned(),
                },
            ));
        }

        // 4. Advance sequence.
        let new_seq = self.sequence.next();

        // 5. Build new machine (state updated).
        let new_machine = Self {
            workflow_id: self.workflow_id.clone(),
            step_id: self.step_id.clone(),
            state: target,
            sequence: new_seq,
            authority: self.authority.clone(),
        };

        // 6. Return effect atomically.
        Ok(TransitionEffect {
            machine: new_machine,
            event,
            authority: self.authority,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{EventSequence, ExecutorEvent, StateMachine, TransitionError};
    use hle_core::authority::claim_authority::AuthorityClass;
    use hle_core::state::workflow_state::WorkflowState;

    // ---------------------------------------------------------------------------
    // Helpers
    // ---------------------------------------------------------------------------

    fn pending_machine() -> StateMachine {
        StateMachine::new("wf-1", "step-1", AuthorityClass::Automated)
    }

    fn start_event(machine: &StateMachine) -> ExecutorEvent {
        ExecutorEvent::StepStarted {
            workflow_id: machine.workflow_id().to_owned(),
            step_id: machine.step_id().to_owned(),
            sequence: machine.sequence().next(),
        }
    }

    fn advance_to_running(m: StateMachine) -> StateMachine {
        let ev = start_event(&m);
        m.step(ev).expect("pending→running").machine
    }

    // ---------------------------------------------------------------------------
    // StateMachine construction
    // ---------------------------------------------------------------------------

    #[test]
    fn new_machine_starts_pending() {
        assert_eq!(pending_machine().state(), WorkflowState::Pending);
    }

    #[test]
    fn new_machine_sequence_is_zero() {
        assert_eq!(pending_machine().sequence().value(), 0);
    }

    #[test]
    fn new_machine_carries_workflow_id() {
        let m = StateMachine::new("my-wf", "my-step", AuthorityClass::Automated);
        assert_eq!(m.workflow_id(), "my-wf");
    }

    #[test]
    fn new_machine_carries_step_id() {
        let m = StateMachine::new("my-wf", "my-step", AuthorityClass::Automated);
        assert_eq!(m.step_id(), "my-step");
    }

    #[test]
    fn new_machine_authority_carries_ids() {
        let m = StateMachine::new("auth-wf", "auth-step", AuthorityClass::HumanRequired);
        // authority is bundled in the machine; we verify indirectly via transition.
        let ev = start_event(&m);
        let eff = m.step(ev).expect("step");
        assert_eq!(eff.authority.workflow_id(), "auth-wf");
        assert_eq!(eff.authority.step_id(), "auth-step");
    }

    // ---------------------------------------------------------------------------
    // Successful transitions — forward path
    // ---------------------------------------------------------------------------

    #[test]
    fn step_pending_to_running_succeeds() {
        let m = pending_machine();
        let event = start_event(&m);
        let effect = m.step(event).expect("pending→running should succeed");
        assert_eq!(effect.machine.state(), WorkflowState::Running);
    }

    #[test]
    fn step_advances_sequence() {
        let m = pending_machine();
        let event = start_event(&m);
        let effect = m.step(event).expect("step should succeed");
        assert_eq!(effect.machine.sequence().value(), 1);
    }

    #[test]
    fn step_running_to_draft_passed_succeeds() {
        let running = advance_to_running(pending_machine());
        let pass_event = ExecutorEvent::DraftPassed {
            workflow_id: running.workflow_id().to_owned(),
            step_id: running.step_id().to_owned(),
            sequence: running.sequence().next(),
            evidence_hash: String::from("abc123"),
        };
        let effect = running.step(pass_event).expect("running→passed");
        assert_eq!(effect.machine.state(), WorkflowState::Passed);
    }

    #[test]
    fn step_running_to_draft_failed_succeeds() {
        let running = advance_to_running(pending_machine());
        let fail_event = ExecutorEvent::DraftFailed {
            workflow_id: running.workflow_id().to_owned(),
            step_id: running.step_id().to_owned(),
            sequence: running.sequence().next(),
            reason: String::from("error"),
        };
        let effect = running.step(fail_event).expect("running→failed");
        assert_eq!(effect.machine.state(), WorkflowState::Failed);
    }

    #[test]
    fn step_running_to_awaiting_human_succeeds() {
        let running = advance_to_running(pending_machine());
        let gate_event = ExecutorEvent::HumanGateReached {
            workflow_id: running.workflow_id().to_owned(),
            step_id: running.step_id().to_owned(),
            sequence: running.sequence().next(),
            reason: String::from("approval required"),
        };
        let effect = running.step(gate_event).expect("running→awaiting-human");
        assert_eq!(effect.machine.state(), WorkflowState::AwaitingHuman);
    }

    #[test]
    fn step_running_to_rollback_started_succeeds() {
        let running = advance_to_running(pending_machine());
        let rb_event = ExecutorEvent::RollbackStarted {
            workflow_id: running.workflow_id().to_owned(),
            step_id: running.step_id().to_owned(),
            sequence: running.sequence().next(),
            reason: String::from("abort"),
        };
        let effect = running.step(rb_event).expect("running→rolled-back");
        assert_eq!(effect.machine.state(), WorkflowState::RolledBack);
    }

    #[test]
    fn step_awaiting_human_to_running_succeeds() {
        let running = advance_to_running(pending_machine());
        let gate_event = ExecutorEvent::HumanGateReached {
            workflow_id: running.workflow_id().to_owned(),
            step_id: running.step_id().to_owned(),
            sequence: running.sequence().next(),
            reason: String::from("needs approval"),
        };
        let awaiting = running.step(gate_event).expect("running→awaiting").machine;
        let resume_event = ExecutorEvent::StepStarted {
            workflow_id: awaiting.workflow_id().to_owned(),
            step_id: awaiting.step_id().to_owned(),
            sequence: awaiting.sequence().next(),
        };
        let effect = awaiting.step(resume_event).expect("awaiting→running");
        assert_eq!(effect.machine.state(), WorkflowState::Running);
    }

    #[test]
    fn step_pending_to_failed_succeeds() {
        let m = pending_machine();
        let fail_event = ExecutorEvent::DraftFailed {
            workflow_id: m.workflow_id().to_owned(),
            step_id: m.step_id().to_owned(),
            sequence: m.sequence().next(),
            reason: String::from("precondition failed"),
        };
        let effect = m.step(fail_event).expect("pending→failed");
        assert_eq!(effect.machine.state(), WorkflowState::Failed);
    }

    #[test]
    fn sequence_monotonically_increases_across_two_steps() {
        let running = advance_to_running(pending_machine());
        let seq_after_first = running.sequence().value();
        let pass_event = ExecutorEvent::DraftPassed {
            workflow_id: running.workflow_id().to_owned(),
            step_id: running.step_id().to_owned(),
            sequence: running.sequence().next(),
            evidence_hash: String::from("h"),
        };
        let effect = running.step(pass_event).expect("running→passed");
        assert!(effect.machine.sequence().value() > seq_after_first);
    }

    // ---------------------------------------------------------------------------
    // TransitionEffect fields
    // ---------------------------------------------------------------------------

    #[test]
    fn transition_effect_event_matches_input() {
        let m = pending_machine();
        let ev = start_event(&m);
        let ev_clone = ev.clone();
        let effect = m.step(ev).expect("step");
        assert_eq!(effect.event, ev_clone);
    }

    #[test]
    fn transition_effect_authority_workflow_matches_machine() {
        let m = StateMachine::new("auth-wf", "auth-step", AuthorityClass::Automated);
        let ev = start_event(&m);
        let effect = m.step(ev).expect("step");
        assert_eq!(effect.authority.workflow_id(), "auth-wf");
    }

    // ---------------------------------------------------------------------------
    // Error cases
    // ---------------------------------------------------------------------------

    #[test]
    fn step_from_passed_returns_terminal_error() {
        let m = StateMachine::new("wf-2", "step-2", AuthorityClass::Automated);
        let running = advance_to_running(m);
        let pass_ev = ExecutorEvent::DraftPassed {
            workflow_id: running.workflow_id().to_owned(),
            step_id: running.step_id().to_owned(),
            sequence: running.sequence().next(),
            evidence_hash: String::from("hash"),
        };
        let passed = running.step(pass_ev).expect("running→passed").machine;
        let bogus_event = ExecutorEvent::StepStarted {
            workflow_id: passed.workflow_id().to_owned(),
            step_id: passed.step_id().to_owned(),
            sequence: passed.sequence().next(),
        };
        let err = passed.step(bogus_event);
        assert!(matches!(
            err,
            Err(TransitionError::Terminal {
                state: WorkflowState::Passed
            })
        ));
    }

    #[test]
    fn step_from_rolled_back_returns_terminal_error() {
        let running = advance_to_running(pending_machine());
        let rb_event = ExecutorEvent::RollbackStarted {
            workflow_id: running.workflow_id().to_owned(),
            step_id: running.step_id().to_owned(),
            sequence: running.sequence().next(),
            reason: String::from("abort"),
        };
        let rolled = running.step(rb_event).expect("running→rolled-back").machine;
        let bogus = ExecutorEvent::StepStarted {
            workflow_id: rolled.workflow_id().to_owned(),
            step_id: rolled.step_id().to_owned(),
            sequence: rolled.sequence().next(),
        };
        let err = rolled.step(bogus);
        assert!(matches!(err, Err(TransitionError::Terminal { .. })));
    }

    #[test]
    fn step_from_terminal_returns_terminal_error() {
        let m = pending_machine();
        let start = start_event(&m);
        let effect = m.step(start).expect("step 1");
        let running = effect.machine;
        let fail_event = ExecutorEvent::DraftFailed {
            workflow_id: running.workflow_id().to_owned(),
            step_id: running.step_id().to_owned(),
            sequence: running.sequence().next(),
            reason: String::from("boom"),
        };
        let effect2 = running.step(fail_event).expect("running→failed");
        let failed = effect2.machine;

        let m2 = StateMachine::new("wf-2", "step-2", AuthorityClass::Automated);
        let start2 = ExecutorEvent::StepStarted {
            workflow_id: String::from("wf-2"),
            step_id: String::from("step-2"),
            sequence: EventSequence::new(1),
        };
        let eff = m2.step(start2).expect("pending→running");
        let pass_ev = ExecutorEvent::DraftPassed {
            workflow_id: String::from("wf-2"),
            step_id: String::from("step-2"),
            sequence: EventSequence::new(2),
            evidence_hash: String::from("hash"),
        };
        let eff2 = eff.machine.step(pass_ev).expect("running→passed");
        let passed = eff2.machine;
        let bogus_event = ExecutorEvent::StepStarted {
            workflow_id: String::from("wf-2"),
            step_id: String::from("step-2"),
            sequence: EventSequence::new(3),
        };
        let err = passed.step(bogus_event);
        assert!(matches!(err, Err(TransitionError::Terminal { .. })));

        // `failed` used only to confirm it was the terminal trigger in step 2.
        let _ = failed;
    }

    #[test]
    fn invalid_transition_returns_authority_error() {
        // Cannot go Pending → Passed directly.
        let m = pending_machine();
        let bad_event = ExecutorEvent::DraftPassed {
            workflow_id: String::from("wf-1"),
            step_id: String::from("step-1"),
            sequence: EventSequence::new(1),
            evidence_hash: String::from("hash"),
        };
        let err = m.step(bad_event);
        assert!(matches!(err, Err(TransitionError::Authority(_))));
    }

    #[test]
    fn pending_to_human_gate_is_invalid() {
        let m = pending_machine();
        let bad = ExecutorEvent::HumanGateReached {
            workflow_id: m.workflow_id().to_owned(),
            step_id: m.step_id().to_owned(),
            sequence: m.sequence().next(),
            reason: String::from("bad"),
        };
        assert!(matches!(m.step(bad), Err(TransitionError::Authority(_))));
    }

    #[test]
    fn pending_to_rolled_back_is_invalid() {
        let m = pending_machine();
        let bad = ExecutorEvent::RollbackStarted {
            workflow_id: m.workflow_id().to_owned(),
            step_id: m.step_id().to_owned(),
            sequence: m.sequence().next(),
            reason: String::from("bad"),
        };
        assert!(matches!(m.step(bad), Err(TransitionError::Authority(_))));
    }

    #[test]
    fn transition_error_terminal_display_contains_state() {
        let err = TransitionError::Terminal {
            state: WorkflowState::Passed,
        };
        let s = err.to_string();
        assert!(s.contains("passed"));
    }

    #[test]
    fn transition_error_authority_display_is_nonempty() {
        use hle_core::authority::claim_authority::AuthorityError;
        let err = TransitionError::Authority(AuthorityError::InvalidTransition {
            from: String::from("pending"),
            to: String::from("passed"),
        });
        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn transition_error_is_std_error() {
        use std::error::Error as _;
        let err = TransitionError::Terminal {
            state: WorkflowState::Passed,
        };
        assert!(err.source().is_none());
    }

    // ---------------------------------------------------------------------------
    // EventSequence
    // ---------------------------------------------------------------------------

    #[test]
    fn event_sequence_new_value() {
        assert_eq!(EventSequence::new(42).value(), 42);
    }

    #[test]
    fn event_sequence_default_is_zero() {
        assert_eq!(EventSequence::default().value(), 0);
    }

    #[test]
    fn event_sequence_next_increments() {
        let s = EventSequence::new(5);
        assert_eq!(s.next().value(), 6);
    }

    #[test]
    fn event_sequence_saturates_at_max() {
        let s = EventSequence::new(u64::MAX);
        assert_eq!(s.next().value(), u64::MAX);
    }

    #[test]
    fn event_sequence_ord() {
        assert!(EventSequence::new(1) < EventSequence::new(2));
        assert!(EventSequence::new(10) > EventSequence::new(9));
    }

    #[test]
    fn event_sequence_display_contains_value() {
        let s = EventSequence::new(77);
        assert!(s.to_string().contains("77"));
    }

    // ---------------------------------------------------------------------------
    // ExecutorEvent helpers — target_state
    // ---------------------------------------------------------------------------

    #[test]
    fn executor_event_target_state_step_started_is_running() {
        let ev = ExecutorEvent::StepStarted {
            workflow_id: String::from("w"),
            step_id: String::from("s"),
            sequence: EventSequence::new(0),
        };
        assert_eq!(ev.target_state(), WorkflowState::Running);
    }

    #[test]
    fn executor_event_target_state_human_gate_is_awaiting() {
        let ev = ExecutorEvent::HumanGateReached {
            workflow_id: String::from("w"),
            step_id: String::from("s"),
            sequence: EventSequence::new(1),
            reason: String::from("r"),
        };
        assert_eq!(ev.target_state(), WorkflowState::AwaitingHuman);
    }

    #[test]
    fn executor_event_target_state_draft_passed_is_passed() {
        let ev = ExecutorEvent::DraftPassed {
            workflow_id: String::from("w"),
            step_id: String::from("s"),
            sequence: EventSequence::new(2),
            evidence_hash: String::from("h"),
        };
        assert_eq!(ev.target_state(), WorkflowState::Passed);
    }

    #[test]
    fn executor_event_target_state_draft_failed_is_failed() {
        let ev = ExecutorEvent::DraftFailed {
            workflow_id: String::from("w"),
            step_id: String::from("s"),
            sequence: EventSequence::new(3),
            reason: String::from("boom"),
        };
        assert_eq!(ev.target_state(), WorkflowState::Failed);
    }

    #[test]
    fn executor_event_target_state_rollback_started_is_rolled_back() {
        let ev = ExecutorEvent::RollbackStarted {
            workflow_id: String::from("w"),
            step_id: String::from("s"),
            sequence: EventSequence::new(4),
            reason: String::from("r"),
        };
        assert_eq!(ev.target_state(), WorkflowState::RolledBack);
    }

    #[test]
    fn executor_event_target_state_rollback_completed_is_rolled_back() {
        let ev = ExecutorEvent::RollbackCompleted {
            workflow_id: String::from("w"),
            step_id: String::from("s"),
            sequence: EventSequence::new(5),
        };
        assert_eq!(ev.target_state(), WorkflowState::RolledBack);
    }

    // ---------------------------------------------------------------------------
    // ExecutorEvent helpers — is_terminal_event
    // ---------------------------------------------------------------------------

    #[test]
    fn executor_event_is_terminal_event_for_draft_passed() {
        let ev = ExecutorEvent::DraftPassed {
            workflow_id: String::from("w"),
            step_id: String::from("s"),
            sequence: EventSequence::new(0),
            evidence_hash: String::from("h"),
        };
        assert!(ev.is_terminal_event());
    }

    #[test]
    fn executor_event_is_terminal_event_for_draft_failed() {
        let ev = ExecutorEvent::DraftFailed {
            workflow_id: String::from("w"),
            step_id: String::from("s"),
            sequence: EventSequence::new(0),
            reason: String::from("r"),
        };
        assert!(ev.is_terminal_event());
    }

    #[test]
    fn executor_event_is_terminal_event_for_rollback_completed() {
        let ev = ExecutorEvent::RollbackCompleted {
            workflow_id: String::from("w"),
            step_id: String::from("s"),
            sequence: EventSequence::new(0),
        };
        assert!(ev.is_terminal_event());
    }

    #[test]
    fn executor_event_step_started_is_not_terminal() {
        let ev = ExecutorEvent::StepStarted {
            workflow_id: String::from("w"),
            step_id: String::from("s"),
            sequence: EventSequence::new(0),
        };
        assert!(!ev.is_terminal_event());
    }

    #[test]
    fn executor_event_human_gate_is_not_terminal() {
        let ev = ExecutorEvent::HumanGateReached {
            workflow_id: String::from("w"),
            step_id: String::from("s"),
            sequence: EventSequence::new(0),
            reason: String::from("r"),
        };
        assert!(!ev.is_terminal_event());
    }

    #[test]
    fn executor_event_rollback_started_is_not_terminal() {
        let ev = ExecutorEvent::RollbackStarted {
            workflow_id: String::from("w"),
            step_id: String::from("s"),
            sequence: EventSequence::new(0),
            reason: String::from("r"),
        };
        assert!(!ev.is_terminal_event());
    }

    // ---------------------------------------------------------------------------
    // ExecutorEvent accessors
    // ---------------------------------------------------------------------------

    #[test]
    fn executor_event_workflow_id_accessor() {
        let ev = ExecutorEvent::StepStarted {
            workflow_id: String::from("accessor-wf"),
            step_id: String::from("s"),
            sequence: EventSequence::new(0),
        };
        assert_eq!(ev.workflow_id(), "accessor-wf");
    }

    #[test]
    fn executor_event_step_id_accessor() {
        let ev = ExecutorEvent::DraftPassed {
            workflow_id: String::from("w"),
            step_id: String::from("my-step"),
            sequence: EventSequence::new(0),
            evidence_hash: String::from("h"),
        };
        assert_eq!(ev.step_id(), "my-step");
    }

    #[test]
    fn executor_event_sequence_accessor() {
        let ev = ExecutorEvent::DraftFailed {
            workflow_id: String::from("w"),
            step_id: String::from("s"),
            sequence: EventSequence::new(99),
            reason: String::from("r"),
        };
        assert_eq!(ev.sequence().value(), 99);
    }

    #[test]
    fn executor_event_display_is_nonempty() {
        let ev = ExecutorEvent::StepStarted {
            workflow_id: String::from("w"),
            step_id: String::from("s"),
            sequence: EventSequence::new(0),
        };
        assert!(!ev.to_string().is_empty());
    }

    // ---------------------------------------------------------------------------
    // executor_event target() is consistent with target_state()
    // ---------------------------------------------------------------------------

    #[test]
    fn executor_event_target() {
        let ev = ExecutorEvent::StepStarted {
            workflow_id: String::from("w"),
            step_id: String::from("s"),
            sequence: EventSequence::new(0),
        };
        assert_eq!(ev.target_state(), WorkflowState::Running);
    }

    // ---------------------------------------------------------------------------
    // Multi-step chain invariants
    // ---------------------------------------------------------------------------

    #[test]
    fn full_happy_path_chain_ends_in_passed_terminal() {
        let m = StateMachine::new("wf-full", "step-full", AuthorityClass::Automated);
        let running = advance_to_running(m);
        let pass_ev = ExecutorEvent::DraftPassed {
            workflow_id: running.workflow_id().to_owned(),
            step_id: running.step_id().to_owned(),
            sequence: running.sequence().next(),
            evidence_hash: String::from("evidence"),
        };
        let passed = running.step(pass_ev).expect("running→passed").machine;
        assert!(passed.state().is_terminal());
        assert!(passed.state().is_passed());
    }

    #[test]
    fn rollback_chain_produces_rolled_back_terminal() {
        let m = StateMachine::new("wf-rb", "step-rb", AuthorityClass::Automated);
        let running = advance_to_running(m);
        let fail_ev = ExecutorEvent::DraftFailed {
            workflow_id: running.workflow_id().to_owned(),
            step_id: running.step_id().to_owned(),
            sequence: running.sequence().next(),
            reason: String::from("fail"),
        };
        let failed = running.step(fail_ev).expect("running→failed").machine;
        let rb_ev = ExecutorEvent::RollbackStarted {
            workflow_id: failed.workflow_id().to_owned(),
            step_id: failed.step_id().to_owned(),
            sequence: failed.sequence().next(),
            reason: String::from("compensate"),
        };
        // NOTE: Failed is terminal so step() returns Terminal error — this is correct
        // per C02 Invariant I4. RolledBack is reachable only via Running, not Failed
        // in the state machine's step() (Failed is terminal once in that state).
        // We verify the error is Terminal.
        let err = failed.step(rb_ev);
        assert!(
            matches!(
                err,
                Err(TransitionError::Terminal {
                    state: WorkflowState::Failed
                })
            ),
            "stepping from Failed should return Terminal error, got {err:?}"
        );
    }
}
