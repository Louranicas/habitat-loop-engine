#![forbid(unsafe_code)]

//! M013 — Static transition table and rollback affordances.
//!
//! **Cluster:** C02 Authority & State | **Layer:** L03
//!
//! Enforces C02 Invariant I4: the set of valid `(from, to)` transitions is
//! closed at compile time in [`ALLOWED_TRANSITIONS`].  No runtime configuration,
//! environment variable, or feature flag can add a transition.
//! `StateMachine::step` (M012) calls [`StatusTransitions::is_allowed`] before
//! any state change; a `false` return causes `TransitionError::Authority`.
//!
//! Cross-reference: `ai_specs/modules/c02-authority-state/M013_STATUS_TRANSITIONS.md`
//! Depends on: M011 `WorkflowState` (hle-core)

pub use hle_core::state::workflow_state::WorkflowState;

// ---------------------------------------------------------------------------
// TransitionRule
// ---------------------------------------------------------------------------

/// One allowed `(from, to)` state transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransitionRule {
    /// Source state.
    pub from: WorkflowState,
    /// Target state.
    pub to: WorkflowState,
}

impl TransitionRule {
    /// Construct a transition rule.
    #[must_use]
    pub const fn new(from: WorkflowState, to: WorkflowState) -> Self {
        Self { from, to }
    }
}

// ---------------------------------------------------------------------------
// RollbackRule
// ---------------------------------------------------------------------------

/// A mapping from a state that can be rolled back to its rollback target.
///
/// Used by rollback orchestration layers (C06 runbook semantics) without
/// requiring them to parse the full transition table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RollbackRule {
    /// The state that can be rolled back.
    pub from: WorkflowState,
    /// The rollback target state (always `RolledBack` in this table).
    pub target: WorkflowState,
}

// ---------------------------------------------------------------------------
// Static transition table
// ---------------------------------------------------------------------------

/// The complete set of allowed forward transitions.
///
/// Any `(from, to)` pair NOT in this array is rejected by
/// [`StatusTransitions::is_allowed`].  The table is a `const` array evaluated
/// at compile time — adding or removing a transition requires a source-level
/// edit, not a runtime configuration change.
pub const ALLOWED_TRANSITIONS: [TransitionRule; 9] = [
    // Pending → Running: executor claims the step.
    TransitionRule::new(WorkflowState::Pending, WorkflowState::Running),
    // Pending → Failed: pre-execution cancellation / precondition failure.
    TransitionRule::new(WorkflowState::Pending, WorkflowState::Failed),
    // Running → Passed: happy path; verifier must confirm.
    TransitionRule::new(WorkflowState::Running, WorkflowState::Passed),
    // Running → Failed: execution error observed.
    TransitionRule::new(WorkflowState::Running, WorkflowState::Failed),
    // Running → AwaitingHuman: step needs human decision to proceed.
    TransitionRule::new(WorkflowState::Running, WorkflowState::AwaitingHuman),
    // Running → RolledBack: mid-run abort triggers immediate rollback.
    TransitionRule::new(WorkflowState::Running, WorkflowState::RolledBack),
    // AwaitingHuman → Running: human unblocked the gate; executor resumes.
    TransitionRule::new(WorkflowState::AwaitingHuman, WorkflowState::Running),
    // AwaitingHuman → Failed: human rejected or timed out.
    TransitionRule::new(WorkflowState::AwaitingHuman, WorkflowState::Failed),
    // Failed → RolledBack: post-failure compensating rollback.
    TransitionRule::new(WorkflowState::Failed, WorkflowState::RolledBack),
];

/// All defined rollback transitions (a subset of [`ALLOWED_TRANSITIONS`]).
pub const ROLLBACK_RULES: [RollbackRule; 2] = [
    RollbackRule {
        from: WorkflowState::Running,
        target: WorkflowState::RolledBack,
    },
    RollbackRule {
        from: WorkflowState::Failed,
        target: WorkflowState::RolledBack,
    },
];

// ---------------------------------------------------------------------------
// StatusTransitions
// ---------------------------------------------------------------------------

/// Namespace for static transition table operations.
///
/// All methods are `const fn` and operate on the compile-time
/// [`ALLOWED_TRANSITIONS`] array.
pub struct StatusTransitions;

impl StatusTransitions {
    /// Check whether a transition from `from` to `to` is in the allowed table.
    ///
    /// Uses a `while` loop (not `for`) to satisfy `const fn` restrictions on
    /// stable Rust.  The 9-element scan is O(9) — no heap allocation.
    ///
    /// # Examples
    ///
    /// ```
    /// use hle_executor::status_transitions::{StatusTransitions, WorkflowState};
    /// assert!(StatusTransitions::is_allowed(WorkflowState::Pending, WorkflowState::Running));
    /// assert!(!StatusTransitions::is_allowed(WorkflowState::Passed, WorkflowState::Running));
    /// ```
    #[must_use]
    pub const fn is_allowed(from: WorkflowState, to: WorkflowState) -> bool {
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

    /// Return the rollback target for a given state, if one is defined.
    ///
    /// Returns `Some(WorkflowState::RolledBack)` for states that have a defined
    /// rollback transition in [`ALLOWED_TRANSITIONS`].
    /// Returns `None` for terminal states and for `Pending` (nothing to roll back).
    ///
    /// # Examples
    ///
    /// ```
    /// use hle_executor::status_transitions::{StatusTransitions, WorkflowState};
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
            WorkflowState::Running | WorkflowState::Failed => Some(WorkflowState::RolledBack),
            WorkflowState::Pending
            | WorkflowState::AwaitingHuman
            | WorkflowState::Passed
            | WorkflowState::RolledBack => None,
        }
    }

    /// Return all states that `from` can legally transition to.
    ///
    /// The return type is `[Option<WorkflowState>; 4]` padded with `None`.
    /// Maximum fanout from any single state is 4 (`Running` has 4 successors).
    /// This is `const fn` with zero heap allocation.
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
    ///
    /// Same fixed-size `[Option<WorkflowState>; 4]` return with `const fn`
    /// semantics and zero allocation.
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

// WorkflowState is imported at the top of this module; callers use the canonical
// hle_core path directly. Re-export removed to avoid E0252 duplicate-name error.

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{
        RollbackRule, StatusTransitions, TransitionRule, WorkflowState, ALLOWED_TRANSITIONS,
        ROLLBACK_RULES,
    };
    use hle_core::state::workflow_state::WorkflowState as WS;

    const ALL_STATES: [WS; 6] = [
        WS::Pending,
        WS::Running,
        WS::AwaitingHuman,
        WS::Passed,
        WS::Failed,
        WS::RolledBack,
    ];

    // ---------------------------------------------------------------------------
    // is_allowed — every allowed pair from the spec table (9 pairs)
    // ---------------------------------------------------------------------------

    #[test]
    fn pending_to_running_is_allowed() {
        assert!(StatusTransitions::is_allowed(WS::Pending, WS::Running));
    }

    #[test]
    fn pending_to_failed_is_allowed() {
        assert!(StatusTransitions::is_allowed(WS::Pending, WS::Failed));
    }

    #[test]
    fn running_to_passed_is_allowed() {
        assert!(StatusTransitions::is_allowed(WS::Running, WS::Passed));
    }

    #[test]
    fn running_to_failed_is_allowed() {
        assert!(StatusTransitions::is_allowed(WS::Running, WS::Failed));
    }

    #[test]
    fn running_to_awaiting_human_is_allowed() {
        assert!(StatusTransitions::is_allowed(
            WS::Running,
            WS::AwaitingHuman
        ));
    }

    #[test]
    fn running_to_rolled_back_is_allowed() {
        assert!(StatusTransitions::is_allowed(WS::Running, WS::RolledBack));
    }

    #[test]
    fn awaiting_human_to_running_is_allowed() {
        assert!(StatusTransitions::is_allowed(
            WS::AwaitingHuman,
            WS::Running
        ));
    }

    #[test]
    fn awaiting_human_to_failed_is_allowed() {
        assert!(StatusTransitions::is_allowed(WS::AwaitingHuman, WS::Failed));
    }

    #[test]
    fn failed_to_rolled_back_is_allowed() {
        assert!(StatusTransitions::is_allowed(WS::Failed, WS::RolledBack));
    }

    // ---------------------------------------------------------------------------
    // is_allowed — forbidden pairs (spot check key misuses)
    // ---------------------------------------------------------------------------

    #[test]
    fn passed_to_running_is_not_allowed() {
        assert!(!StatusTransitions::is_allowed(WS::Passed, WS::Running));
    }

    #[test]
    fn passed_cannot_transition_to_any_state() {
        for target in ALL_STATES {
            assert!(
                !StatusTransitions::is_allowed(WS::Passed, target),
                "Passed should not transition to {target}"
            );
        }
    }

    #[test]
    fn rolled_back_cannot_transition_to_any_state() {
        for target in ALL_STATES {
            assert!(
                !StatusTransitions::is_allowed(WS::RolledBack, target),
                "RolledBack should not transition to {target}"
            );
        }
    }

    #[test]
    fn pending_to_passed_is_not_allowed() {
        assert!(!StatusTransitions::is_allowed(WS::Pending, WS::Passed));
    }

    #[test]
    fn pending_to_awaiting_human_is_not_allowed() {
        assert!(!StatusTransitions::is_allowed(
            WS::Pending,
            WS::AwaitingHuman
        ));
    }

    #[test]
    fn pending_to_rolled_back_is_not_allowed() {
        assert!(!StatusTransitions::is_allowed(WS::Pending, WS::RolledBack));
    }

    #[test]
    fn failed_to_running_is_not_allowed() {
        assert!(!StatusTransitions::is_allowed(WS::Failed, WS::Running));
    }

    #[test]
    fn failed_to_passed_is_not_allowed() {
        assert!(!StatusTransitions::is_allowed(WS::Failed, WS::Passed));
    }

    #[test]
    fn awaiting_human_to_passed_is_not_allowed() {
        assert!(!StatusTransitions::is_allowed(
            WS::AwaitingHuman,
            WS::Passed
        ));
    }

    #[test]
    fn awaiting_human_to_rolled_back_is_not_allowed() {
        assert!(!StatusTransitions::is_allowed(
            WS::AwaitingHuman,
            WS::RolledBack
        ));
    }

    #[test]
    fn no_self_transitions_allowed() {
        for state in ALL_STATES {
            assert!(
                !StatusTransitions::is_allowed(state, state),
                "self-transition should be forbidden for {state}"
            );
        }
    }

    #[test]
    fn total_allowed_count_is_nine() {
        let count = ALL_STATES
            .iter()
            .flat_map(|&from| ALL_STATES.iter().map(move |&to| (from, to)))
            .filter(|&(from, to)| StatusTransitions::is_allowed(from, to))
            .count();
        assert_eq!(count, 9);
    }

    // ---------------------------------------------------------------------------
    // rollback_target — exhaustive
    // ---------------------------------------------------------------------------

    #[test]
    fn running_has_rollback_target() {
        assert_eq!(
            StatusTransitions::rollback_target(WS::Running),
            Some(WS::RolledBack)
        );
    }

    #[test]
    fn failed_has_rollback_target() {
        assert_eq!(
            StatusTransitions::rollback_target(WS::Failed),
            Some(WS::RolledBack)
        );
    }

    #[test]
    fn passed_has_no_rollback_target() {
        assert_eq!(StatusTransitions::rollback_target(WS::Passed), None);
    }

    #[test]
    fn pending_has_no_rollback_target() {
        assert_eq!(StatusTransitions::rollback_target(WS::Pending), None);
    }

    #[test]
    fn awaiting_human_has_no_rollback_target() {
        assert_eq!(StatusTransitions::rollback_target(WS::AwaitingHuman), None);
    }

    #[test]
    fn rolled_back_has_no_rollback_target() {
        assert_eq!(StatusTransitions::rollback_target(WS::RolledBack), None);
    }

    #[test]
    fn rollback_targets_match_rollback_rules() {
        for rule in ROLLBACK_RULES {
            assert_eq!(
                StatusTransitions::rollback_target(rule.from),
                Some(rule.target),
                "rollback_target({:?}) should be Some({:?})",
                rule.from,
                rule.target
            );
        }
    }

    // ---------------------------------------------------------------------------
    // successors — exhaustive counts
    // ---------------------------------------------------------------------------

    #[test]
    fn running_has_four_successors() {
        let s = StatusTransitions::successors(WS::Running);
        let count = s.iter().filter(|x| x.is_some()).count();
        assert_eq!(count, 4);
    }

    #[test]
    fn pending_has_two_successors() {
        let s = StatusTransitions::successors(WS::Pending);
        let count = s.iter().filter(|x| x.is_some()).count();
        assert_eq!(count, 2);
    }

    #[test]
    fn awaiting_human_has_two_successors() {
        let s = StatusTransitions::successors(WS::AwaitingHuman);
        let count = s.iter().filter(|x| x.is_some()).count();
        assert_eq!(count, 2);
    }

    #[test]
    fn failed_has_one_successor() {
        let s = StatusTransitions::successors(WS::Failed);
        let count = s.iter().filter(|x| x.is_some()).count();
        assert_eq!(count, 1);
    }

    #[test]
    fn passed_has_no_successors() {
        let s = StatusTransitions::successors(WS::Passed);
        assert!(s.iter().all(|x| x.is_none()));
    }

    #[test]
    fn rolled_back_has_no_successors() {
        let s = StatusTransitions::successors(WS::RolledBack);
        assert!(s.iter().all(|x| x.is_none()));
    }

    #[test]
    fn running_successors_contain_passed() {
        let s = StatusTransitions::successors(WS::Running);
        assert!(s.contains(&Some(WS::Passed)));
    }

    #[test]
    fn running_successors_contain_failed() {
        let s = StatusTransitions::successors(WS::Running);
        assert!(s.contains(&Some(WS::Failed)));
    }

    // ---------------------------------------------------------------------------
    // predecessors — exhaustive counts
    // ---------------------------------------------------------------------------

    #[test]
    fn rolled_back_has_multiple_predecessors() {
        let p = StatusTransitions::predecessors(WS::RolledBack);
        let count = p.iter().filter(|x| x.is_some()).count();
        assert!(count >= 2, "RolledBack should have at least 2 predecessors");
    }

    #[test]
    fn running_has_two_predecessors() {
        let p = StatusTransitions::predecessors(WS::Running);
        let count = p.iter().filter(|x| x.is_some()).count();
        assert_eq!(count, 2); // Pending and AwaitingHuman
    }

    #[test]
    fn passed_has_one_predecessor() {
        let p = StatusTransitions::predecessors(WS::Passed);
        let count = p.iter().filter(|x| x.is_some()).count();
        assert_eq!(count, 1); // Running
    }

    #[test]
    fn pending_has_no_predecessors() {
        let p = StatusTransitions::predecessors(WS::Pending);
        assert!(p.iter().all(|x| x.is_none()));
    }

    // ---------------------------------------------------------------------------
    // Table structural invariants
    // ---------------------------------------------------------------------------

    #[test]
    fn all_allowed_transitions_are_consistent_with_is_allowed() {
        for rule in ALLOWED_TRANSITIONS {
            assert!(
                StatusTransitions::is_allowed(rule.from, rule.to),
                "ALLOWED_TRANSITIONS entry {rule:?} not found by is_allowed"
            );
        }
    }

    #[test]
    fn rollback_rules_are_a_subset_of_allowed_transitions() {
        for rule in ROLLBACK_RULES {
            assert!(
                StatusTransitions::is_allowed(rule.from, rule.target),
                "ROLLBACK_RULES entry {rule:?} is not in ALLOWED_TRANSITIONS"
            );
        }
    }

    #[test]
    fn no_self_transitions_in_table() {
        for rule in ALLOWED_TRANSITIONS {
            assert_ne!(
                rule.from, rule.to,
                "self-transition {rule:?} found in ALLOWED_TRANSITIONS"
            );
        }
    }

    #[test]
    fn allowed_transitions_table_has_nine_entries() {
        assert_eq!(ALLOWED_TRANSITIONS.len(), 9);
    }

    #[test]
    fn rollback_rules_table_has_two_entries() {
        assert_eq!(ROLLBACK_RULES.len(), 2);
    }

    #[test]
    fn all_table_entries_are_unique_pairs() {
        for i in 0..ALLOWED_TRANSITIONS.len() {
            for j in (i + 1)..ALLOWED_TRANSITIONS.len() {
                let a = ALLOWED_TRANSITIONS[i];
                let b = ALLOWED_TRANSITIONS[j];
                assert!(
                    !(a.from == b.from && a.to == b.to),
                    "duplicate entry ({:?},{:?}) at positions {i} and {j}",
                    a.from,
                    a.to
                );
            }
        }
    }

    #[test]
    fn successors_count_matches_is_allowed_count() {
        for state in ALL_STATES {
            let successor_count = StatusTransitions::successors(state)
                .iter()
                .filter(|x| x.is_some())
                .count();
            let is_allowed_count = ALL_STATES
                .iter()
                .filter(|&&t| StatusTransitions::is_allowed(state, t))
                .count();
            assert_eq!(
                successor_count, is_allowed_count,
                "successor count mismatch for {state}"
            );
        }
    }

    #[test]
    fn predecessors_count_matches_is_allowed_reverse() {
        for state in ALL_STATES {
            let pred_count = StatusTransitions::predecessors(state)
                .iter()
                .filter(|x| x.is_some())
                .count();
            let is_allowed_count = ALL_STATES
                .iter()
                .filter(|&&f| StatusTransitions::is_allowed(f, state))
                .count();
            assert_eq!(
                pred_count, is_allowed_count,
                "predecessor count mismatch for {state}"
            );
        }
    }

    // ---------------------------------------------------------------------------
    // TransitionRule / RollbackRule types
    // ---------------------------------------------------------------------------

    #[test]
    fn transition_rule_new_stores_from_and_to() {
        let r = TransitionRule::new(WS::Pending, WS::Running);
        assert_eq!(r.from, WS::Pending);
        assert_eq!(r.to, WS::Running);
    }

    #[test]
    fn transition_rule_eq_reflexive() {
        let r = TransitionRule::new(WS::Pending, WS::Running);
        assert_eq!(r, r);
    }

    #[test]
    fn rollback_rule_stores_from_and_target() {
        let r = RollbackRule {
            from: WS::Failed,
            target: WS::RolledBack,
        };
        assert_eq!(r.from, WS::Failed);
        assert_eq!(r.target, WS::RolledBack);
    }

    // Use WorkflowState alias to silence the import
    #[allow(unused_imports)]
    #[test]
    fn workflow_state_re_export_is_same_type() {
        // WorkflowState imported at top of module is the same as hle_core's type.
        let _: WorkflowState = WS::Pending;
    }
}
