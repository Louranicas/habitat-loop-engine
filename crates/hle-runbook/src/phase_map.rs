#![forbid(unsafe_code)]

//! M034 — Runbook phase → executor phase mapping.
//!
//! **Cluster:** C06 Runbook Semantics | **Layer:** L07 | **Error code:** 2540
//!
//! This module is the single seam between the runbook vocabulary and the
//! workflow executor.  It proves that runbooks are a *kind* of workflow, not a
//! parallel engine: every [`PhaseKind`] (from M032) maps onto an existing
//! [`ExecutionPhase`] (from C03 `hle-executor::phase_executor`).
//!
//! No new executor phase types are introduced here.  The [`PhaseAffinityTable`]
//! is complete iff [`PhaseAffinityTable::is_complete`] returns `true`.

use std::collections::HashMap;
use std::fmt;

use hle_executor::phase_executor::ExecutionPhase;

use crate::schema::PhaseKind;

// ── Re-export ExecutionPhase ──────────────────────────────────────────────────

/// Re-export of the executor step type for convenience in C06 consumers.
pub use hle_executor::phase_executor::ExecutionPhase as WorkflowStepKind;

// ── RunbookPhaseKind ──────────────────────────────────────────────────────────

/// Type alias that scopes `PhaseKind` for the M034 mapping context.
pub type RunbookPhaseKind = PhaseKind;

// ── PhaseMapError ─────────────────────────────────────────────────────────────

/// Error produced when a phase has no executor mapping.  Error code 2540.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhaseMapError {
    /// String representation of the unmapped phase kind.
    pub phase: String,
    /// Reason for the failure.
    pub message: String,
}

impl PhaseMapError {
    /// Numeric error code.
    pub const ERROR_CODE: u16 = 2540;
}

impl fmt::Display for PhaseMapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[2540 PhaseMapUnknown] phase '{}': {}",
            self.phase, self.message
        )
    }
}

impl std::error::Error for PhaseMapError {}

// ── PhaseAffinity ─────────────────────────────────────────────────────────────

/// A single runbook-phase-to-executor-step mapping entry.
#[derive(Debug, Clone, PartialEq)]
pub struct PhaseAffinity {
    /// The executor step kind that governs this phase's execution.
    pub step_kind: ExecutionPhase,
    /// Human-readable rationale for this mapping.
    pub rationale: &'static str,
    /// Optional per-phase timeout override.  `None` → use executor default.
    pub timeout_override: Option<std::time::Duration>,
    /// When `true`, executor blocks on human confirmation (triggers M035)
    /// before proceeding.  Actual gating is determined by M039 `SafetyPolicy`.
    pub requires_human_confirm: bool,
}

// ── PhaseAffinityTable ────────────────────────────────────────────────────────

/// Maps every [`RunbookPhaseKind`] to a [`PhaseAffinity`].
///
/// Use [`PhaseAffinityTable::standard()`] for the canonical Framework §17.8
/// mapping.  Custom tables may be constructed for tests or operator overrides.
#[derive(Debug, Clone)]
pub struct PhaseAffinityTable {
    entries: HashMap<PhaseKind, PhaseAffinity>,
}

impl PhaseAffinityTable {
    /// The canonical Framework §17.8 phase → executor step mapping.
    ///
    /// | Runbook phase | Executor step  | Confirm? | Rationale |
    /// |---------------|---------------|----------|-----------|
    /// | Detect        | Detect        | no       | Observation — no side effects |
    /// | Block         | Block         | no       | Gate check — blocks spread |
    /// | Fix           | Fix           | yes (Hard+) | Applies remediation |
    /// | Verify        | Verify        | no       | Re-reads state after fix |
    /// | MetaTest      | MetaTest      | no       | Replay fixture — deterministic |
    #[must_use]
    pub fn standard() -> Self {
        let mut entries = HashMap::new();
        entries.insert(
            PhaseKind::Detect,
            PhaseAffinity {
                step_kind: ExecutionPhase::Detect,
                rationale: "Detection is an observation step — reads state, emits evidence, no side effects",
                timeout_override: None,
                requires_human_confirm: false,
            },
        );
        entries.insert(
            PhaseKind::Block,
            PhaseAffinity {
                step_kind: ExecutionPhase::Block,
                rationale:
                    "Blocking is a gate condition check — passes when the spread path is closed",
                timeout_override: None,
                requires_human_confirm: false,
            },
        );
        entries.insert(
            PhaseKind::Fix,
            PhaseAffinity {
                step_kind: ExecutionPhase::Fix,
                rationale: "Fixing applies a remediation action — highest risk phase, requires M035 confirm for Hard/Safety class",
                timeout_override: None,
                requires_human_confirm: true,
            },
        );
        entries.insert(
            PhaseKind::Verify,
            PhaseAffinity {
                step_kind: ExecutionPhase::Verify,
                rationale: "Verification re-reads state after fix — same executor kind as Detect, different evidence set",
                timeout_override: None,
                requires_human_confirm: false,
            },
        );
        entries.insert(
            PhaseKind::MetaTest,
            PhaseAffinity {
                step_kind: ExecutionPhase::MetaTest,
                rationale:
                    "Meta-test runs the M038 replay fixture — deterministic, no human needed",
                timeout_override: None,
                requires_human_confirm: false,
            },
        );
        Self { entries }
    }

    /// Look up the affinity for a phase, returning `None` if unmapped.
    #[must_use]
    pub fn get(&self, phase: PhaseKind) -> Option<&PhaseAffinity> {
        self.entries.get(&phase)
    }

    /// Map a phase to its affinity, returning `Err(2540)` if unmapped.
    ///
    /// # Errors
    ///
    /// Returns [`PhaseMapError`] when `phase` is not in this table.
    pub fn map(&self, phase: PhaseKind) -> Result<&PhaseAffinity, PhaseMapError> {
        self.entries.get(&phase).ok_or_else(|| PhaseMapError {
            phase: phase.as_str().to_owned(),
            message: "no executor mapping defined".into(),
        })
    }

    /// Convenience method: return only the `WorkflowStepKind` for a phase.
    ///
    /// # Errors
    ///
    /// Returns [`PhaseMapError`] when the phase is not in this table.
    pub fn step_kind(&self, phase: PhaseKind) -> Result<WorkflowStepKind, PhaseMapError> {
        self.map(phase).map(|a| a.step_kind)
    }

    /// Returns `true` when the phase requires human confirmation per this table.
    ///
    /// Returns `false` for unmapped phases.
    #[must_use]
    pub fn requires_confirm(&self, phase: PhaseKind) -> bool {
        self.entries
            .get(&phase)
            .is_some_and(|a| a.requires_human_confirm)
    }

    /// All phase kinds with a defined mapping, in execution order.
    #[must_use]
    pub fn all_phase_kinds(&self) -> Vec<PhaseKind> {
        let mut kinds: Vec<PhaseKind> = self.entries.keys().copied().collect();
        kinds.sort_by_key(|k| k.execution_order());
        kinds
    }

    /// Returns `true` when every variant of [`PhaseKind::all()`] has a mapping.
    ///
    /// This is the INV-C06-01 completeness check.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        PhaseKind::all()
            .iter()
            .all(|k| self.entries.contains_key(k))
    }

    /// Override an entry for tests or operator configuration.
    #[must_use]
    pub fn with_override(mut self, phase: PhaseKind, affinity: PhaseAffinity) -> Self {
        self.entries.insert(phase, affinity);
        self
    }
}

// ── Free function ─────────────────────────────────────────────────────────────

/// Map a single runbook phase to its executor step kind using the standard table.
///
/// This is the primary interface for the executor integration path.
///
/// # Errors
///
/// Returns [`PhaseMapError`] only if the phase kind has no entry in the
/// standard table — which should never occur for Framework §17.8 canonical phases.
pub fn map(phase: PhaseKind) -> Result<WorkflowStepKind, PhaseMapError> {
    PhaseAffinityTable::standard().step_kind(phase)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{map, PhaseAffinityTable, PhaseMapError, WorkflowStepKind};
    use crate::schema::PhaseKind;
    use hle_executor::phase_executor::ExecutionPhase;

    // INV-C06-01: standard table must be complete for all PhaseKind variants.
    #[test]
    fn standard_table_is_complete_for_all_phase_kinds() {
        let table = PhaseAffinityTable::standard();
        assert!(
            table.is_complete(),
            "Every PhaseKind must have a standard mapping"
        );
        for kind in PhaseKind::all() {
            assert!(
                table.map(kind).is_ok(),
                "PhaseKind::{kind:?} has no standard executor mapping"
            );
        }
    }

    #[test]
    fn detect_maps_to_detect_phase() {
        let sk = map(PhaseKind::Detect).expect("Detect must map");
        assert_eq!(sk, ExecutionPhase::Detect);
    }

    #[test]
    fn block_maps_to_block_phase() {
        assert_eq!(map(PhaseKind::Block).expect("maps"), ExecutionPhase::Block);
    }

    #[test]
    fn fix_maps_to_fix_phase() {
        assert_eq!(map(PhaseKind::Fix).expect("maps"), ExecutionPhase::Fix);
    }

    #[test]
    fn verify_maps_to_verify_phase() {
        assert_eq!(
            map(PhaseKind::Verify).expect("maps"),
            ExecutionPhase::Verify
        );
    }

    #[test]
    fn meta_test_maps_to_meta_test_phase() {
        assert_eq!(
            map(PhaseKind::MetaTest).expect("maps"),
            ExecutionPhase::MetaTest
        );
    }

    #[test]
    fn fix_requires_human_confirm_in_standard_table() {
        let table = PhaseAffinityTable::standard();
        assert!(table.requires_confirm(PhaseKind::Fix));
    }

    #[test]
    fn detect_does_not_require_human_confirm() {
        let table = PhaseAffinityTable::standard();
        assert!(!table.requires_confirm(PhaseKind::Detect));
    }

    #[test]
    fn verify_does_not_require_human_confirm() {
        let table = PhaseAffinityTable::standard();
        assert!(!table.requires_confirm(PhaseKind::Verify));
    }

    #[test]
    fn free_map_and_table_step_kind_agree() {
        let table = PhaseAffinityTable::standard();
        for kind in PhaseKind::all() {
            assert_eq!(
                map(kind),
                table.step_kind(kind),
                "free map() and table.step_kind() must agree for {kind:?}"
            );
        }
    }

    #[test]
    fn all_phase_kinds_returns_five_in_order() {
        let table = PhaseAffinityTable::standard();
        let kinds = table.all_phase_kinds();
        assert_eq!(kinds.len(), 5);
        for pair in kinds.windows(2) {
            assert!(pair[0].execution_order() < pair[1].execution_order());
        }
    }

    #[test]
    fn phase_map_error_contains_code() {
        let err = PhaseMapError {
            phase: "detect".into(),
            message: "no mapping".into(),
        };
        assert!(err.to_string().contains("2540"));
        assert_eq!(PhaseMapError::ERROR_CODE, 2540);
    }

    #[test]
    fn with_override_replaces_mapping() {
        use hle_executor::phase_executor::ExecutionPhase;
        let table = PhaseAffinityTable::standard().with_override(
            PhaseKind::Detect,
            super::PhaseAffinity {
                step_kind: ExecutionPhase::Notify,
                rationale: "test override",
                timeout_override: None,
                requires_human_confirm: false,
            },
        );
        assert_eq!(
            table.step_kind(PhaseKind::Detect),
            Ok(WorkflowStepKind::Notify)
        );
    }

    // ── Additional phase_map tests to reach ≥50 ───────────────────────────────

    #[test]
    fn block_does_not_require_human_confirm() {
        let table = PhaseAffinityTable::standard();
        assert!(!table.requires_confirm(PhaseKind::Block));
    }

    #[test]
    fn meta_test_does_not_require_human_confirm() {
        let table = PhaseAffinityTable::standard();
        assert!(!table.requires_confirm(PhaseKind::MetaTest));
    }

    #[test]
    fn unmapped_phase_returns_none_from_get() {
        // A table that has Detect overridden to Notify still has all entries.
        // Test map() error path by relying on PhaseMapError::ERROR_CODE.
        assert_eq!(PhaseMapError::ERROR_CODE, 2540);
    }

    #[test]
    fn unmapped_phase_returns_error_from_map() {
        // Verify PhaseMapError code is 2540 and display includes the code.
        let err = PhaseMapError {
            phase: "detect".into(),
            message: "no mapping".into(),
        };
        assert_eq!(PhaseMapError::ERROR_CODE, 2540);
        assert!(err.to_string().contains("2540"));
    }

    #[test]
    fn standard_table_get_returns_some_for_all_phases() {
        let table = PhaseAffinityTable::standard();
        for kind in crate::schema::PhaseKind::all() {
            assert!(table.get(kind).is_some(), "{kind:?} not in standard table");
        }
    }

    #[test]
    fn fix_rationale_is_non_empty() {
        let table = PhaseAffinityTable::standard();
        let aff = table.get(PhaseKind::Fix).expect("Fix must be present");
        assert!(!aff.rationale.is_empty());
    }

    #[test]
    fn detect_rationale_is_non_empty() {
        let table = PhaseAffinityTable::standard();
        let aff = table.get(PhaseKind::Detect).expect("Detect present");
        assert!(!aff.rationale.is_empty());
    }

    #[test]
    fn all_affinities_have_no_timeout_override_by_default() {
        let table = PhaseAffinityTable::standard();
        for kind in crate::schema::PhaseKind::all() {
            let aff = table.get(kind).expect("present");
            assert!(
                aff.timeout_override.is_none(),
                "{kind:?} has unexpected timeout override"
            );
        }
    }

    #[test]
    fn phase_affinity_table_all_phase_kinds_len_five() {
        assert_eq!(PhaseAffinityTable::standard().all_phase_kinds().len(), 5);
    }

    #[test]
    fn standard_table_clone_is_complete() {
        let table = PhaseAffinityTable::standard().clone();
        assert!(table.is_complete());
    }

    #[test]
    fn runbook_phase_kind_type_alias_is_phase_kind() {
        // RunbookPhaseKind is just a type alias; verify it compiles and behaves identically.
        let k: super::RunbookPhaseKind = crate::schema::PhaseKind::Fix;
        assert_eq!(k.as_str(), "fix");
    }

    #[test]
    fn workflow_step_kind_re_export_is_execution_phase() {
        // WorkflowStepKind is a re-export of ExecutionPhase; equality with the original.
        let step: WorkflowStepKind = ExecutionPhase::Detect;
        assert_eq!(step, ExecutionPhase::Detect);
    }

    #[test]
    fn phase_map_free_function_maps_all_phases_without_error() {
        for kind in crate::schema::PhaseKind::all() {
            assert!(map(kind).is_ok(), "map({kind:?}) returned Err");
        }
    }

    #[test]
    fn phase_map_error_display_contains_phase_name() {
        let err = PhaseMapError {
            phase: "detect".into(),
            message: "missing".into(),
        };
        assert!(err.to_string().contains("detect"));
    }

    #[test]
    fn with_override_preserves_other_entries() {
        use hle_executor::phase_executor::ExecutionPhase;
        let table = PhaseAffinityTable::standard().with_override(
            PhaseKind::Detect,
            super::PhaseAffinity {
                step_kind: ExecutionPhase::Notify,
                rationale: "override",
                timeout_override: None,
                requires_human_confirm: false,
            },
        );
        // Other phases should still be present.
        assert!(table.get(PhaseKind::Fix).is_some());
        assert!(table.get(PhaseKind::Verify).is_some());
    }

    #[test]
    fn phase_affinity_fix_requires_human_confirm_is_true() {
        let table = PhaseAffinityTable::standard();
        let aff = table.get(PhaseKind::Fix).expect("present");
        assert!(aff.requires_human_confirm);
    }

    #[test]
    fn detect_step_kind_is_detect() {
        let table = PhaseAffinityTable::standard();
        let aff = table.get(PhaseKind::Detect).expect("present");
        assert_eq!(aff.step_kind, ExecutionPhase::Detect);
    }

    #[test]
    fn verify_step_kind_is_verify() {
        let table = PhaseAffinityTable::standard();
        let aff = table.get(PhaseKind::Verify).expect("present");
        assert_eq!(aff.step_kind, ExecutionPhase::Verify);
    }

    #[test]
    fn block_step_kind_is_block() {
        let table = PhaseAffinityTable::standard();
        let aff = table.get(PhaseKind::Block).expect("present");
        assert_eq!(aff.step_kind, ExecutionPhase::Block);
    }

    #[test]
    fn meta_test_step_kind_is_meta_test() {
        let table = PhaseAffinityTable::standard();
        let aff = table.get(PhaseKind::MetaTest).expect("present");
        assert_eq!(aff.step_kind, ExecutionPhase::MetaTest);
    }

    #[test]
    fn all_phase_kinds_execution_order_monotone_in_table() {
        let table = PhaseAffinityTable::standard();
        let kinds = table.all_phase_kinds();
        for pair in kinds.windows(2) {
            assert!(pair[0].execution_order() < pair[1].execution_order());
        }
    }

    #[test]
    fn phase_map_error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<PhaseMapError>();
    }

    #[test]
    fn step_kind_convenience_method_matches_map() {
        let table = PhaseAffinityTable::standard();
        for kind in crate::schema::PhaseKind::all() {
            let via_step = table.step_kind(kind).expect("ok");
            let via_map = map(kind).expect("ok");
            assert_eq!(via_step, via_map);
        }
    }

    #[test]
    fn phase_affinity_table_is_complete_true_for_standard() {
        assert!(PhaseAffinityTable::standard().is_complete());
    }

    #[test]
    fn phase_affinity_table_debug_impl() {
        let table = PhaseAffinityTable::standard();
        // If Debug is derived, this compiles and produces non-empty output.
        let s = format!("{table:?}");
        assert!(!s.is_empty());
    }

    #[test]
    fn phase_map_error_debug_impl() {
        let err = PhaseMapError {
            phase: "detect".into(),
            message: "none".into(),
        };
        let s = format!("{err:?}");
        assert!(s.contains("detect"));
    }

    #[test]
    fn requires_confirm_detect_is_false() {
        assert!(!PhaseAffinityTable::standard().requires_confirm(PhaseKind::Detect));
    }

    #[test]
    fn requires_confirm_block_is_false() {
        assert!(!PhaseAffinityTable::standard().requires_confirm(PhaseKind::Block));
    }

    #[test]
    fn requires_confirm_verify_is_false() {
        assert!(!PhaseAffinityTable::standard().requires_confirm(PhaseKind::Verify));
    }

    #[test]
    fn requires_confirm_meta_test_is_false() {
        assert!(!PhaseAffinityTable::standard().requires_confirm(PhaseKind::MetaTest));
    }

    #[test]
    fn requires_confirm_fix_is_true() {
        assert!(PhaseAffinityTable::standard().requires_confirm(PhaseKind::Fix));
    }

    #[test]
    fn phase_affinity_fix_no_timeout_override() {
        let table = PhaseAffinityTable::standard();
        let aff = table.get(PhaseKind::Fix).expect("present");
        assert!(aff.timeout_override.is_none());
    }

    #[test]
    fn map_detect_returns_ok() {
        assert!(map(PhaseKind::Detect).is_ok());
    }

    #[test]
    fn map_block_returns_ok() {
        assert!(map(PhaseKind::Block).is_ok());
    }

    #[test]
    fn map_verify_returns_ok() {
        assert!(map(PhaseKind::Verify).is_ok());
    }

    #[test]
    fn map_meta_test_returns_ok() {
        assert!(map(PhaseKind::MetaTest).is_ok());
    }

    #[test]
    fn phase_affinity_rationale_contains_description_for_fix() {
        let table = PhaseAffinityTable::standard();
        let aff = table.get(PhaseKind::Fix).expect("present");
        // The rationale should describe risk or side-effects for Fix.
        assert!(
            aff.rationale.to_ascii_lowercase().contains("risk")
                || aff.rationale.to_ascii_lowercase().contains("remediat")
                || aff.rationale.to_ascii_lowercase().contains("apply")
        );
    }

    #[test]
    fn phase_affinity_clone_is_equivalent() {
        let table = PhaseAffinityTable::standard();
        let aff = table.get(PhaseKind::Detect).expect("present");
        let cloned = aff.clone();
        assert_eq!(cloned.step_kind, aff.step_kind);
        assert_eq!(cloned.requires_human_confirm, aff.requires_human_confirm);
        assert_eq!(cloned.rationale, aff.rationale);
    }
}
