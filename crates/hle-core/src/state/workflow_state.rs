#![forbid(unsafe_code)]

//! M011 — `WorkflowState` FSM enum and invariants.
//!
//! **Cluster:** C02 Authority & State | **Layer:** L01
//!
//! Supersedes `substrate_types::StepState` for the planned full-codebase
//! topology.  `StepState` remains the substrate wire type; `WorkflowState`
//! adds richer predicate methods, a bitmask companion, and compile-time
//! const expressions used by the transition table (M013).
//!
//! Enforces C02 Invariant I2: `WorkflowState::Passed` is reachable only via
//! a verifier-issued PASS receipt.
//!
//! Cross-reference: `ai_specs/modules/c02-authority-state/M011_WORKFLOW_STATE.md`
//! Use pattern: `ai_docs/use_patterns/UP_EXECUTOR_VERIFIER_SPLIT.md` (HLE-UP-001)

use std::fmt;
use std::str::FromStr;

use substrate_types::StepState;

use crate::authority::claim_authority::AuthorityError;

// ---------------------------------------------------------------------------
// WorkflowState
// ---------------------------------------------------------------------------

/// Authoritative workflow step state.
///
/// Supersedes [`substrate_types::StepState`] for the planned full-codebase
/// topology while providing a bridge via `From` conversions so that existing
/// substrate code continues to compile unchanged.
///
/// All predicate methods are `const fn` and can be used in `const` contexts
/// such as the static transition table (M013).
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

impl WorkflowState {
    /// Wire string matching `substrate_types::StepState` wire values.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::AwaitingHuman => "awaiting-human",
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::RolledBack => "rolled-back",
        }
    }

    /// Bitmask bit for use with [`WorkflowStateSet`].  `1 << (self as u8)`.
    #[must_use]
    pub const fn bit(self) -> u8 {
        1u8 << (self as u8)
    }

    // -----------------------------------------------------------------------
    // Predicate methods
    // -----------------------------------------------------------------------

    /// Returns `true` when the state is a terminal state (`Passed`, `Failed`,
    /// or `RolledBack`).  Terminal states cannot be transitioned away from.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Passed | Self::Failed | Self::RolledBack)
    }

    /// Returns `true` when the step is blocked waiting for a human decision.
    #[must_use]
    pub const fn is_blocking_human(self) -> bool {
        matches!(self, Self::AwaitingHuman)
    }

    /// Returns `true` when the executor is actively running this step.
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Running)
    }

    /// Returns `true` when the step has not yet been claimed by any executor.
    #[must_use]
    pub const fn is_pending(self) -> bool {
        matches!(self, Self::Pending)
    }

    /// Returns `true` when the verifier has issued a PASS receipt for this step.
    #[must_use]
    pub const fn is_passed(self) -> bool {
        matches!(self, Self::Passed)
    }

    /// Returns `true` when the verifier has issued a FAIL receipt for this step.
    #[must_use]
    pub const fn is_failed(self) -> bool {
        matches!(self, Self::Failed)
    }

    /// Returns `true` when the step has been reversed by a compensating rollback.
    #[must_use]
    pub const fn is_rolled_back(self) -> bool {
        matches!(self, Self::RolledBack)
    }

    /// Returns `true` when `self → target` is in the static allowed-transition
    /// table.
    ///
    /// This embeds the same logic as M013's `is_allowed` as a convenience
    /// predicate so callers in `hle-core` (L01) can check transition validity
    /// without importing `hle-executor` (L03).
    #[must_use]
    pub const fn can_transition_to(self, target: Self) -> bool {
        // Mirror of ALLOWED_TRANSITIONS in M013 — kept in sync manually.
        // const fn: use while loop + u8 comparisons (matches! is not const-stable).
        // Each allowed pair is encoded as (from_u8 << 4 | to_u8).
        const PAIRS: [(u8, u8); 9] = [
            (0, 1), // Pending   → Running
            (0, 4), // Pending   → Failed
            (1, 3), // Running   → Passed
            (1, 4), // Running   → Failed
            (1, 2), // Running   → AwaitingHuman
            (1, 5), // Running   → RolledBack
            (2, 1), // AwaitingHuman → Running
            (2, 4), // AwaitingHuman → Failed
            (4, 5), // Failed    → RolledBack
        ];
        let f = self as u8;
        let t = target as u8;
        let mut i = 0;
        while i < PAIRS.len() {
            if PAIRS[i].0 == f && PAIRS[i].1 == t {
                return true;
            }
            i += 1;
        }
        false
    }

    /// Returns `true` for `Passed` — documents that this state is only reachable
    /// via a verifier-issued PASS receipt (C02 Invariant I2).
    #[must_use]
    pub const fn requires_verifier_authority(self) -> bool {
        matches!(self, Self::Passed)
    }
}

impl fmt::Display for WorkflowState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for WorkflowState {
    type Err = AuthorityError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "awaiting-human" => Ok(Self::AwaitingHuman),
            "passed" => Ok(Self::Passed),
            "failed" => Ok(Self::Failed),
            "rolled-back" => Ok(Self::RolledBack),
            other => Err(AuthorityError::Other(format!(
                "unknown workflow state: {other}"
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// StepState bridge
// ---------------------------------------------------------------------------

impl From<WorkflowState> for StepState {
    fn from(ws: WorkflowState) -> Self {
        match ws {
            WorkflowState::Pending => Self::Pending,
            WorkflowState::Running => Self::Running,
            WorkflowState::AwaitingHuman => Self::AwaitingHuman,
            WorkflowState::Passed => Self::Passed,
            WorkflowState::Failed => Self::Failed,
            WorkflowState::RolledBack => Self::RolledBack,
        }
    }
}

impl From<StepState> for WorkflowState {
    fn from(ss: StepState) -> Self {
        match ss {
            StepState::Pending => Self::Pending,
            StepState::Running => Self::Running,
            StepState::AwaitingHuman => Self::AwaitingHuman,
            StepState::Passed => Self::Passed,
            StepState::Failed => Self::Failed,
            StepState::RolledBack => Self::RolledBack,
        }
    }
}

// ---------------------------------------------------------------------------
// WorkflowStateSet  (bitmask — all const fn)
// ---------------------------------------------------------------------------

/// Compact bitmask representing a subset of [`WorkflowState`] values.
///
/// At most 6 bits are used (one per variant).  All operations are `const fn`
/// so the transition table constants can be evaluated at compile time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct WorkflowStateSet(u8);

impl WorkflowStateSet {
    /// Empty set (all bits clear).
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Return a new set with `state` included.  Chainable.
    #[must_use]
    pub const fn with(self, state: WorkflowState) -> Self {
        Self(self.0 | state.bit())
    }

    /// Returns `true` when `state` is a member of this set.
    #[must_use]
    pub const fn contains(self, state: WorkflowState) -> bool {
        self.0 & state.bit() != 0
    }

    /// Returns `true` when the set is empty.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Bitwise union of two sets.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Bitwise intersection of two sets.
    #[must_use]
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }
}

impl fmt::Display for WorkflowStateSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let states = [
            WorkflowState::Pending,
            WorkflowState::Running,
            WorkflowState::AwaitingHuman,
            WorkflowState::Passed,
            WorkflowState::Failed,
            WorkflowState::RolledBack,
        ];
        write!(f, "WorkflowStateSet{{")?;
        let mut first = true;
        for state in states {
            if self.contains(state) {
                if !first {
                    write!(f, ", ")?;
                }
                write!(f, "{}", state.as_str())?;
                first = false;
            }
        }
        write!(f, "}}")
    }
}

// ---------------------------------------------------------------------------
// Compile-time constants
// ---------------------------------------------------------------------------

/// The set of terminal states: `Passed`, `Failed`, `RolledBack`.
pub const TERMINAL_STATES: WorkflowStateSet = WorkflowStateSet::empty()
    .with(WorkflowState::Passed)
    .with(WorkflowState::Failed)
    .with(WorkflowState::RolledBack);

/// The set of human-blocking states: `AwaitingHuman`.
pub const BLOCKING_STATES: WorkflowStateSet =
    WorkflowStateSet::empty().with(WorkflowState::AwaitingHuman);

/// The set of actively-running states: `Running`.
pub const ACTIVE_STATES: WorkflowStateSet = WorkflowStateSet::empty().with(WorkflowState::Running);

// ---------------------------------------------------------------------------
// StateTransitionKind
// ---------------------------------------------------------------------------

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

impl StateTransitionKind {
    /// Wire-format label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Forward => "forward",
            Self::Rollback => "rollback",
            Self::HumanGate => "human-gate",
        }
    }

    /// Returns `true` when this is a compensating rollback transition.
    #[must_use]
    pub const fn is_rollback(self) -> bool {
        matches!(self, Self::Rollback)
    }
}

impl fmt::Display for StateTransitionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{
        StateTransitionKind, WorkflowState, WorkflowStateSet, ACTIVE_STATES, BLOCKING_STATES,
        TERMINAL_STATES,
    };
    use substrate_types::StepState;

    const ALL_STATES: [WorkflowState; 6] = [
        WorkflowState::Pending,
        WorkflowState::Running,
        WorkflowState::AwaitingHuman,
        WorkflowState::Passed,
        WorkflowState::Failed,
        WorkflowState::RolledBack,
    ];

    // ---------------------------------------------------------------------------
    // is_terminal — exhaustive per variant
    // ---------------------------------------------------------------------------

    #[test]
    fn pending_is_not_terminal() {
        assert!(!WorkflowState::Pending.is_terminal());
    }

    #[test]
    fn running_is_not_terminal() {
        assert!(!WorkflowState::Running.is_terminal());
    }

    #[test]
    fn awaiting_human_is_not_terminal() {
        assert!(!WorkflowState::AwaitingHuman.is_terminal());
    }

    #[test]
    fn passed_is_terminal() {
        assert!(WorkflowState::Passed.is_terminal());
    }

    #[test]
    fn failed_is_terminal() {
        assert!(WorkflowState::Failed.is_terminal());
    }

    #[test]
    fn rolled_back_is_terminal() {
        assert!(WorkflowState::RolledBack.is_terminal());
    }

    #[test]
    fn exactly_three_terminal_states() {
        let count = ALL_STATES.iter().filter(|s| s.is_terminal()).count();
        assert_eq!(count, 3);
    }

    #[test]
    fn terminal_states_are_passed_failed_rolled_back() {
        assert!(WorkflowState::Passed.is_terminal());
        assert!(WorkflowState::Failed.is_terminal());
        assert!(WorkflowState::RolledBack.is_terminal());
        assert!(!WorkflowState::Pending.is_terminal());
        assert!(!WorkflowState::Running.is_terminal());
        assert!(!WorkflowState::AwaitingHuman.is_terminal());
    }

    // ---------------------------------------------------------------------------
    // is_blocking_human — exhaustive
    // ---------------------------------------------------------------------------

    #[test]
    fn awaiting_human_blocks_human() {
        assert!(WorkflowState::AwaitingHuman.is_blocking_human());
    }

    #[test]
    fn running_does_not_block_human() {
        assert!(!WorkflowState::Running.is_blocking_human());
    }

    #[test]
    fn only_awaiting_human_blocks() {
        for state in ALL_STATES {
            let expected = matches!(state, WorkflowState::AwaitingHuman);
            assert_eq!(
                state.is_blocking_human(),
                expected,
                "is_blocking_human mismatch for {state}"
            );
        }
    }

    // ---------------------------------------------------------------------------
    // is_active / is_pending / is_passed / is_failed / is_rolled_back
    // ---------------------------------------------------------------------------

    #[test]
    fn only_running_is_active() {
        for state in ALL_STATES {
            assert_eq!(
                state.is_active(),
                matches!(state, WorkflowState::Running),
                "is_active mismatch for {state}"
            );
        }
    }

    #[test]
    fn only_pending_is_pending() {
        for state in ALL_STATES {
            assert_eq!(
                state.is_pending(),
                matches!(state, WorkflowState::Pending),
                "is_pending mismatch for {state}"
            );
        }
    }

    #[test]
    fn only_passed_is_passed() {
        for state in ALL_STATES {
            assert_eq!(
                state.is_passed(),
                matches!(state, WorkflowState::Passed),
                "is_passed mismatch for {state}"
            );
        }
    }

    #[test]
    fn only_failed_is_failed() {
        for state in ALL_STATES {
            assert_eq!(
                state.is_failed(),
                matches!(state, WorkflowState::Failed),
                "is_failed mismatch for {state}"
            );
        }
    }

    #[test]
    fn only_rolled_back_is_rolled_back() {
        for state in ALL_STATES {
            assert_eq!(
                state.is_rolled_back(),
                matches!(state, WorkflowState::RolledBack),
                "is_rolled_back mismatch for {state}"
            );
        }
    }

    // ---------------------------------------------------------------------------
    // requires_verifier_authority
    // ---------------------------------------------------------------------------

    #[test]
    fn passed_requires_verifier_authority() {
        assert!(WorkflowState::Passed.requires_verifier_authority());
    }

    #[test]
    fn failed_does_not_require_verifier_authority() {
        assert!(!WorkflowState::Failed.requires_verifier_authority());
    }

    #[test]
    fn only_passed_requires_verifier_authority() {
        for state in ALL_STATES {
            assert_eq!(
                state.requires_verifier_authority(),
                matches!(state, WorkflowState::Passed),
                "requires_verifier_authority mismatch for {state}"
            );
        }
    }

    // ---------------------------------------------------------------------------
    // can_transition_to — allowed pairs (9 rules)
    // ---------------------------------------------------------------------------

    #[test]
    fn pending_can_transition_to_running() {
        assert!(WorkflowState::Pending.can_transition_to(WorkflowState::Running));
    }

    #[test]
    fn pending_can_transition_to_failed() {
        assert!(WorkflowState::Pending.can_transition_to(WorkflowState::Failed));
    }

    #[test]
    fn running_can_transition_to_passed() {
        assert!(WorkflowState::Running.can_transition_to(WorkflowState::Passed));
    }

    #[test]
    fn running_can_transition_to_failed() {
        assert!(WorkflowState::Running.can_transition_to(WorkflowState::Failed));
    }

    #[test]
    fn running_can_transition_to_awaiting_human() {
        assert!(WorkflowState::Running.can_transition_to(WorkflowState::AwaitingHuman));
    }

    #[test]
    fn running_can_transition_to_rolled_back() {
        assert!(WorkflowState::Running.can_transition_to(WorkflowState::RolledBack));
    }

    #[test]
    fn awaiting_human_can_transition_to_running() {
        assert!(WorkflowState::AwaitingHuman.can_transition_to(WorkflowState::Running));
    }

    #[test]
    fn awaiting_human_can_transition_to_failed() {
        assert!(WorkflowState::AwaitingHuman.can_transition_to(WorkflowState::Failed));
    }

    #[test]
    fn failed_can_transition_to_rolled_back() {
        assert!(WorkflowState::Failed.can_transition_to(WorkflowState::RolledBack));
    }

    // --- forbidden transitions ---

    #[test]
    fn passed_cannot_transition_to_running() {
        assert!(!WorkflowState::Passed.can_transition_to(WorkflowState::Running));
    }

    #[test]
    fn passed_cannot_transition_to_any_state() {
        for target in ALL_STATES {
            assert!(
                !WorkflowState::Passed.can_transition_to(target),
                "Passed should not transition to {target}"
            );
        }
    }

    #[test]
    fn rolled_back_cannot_transition_to_any_state() {
        for target in ALL_STATES {
            assert!(
                !WorkflowState::RolledBack.can_transition_to(target),
                "RolledBack should not transition to {target}"
            );
        }
    }

    #[test]
    fn pending_cannot_transition_to_passed() {
        assert!(!WorkflowState::Pending.can_transition_to(WorkflowState::Passed));
    }

    #[test]
    fn pending_cannot_transition_to_awaiting_human() {
        assert!(!WorkflowState::Pending.can_transition_to(WorkflowState::AwaitingHuman));
    }

    #[test]
    fn pending_cannot_transition_to_rolled_back() {
        assert!(!WorkflowState::Pending.can_transition_to(WorkflowState::RolledBack));
    }

    #[test]
    fn no_self_transitions() {
        for state in ALL_STATES {
            assert!(
                !state.can_transition_to(state),
                "self-transition should be forbidden for {state}"
            );
        }
    }

    #[test]
    fn can_transition_to_count_for_running_is_four() {
        let count = ALL_STATES
            .iter()
            .filter(|&&t| WorkflowState::Running.can_transition_to(t))
            .count();
        assert_eq!(count, 4);
    }

    #[test]
    fn can_transition_to_count_for_pending_is_two() {
        let count = ALL_STATES
            .iter()
            .filter(|&&t| WorkflowState::Pending.can_transition_to(t))
            .count();
        assert_eq!(count, 2);
    }

    // ---------------------------------------------------------------------------
    // as_str / Display / FromStr
    // ---------------------------------------------------------------------------

    #[test]
    fn as_str_is_stable_for_all_variants() {
        assert_eq!(WorkflowState::Pending.as_str(), "pending");
        assert_eq!(WorkflowState::Running.as_str(), "running");
        assert_eq!(WorkflowState::AwaitingHuman.as_str(), "awaiting-human");
        assert_eq!(WorkflowState::Passed.as_str(), "passed");
        assert_eq!(WorkflowState::Failed.as_str(), "failed");
        assert_eq!(WorkflowState::RolledBack.as_str(), "rolled-back");
    }

    #[test]
    fn display_matches_as_str_for_all_variants() {
        for state in ALL_STATES {
            assert_eq!(state.to_string(), state.as_str());
        }
    }

    #[test]
    fn parse_roundtrip_for_all_variants() {
        for state in ALL_STATES {
            assert_eq!(state.as_str().parse::<WorkflowState>(), Ok(state));
        }
    }

    #[test]
    fn parse_unknown_returns_error() {
        assert!("unknown".parse::<WorkflowState>().is_err());
    }

    #[test]
    fn parse_empty_string_returns_error() {
        assert!("".parse::<WorkflowState>().is_err());
    }

    #[test]
    fn parse_wrong_case_returns_error() {
        assert!("Pending".parse::<WorkflowState>().is_err());
        assert!("PASSED".parse::<WorkflowState>().is_err());
    }

    // ---------------------------------------------------------------------------
    // bit() mask — each bit must be unique and power-of-two
    // ---------------------------------------------------------------------------

    #[test]
    fn each_state_has_unique_bit() {
        let bits: Vec<u8> = ALL_STATES.iter().map(|s| s.bit()).collect();
        for i in 0..bits.len() {
            for j in 0..bits.len() {
                if i != j {
                    assert_ne!(bits[i], bits[j], "states {i} and {j} share bit");
                }
            }
        }
    }

    #[test]
    fn each_state_bit_is_power_of_two() {
        for state in ALL_STATES {
            let b = state.bit();
            assert!(
                b.count_ones() == 1,
                "bit for {state} is not a power of two: {b}"
            );
        }
    }

    // ---------------------------------------------------------------------------
    // StepState bridge
    // ---------------------------------------------------------------------------

    #[test]
    fn from_workflow_state_to_step_state_roundtrip() {
        for state in ALL_STATES {
            let ss = StepState::from(state);
            let back = WorkflowState::from(ss);
            assert_eq!(back, state);
        }
    }

    #[test]
    fn step_state_pending_bridges_to_pending() {
        assert_eq!(
            WorkflowState::from(StepState::Pending),
            WorkflowState::Pending
        );
    }

    #[test]
    fn step_state_running_bridges_to_running() {
        assert_eq!(
            WorkflowState::from(StepState::Running),
            WorkflowState::Running
        );
    }

    #[test]
    fn step_state_passed_bridges_to_passed() {
        assert_eq!(
            WorkflowState::from(StepState::Passed),
            WorkflowState::Passed
        );
    }

    #[test]
    fn step_state_failed_bridges_to_failed() {
        assert_eq!(
            WorkflowState::from(StepState::Failed),
            WorkflowState::Failed
        );
    }

    #[test]
    fn workflow_state_to_step_state_awaiting_human() {
        let ss = StepState::from(WorkflowState::AwaitingHuman);
        assert_eq!(WorkflowState::from(ss), WorkflowState::AwaitingHuman);
    }

    #[test]
    fn workflow_state_to_step_state_rolled_back() {
        let ss = StepState::from(WorkflowState::RolledBack);
        assert_eq!(WorkflowState::from(ss), WorkflowState::RolledBack);
    }

    // ---------------------------------------------------------------------------
    // WorkflowStateSet
    // ---------------------------------------------------------------------------

    #[test]
    fn empty_set_contains_no_states() {
        let set = WorkflowStateSet::empty();
        assert!(set.is_empty());
        assert!(!set.contains(WorkflowState::Pending));
    }

    #[test]
    fn empty_set_default_is_empty() {
        let set = WorkflowStateSet::default();
        assert!(set.is_empty());
    }

    #[test]
    fn with_adds_state_to_set() {
        let set = WorkflowStateSet::empty().with(WorkflowState::Running);
        assert!(set.contains(WorkflowState::Running));
        assert!(!set.contains(WorkflowState::Pending));
    }

    #[test]
    fn with_all_states_contains_all() {
        let mut set = WorkflowStateSet::empty();
        for state in ALL_STATES {
            set = set.with(state);
        }
        for state in ALL_STATES {
            assert!(set.contains(state));
        }
        assert!(!set.is_empty());
    }

    #[test]
    fn with_is_idempotent() {
        let a = WorkflowStateSet::empty().with(WorkflowState::Running);
        let b = a.with(WorkflowState::Running);
        assert_eq!(a, b);
    }

    #[test]
    fn terminal_states_constant_contains_expected_members() {
        assert!(TERMINAL_STATES.contains(WorkflowState::Passed));
        assert!(TERMINAL_STATES.contains(WorkflowState::Failed));
        assert!(TERMINAL_STATES.contains(WorkflowState::RolledBack));
        assert!(!TERMINAL_STATES.contains(WorkflowState::Running));
    }

    #[test]
    fn terminal_states_constant_excludes_non_terminal() {
        assert!(!TERMINAL_STATES.contains(WorkflowState::Pending));
        assert!(!TERMINAL_STATES.contains(WorkflowState::AwaitingHuman));
    }

    #[test]
    fn blocking_states_constant_contains_awaiting_human() {
        assert!(BLOCKING_STATES.contains(WorkflowState::AwaitingHuman));
        assert!(!BLOCKING_STATES.contains(WorkflowState::Running));
    }

    #[test]
    fn active_states_constant_contains_running() {
        assert!(ACTIVE_STATES.contains(WorkflowState::Running));
        assert!(!ACTIVE_STATES.contains(WorkflowState::Pending));
    }

    #[test]
    fn union_combines_sets() {
        let a = WorkflowStateSet::empty().with(WorkflowState::Pending);
        let b = WorkflowStateSet::empty().with(WorkflowState::Running);
        let u = a.union(b);
        assert!(u.contains(WorkflowState::Pending));
        assert!(u.contains(WorkflowState::Running));
    }

    #[test]
    fn union_is_commutative() {
        let a = WorkflowStateSet::empty().with(WorkflowState::Pending);
        let b = WorkflowStateSet::empty().with(WorkflowState::Running);
        assert_eq!(a.union(b), b.union(a));
    }

    #[test]
    fn union_with_empty_is_identity() {
        let a = WorkflowStateSet::empty().with(WorkflowState::Running);
        let empty = WorkflowStateSet::empty();
        assert_eq!(a.union(empty), a);
    }

    #[test]
    fn intersection_narrows_sets() {
        let a = WorkflowStateSet::empty()
            .with(WorkflowState::Pending)
            .with(WorkflowState::Running);
        let b = WorkflowStateSet::empty().with(WorkflowState::Running);
        let i = a.intersection(b);
        assert!(i.contains(WorkflowState::Running));
        assert!(!i.contains(WorkflowState::Pending));
    }

    #[test]
    fn intersection_with_empty_is_empty() {
        let a = WorkflowStateSet::empty().with(WorkflowState::Running);
        let empty = WorkflowStateSet::empty();
        assert!(a.intersection(empty).is_empty());
    }

    #[test]
    fn intersection_with_disjoint_is_empty() {
        let a = WorkflowStateSet::empty().with(WorkflowState::Pending);
        let b = WorkflowStateSet::empty().with(WorkflowState::Running);
        assert!(a.intersection(b).is_empty());
    }

    #[test]
    fn intersection_is_commutative() {
        let a = WorkflowStateSet::empty()
            .with(WorkflowState::Running)
            .with(WorkflowState::Pending);
        let b = WorkflowStateSet::empty()
            .with(WorkflowState::Running)
            .with(WorkflowState::Failed);
        assert_eq!(a.intersection(b), b.intersection(a));
    }

    #[test]
    fn state_set_display_contains_member_names() {
        let set = WorkflowStateSet::empty()
            .with(WorkflowState::Running)
            .with(WorkflowState::Passed);
        let s = set.to_string();
        assert!(s.contains("running"));
        assert!(s.contains("passed"));
    }

    #[test]
    fn state_set_display_empty_set() {
        let s = WorkflowStateSet::empty().to_string();
        assert!(s.contains("WorkflowStateSet"));
    }

    // ---------------------------------------------------------------------------
    // StateTransitionKind
    // ---------------------------------------------------------------------------

    #[test]
    fn rollback_kind_is_rollback() {
        assert!(StateTransitionKind::Rollback.is_rollback());
        assert!(!StateTransitionKind::Forward.is_rollback());
    }

    #[test]
    fn human_gate_kind_is_not_rollback() {
        assert!(!StateTransitionKind::HumanGate.is_rollback());
    }

    #[test]
    fn transition_kind_as_str_is_stable() {
        assert_eq!(StateTransitionKind::Forward.as_str(), "forward");
        assert_eq!(StateTransitionKind::Rollback.as_str(), "rollback");
        assert_eq!(StateTransitionKind::HumanGate.as_str(), "human-gate");
    }

    #[test]
    fn transition_kind_display_matches_as_str() {
        for kind in [
            StateTransitionKind::Forward,
            StateTransitionKind::Rollback,
            StateTransitionKind::HumanGate,
        ] {
            assert_eq!(kind.to_string(), kind.as_str());
        }
    }

    #[test]
    fn transition_kind_eq_reflexive() {
        assert_eq!(StateTransitionKind::Forward, StateTransitionKind::Forward);
        assert_eq!(StateTransitionKind::Rollback, StateTransitionKind::Rollback);
        assert_eq!(
            StateTransitionKind::HumanGate,
            StateTransitionKind::HumanGate
        );
    }

    #[test]
    fn transition_kind_neq_distinct() {
        assert_ne!(StateTransitionKind::Forward, StateTransitionKind::Rollback);
        assert_ne!(
            StateTransitionKind::Rollback,
            StateTransitionKind::HumanGate
        );
    }

    // ---------------------------------------------------------------------------
    // PartialOrd / Ord invariant
    // ---------------------------------------------------------------------------

    #[test]
    fn workflow_state_ord_pending_lt_running() {
        assert!(WorkflowState::Pending < WorkflowState::Running);
    }

    #[test]
    fn workflow_state_ord_passed_gt_running() {
        assert!(WorkflowState::Passed > WorkflowState::Running);
    }
}
