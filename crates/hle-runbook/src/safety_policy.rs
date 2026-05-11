#![forbid(unsafe_code)]

//! M039 — Safety gate enforcement for C06 Runbook Semantics.
//!
//! **Cluster:** C06 Runbook Semantics | **Layer:** L07 | **Error codes:** 2590-2595
//!
//! [`SafetyPolicy`] is the single authority that decides whether a runbook
//! phase transition is permitted.  It encodes four rules in strict precedence
//! order:
//!
//! 1. **`TraversalGuard`** — reject when `max_traversals` exceeded.
//! 2. **`ElevationDenied`** — reject when `Safety`-class runbook is not elevated.
//! 3. **`PolicyViolation`** — reject when the current mode doesn't permit the phase.
//! 4. **Pass** — all rules satisfied; emit `SafetyCheckResult::Pass`.
//!
//! The policy is checked synchronously and has no I/O.

use std::fmt;

use crate::schema::{OperationalMode, PhaseKind, Runbook, SafetyClass};

// ── SafetyViolation ───────────────────────────────────────────────────────────

/// Describes the specific reason a safety check failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SafetyViolation {
    /// Code 2591 — A policy rule blocked the transition.
    PolicyViolation {
        /// Human-readable reason.
        reason: String,
    },
    /// Code 2592 — Phase traversal count exceeded `max_traversals`.
    TraversalExceeded {
        /// Current traversal count.
        count: u32,
        /// Configured maximum.
        max: u32,
    },
    /// Code 2593 — `Safety`-class runbook attempted without authority elevation.
    ElevationDenied {
        /// The safety class that required elevation.
        class: SafetyClass,
    },
}

impl SafetyViolation {
    /// Numeric error code.
    #[must_use]
    pub const fn error_code(&self) -> u16 {
        match self {
            Self::PolicyViolation { .. } => 2591,
            Self::TraversalExceeded { .. } => 2592,
            Self::ElevationDenied { .. } => 2593,
        }
    }
}

impl fmt::Display for SafetyViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PolicyViolation { reason } => {
                write!(f, "[2591 PolicyViolation] {reason}")
            }
            Self::TraversalExceeded { count, max } => {
                write!(
                    f,
                    "[2592 TraversalExceeded] traversal count {count} exceeds max {max}"
                )
            }
            Self::ElevationDenied { class } => {
                write!(
                    f,
                    "[2593 ElevationDenied] class '{}' requires authority elevation",
                    class.as_str()
                )
            }
        }
    }
}

impl std::error::Error for SafetyViolation {}

// ── SafetyCheckResult ─────────────────────────────────────────────────────────

/// The outcome of a safety check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SafetyCheckResult {
    /// All safety rules passed — the phase transition is permitted.
    Pass,
    /// One or more rules blocked the transition.
    Blocked(SafetyViolation),
}

impl SafetyCheckResult {
    /// Return `true` when the result is `Pass`.
    #[must_use]
    pub const fn is_pass(&self) -> bool {
        matches!(self, Self::Pass)
    }

    /// Return `true` when the result is `Blocked`.
    #[must_use]
    pub const fn is_blocked(&self) -> bool {
        matches!(self, Self::Blocked(_))
    }

    /// Unwrap the violation, panicking when the result is `Pass`.
    ///
    /// Only use in tests.
    #[cfg(test)]
    pub fn violation(&self) -> &SafetyViolation {
        match self {
            Self::Blocked(v) => v,
            Self::Pass => panic!("SafetyCheckResult::Pass has no violation"),
        }
    }
}

impl fmt::Display for SafetyCheckResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pass => f.write_str("pass"),
            Self::Blocked(v) => write!(f, "blocked({v})"),
        }
    }
}

// ── PolicyConfig ──────────────────────────────────────────────────────────────

/// Configurable knobs for a [`SafetyPolicy`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyConfig {
    /// Maximum number of times any single phase may be traversed within one
    /// runbook execution.  Default: 3.
    pub max_traversals: u32,
    /// Current operational mode.  The policy checks runbook
    /// `ModeApplicability` against this value.
    pub current_mode: OperationalMode,
    /// Whether the requesting agent has been granted authority elevation.
    /// Required for `Safety`-class runbooks and `Hard`-class in Production mode.
    pub elevated: bool,
    /// When `true`, `Soft`-class runbooks skip human-confirmation gating.
    pub allow_soft_autorun: bool,
}

impl PolicyConfig {
    /// Return a permissive config suited to unit tests.
    #[must_use]
    pub fn test_permissive() -> Self {
        Self {
            max_traversals: 10,
            current_mode: OperationalMode::LocalM0,
            elevated: true,
            allow_soft_autorun: true,
        }
    }

    /// Return the default local-M0 config (non-elevated, max 3 traversals).
    #[must_use]
    pub fn local_m0() -> Self {
        Self {
            max_traversals: 3,
            current_mode: OperationalMode::LocalM0,
            elevated: false,
            allow_soft_autorun: false,
        }
    }
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self::local_m0()
    }
}

// ── TraversalGuard ────────────────────────────────────────────────────────────

/// Per-phase traversal counter.
///
/// Tracks how many times each phase has been entered.  The guard is checked
/// at the top of the rule stack (Rule 1) before any other rule.
#[derive(Debug, Clone, Default)]
pub struct TraversalGuard {
    counts: std::collections::HashMap<PhaseKind, u32>,
}

impl TraversalGuard {
    /// Create an empty guard.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the traversal count for `phase`.
    #[must_use]
    pub fn count(&self, phase: PhaseKind) -> u32 {
        *self.counts.get(&phase).unwrap_or(&0)
    }

    /// Record one traversal of `phase` and return the new count.
    pub fn record(&mut self, phase: PhaseKind) -> u32 {
        let entry = self.counts.entry(phase).or_insert(0);
        *entry += 1;
        *entry
    }

    /// Return `true` when `phase` count is less than `max_traversals`.
    #[must_use]
    pub fn within_limit(&self, phase: PhaseKind, max: u32) -> bool {
        self.count(phase) < max
    }

    /// Reset the counter for a single phase.
    pub fn reset_phase(&mut self, phase: PhaseKind) {
        self.counts.remove(&phase);
    }

    /// Reset all counters.
    pub fn reset_all(&mut self) {
        self.counts.clear();
    }
}

// ── SafetyPolicy ──────────────────────────────────────────────────────────────

/// Enforces Framework §17.8 safety rules for phase transitions.
///
/// Rules are evaluated in this strict precedence order:
///
/// 1. `TraversalGuard` (Rule 1 — traversal limit)
/// 2. `ElevationDenied` (Rule 2 — Safety-class elevation)
/// 3. `PolicyViolation` (Rule 3 — mode applicability + phase constraints)
/// 4. Pass
#[derive(Debug, Clone)]
pub struct SafetyPolicy {
    /// Policy configuration.
    config: PolicyConfig,
    /// Per-phase traversal tracking.
    guard: TraversalGuard,
}

impl SafetyPolicy {
    /// Create a policy with the given config and a fresh traversal guard.
    #[must_use]
    pub fn new(config: PolicyConfig) -> Self {
        Self {
            config,
            guard: TraversalGuard::new(),
        }
    }

    /// Create a permissive policy for tests.
    #[must_use]
    pub fn test_permissive() -> Self {
        Self::new(PolicyConfig::test_permissive())
    }

    /// Create a default local-M0 policy.
    #[must_use]
    pub fn local_m0() -> Self {
        Self::new(PolicyConfig::local_m0())
    }

    /// Return a shared reference to the current config.
    #[must_use]
    pub fn config(&self) -> &PolicyConfig {
        &self.config
    }

    /// Return the current traversal count for `phase`.
    #[must_use]
    pub fn traversal_count(&self, phase: PhaseKind) -> u32 {
        self.guard.count(phase)
    }

    /// Record a traversal of `phase` without applying safety checks.
    ///
    /// Call this after a successful `check()` to increment the counter.
    pub fn record_traversal(&mut self, phase: PhaseKind) {
        self.guard.record(phase);
    }

    /// Check whether the transition to `phase` is permitted for `runbook`.
    ///
    /// The four rules are applied in precedence order.  The first rule that
    /// fires returns a `Blocked` result; all rules passing returns `Pass`.
    ///
    /// This method is **read-only** — it does not record the traversal.
    /// Call [`SafetyPolicy::record_traversal`] after a successful check.
    #[must_use]
    pub fn check(&self, runbook: &Runbook, phase: PhaseKind) -> SafetyCheckResult {
        // Rule 1 — Traversal limit.
        let current_count = self.guard.count(phase);
        if current_count >= self.config.max_traversals {
            return SafetyCheckResult::Blocked(SafetyViolation::TraversalExceeded {
                count: current_count,
                max: self.config.max_traversals,
            });
        }

        // Rule 2 — Authority elevation for Safety-class runbooks.
        if runbook.safety_class == SafetyClass::Safety && !self.config.elevated {
            return SafetyCheckResult::Blocked(SafetyViolation::ElevationDenied {
                class: SafetyClass::Safety,
            });
        }

        // Rule 2b — Hard-class in Production mode also requires elevation.
        if runbook.safety_class == SafetyClass::Hard
            && self.config.current_mode == OperationalMode::Production
            && !self.config.elevated
        {
            return SafetyCheckResult::Blocked(SafetyViolation::ElevationDenied {
                class: SafetyClass::Hard,
            });
        }

        // Rule 3 — Mode applicability.
        if !runbook
            .mode_applicability
            .applies_in(&self.config.current_mode)
        {
            let mode_str = match self.config.current_mode {
                OperationalMode::Scaffold => "scaffold",
                OperationalMode::LocalM0 => "local_m0",
                OperationalMode::Production => "production",
            };
            return SafetyCheckResult::Blocked(SafetyViolation::PolicyViolation {
                reason: format!(
                    "runbook '{}' does not apply in mode '{}'",
                    runbook.id.as_str(),
                    mode_str,
                ),
            });
        }

        // Rule 3b — Fix phase on Soft-class only permitted when `allow_soft_autorun`.
        if phase == PhaseKind::Fix
            && runbook.safety_class == SafetyClass::Soft
            && !self.config.allow_soft_autorun
        {
            return SafetyCheckResult::Blocked(SafetyViolation::PolicyViolation {
                reason: format!(
                    "Fix phase on Soft-class runbook '{}' requires allow_soft_autorun=true",
                    runbook.id.as_str()
                ),
            });
        }

        // All rules passed.
        SafetyCheckResult::Pass
    }

    /// Convenience: check and, if `Pass`, record the traversal.
    ///
    /// Returns the check result.  If `Pass`, the traversal counter for
    /// `phase` is incremented before returning.
    pub fn check_and_record(&mut self, runbook: &Runbook, phase: PhaseKind) -> SafetyCheckResult {
        let result = self.check(runbook, phase);
        if result.is_pass() {
            self.guard.record(phase);
        }
        result
    }

    /// Reset the traversal guard (all phase counts to zero).
    pub fn reset_traversals(&mut self) {
        self.guard.reset_all();
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{PolicyConfig, SafetyCheckResult, SafetyPolicy, SafetyViolation, TraversalGuard};
    use crate::schema::{
        ModeApplicability, OperationalMode, Phase, PhaseKind, Runbook, RunbookBuilder, SafetyClass,
    };

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn build_runbook(id: &str, class: SafetyClass, mode_app: ModeApplicability) -> Runbook {
        RunbookBuilder::new(id, "Test Runbook")
            .safety_class(class)
            .mode_applicability(mode_app)
            .add_phase(PhaseKind::Detect, Phase::default())
            .build()
            .expect("valid runbook")
    }

    fn hard_runbook_all_modes() -> Runbook {
        build_runbook("rb-hard", SafetyClass::Hard, ModeApplicability::all())
    }

    fn soft_runbook_all_modes() -> Runbook {
        build_runbook("rb-soft", SafetyClass::Soft, ModeApplicability::all())
    }

    fn safety_runbook_all_modes() -> Runbook {
        build_runbook("rb-safety", SafetyClass::Safety, ModeApplicability::all())
    }

    fn scaffold_only_runbook() -> Runbook {
        build_runbook(
            "rb-scaffold",
            SafetyClass::Soft,
            ModeApplicability::scaffold_only(),
        )
    }

    // ── TraversalGuard ────────────────────────────────────────────────────────

    #[test]
    fn traversal_guard_starts_at_zero() {
        let g = TraversalGuard::new();
        assert_eq!(g.count(PhaseKind::Detect), 0);
    }

    #[test]
    fn traversal_guard_increments() {
        let mut g = TraversalGuard::new();
        assert_eq!(g.record(PhaseKind::Detect), 1);
        assert_eq!(g.record(PhaseKind::Detect), 2);
        assert_eq!(g.count(PhaseKind::Detect), 2);
    }

    #[test]
    fn traversal_guard_within_limit_true_when_below() {
        let mut g = TraversalGuard::new();
        g.record(PhaseKind::Detect);
        assert!(g.within_limit(PhaseKind::Detect, 3));
    }

    #[test]
    fn traversal_guard_within_limit_false_at_max() {
        let mut g = TraversalGuard::new();
        g.record(PhaseKind::Detect);
        g.record(PhaseKind::Detect);
        g.record(PhaseKind::Detect);
        assert!(!g.within_limit(PhaseKind::Detect, 3));
    }

    #[test]
    fn traversal_guard_reset_phase() {
        let mut g = TraversalGuard::new();
        g.record(PhaseKind::Detect);
        g.record(PhaseKind::Fix);
        g.reset_phase(PhaseKind::Detect);
        assert_eq!(g.count(PhaseKind::Detect), 0);
        assert_eq!(g.count(PhaseKind::Fix), 1);
    }

    #[test]
    fn traversal_guard_reset_all() {
        let mut g = TraversalGuard::new();
        g.record(PhaseKind::Detect);
        g.record(PhaseKind::Fix);
        g.reset_all();
        assert_eq!(g.count(PhaseKind::Detect), 0);
        assert_eq!(g.count(PhaseKind::Fix), 0);
    }

    // ── PolicyConfig ──────────────────────────────────────────────────────────

    #[test]
    fn test_permissive_config_is_elevated() {
        assert!(PolicyConfig::test_permissive().elevated);
    }

    #[test]
    fn local_m0_config_is_not_elevated() {
        assert!(!PolicyConfig::local_m0().elevated);
    }

    #[test]
    fn local_m0_config_has_three_max_traversals() {
        assert_eq!(PolicyConfig::local_m0().max_traversals, 3);
    }

    // ── SafetyPolicy — Rule 1: Traversal Exceeded ──────────────────────────

    #[test]
    fn traversal_exceeded_blocks_at_max() {
        let rb = hard_runbook_all_modes();
        let mut policy = SafetyPolicy::test_permissive();
        // Set max to 2 for this test.
        policy.config.max_traversals = 2;
        policy.record_traversal(PhaseKind::Detect);
        policy.record_traversal(PhaseKind::Detect);
        let result = policy.check(&rb, PhaseKind::Detect);
        assert!(result.is_blocked());
        assert_eq!(result.violation().error_code(), 2592);
        assert!(matches!(
            result.violation(),
            SafetyViolation::TraversalExceeded { count: 2, max: 2 }
        ));
    }

    #[test]
    fn traversal_below_max_passes() {
        let rb = hard_runbook_all_modes();
        let mut policy = SafetyPolicy::test_permissive();
        policy.record_traversal(PhaseKind::Detect);
        let result = policy.check(&rb, PhaseKind::Detect);
        assert!(result.is_pass());
    }

    // ── SafetyPolicy — Rule 2: Elevation Denied ────────────────────────────

    #[test]
    fn safety_class_without_elevation_blocked() {
        let rb = safety_runbook_all_modes();
        let mut config = PolicyConfig::test_permissive();
        config.elevated = false;
        let policy = SafetyPolicy::new(config);
        let result = policy.check(&rb, PhaseKind::Detect);
        assert!(result.is_blocked());
        assert_eq!(result.violation().error_code(), 2593);
        assert!(matches!(
            result.violation(),
            SafetyViolation::ElevationDenied {
                class: SafetyClass::Safety
            }
        ));
    }

    #[test]
    fn safety_class_with_elevation_passes() {
        let rb = safety_runbook_all_modes();
        let policy = SafetyPolicy::test_permissive(); // elevated=true
        let result = policy.check(&rb, PhaseKind::Detect);
        assert!(result.is_pass());
    }

    #[test]
    fn hard_class_in_production_without_elevation_blocked() {
        let rb = hard_runbook_all_modes();
        let config = PolicyConfig {
            max_traversals: 10,
            current_mode: OperationalMode::Production,
            elevated: false,
            allow_soft_autorun: true,
        };
        let policy = SafetyPolicy::new(config);
        let result = policy.check(&rb, PhaseKind::Detect);
        assert!(result.is_blocked());
        assert_eq!(result.violation().error_code(), 2593);
        assert!(matches!(
            result.violation(),
            SafetyViolation::ElevationDenied {
                class: SafetyClass::Hard
            }
        ));
    }

    #[test]
    fn hard_class_in_local_m0_without_elevation_passes() {
        let rb = hard_runbook_all_modes();
        let policy = SafetyPolicy::local_m0(); // elevated=false, LocalM0
        let result = policy.check(&rb, PhaseKind::Detect);
        assert!(result.is_pass());
    }

    #[test]
    fn soft_class_without_elevation_passes() {
        let rb = soft_runbook_all_modes();
        let mut config = PolicyConfig::test_permissive();
        config.elevated = false;
        let policy = SafetyPolicy::new(config);
        let result = policy.check(&rb, PhaseKind::Detect);
        assert!(result.is_pass());
    }

    // ── SafetyPolicy — Rule 3: Mode Applicability ──────────────────────────

    #[test]
    fn scaffold_only_runbook_blocked_in_local_m0() {
        let rb = scaffold_only_runbook();
        let policy = SafetyPolicy::local_m0();
        let result = policy.check(&rb, PhaseKind::Detect);
        assert!(result.is_blocked());
        assert_eq!(result.violation().error_code(), 2591);
        assert!(matches!(
            result.violation(),
            SafetyViolation::PolicyViolation { .. }
        ));
    }

    #[test]
    fn scaffold_only_runbook_allowed_in_scaffold_mode() {
        let rb = scaffold_only_runbook();
        let config = PolicyConfig {
            max_traversals: 10,
            current_mode: OperationalMode::Scaffold,
            elevated: false,
            allow_soft_autorun: true,
        };
        let policy = SafetyPolicy::new(config);
        let result = policy.check(&rb, PhaseKind::Detect);
        assert!(result.is_pass());
    }

    #[test]
    fn mode_violation_message_contains_mode_and_id() {
        let rb = scaffold_only_runbook();
        let policy = SafetyPolicy::local_m0();
        let result = policy.check(&rb, PhaseKind::Detect);
        let v = result.violation();
        let msg = v.to_string();
        assert!(
            msg.contains("rb-scaffold"),
            "message should contain runbook id"
        );
        assert!(msg.contains("local_m0"), "message should contain mode name");
    }

    // ── SafetyPolicy — Rule 3b: Soft Fix without autorun ──────────────────

    #[test]
    fn soft_fix_without_allow_autorun_blocked() {
        let rb = soft_runbook_all_modes();
        let config = PolicyConfig {
            max_traversals: 10,
            current_mode: OperationalMode::LocalM0,
            elevated: false,
            allow_soft_autorun: false,
        };
        let policy = SafetyPolicy::new(config);
        let result = policy.check(&rb, PhaseKind::Fix);
        assert!(result.is_blocked());
        assert_eq!(result.violation().error_code(), 2591);
    }

    #[test]
    fn soft_fix_with_allow_autorun_passes() {
        let rb = soft_runbook_all_modes();
        let policy = SafetyPolicy::test_permissive(); // allow_soft_autorun=true
        let result = policy.check(&rb, PhaseKind::Fix);
        assert!(result.is_pass());
    }

    #[test]
    fn hard_fix_without_autorun_still_passes_rule3b() {
        // Rule 3b only applies to Soft-class; Hard-class is not gated by allow_soft_autorun.
        let rb = hard_runbook_all_modes();
        let config = PolicyConfig {
            max_traversals: 10,
            current_mode: OperationalMode::LocalM0,
            elevated: false,
            allow_soft_autorun: false,
        };
        let policy = SafetyPolicy::new(config);
        let result = policy.check(&rb, PhaseKind::Fix);
        assert!(result.is_pass());
    }

    // ── SafetyPolicy — check_and_record ───────────────────────────────────

    #[test]
    fn check_and_record_increments_counter_on_pass() {
        let rb = hard_runbook_all_modes();
        let mut policy = SafetyPolicy::test_permissive();
        assert_eq!(policy.traversal_count(PhaseKind::Detect), 0);
        let result = policy.check_and_record(&rb, PhaseKind::Detect);
        assert!(result.is_pass());
        assert_eq!(policy.traversal_count(PhaseKind::Detect), 1);
    }

    #[test]
    fn check_and_record_does_not_increment_on_block() {
        let rb = scaffold_only_runbook();
        let mut policy = SafetyPolicy::local_m0();
        assert_eq!(policy.traversal_count(PhaseKind::Detect), 0);
        let result = policy.check_and_record(&rb, PhaseKind::Detect);
        assert!(result.is_blocked());
        assert_eq!(policy.traversal_count(PhaseKind::Detect), 0);
    }

    #[test]
    fn reset_traversals_clears_counts() {
        let rb = hard_runbook_all_modes();
        let mut policy = SafetyPolicy::test_permissive();
        policy.check_and_record(&rb, PhaseKind::Detect);
        policy.check_and_record(&rb, PhaseKind::Fix);
        policy.reset_traversals();
        assert_eq!(policy.traversal_count(PhaseKind::Detect), 0);
        assert_eq!(policy.traversal_count(PhaseKind::Fix), 0);
    }

    // ── SafetyPolicy — Rule precedence (Rule 1 fires before Rule 2) ────────

    #[test]
    fn traversal_limit_takes_precedence_over_elevation_rule() {
        // Safety-class runbook without elevation AND traversal exceeded.
        // Rule 1 should fire (code 2592), not Rule 2 (code 2593).
        let rb = safety_runbook_all_modes();
        let config = PolicyConfig {
            max_traversals: 1,
            current_mode: OperationalMode::LocalM0,
            elevated: false, // would trigger rule 2
            allow_soft_autorun: false,
        };
        let mut policy = SafetyPolicy::new(config);
        policy.record_traversal(PhaseKind::Detect); // count = 1 = max
        let result = policy.check(&rb, PhaseKind::Detect);
        assert!(result.is_blocked());
        // Must be Rule 1 (traversal), not Rule 2 (elevation).
        assert_eq!(result.violation().error_code(), 2592);
    }

    // ── SafetyCheckResult ─────────────────────────────────────────────────────

    #[test]
    fn check_result_pass_is_pass() {
        assert!(SafetyCheckResult::Pass.is_pass());
        assert!(!SafetyCheckResult::Pass.is_blocked());
    }

    #[test]
    fn check_result_blocked_is_blocked() {
        let v = SafetyViolation::PolicyViolation { reason: "x".into() };
        let r = SafetyCheckResult::Blocked(v);
        assert!(r.is_blocked());
        assert!(!r.is_pass());
    }

    #[test]
    fn check_result_display_pass() {
        assert_eq!(SafetyCheckResult::Pass.to_string(), "pass");
    }

    #[test]
    fn check_result_display_blocked() {
        let v = SafetyViolation::PolicyViolation {
            reason: "mode mismatch".into(),
        };
        let r = SafetyCheckResult::Blocked(v);
        assert!(r.to_string().starts_with("blocked("));
    }

    // ── SafetyViolation display ───────────────────────────────────────────────

    #[test]
    fn violation_policy_display_contains_code() {
        let v = SafetyViolation::PolicyViolation {
            reason: "oops".into(),
        };
        assert!(v.to_string().contains("2591"));
    }

    #[test]
    fn violation_traversal_display_contains_code() {
        let v = SafetyViolation::TraversalExceeded { count: 5, max: 3 };
        let s = v.to_string();
        assert!(s.contains("2592"));
        assert!(s.contains('5'));
        assert!(s.contains('3'));
    }

    #[test]
    fn violation_elevation_display_contains_code() {
        let v = SafetyViolation::ElevationDenied {
            class: SafetyClass::Safety,
        };
        let s = v.to_string();
        assert!(s.contains("2593"));
        assert!(s.contains("safety"));
    }

    // ── Full workflow simulation ───────────────────────────────────────────────

    #[test]
    fn full_detect_block_fix_verify_sequence_passes() {
        let rb = hard_runbook_all_modes();
        let mut policy = SafetyPolicy::test_permissive();

        for phase in &[
            PhaseKind::Detect,
            PhaseKind::Block,
            PhaseKind::Fix,
            PhaseKind::Verify,
        ] {
            let result = policy.check_and_record(&rb, *phase);
            assert!(result.is_pass(), "phase {phase:?} should pass");
        }
        // Each phase traversed exactly once.
        assert_eq!(policy.traversal_count(PhaseKind::Detect), 1);
        assert_eq!(policy.traversal_count(PhaseKind::Fix), 1);
    }

    #[test]
    fn max_traversals_enforced_across_repeated_phase_calls() {
        let rb = hard_runbook_all_modes();
        let mut policy = SafetyPolicy::new(PolicyConfig {
            max_traversals: 2,
            current_mode: OperationalMode::LocalM0,
            elevated: false,
            allow_soft_autorun: true,
        });
        assert!(policy.check_and_record(&rb, PhaseKind::Detect).is_pass());
        assert!(policy.check_and_record(&rb, PhaseKind::Detect).is_pass());
        let result = policy.check_and_record(&rb, PhaseKind::Detect);
        assert!(result.is_blocked());
        assert_eq!(result.violation().error_code(), 2592);
    }

    // ── Additional safety_policy tests to reach ≥50 ───────────────────────────

    #[test]
    fn safety_policy_config_accessor_returns_config() {
        let policy = SafetyPolicy::test_permissive();
        assert!(policy.config().elevated);
        assert_eq!(policy.config().max_traversals, 10);
    }

    #[test]
    fn safety_policy_traversal_count_zero_initially() {
        let policy = SafetyPolicy::test_permissive();
        assert_eq!(policy.traversal_count(PhaseKind::Detect), 0);
    }

    #[test]
    fn safety_policy_record_traversal_increments_count() {
        let mut policy = SafetyPolicy::test_permissive();
        policy.record_traversal(PhaseKind::Fix);
        assert_eq!(policy.traversal_count(PhaseKind::Fix), 1);
        policy.record_traversal(PhaseKind::Fix);
        assert_eq!(policy.traversal_count(PhaseKind::Fix), 2);
    }

    #[test]
    fn safety_policy_each_phase_tracked_independently() {
        let mut policy = SafetyPolicy::test_permissive();
        policy.record_traversal(PhaseKind::Detect);
        policy.record_traversal(PhaseKind::Fix);
        policy.record_traversal(PhaseKind::Fix);
        assert_eq!(policy.traversal_count(PhaseKind::Detect), 1);
        assert_eq!(policy.traversal_count(PhaseKind::Fix), 2);
        assert_eq!(policy.traversal_count(PhaseKind::Verify), 0);
    }

    #[test]
    fn safety_policy_check_does_not_modify_count() {
        let rb = hard_runbook_all_modes();
        let policy = SafetyPolicy::test_permissive();
        let _ = policy.check(&rb, PhaseKind::Detect);
        // check() is read-only; count must remain 0.
        assert_eq!(policy.traversal_count(PhaseKind::Detect), 0);
    }

    #[test]
    fn safety_policy_local_m0_has_correct_defaults() {
        let policy = SafetyPolicy::local_m0();
        assert!(!policy.config().elevated);
        assert_eq!(policy.config().max_traversals, 3);
        assert!(!policy.config().allow_soft_autorun);
    }

    #[test]
    fn safety_class_elevation_required_returns_true_for_hard_and_safety() {
        assert!(SafetyClass::Hard.requires_elevation());
        assert!(SafetyClass::Safety.requires_elevation());
        assert!(!SafetyClass::Soft.requires_elevation());
    }

    #[test]
    fn traversal_guard_within_limit_at_zero() {
        let g = TraversalGuard::new();
        // 0 < max = 3 → within limit.
        assert!(g.within_limit(PhaseKind::Detect, 3));
    }

    #[test]
    fn traversal_guard_record_returns_new_count() {
        let mut g = TraversalGuard::new();
        assert_eq!(g.record(PhaseKind::Block), 1);
        assert_eq!(g.record(PhaseKind::Block), 2);
    }

    #[test]
    fn traversal_guard_multiple_phases_independent() {
        let mut g = TraversalGuard::new();
        g.record(PhaseKind::Detect);
        g.record(PhaseKind::Fix);
        g.record(PhaseKind::Fix);
        assert_eq!(g.count(PhaseKind::Detect), 1);
        assert_eq!(g.count(PhaseKind::Fix), 2);
    }

    #[test]
    fn safety_violation_error_codes() {
        assert_eq!(
            SafetyViolation::PolicyViolation { reason: "x".into() }.error_code(),
            2591
        );
        assert_eq!(
            SafetyViolation::TraversalExceeded { count: 1, max: 1 }.error_code(),
            2592
        );
        assert_eq!(
            SafetyViolation::ElevationDenied {
                class: SafetyClass::Safety
            }
            .error_code(),
            2593
        );
    }

    #[test]
    fn policy_violation_reason_in_display() {
        let v = SafetyViolation::PolicyViolation {
            reason: "mode mismatch".into(),
        };
        assert!(v.to_string().contains("mode mismatch"));
    }

    #[test]
    fn elevation_denied_class_in_display() {
        let v = SafetyViolation::ElevationDenied {
            class: SafetyClass::Hard,
        };
        assert!(v.to_string().contains("hard"));
    }

    #[test]
    fn soft_class_fix_phase_passes_when_allow_autorun_true_not_elevated() {
        let rb = soft_runbook_all_modes();
        let config = PolicyConfig {
            max_traversals: 5,
            current_mode: OperationalMode::LocalM0,
            elevated: false,
            allow_soft_autorun: true,
        };
        let policy = SafetyPolicy::new(config);
        assert!(policy.check(&rb, PhaseKind::Fix).is_pass());
    }

    #[test]
    fn safety_class_all_phases_pass_with_elevation_in_production() {
        let rb = safety_runbook_all_modes();
        let config = PolicyConfig {
            max_traversals: 5,
            current_mode: OperationalMode::Production,
            elevated: true,
            allow_soft_autorun: true,
        };
        let policy = SafetyPolicy::new(config);
        for phase in [PhaseKind::Detect, PhaseKind::Block, PhaseKind::Verify] {
            assert!(policy.check(&rb, phase).is_pass(), "{phase:?} should pass");
        }
    }

    #[test]
    fn reset_traversals_allows_re_execution() {
        let rb = hard_runbook_all_modes();
        let mut policy = SafetyPolicy::new(PolicyConfig {
            max_traversals: 1,
            current_mode: OperationalMode::LocalM0,
            elevated: false,
            allow_soft_autorun: true,
        });
        policy.check_and_record(&rb, PhaseKind::Detect);
        // Now at max; blocked.
        assert!(policy.check(&rb, PhaseKind::Detect).is_blocked());
        policy.reset_traversals();
        // After reset, should pass again.
        assert!(policy.check(&rb, PhaseKind::Detect).is_pass());
    }

    #[test]
    fn safety_policy_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SafetyPolicy>();
    }

    #[test]
    fn policy_config_default_is_local_m0() {
        let cfg = PolicyConfig::default();
        assert!(!cfg.elevated);
        assert_eq!(cfg.current_mode, OperationalMode::LocalM0);
    }
}
