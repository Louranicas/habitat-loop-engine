#![forbid(unsafe_code)]

//! M023 Test Taxonomy Verifier — rejects vacuous and inflated test suites.
//!
//! Consumes M022 vocabulary types (`TestDescriptor`, `TestKind`, `VacuousPattern`,
//! `LoopPhase`) from `hle-core::testing::test_taxonomy`. No vocabulary is redefined here.
//!
//! Vacuous-test detection uses simple name-pattern heuristics for the stub:
//! - Test names containing `_passes_trivially`, `_assert_true`, or `_no_assertions`
//!   are flagged as `VacuousPattern::AssertTrue` / `NoAssertions`.
//!
//! Layer: L04 | Cluster: C04

use std::fmt;

use hle_core::testing::test_taxonomy::{TaxonomyLabel, TestDescriptor, TestKind, VacuousPattern};
use substrate_types::HleError;

// ---------------------------------------------------------------------------
// TaxonomyPolicy
// ---------------------------------------------------------------------------

/// Acceptance thresholds for a module's test suite.
#[derive(Debug, Clone)]
pub struct TaxonomyPolicy {
    /// Minimum number of behavior-bearing (Behavioral or Property) tests.
    pub min_behavior_bearing: usize,
    /// Maximum fraction of Smoke tests; clamped to `[0.0, 1.0]`.
    pub max_smoke_fraction: f64,
    /// When true, Doctest counts toward the behavior-bearing minimum.
    pub doctests_count_as_behavioral: bool,
    /// Hard limit on Excluded tests per module.
    pub max_excluded: usize,
}

impl TaxonomyPolicy {
    /// Default policy as specified in §17.7:
    /// min_behavior_bearing=1, max_smoke_fraction=0.5, doctests=false, max_excluded=5.
    #[must_use]
    pub fn default_policy() -> Self {
        Self {
            min_behavior_bearing: 1,
            max_smoke_fraction: 0.5,
            doctests_count_as_behavioral: false,
            max_excluded: 5,
        }
    }
}

impl Default for TaxonomyPolicy {
    fn default() -> Self {
        Self::default_policy()
    }
}

// ---------------------------------------------------------------------------
// ModuleTestProfile
// ---------------------------------------------------------------------------

/// Summary counts used to evaluate a single module's test suite.
#[derive(Debug, Clone)]
pub struct ModuleTestProfile {
    /// Total tests in the module.
    pub total: usize,
    /// Count of Behavioral tests.
    pub behavioral_count: usize,
    /// Count of Property tests.
    pub property_count: usize,
    /// Count of Smoke tests.
    pub smoke_count: usize,
    /// Count of Doctest tests.
    pub doctest_count: usize,
    /// Count of Excluded tests.
    pub excluded_count: usize,
    /// Count of tests flagged with any vacuous pattern.
    pub vacuous_count: usize,
    /// Effective behavior-bearing count (Behavioral + Property + optionally Doctest).
    pub behavior_bearing: usize,
    /// `smoke_count / max(total, 1)`.
    pub smoke_fraction: f64,
}

impl ModuleTestProfile {
    fn from_descriptors(descriptors: &[TestDescriptor], policy: &TaxonomyPolicy) -> Self {
        let total = descriptors.len();
        let behavioral_count = descriptors
            .iter()
            .filter(|d| d.kind == TestKind::Behavioral)
            .count();
        let property_count = descriptors
            .iter()
            .filter(|d| d.kind == TestKind::Property)
            .count();
        let smoke_count = descriptors
            .iter()
            .filter(|d| d.kind == TestKind::Smoke)
            .count();
        let doctest_count = descriptors
            .iter()
            .filter(|d| d.kind == TestKind::Doctest)
            .count();
        let excluded_count = descriptors
            .iter()
            .filter(|d| d.kind == TestKind::Excluded)
            .count();
        let vacuous_count = descriptors.iter().filter(|d| d.is_vacuous()).count();

        let mut behavior_bearing = behavioral_count + property_count;
        if policy.doctests_count_as_behavioral {
            behavior_bearing += doctest_count;
        }

        let smoke_fraction = if total == 0 {
            0.0
        } else {
            smoke_count as f64 / total as f64
        };

        Self {
            total,
            behavioral_count,
            property_count,
            smoke_count,
            doctest_count,
            excluded_count,
            vacuous_count,
            behavior_bearing,
            smoke_fraction,
        }
    }

    /// Returns the first policy violation, or `Ok(())` when the profile passes.
    ///
    /// # Errors
    ///
    /// Returns a `RejectionReason` when the profile violates any policy threshold.
    pub fn meets_policy(&self, policy: &TaxonomyPolicy) -> Result<(), RejectionReason> {
        if self.behavior_bearing < policy.min_behavior_bearing {
            return Err(RejectionReason::NoBehavioralTests);
        }
        if self.smoke_count > 0 && self.behavioral_count == 0 && self.property_count == 0 {
            return Err(RejectionReason::SmokeOnlyModule);
        }
        if self.smoke_fraction > policy.max_smoke_fraction {
            return Err(RejectionReason::TooManySmoke);
        }
        if self.excluded_count > policy.max_excluded {
            return Err(RejectionReason::TooManyExcluded);
        }
        if self.vacuous_count > 0 {
            return Err(RejectionReason::VacuousTestInflation);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// RejectionReason
// ---------------------------------------------------------------------------

/// Classified reason for a module-level rejection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RejectionReason {
    /// No behavior-bearing (Behavioral or Property) tests present.
    NoBehavioralTests,
    /// Smoke fraction exceeds `TaxonomyPolicy::max_smoke_fraction`.
    TooManySmoke,
    /// More Excluded tests than `TaxonomyPolicy::max_excluded`.
    TooManyExcluded,
    /// One or more tests contain a `VacuousPattern`.
    VacuousTestInflation,
    /// All tests are Smoke with no Behavioral companion.
    SmokeOnlyModule,
}

impl fmt::Display for RejectionReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoBehavioralTests => {
                f.write_str("NoBehavioralTests: module has no Behavioral or Property tests")
            }
            Self::TooManySmoke => {
                f.write_str("TooManySmoke: Smoke-test fraction exceeds policy threshold")
            }
            Self::TooManyExcluded => {
                f.write_str("TooManyExcluded: more Excluded tests than policy permits")
            }
            Self::VacuousTestInflation => f.write_str(
                "VacuousTestInflation: at least one test contains assert!(true) \
                 or an equivalent tautological assertion",
            ),
            Self::SmokeOnlyModule => {
                f.write_str("SmokeOnlyModule: all tests are Smoke with no Behavioral companion")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// InflationClass
// ---------------------------------------------------------------------------

/// Subtype of inflation finding produced by verifier source analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InflationClass {
    /// `assert!(true)` or `assert!(false)`.
    Tautological,
    /// `assert_eq!(x, x)` — identical identifiers on both sides.
    SameVariableBothSides,
    /// Test body has no `assert*` call at all.
    NoAssertions,
    /// `Result<T>` return value silently discarded without check.
    SilentResultDiscard,
    /// Assertion condition is a compile-time literal constant.
    LiteralConstantCondition,
}

// ---------------------------------------------------------------------------
// TestVerdict
// ---------------------------------------------------------------------------

/// Per-test pass/reject decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestVerdict {
    /// Test passed policy checks.
    Passing,
    /// Test was rejected by the verifier.
    Rejected,
}

// ---------------------------------------------------------------------------
// TestVerdictEntry
// ---------------------------------------------------------------------------

/// Per-test decision with rationale.
#[derive(Debug, Clone)]
pub struct TestVerdictEntry {
    /// The classified test.
    pub descriptor: TestDescriptor,
    /// Pass or reject.
    pub verdict: TestVerdict,
    /// Inflation classification when rejected.
    pub inflation_class: Option<InflationClass>,
    /// Human-readable explanation.
    pub rationale: String,
}

// ---------------------------------------------------------------------------
// TaxonomyReport
// ---------------------------------------------------------------------------

/// Per-module verdict and per-test labels from one `verify_module` call.
#[derive(Debug, Clone)]
pub struct TaxonomyReport {
    /// Module path being evaluated.
    pub module_path: String,
    /// Aggregate counts for this module.
    pub profile: ModuleTestProfile,
    /// Per-test verdict entries.
    pub test_entries: Vec<TestVerdictEntry>,
    /// Module-level rejection reason, if any.
    pub rejection: Option<RejectionReason>,
    /// Human-readable summary. Required even for passing reports.
    pub rationale: String,
}

impl TaxonomyReport {
    /// True when no rejection reason is set.
    #[must_use]
    pub fn is_passing(&self) -> bool {
        self.rejection.is_none()
    }

    /// Entries marked `Rejected`.
    #[must_use]
    pub fn rejected_tests(&self) -> Vec<&TestVerdictEntry> {
        self.test_entries
            .iter()
            .filter(|e| e.verdict == TestVerdict::Rejected)
            .collect()
    }

    /// Entries marked `Passing`.
    #[must_use]
    pub fn passing_tests(&self) -> Vec<&TestVerdictEntry> {
        self.test_entries
            .iter()
            .filter(|e| e.verdict == TestVerdict::Passing)
            .collect()
    }

    /// Count of tests flagged with any vacuous pattern.
    #[must_use]
    pub fn vacuous_count(&self) -> usize {
        self.test_entries
            .iter()
            .filter(|e| !e.descriptor.vacuous_patterns.is_empty())
            .count()
    }

    /// Emit compact `TaxonomyLabel` values for all entries.
    #[must_use]
    pub fn labels(&self) -> Vec<TaxonomyLabel> {
        self.test_entries
            .iter()
            .map(|e| TaxonomyLabel {
                test_name: e.descriptor.test_name.clone(),
                kind: e.descriptor.kind,
                cluster_role: e.descriptor.cluster_role,
                phases: e.descriptor.loop_phase_set,
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// TaxonomyVerifier
// ---------------------------------------------------------------------------

/// Entry point — inspects test suites and rejects inflated or vacuous collections.
pub struct TaxonomyVerifier {
    policy: TaxonomyPolicy,
}

impl TaxonomyVerifier {
    /// Construct with the given policy.
    #[must_use]
    pub fn new(policy: TaxonomyPolicy) -> Self {
        Self { policy }
    }

    /// Construct with the default policy.
    #[must_use]
    pub fn with_default_policy() -> Self {
        Self::new(TaxonomyPolicy::default())
    }

    /// Evaluate one module's test suite.
    ///
    /// # Errors
    ///
    /// Returns `[E2320]` when `module_path` is empty, or when the generated
    /// `rationale` string would be empty (indicating the verifier short-circuited).
    pub fn verify_module(
        &self,
        module_path: &str,
        descriptors: &[TestDescriptor],
    ) -> Result<TaxonomyReport, HleError> {
        if module_path.is_empty() {
            return Err(HleError::new("[E2320] module_path cannot be empty"));
        }

        // Classify each test for vacuous patterns using name heuristics.
        let mut classified: Vec<TestDescriptor> = descriptors.to_vec();
        for d in &mut classified {
            let lower = d.test_name.to_ascii_lowercase();
            if lower.contains("_passes_trivially") || lower.contains("_assert_true") {
                d.vacuous_patterns.push(VacuousPattern::AssertTrue);
            } else if lower.contains("_no_assertions") {
                d.vacuous_patterns.push(VacuousPattern::NoAssertions);
            }
        }

        let profile = ModuleTestProfile::from_descriptors(&classified, &self.policy);

        let mut test_entries: Vec<TestVerdictEntry> = classified
            .into_iter()
            .map(|d| {
                if d.is_vacuous() {
                    let inflation = match d.vacuous_patterns.first() {
                        Some(VacuousPattern::AssertTrue) => Some(InflationClass::Tautological),
                        Some(VacuousPattern::TautologicalEq) => {
                            Some(InflationClass::SameVariableBothSides)
                        }
                        Some(VacuousPattern::NoAssertions) => Some(InflationClass::NoAssertions),
                        Some(VacuousPattern::ConstantTrue) => {
                            Some(InflationClass::LiteralConstantCondition)
                        }
                        Some(VacuousPattern::UnassertedResult) => {
                            Some(InflationClass::SilentResultDiscard)
                        }
                        None => None,
                    };
                    let rationale = format!(
                        "[E2321] test '{}' contains vacuous pattern: {}",
                        d.test_name,
                        d.vacuous_patterns
                            .iter()
                            .map(|p| p.description())
                            .collect::<Vec<_>>()
                            .join("; ")
                    );
                    TestVerdictEntry {
                        descriptor: d,
                        verdict: TestVerdict::Rejected,
                        inflation_class: inflation,
                        rationale,
                    }
                } else {
                    TestVerdictEntry {
                        descriptor: d,
                        verdict: TestVerdict::Passing,
                        inflation_class: None,
                        rationale: String::from("test passes policy checks"),
                    }
                }
            })
            .collect();

        let rejection = profile.meets_policy(&self.policy).err();

        let rationale = build_rationale(module_path, &profile, rejection.as_ref());

        if rationale.is_empty() {
            return Err(HleError::new(
                "[E2320] verifier generated empty rationale — evaluation was not performed",
            ));
        }

        // Re-mark all entries as Rejected when the module-level check fails.
        if rejection.is_some() {
            for entry in &mut test_entries {
                if entry.verdict == TestVerdict::Passing && entry.descriptor.is_vacuous() {
                    entry.verdict = TestVerdict::Rejected;
                }
            }
        }

        Ok(TaxonomyReport {
            module_path: module_path.to_owned(),
            profile,
            test_entries,
            rejection,
            rationale,
        })
    }

    /// Evaluate all modules in a workspace.
    ///
    /// # Errors
    ///
    /// Propagates any error from `verify_module`.
    pub fn verify_workspace(
        &self,
        modules: &[(&str, Vec<TestDescriptor>)],
    ) -> Result<Vec<TaxonomyReport>, HleError> {
        modules
            .iter()
            .map(|(path, descriptors)| self.verify_module(path, descriptors))
            .collect()
    }

    /// True when the report has no rejection reason.
    #[must_use]
    pub fn is_module_passing(&self, report: &TaxonomyReport) -> bool {
        report.is_passing()
    }
}

fn build_rationale(
    module_path: &str,
    profile: &ModuleTestProfile,
    rejection: Option<&RejectionReason>,
) -> String {
    match rejection {
        Some(reason) => format!(
            "module '{module_path}' REJECTED — {reason} \
             (total={}, behavioral={}, smoke={}, excluded={}, vacuous={})",
            profile.total,
            profile.behavioral_count,
            profile.smoke_count,
            profile.excluded_count,
            profile.vacuous_count,
        ),
        None => format!(
            "module '{module_path}' PASSED taxonomy checks \
             (total={}, behavioral={}, smoke={}, excluded={}, vacuous={})",
            profile.total,
            profile.behavioral_count,
            profile.smoke_count,
            profile.excluded_count,
            profile.vacuous_count,
        ),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use hle_core::testing::test_taxonomy::{
        ClusterRole, LoopPhase, TestDescriptor, TestKind, VacuousPattern,
    };

    use super::{
        InflationClass, ModuleTestProfile, RejectionReason, TaxonomyPolicy, TaxonomyVerifier,
        TestVerdict,
    };

    fn behavioral(name: &str) -> TestDescriptor {
        TestDescriptor::builder(name, "my::module")
            .kind(TestKind::Behavioral)
            .cluster_role(ClusterRole::Verifier)
            .phase(LoopPhase::Verify)
            .build()
            .map_err(|e| e.to_string())
            .unwrap()
    }

    fn smoke(name: &str) -> TestDescriptor {
        TestDescriptor::builder(name, "my::module")
            .kind(TestKind::Smoke)
            .cluster_role(ClusterRole::Verifier)
            .build()
            .map_err(|e| e.to_string())
            .unwrap()
    }

    fn property_test(name: &str) -> TestDescriptor {
        TestDescriptor::builder(name, "my::module")
            .kind(TestKind::Property)
            .cluster_role(ClusterRole::Verifier)
            .build()
            .map_err(|e| e.to_string())
            .unwrap()
    }

    fn excluded(name: &str) -> TestDescriptor {
        TestDescriptor::builder(name, "my::module")
            .kind(TestKind::Excluded)
            .cluster_role(ClusterRole::Verifier)
            .exclusion_reason("tracked in HLE-001")
            .map_err(|e| e.to_string())
            .unwrap()
            .build()
            .map_err(|e| e.to_string())
            .unwrap()
    }

    fn doctest(name: &str) -> TestDescriptor {
        TestDescriptor::builder(name, "my::module")
            .kind(TestKind::Doctest)
            .cluster_role(ClusterRole::Verifier)
            .build()
            .map_err(|e| e.to_string())
            .unwrap()
    }

    // -----------------------------------------------------------------------
    // RejectionReason — every variant
    // -----------------------------------------------------------------------

    #[test]
    fn rejection_reason_no_behavioral_tests_display() {
        assert!(!RejectionReason::NoBehavioralTests.to_string().is_empty());
    }

    #[test]
    fn rejection_reason_too_many_smoke_display() {
        assert!(!RejectionReason::TooManySmoke.to_string().is_empty());
    }

    #[test]
    fn rejection_reason_too_many_excluded_display() {
        assert!(!RejectionReason::TooManyExcluded.to_string().is_empty());
    }

    #[test]
    fn rejection_reason_vacuous_test_inflation_display() {
        assert!(!RejectionReason::VacuousTestInflation.to_string().is_empty());
    }

    #[test]
    fn rejection_reason_smoke_only_module_display() {
        assert!(!RejectionReason::SmokeOnlyModule.to_string().is_empty());
    }

    // -----------------------------------------------------------------------
    // InflationClass — every variant is addressable
    // -----------------------------------------------------------------------

    #[test]
    fn inflation_class_tautological_is_distinct() {
        let ic = InflationClass::Tautological;
        assert_ne!(ic, InflationClass::NoAssertions);
    }

    #[test]
    fn inflation_class_same_variable_both_sides_is_distinct() {
        assert_ne!(
            InflationClass::SameVariableBothSides,
            InflationClass::Tautological
        );
    }

    #[test]
    fn inflation_class_no_assertions_is_distinct() {
        assert_ne!(
            InflationClass::NoAssertions,
            InflationClass::SilentResultDiscard
        );
    }

    #[test]
    fn inflation_class_silent_result_discard_is_distinct() {
        assert_ne!(
            InflationClass::SilentResultDiscard,
            InflationClass::LiteralConstantCondition
        );
    }

    #[test]
    fn inflation_class_literal_constant_condition_is_distinct() {
        assert_ne!(
            InflationClass::LiteralConstantCondition,
            InflationClass::Tautological
        );
    }

    // -----------------------------------------------------------------------
    // Policy enforcement — min_behavior_bearing
    // -----------------------------------------------------------------------

    #[test]
    fn verifier_passes_module_with_behavioral_tests() {
        let verifier = TaxonomyVerifier::with_default_policy();
        let descriptors = vec![behavioral("test_basic_flow"), behavioral("test_edge_case")];
        let report = verifier.verify_module("my::module", &descriptors).unwrap();
        assert!(
            report.is_passing(),
            "expected passing: {:?}",
            report.rejection
        );
    }

    #[test]
    fn verifier_passes_module_with_property_tests() {
        let verifier = TaxonomyVerifier::with_default_policy();
        let report = verifier
            .verify_module("my::module", &[property_test("prop_does_not_panic")])
            .unwrap();
        assert!(report.is_passing());
    }

    #[test]
    fn verifier_rejects_empty_module() {
        let verifier = TaxonomyVerifier::with_default_policy();
        let report = verifier.verify_module("my::module", &[]).unwrap();
        assert!(!report.is_passing());
        assert_eq!(report.rejection, Some(RejectionReason::NoBehavioralTests));
    }

    #[test]
    fn verifier_rejects_doctest_only_module_by_default() {
        let verifier = TaxonomyVerifier::with_default_policy();
        let report = verifier
            .verify_module("my::module", &[doctest("example_usage")])
            .unwrap();
        // Default policy: doctests_count_as_behavioral = false → no behavior bearing.
        assert!(!report.is_passing());
    }

    #[test]
    fn verifier_passes_doctest_when_policy_allows_it() {
        let policy = TaxonomyPolicy {
            min_behavior_bearing: 1,
            max_smoke_fraction: 0.5,
            doctests_count_as_behavioral: true,
            max_excluded: 5,
        };
        let verifier = TaxonomyVerifier::new(policy);
        let report = verifier
            .verify_module("my::module", &[doctest("example_usage")])
            .unwrap();
        assert!(report.is_passing());
    }

    // -----------------------------------------------------------------------
    // Policy enforcement — SmokeOnlyModule
    // -----------------------------------------------------------------------

    #[test]
    fn verifier_rejects_smoke_only_module() {
        let verifier = TaxonomyVerifier::with_default_policy();
        let descriptors = vec![smoke("test_happy_path")];
        let report = verifier.verify_module("my::module", &descriptors).unwrap();
        assert!(!report.is_passing());
        assert!(matches!(
            report.rejection,
            Some(RejectionReason::SmokeOnlyModule | RejectionReason::NoBehavioralTests)
        ));
    }

    #[test]
    fn verifier_passes_module_with_smoke_and_behavioral() {
        let verifier = TaxonomyVerifier::with_default_policy();
        let descriptors = vec![behavioral("test_real"), smoke("test_smoke")];
        let report = verifier.verify_module("my::module", &descriptors).unwrap();
        assert!(report.is_passing(), "{:?}", report.rejection);
    }

    // -----------------------------------------------------------------------
    // Policy enforcement — TooManySmoke
    // -----------------------------------------------------------------------

    #[test]
    fn verifier_rejects_when_smoke_fraction_exceeds_policy() {
        let policy = TaxonomyPolicy {
            min_behavior_bearing: 1,
            max_smoke_fraction: 0.1,
            doctests_count_as_behavioral: false,
            max_excluded: 5,
        };
        let verifier = TaxonomyVerifier::new(policy);
        // 1 behavioral, 9 smoke → smoke_fraction = 0.9 > 0.1
        let mut descriptors = vec![behavioral("real")];
        for i in 0..9 {
            descriptors.push(smoke(&format!("smoke_{i}")));
        }
        let report = verifier.verify_module("my::module", &descriptors).unwrap();
        assert_eq!(report.rejection, Some(RejectionReason::TooManySmoke));
    }

    // -----------------------------------------------------------------------
    // Policy enforcement — TooManyExcluded
    // -----------------------------------------------------------------------

    #[test]
    fn verifier_too_many_excluded_triggers_rejection() {
        let policy = TaxonomyPolicy {
            min_behavior_bearing: 1,
            max_smoke_fraction: 0.5,
            doctests_count_as_behavioral: false,
            max_excluded: 1,
        };
        let verifier = TaxonomyVerifier::new(policy);
        let mut descriptors = vec![behavioral("test_real")];
        for i in 0..3usize {
            descriptors.push(excluded(&format!("ex_{i}")));
        }
        let report = verifier.verify_module("my::module", &descriptors).unwrap();
        assert_eq!(report.rejection, Some(RejectionReason::TooManyExcluded));
    }

    #[test]
    fn verifier_passes_when_excluded_at_policy_limit() {
        let policy = TaxonomyPolicy {
            min_behavior_bearing: 1,
            max_smoke_fraction: 0.5,
            doctests_count_as_behavioral: false,
            max_excluded: 3,
        };
        let verifier = TaxonomyVerifier::new(policy);
        let descriptors = vec![
            behavioral("test_real"),
            excluded("ex_1"),
            excluded("ex_2"),
            excluded("ex_3"),
        ];
        let report = verifier.verify_module("my::module", &descriptors).unwrap();
        assert!(report.is_passing(), "{:?}", report.rejection);
    }

    // -----------------------------------------------------------------------
    // Policy enforcement — VacuousTestInflation
    // -----------------------------------------------------------------------

    #[test]
    fn verifier_flags_vacuous_pattern_passes_trivially() {
        let verifier = TaxonomyVerifier::with_default_policy();
        let good = behavioral("test_real_behaviour");
        let vacuous_descriptor = TestDescriptor::builder("test_x_passes_trivially", "my::module")
            .kind(TestKind::Behavioral)
            .cluster_role(ClusterRole::Verifier)
            .build()
            .map_err(|e| e.to_string())
            .unwrap();
        let descriptors = vec![good, vacuous_descriptor];
        let report = verifier.verify_module("my::module", &descriptors).unwrap();
        assert!(
            !report.is_passing(),
            "vacuous pattern should cause rejection"
        );
        assert_eq!(
            report.rejection,
            Some(RejectionReason::VacuousTestInflation)
        );
    }

    #[test]
    fn verifier_flags_vacuous_pattern_assert_true() {
        let verifier = TaxonomyVerifier::with_default_policy();
        let good = behavioral("test_real");
        let vac = TestDescriptor::builder("test_y_assert_true", "my::module")
            .kind(TestKind::Behavioral)
            .cluster_role(ClusterRole::Verifier)
            .build()
            .map_err(|e| e.to_string())
            .unwrap();
        let report = verifier.verify_module("my::module", &[good, vac]).unwrap();
        assert_eq!(
            report.rejection,
            Some(RejectionReason::VacuousTestInflation)
        );
        let rejected = report.rejected_tests();
        assert_eq!(rejected.len(), 1);
        assert!(rejected[0].descriptor.test_name.contains("assert_true"));
    }

    #[test]
    fn verifier_flags_no_assertions_name() {
        let verifier = TaxonomyVerifier::with_default_policy();
        let good = behavioral("test_real");
        let vac = TestDescriptor::builder("test_z_no_assertions", "m")
            .kind(TestKind::Behavioral)
            .cluster_role(ClusterRole::Verifier)
            .build()
            .map_err(|e| e.to_string())
            .unwrap();
        let report = verifier.verify_module("m", &[good, vac]).unwrap();
        assert_eq!(report.vacuous_count(), 1);
    }

    #[test]
    fn verifier_inflation_class_tautological_for_assert_true_name() {
        let verifier = TaxonomyVerifier::with_default_policy();
        let good = behavioral("real");
        let vac = TestDescriptor::builder("test_assert_true_foo", "m")
            .kind(TestKind::Behavioral)
            .cluster_role(ClusterRole::Verifier)
            .build()
            .unwrap();
        let report = verifier.verify_module("m", &[good, vac]).unwrap();
        let rejected = report.rejected_tests();
        assert_eq!(rejected.len(), 1);
        assert_eq!(
            rejected[0].inflation_class,
            Some(InflationClass::Tautological)
        );
    }

    #[test]
    fn verifier_clean_test_has_passing_verdict() {
        let verifier = TaxonomyVerifier::with_default_policy();
        let report = verifier
            .verify_module("m", &[behavioral("test_clean")])
            .unwrap();
        assert!(report
            .passing_tests()
            .iter()
            .any(|e| e.verdict == TestVerdict::Passing));
    }

    // -----------------------------------------------------------------------
    // Error conditions
    // -----------------------------------------------------------------------

    #[test]
    fn verifier_rejects_module_path_empty() {
        let verifier = TaxonomyVerifier::with_default_policy();
        assert!(verifier.verify_module("", &[behavioral("t")]).is_err());
    }

    // -----------------------------------------------------------------------
    // Report content
    // -----------------------------------------------------------------------

    #[test]
    fn verifier_report_rationale_is_nonempty() {
        let verifier = TaxonomyVerifier::with_default_policy();
        let report = verifier
            .verify_module("my::module", &[behavioral("t")])
            .unwrap();
        assert!(!report.rationale.is_empty());
    }

    #[test]
    fn verifier_report_rationale_contains_module_path() {
        let verifier = TaxonomyVerifier::with_default_policy();
        let report = verifier
            .verify_module("special::path", &[behavioral("t")])
            .unwrap();
        assert!(report.rationale.contains("special::path"));
    }

    #[test]
    fn verifier_rejected_tests_subset_of_entries() {
        let verifier = TaxonomyVerifier::with_default_policy();
        let good = behavioral("test_real");
        let vacuous = TestDescriptor::builder("test_y_assert_true", "my::module")
            .kind(TestKind::Behavioral)
            .cluster_role(ClusterRole::Verifier)
            .build()
            .map_err(|e| e.to_string())
            .unwrap();
        let report = verifier
            .verify_module("my::module", &[good, vacuous])
            .unwrap();
        let rejected = report.rejected_tests();
        assert_eq!(rejected.len(), 1);
        assert!(rejected[0].descriptor.test_name.contains("assert_true"));
    }

    #[test]
    fn taxonomy_report_labels_count_matches_entries() {
        let verifier = TaxonomyVerifier::with_default_policy();
        let report = verifier
            .verify_module("my::module", &[behavioral("t1"), behavioral("t2")])
            .unwrap();
        assert_eq!(report.labels().len(), report.test_entries.len());
    }

    #[test]
    fn is_module_passing_delegates_to_report() {
        let verifier = TaxonomyVerifier::with_default_policy();
        let report = verifier.verify_module("m", &[behavioral("t")]).unwrap();
        assert!(verifier.is_module_passing(&report));
    }

    #[test]
    fn vacuous_count_on_report_matches_flagged_entries() {
        let verifier = TaxonomyVerifier::with_default_policy();
        let good = behavioral("test_real");
        let vac = TestDescriptor::builder("test_z_no_assertions", "m")
            .kind(TestKind::Behavioral)
            .cluster_role(ClusterRole::Verifier)
            .build()
            .map_err(|e| e.to_string())
            .unwrap();
        let report = verifier.verify_module("m", &[good, vac]).unwrap();
        assert_eq!(report.vacuous_count(), 1);
    }

    // -----------------------------------------------------------------------
    // Workspace verify
    // -----------------------------------------------------------------------

    #[test]
    fn verifier_workspace_returns_one_report_per_module() {
        let verifier = TaxonomyVerifier::with_default_policy();
        let modules = vec![
            ("mod_a", vec![behavioral("test_one")]),
            ("mod_b", vec![behavioral("test_two")]),
        ];
        let reports = verifier.verify_workspace(&modules).unwrap();
        assert_eq!(reports.len(), 2);
    }

    #[test]
    fn verifier_workspace_mixed_pass_fail() {
        let verifier = TaxonomyVerifier::with_default_policy();
        let modules = vec![
            ("good_mod", vec![behavioral("test_one")]),
            ("bad_mod", vec![]),
        ];
        let reports = verifier.verify_workspace(&modules).unwrap();
        assert!(reports[0].is_passing());
        assert!(!reports[1].is_passing());
    }

    // -----------------------------------------------------------------------
    // ModuleTestProfile computation
    // -----------------------------------------------------------------------

    #[test]
    fn module_profile_counts_behavioral_correctly() {
        let verifier = TaxonomyVerifier::with_default_policy();
        let descriptors = vec![behavioral("a"), behavioral("b"), smoke("s")];
        let report = verifier.verify_module("m", &descriptors).unwrap();
        assert_eq!(report.profile.behavioral_count, 2);
        assert_eq!(report.profile.smoke_count, 1);
    }

    #[test]
    fn module_profile_smoke_fraction_computed_correctly() {
        let verifier = TaxonomyVerifier::with_default_policy();
        // 1 behavioral, 1 smoke → fraction = 0.5
        let descriptors = vec![behavioral("a"), smoke("s")];
        let report = verifier.verify_module("m", &descriptors).unwrap();
        let expected = 1.0 / 2.0;
        assert!(
            (report.profile.smoke_fraction - expected).abs() < 1e-9,
            "smoke_fraction mismatch: {} vs {}",
            report.profile.smoke_fraction,
            expected
        );
    }

    #[test]
    fn module_profile_excluded_count_correct() {
        let verifier = TaxonomyVerifier::with_default_policy();
        let descriptors = vec![behavioral("a"), excluded("ex_1"), excluded("ex_2")];
        let report = verifier.verify_module("m", &descriptors).unwrap();
        assert_eq!(report.profile.excluded_count, 2);
    }

    #[test]
    fn module_profile_property_count_correct() {
        let verifier = TaxonomyVerifier::with_default_policy();
        let descriptors = vec![property_test("prop_a"), property_test("prop_b")];
        let report = verifier.verify_module("m", &descriptors).unwrap();
        assert_eq!(report.profile.property_count, 2);
        assert_eq!(report.profile.behavior_bearing, 2);
    }

    // -----------------------------------------------------------------------
    // Policy override
    // -----------------------------------------------------------------------

    #[test]
    fn custom_policy_min_behavior_bearing_five_rejects_with_four() {
        let policy = TaxonomyPolicy {
            min_behavior_bearing: 5,
            max_smoke_fraction: 0.5,
            doctests_count_as_behavioral: false,
            max_excluded: 5,
        };
        let verifier = TaxonomyVerifier::new(policy);
        let descriptors: Vec<_> = (0..4).map(|i| behavioral(&format!("t{i}"))).collect();
        let report = verifier.verify_module("m", &descriptors).unwrap();
        // 4 behavioral < 5 minimum
        assert!(!report.is_passing());
    }

    #[test]
    fn custom_policy_min_behavior_bearing_five_passes_with_five() {
        let policy = TaxonomyPolicy {
            min_behavior_bearing: 5,
            max_smoke_fraction: 0.5,
            doctests_count_as_behavioral: false,
            max_excluded: 5,
        };
        let verifier = TaxonomyVerifier::new(policy);
        let descriptors: Vec<_> = (0..5).map(|i| behavioral(&format!("t{i}"))).collect();
        let report = verifier.verify_module("m", &descriptors).unwrap();
        assert!(report.is_passing(), "{:?}", report.rejection);
    }

    // -----------------------------------------------------------------------
    // ModuleTestProfile: various combinations
    // -----------------------------------------------------------------------

    #[test]
    fn module_profile_total_includes_all_kinds() {
        let verifier = TaxonomyVerifier::with_default_policy();
        let descriptors = vec![behavioral("b"), smoke("s"), excluded("ex"), doctest("dt")];
        let report = verifier.verify_module("m", &descriptors).unwrap();
        assert_eq!(report.profile.total, 4);
    }

    #[test]
    fn module_profile_behavior_bearing_includes_property() {
        let verifier = TaxonomyVerifier::with_default_policy();
        let descriptors = vec![property_test("prop"), property_test("prop2")];
        let report = verifier.verify_module("m", &descriptors).unwrap();
        assert_eq!(report.profile.behavior_bearing, 2);
        assert_eq!(report.profile.property_count, 2);
    }

    #[test]
    fn module_profile_smoke_fraction_zero_with_no_smoke() {
        let verifier = TaxonomyVerifier::with_default_policy();
        let report = verifier.verify_module("m", &[behavioral("b")]).unwrap();
        assert_eq!(report.profile.smoke_fraction, 0.0);
    }

    #[test]
    fn module_profile_vacuous_count_zero_for_clean_suite() {
        let verifier = TaxonomyVerifier::with_default_policy();
        let report = verifier
            .verify_module("m", &[behavioral("clean_test")])
            .unwrap();
        assert_eq!(report.profile.vacuous_count, 0);
    }

    // -----------------------------------------------------------------------
    // TaxonomyReport: passing_tests and rejected_tests
    // -----------------------------------------------------------------------

    #[test]
    fn report_passing_tests_nonempty_when_clean_suite() {
        let verifier = TaxonomyVerifier::with_default_policy();
        let report = verifier
            .verify_module("m", &[behavioral("t1"), behavioral("t2")])
            .unwrap();
        assert_eq!(report.passing_tests().len(), 2);
    }

    #[test]
    fn report_rejected_tests_empty_when_clean_suite() {
        let verifier = TaxonomyVerifier::with_default_policy();
        let report = verifier.verify_module("m", &[behavioral("t1")]).unwrap();
        assert!(report.rejected_tests().is_empty());
    }

    #[test]
    fn report_is_passing_true_for_clean_module() {
        let verifier = TaxonomyVerifier::with_default_policy();
        let report = verifier.verify_module("m", &[behavioral("t")]).unwrap();
        assert!(report.is_passing());
        assert_eq!(report.rejection, None);
    }

    #[test]
    fn report_module_path_preserved() {
        let verifier = TaxonomyVerifier::with_default_policy();
        let report = verifier
            .verify_module("crate::my_module::sub", &[behavioral("t")])
            .unwrap();
        assert_eq!(report.module_path, "crate::my_module::sub");
    }

    // -----------------------------------------------------------------------
    // Smoke fraction boundary cases
    // -----------------------------------------------------------------------

    #[test]
    fn verifier_smoke_fraction_exactly_at_threshold_passes() {
        // Default max_smoke_fraction = 0.5; 1 behavioral + 1 smoke = 0.5 exactly → pass.
        let verifier = TaxonomyVerifier::with_default_policy();
        let report = verifier
            .verify_module("m", &[behavioral("b"), smoke("s")])
            .unwrap();
        assert!(report.is_passing(), "{:?}", report.rejection);
    }

    #[test]
    fn verifier_smoke_fraction_just_above_threshold_fails() {
        // Policy: max_smoke_fraction=0.49; 1 behavioral + 1 smoke = 0.5 > 0.49 → fail.
        let policy = TaxonomyPolicy {
            min_behavior_bearing: 1,
            max_smoke_fraction: 0.49,
            doctests_count_as_behavioral: false,
            max_excluded: 5,
        };
        let verifier = TaxonomyVerifier::new(policy);
        let report = verifier
            .verify_module("m", &[behavioral("b"), smoke("s")])
            .unwrap();
        assert_eq!(report.rejection, Some(RejectionReason::TooManySmoke));
    }

    // -----------------------------------------------------------------------
    // VacuousPattern→InflationClass mapping completeness
    // -----------------------------------------------------------------------

    #[test]
    fn inflation_class_no_assertions_returned_for_no_assertions_name() {
        let verifier = TaxonomyVerifier::with_default_policy();
        let good = behavioral("real");
        let vac = TestDescriptor::builder("test_no_assertions_scenario", "m")
            .kind(TestKind::Behavioral)
            .cluster_role(ClusterRole::Verifier)
            .build()
            .unwrap();
        let report = verifier.verify_module("m", &[good, vac]).unwrap();
        let rejected = report.rejected_tests();
        assert_eq!(rejected.len(), 1);
        assert_eq!(
            rejected[0].inflation_class,
            Some(InflationClass::NoAssertions)
        );
    }

    // -----------------------------------------------------------------------
    // Workspace: empty module list produces empty report list
    // -----------------------------------------------------------------------

    #[test]
    fn verifier_workspace_empty_modules_returns_empty() {
        let verifier = TaxonomyVerifier::with_default_policy();
        let reports = verifier.verify_workspace(&[]).unwrap();
        assert!(reports.is_empty());
    }
}
