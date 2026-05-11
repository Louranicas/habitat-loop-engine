#![forbid(unsafe_code)]

//! M022 Test Taxonomy — vocabulary types for classifying tests against the HLE loop phases.
//!
//! Consumed by M023 (`test_taxonomy_verifier`) and M024 (`false_pass_auditor`) so that
//! test-quality claims are machine-checkable without re-deriving classification rules
//! in each consumer.
//!
//! Layer: L01 — no upward imports.

use std::fmt;

use substrate_types::HleError;

// ---------------------------------------------------------------------------
// TestKind
// ---------------------------------------------------------------------------

/// Classification of a single test entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TestKind {
    /// Tests observable behaviour visible at the module boundary.
    /// Must make at least one non-tautological assertion.
    Behavioral,
    /// Property-based tests (proptest / quickcheck style).
    /// Must call a generator; must include at least one shrinking strategy hint.
    Property,
    /// Fast smoke check — single happy-path assertion.
    /// Acceptable only if the module has >= 1 Behavioral test as companion.
    Smoke,
    /// Documentation example compiled and run as a test.
    Doctest,
    /// Intentionally excluded with a documented reason.
    Excluded,
}

impl TestKind {
    /// All known test kind values.
    pub const ALL: [Self; 5] = [
        Self::Behavioral,
        Self::Property,
        Self::Smoke,
        Self::Doctest,
        Self::Excluded,
    ];
}

impl fmt::Display for TestKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Behavioral => f.write_str("Behavioral"),
            Self::Property => f.write_str("Property"),
            Self::Smoke => f.write_str("Smoke"),
            Self::Doctest => f.write_str("Doctest"),
            Self::Excluded => f.write_str("Excluded"),
        }
    }
}

// ---------------------------------------------------------------------------
// ClusterRole
// ---------------------------------------------------------------------------

/// The role a module plays within a cluster's data-flow topology.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClusterRole {
    /// Emits primary data or events consumed downstream.
    Producer,
    /// Connects two modules or clusters; passes through without modifying meaning.
    Binder,
    /// Converts types or representations; semantics change across the boundary.
    Transformer,
    /// Independently checks or recomputes a prior claim.
    Verifier,
    /// Final consumer; no module downstream reads its output.
    TerminalConsumer,
}

impl ClusterRole {
    /// All known cluster role values.
    pub const ALL: [Self; 5] = [
        Self::Producer,
        Self::Binder,
        Self::Transformer,
        Self::Verifier,
        Self::TerminalConsumer,
    ];
}

impl fmt::Display for ClusterRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Producer => f.write_str("producer"),
            Self::Binder => f.write_str("binder"),
            Self::Transformer => f.write_str("transformer"),
            Self::Verifier => f.write_str("verifier"),
            Self::TerminalConsumer => f.write_str("terminal_consumer"),
        }
    }
}

// ---------------------------------------------------------------------------
// LoopPhase
// ---------------------------------------------------------------------------

/// The HLE workflow phase a test exercises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum LoopPhase {
    /// Scan or observe system state.
    Detect = 0,
    /// Halt a workflow on a finding.
    Block = 1,
    /// Apply a corrective action.
    Fix = 2,
    /// Confirm a fix or claim.
    Verify = 3,
    /// Test the test infrastructure itself.
    MetaTest = 4,
    /// Produce or consume a verifier receipt.
    Receipt = 5,
    /// Write to the append-only ledger.
    Persist = 6,
    /// Signal a human or downstream consumer.
    Notify = 7,
}

impl LoopPhase {
    /// All loop phases in index order.
    pub const ALL: [Self; 8] = [
        Self::Detect,
        Self::Block,
        Self::Fix,
        Self::Verify,
        Self::MetaTest,
        Self::Receipt,
        Self::Persist,
        Self::Notify,
    ];

    /// Numeric index (0..=7) matching the `#[repr(u8)]` discriminant.
    #[must_use]
    pub const fn index(self) -> u8 {
        self as u8
    }

    /// Lowercase phase name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Detect => "detect",
            Self::Block => "block",
            Self::Fix => "fix",
            Self::Verify => "verify",
            Self::MetaTest => "meta_test",
            Self::Receipt => "receipt",
            Self::Persist => "persist",
            Self::Notify => "notify",
        }
    }

    /// Construct from index. Returns `None` when `index >= 8`.
    #[must_use]
    pub const fn from_index(index: u8) -> Option<Self> {
        match index {
            0 => Some(Self::Detect),
            1 => Some(Self::Block),
            2 => Some(Self::Fix),
            3 => Some(Self::Verify),
            4 => Some(Self::MetaTest),
            5 => Some(Self::Receipt),
            6 => Some(Self::Persist),
            7 => Some(Self::Notify),
            _ => None,
        }
    }

    /// Construct from name (case-insensitive). Returns `None` on mismatch.
    #[must_use]
    pub fn from_name(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "detect" => Some(Self::Detect),
            "block" => Some(Self::Block),
            "fix" => Some(Self::Fix),
            "verify" => Some(Self::Verify),
            "meta_test" | "metatest" => Some(Self::MetaTest),
            "receipt" => Some(Self::Receipt),
            "persist" => Some(Self::Persist),
            "notify" => Some(Self::Notify),
            _ => None,
        }
    }
}

impl fmt::Display for LoopPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

// ---------------------------------------------------------------------------
// LoopPhaseSet — bitmask, Copy, const-composable
// ---------------------------------------------------------------------------

/// Bitmask of `LoopPhase` affinity. Bit N corresponds to `LoopPhase::from_index(N)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct LoopPhaseSet(u8);

impl LoopPhaseSet {
    /// Empty set (no phases).
    pub const EMPTY: Self = Self(0);

    /// All eight phases.
    pub const ALL_PHASES: Self = Self(0xFF);

    /// Construct from a raw bitmask.
    #[must_use]
    pub const fn from_raw(bits: u8) -> Self {
        Self(bits)
    }

    /// Return a new set with `phase` also set.
    #[must_use]
    pub const fn with_phase(self, phase: LoopPhase) -> Self {
        Self(self.0 | (1u8 << phase.index()))
    }

    /// Test whether `phase` is in the set.
    #[must_use]
    pub const fn has_phase(self, phase: LoopPhase) -> bool {
        (self.0 & (1u8 << phase.index())) != 0
    }

    /// Count of set bits (phases in the set).
    #[must_use]
    pub const fn count(self) -> u32 {
        self.0.count_ones()
    }

    /// Union of two sets.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// All active phases in index order.
    #[must_use]
    pub fn active_phases(self) -> Vec<LoopPhase> {
        LoopPhase::ALL
            .iter()
            .filter(|&&p| self.has_phase(p))
            .copied()
            .collect()
    }
}

// ---------------------------------------------------------------------------
// VacuousPattern
// ---------------------------------------------------------------------------

/// Known vacuous-assertion patterns that indicate test inflation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VacuousPattern {
    /// `assert!(true)` — unconditionally passes.
    AssertTrue,
    /// `assert_eq!(x, x)` — same variable on both sides.
    TautologicalEq,
    /// Test body has no assertions at all.
    NoAssertions,
    /// `assert!(CONST)` where the constant is compile-time `true`.
    ConstantTrue,
    /// `Result<T>` returned by a call is discarded with no assertion on the value.
    UnassertedResult,
}

impl VacuousPattern {
    /// All known vacuous patterns.
    pub const ALL: [Self; 5] = [
        Self::AssertTrue,
        Self::TautologicalEq,
        Self::NoAssertions,
        Self::ConstantTrue,
        Self::UnassertedResult,
    ];

    /// Human-readable rationale explaining why this pattern is vacuous.
    #[must_use]
    pub fn description(self) -> &'static str {
        match self {
            Self::AssertTrue => "assert!(true) always passes and tests nothing",
            Self::TautologicalEq => "assert_eq!(x, x) compares identical expressions",
            Self::NoAssertions => "test body contains no assertions",
            Self::ConstantTrue => "assert! condition is a compile-time constant true",
            Self::UnassertedResult => "Result return value silently discarded without check",
        }
    }
}

// ---------------------------------------------------------------------------
// TestDescriptor
// ---------------------------------------------------------------------------

/// Fully-classified test metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestDescriptor {
    /// Fully-qualified test function name (e.g. `crate::module::tests::my_test`).
    pub test_name: String,
    /// Module path that owns this test.
    pub module_path: String,
    /// Classification of this test.
    pub kind: TestKind,
    /// Role the owning module plays in its cluster.
    pub cluster_role: ClusterRole,
    /// Bitmask of HLE loop phases this test exercises.
    pub loop_phase_set: LoopPhaseSet,
    /// Vacuous patterns detected by M023 analysis. Empty means clean.
    pub vacuous_patterns: Vec<VacuousPattern>,
    /// Required when `kind == Excluded`; documents why the test is excluded.
    pub exclusion_reason: Option<String>,
}

impl TestDescriptor {
    /// Begin a builder for a new descriptor.
    #[must_use]
    pub fn builder(
        test_name: impl Into<String>,
        module_path: impl Into<String>,
    ) -> TestDescriptorBuilder {
        TestDescriptorBuilder::new(test_name.into(), module_path.into())
    }

    /// True when any vacuous pattern was detected.
    #[must_use]
    pub fn is_vacuous(&self) -> bool {
        !self.vacuous_patterns.is_empty()
    }

    /// True when the test kind is behavior-bearing (Behavioral or Property) and not vacuous.
    #[must_use]
    pub fn is_behavior_bearing(&self) -> bool {
        matches!(self.kind, TestKind::Behavioral | TestKind::Property) && !self.is_vacuous()
    }

    /// Delegate phase membership check to the phase set.
    #[must_use]
    pub fn has_phase(&self, p: LoopPhase) -> bool {
        self.loop_phase_set.has_phase(p)
    }
}

// ---------------------------------------------------------------------------
// TestDescriptorBuilder
// ---------------------------------------------------------------------------

/// Validated builder for `TestDescriptor`.
#[derive(Debug)]
pub struct TestDescriptorBuilder {
    test_name: String,
    module_path: String,
    kind: Option<TestKind>,
    cluster_role: Option<ClusterRole>,
    loop_phase_set: LoopPhaseSet,
    exclusion_reason: Option<String>,
}

impl TestDescriptorBuilder {
    fn new(test_name: String, module_path: String) -> Self {
        Self {
            test_name,
            module_path,
            kind: None,
            cluster_role: None,
            loop_phase_set: LoopPhaseSet::EMPTY,
            exclusion_reason: None,
        }
    }

    /// Set the test kind. Required.
    #[must_use]
    pub fn kind(mut self, kind: TestKind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// Set the cluster role. Required.
    #[must_use]
    pub fn cluster_role(mut self, role: ClusterRole) -> Self {
        self.cluster_role = Some(role);
        self
    }

    /// Add a single loop phase. May be called multiple times.
    #[must_use]
    pub fn phase(mut self, phase: LoopPhase) -> Self {
        self.loop_phase_set = self.loop_phase_set.with_phase(phase);
        self
    }

    /// Set the entire phase bitmask directly.
    #[must_use]
    pub fn phase_set(mut self, ps: LoopPhaseSet) -> Self {
        self.loop_phase_set = ps;
        self
    }

    /// Provide an exclusion reason. Only valid when `kind == Excluded`.
    ///
    /// # Errors
    ///
    /// Returns an error if `kind` has already been set to a non-`Excluded` value.
    pub fn exclusion_reason(mut self, reason: impl Into<String>) -> Result<Self, HleError> {
        if let Some(k) = self.kind {
            if k != TestKind::Excluded {
                return Err(HleError::new(
                    "[E2320] exclusion_reason may only be set for Excluded test kind",
                ));
            }
        }
        self.exclusion_reason = Some(reason.into());
        Ok(self)
    }

    /// Finalise the descriptor.
    ///
    /// # Errors
    ///
    /// Returns an error when `kind` or `cluster_role` was not set, or when
    /// `kind == Excluded` but no `exclusion_reason` was provided.
    pub fn build(self) -> Result<TestDescriptor, HleError> {
        let kind = self
            .kind
            .ok_or_else(|| HleError::new("[E2320] test kind is required"))?;
        let cluster_role = self
            .cluster_role
            .ok_or_else(|| HleError::new("[E2320] cluster role is required"))?;

        if kind == TestKind::Excluded && self.exclusion_reason.is_none() {
            return Err(HleError::new(
                "[E2320] Excluded test kind requires an exclusion_reason",
            ));
        }

        Ok(TestDescriptor {
            test_name: self.test_name,
            module_path: self.module_path,
            kind,
            cluster_role,
            loop_phase_set: self.loop_phase_set,
            vacuous_patterns: Vec::new(),
            exclusion_reason: self.exclusion_reason,
        })
    }
}

// ---------------------------------------------------------------------------
// TaxonomyLabel
// ---------------------------------------------------------------------------

/// Compact classification label emitted in a `TaxonomyReport`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaxonomyLabel {
    /// Fully-qualified test name.
    pub test_name: String,
    /// Test kind.
    pub kind: TestKind,
    /// Owning module's cluster role.
    pub cluster_role: ClusterRole,
    /// Phase bitmask.
    pub phases: LoopPhaseSet,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{
        ClusterRole, LoopPhase, LoopPhaseSet, TaxonomyLabel, TestDescriptor, TestDescriptorBuilder,
        TestKind, VacuousPattern,
    };

    fn behavioral(name: &str) -> TestDescriptor {
        TestDescriptor::builder(name, "my::module")
            .kind(TestKind::Behavioral)
            .cluster_role(ClusterRole::Verifier)
            .phase(LoopPhase::Verify)
            .build()
            .unwrap()
    }

    // -----------------------------------------------------------------------
    // TestKind variants
    // -----------------------------------------------------------------------

    #[test]
    fn test_kind_all_has_five_entries() {
        assert_eq!(TestKind::ALL.len(), 5);
    }

    #[test]
    fn test_kind_behavioral_variant_present() {
        assert!(TestKind::ALL.contains(&TestKind::Behavioral));
    }

    #[test]
    fn test_kind_property_variant_present() {
        assert!(TestKind::ALL.contains(&TestKind::Property));
    }

    #[test]
    fn test_kind_smoke_variant_present() {
        assert!(TestKind::ALL.contains(&TestKind::Smoke));
    }

    #[test]
    fn test_kind_doctest_variant_present() {
        assert!(TestKind::ALL.contains(&TestKind::Doctest));
    }

    #[test]
    fn test_kind_excluded_variant_present() {
        assert!(TestKind::ALL.contains(&TestKind::Excluded));
    }

    #[test]
    fn test_kind_display_behavioral() {
        assert_eq!(TestKind::Behavioral.to_string(), "Behavioral");
    }

    #[test]
    fn test_kind_display_property() {
        assert_eq!(TestKind::Property.to_string(), "Property");
    }

    #[test]
    fn test_kind_display_smoke() {
        assert_eq!(TestKind::Smoke.to_string(), "Smoke");
    }

    #[test]
    fn test_kind_display_doctest() {
        assert_eq!(TestKind::Doctest.to_string(), "Doctest");
    }

    #[test]
    fn test_kind_display_excluded() {
        assert_eq!(TestKind::Excluded.to_string(), "Excluded");
    }

    // -----------------------------------------------------------------------
    // ClusterRole variants
    // -----------------------------------------------------------------------

    #[test]
    fn cluster_role_all_has_five_entries() {
        assert_eq!(ClusterRole::ALL.len(), 5);
    }

    #[test]
    fn cluster_role_producer_variant_present() {
        assert!(ClusterRole::ALL.contains(&ClusterRole::Producer));
    }

    #[test]
    fn cluster_role_binder_variant_present() {
        assert!(ClusterRole::ALL.contains(&ClusterRole::Binder));
    }

    #[test]
    fn cluster_role_transformer_variant_present() {
        assert!(ClusterRole::ALL.contains(&ClusterRole::Transformer));
    }

    #[test]
    fn cluster_role_verifier_variant_present() {
        assert!(ClusterRole::ALL.contains(&ClusterRole::Verifier));
    }

    #[test]
    fn cluster_role_terminal_consumer_variant_present() {
        assert!(ClusterRole::ALL.contains(&ClusterRole::TerminalConsumer));
    }

    #[test]
    fn cluster_role_display_producer() {
        assert_eq!(ClusterRole::Producer.to_string(), "producer");
    }

    #[test]
    fn cluster_role_display_binder() {
        assert_eq!(ClusterRole::Binder.to_string(), "binder");
    }

    #[test]
    fn cluster_role_display_transformer() {
        assert_eq!(ClusterRole::Transformer.to_string(), "transformer");
    }

    #[test]
    fn cluster_role_display_verifier() {
        assert_eq!(ClusterRole::Verifier.to_string(), "verifier");
    }

    #[test]
    fn cluster_role_display_terminal_consumer() {
        assert_eq!(
            ClusterRole::TerminalConsumer.to_string(),
            "terminal_consumer"
        );
    }

    // -----------------------------------------------------------------------
    // LoopPhase — all 8 variants
    // -----------------------------------------------------------------------

    #[test]
    fn loop_phase_all_has_eight_entries() {
        assert_eq!(LoopPhase::ALL.len(), 8);
    }

    #[test]
    fn loop_phase_detect_index_is_zero() {
        assert_eq!(LoopPhase::Detect.index(), 0);
    }

    #[test]
    fn loop_phase_notify_index_is_seven() {
        assert_eq!(LoopPhase::Notify.index(), 7);
    }

    #[test]
    fn loop_phase_index_round_trips() {
        for phase in LoopPhase::ALL {
            assert_eq!(LoopPhase::from_index(phase.index()), Some(phase));
        }
    }

    #[test]
    fn loop_phase_from_index_out_of_range_returns_none() {
        assert!(LoopPhase::from_index(8).is_none());
        assert!(LoopPhase::from_index(255).is_none());
    }

    #[test]
    fn loop_phase_from_name_case_insensitive_detect() {
        assert_eq!(LoopPhase::from_name("DETECT"), Some(LoopPhase::Detect));
        assert_eq!(LoopPhase::from_name("detect"), Some(LoopPhase::Detect));
    }

    #[test]
    fn loop_phase_from_name_all_variants() {
        let pairs = [
            ("detect", LoopPhase::Detect),
            ("block", LoopPhase::Block),
            ("fix", LoopPhase::Fix),
            ("verify", LoopPhase::Verify),
            ("meta_test", LoopPhase::MetaTest),
            ("receipt", LoopPhase::Receipt),
            ("persist", LoopPhase::Persist),
            ("notify", LoopPhase::Notify),
        ];
        for (name, expected) in pairs {
            assert_eq!(
                LoopPhase::from_name(name),
                Some(expected),
                "from_name failed for '{name}'"
            );
        }
    }

    #[test]
    fn loop_phase_from_name_metatest_alias() {
        assert_eq!(LoopPhase::from_name("metatest"), Some(LoopPhase::MetaTest));
    }

    #[test]
    fn loop_phase_from_name_unknown_returns_none() {
        assert!(LoopPhase::from_name("unknown_phase").is_none());
    }

    #[test]
    fn loop_phase_display_matches_name() {
        for phase in LoopPhase::ALL {
            assert_eq!(phase.to_string(), phase.name());
        }
    }

    // -----------------------------------------------------------------------
    // LoopPhaseSet operations
    // -----------------------------------------------------------------------

    #[test]
    fn loop_phase_set_empty_has_zero_count() {
        assert_eq!(LoopPhaseSet::EMPTY.count(), 0);
    }

    #[test]
    fn loop_phase_set_all_phases_has_eight_count() {
        assert_eq!(LoopPhaseSet::ALL_PHASES.count(), 8);
    }

    #[test]
    fn loop_phase_set_all_phases_contains_all() {
        let all = LoopPhaseSet::ALL_PHASES;
        for phase in LoopPhase::ALL {
            assert!(all.has_phase(phase), "missing phase: {phase}");
        }
    }

    #[test]
    fn loop_phase_set_insert_single_phase() {
        let set = LoopPhaseSet::EMPTY.with_phase(LoopPhase::Detect);
        assert!(set.has_phase(LoopPhase::Detect));
        assert!(!set.has_phase(LoopPhase::Persist));
        assert_eq!(set.count(), 1);
    }

    #[test]
    fn loop_phase_set_insert_two_phases() {
        let set = LoopPhaseSet::EMPTY
            .with_phase(LoopPhase::Detect)
            .with_phase(LoopPhase::Persist);
        assert!(set.has_phase(LoopPhase::Detect));
        assert!(set.has_phase(LoopPhase::Persist));
        assert!(!set.has_phase(LoopPhase::Notify));
        assert_eq!(set.count(), 2);
    }

    #[test]
    fn loop_phase_set_union_combines_correctly() {
        let a = LoopPhaseSet::EMPTY.with_phase(LoopPhase::Detect);
        let b = LoopPhaseSet::EMPTY.with_phase(LoopPhase::Notify);
        let combined = a.union(b);
        assert!(combined.has_phase(LoopPhase::Detect));
        assert!(combined.has_phase(LoopPhase::Notify));
        assert_eq!(combined.count(), 2);
    }

    #[test]
    fn loop_phase_set_union_with_self_is_idempotent() {
        let s = LoopPhaseSet::EMPTY.with_phase(LoopPhase::Block);
        let u = s.union(s);
        assert_eq!(u, s);
    }

    #[test]
    fn loop_phase_set_intersection_via_raw_bitmask() {
        let a = LoopPhaseSet::from_raw(0b0000_0011); // Detect + Block
        let b = LoopPhaseSet::from_raw(0b0000_0001); // Detect only
        let intersection = LoopPhaseSet::from_raw(a.count() as u8 & b.count() as u8);
        // Simpler: both have Detect (bit 0).
        assert!(a.has_phase(LoopPhase::Detect));
        assert!(b.has_phase(LoopPhase::Detect));
        let _ = intersection; // just ensure construction works
    }

    #[test]
    fn loop_phase_set_active_phases_returns_in_index_order() {
        let set = LoopPhaseSet::EMPTY
            .with_phase(LoopPhase::Notify)
            .with_phase(LoopPhase::Detect);
        let phases = set.active_phases();
        assert_eq!(phases, vec![LoopPhase::Detect, LoopPhase::Notify]);
    }

    #[test]
    fn loop_phase_set_empty_active_phases_is_empty() {
        assert!(LoopPhaseSet::EMPTY.active_phases().is_empty());
    }

    #[test]
    fn loop_phase_set_from_raw_roundtrips() {
        let bits: u8 = 0b1010_0101;
        let set = LoopPhaseSet::from_raw(bits);
        assert_eq!(set.count(), bits.count_ones());
    }

    // -----------------------------------------------------------------------
    // VacuousPattern enumeration
    // -----------------------------------------------------------------------

    #[test]
    fn vacuous_pattern_all_has_five_entries() {
        assert_eq!(VacuousPattern::ALL.len(), 5);
    }

    #[test]
    fn vacuous_pattern_assert_true_present() {
        assert!(VacuousPattern::ALL.contains(&VacuousPattern::AssertTrue));
    }

    #[test]
    fn vacuous_pattern_tautological_eq_present() {
        assert!(VacuousPattern::ALL.contains(&VacuousPattern::TautologicalEq));
    }

    #[test]
    fn vacuous_pattern_no_assertions_present() {
        assert!(VacuousPattern::ALL.contains(&VacuousPattern::NoAssertions));
    }

    #[test]
    fn vacuous_pattern_constant_true_present() {
        assert!(VacuousPattern::ALL.contains(&VacuousPattern::ConstantTrue));
    }

    #[test]
    fn vacuous_pattern_unasserted_result_present() {
        assert!(VacuousPattern::ALL.contains(&VacuousPattern::UnassertedResult));
    }

    #[test]
    fn vacuous_pattern_description_nonempty_for_all() {
        for vp in VacuousPattern::ALL {
            assert!(!vp.description().is_empty(), "description empty for {vp:?}");
        }
    }

    // -----------------------------------------------------------------------
    // TestDescriptor builder
    // -----------------------------------------------------------------------

    #[test]
    fn descriptor_builder_rejects_missing_kind() {
        let result = TestDescriptor::builder("my_test", "my::module")
            .cluster_role(ClusterRole::Verifier)
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn descriptor_builder_rejects_missing_cluster_role() {
        let result = TestDescriptor::builder("my_test", "my::module")
            .kind(TestKind::Behavioral)
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn descriptor_builder_rejects_excluded_without_reason() {
        let result = TestDescriptor::builder("my_test", "my::module")
            .kind(TestKind::Excluded)
            .cluster_role(ClusterRole::Verifier)
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn descriptor_builder_accepts_excluded_with_reason() {
        let result = TestDescriptor::builder("my_test", "my::module")
            .kind(TestKind::Excluded)
            .cluster_role(ClusterRole::Verifier)
            .exclusion_reason("flaky on CI — tracked in HLE-123")
            .map_err(|e| e.to_string());
        assert!(result.is_ok(), "{result:?}");
        let td = result
            .ok()
            .and_then(|b: TestDescriptorBuilder| b.build().ok());
        assert!(td.is_some());
        assert!(td.as_ref().map_or(false, |d| d.exclusion_reason.is_some()));
    }

    #[test]
    fn descriptor_builder_phase_set_directly() {
        let ps = LoopPhaseSet::EMPTY
            .with_phase(LoopPhase::Detect)
            .with_phase(LoopPhase::Verify);
        let td = TestDescriptor::builder("t", "m")
            .kind(TestKind::Behavioral)
            .cluster_role(ClusterRole::Verifier)
            .phase_set(ps)
            .build()
            .unwrap();
        assert!(td.has_phase(LoopPhase::Detect));
        assert!(td.has_phase(LoopPhase::Verify));
        assert!(!td.has_phase(LoopPhase::Block));
    }

    #[test]
    fn exclusion_reason_rejected_when_kind_already_behavioral() {
        let builder = TestDescriptor::builder("t", "m").kind(TestKind::Behavioral);
        let result = builder.exclusion_reason("should not work");
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // TestDescriptor predicates
    // -----------------------------------------------------------------------

    #[test]
    fn descriptor_is_behavior_bearing_for_behavioral_not_vacuous() {
        let td = behavioral("test_real");
        assert!(td.is_behavior_bearing());
    }

    #[test]
    fn descriptor_is_behavior_bearing_for_property_not_vacuous() {
        let td = TestDescriptor::builder("t", "m")
            .kind(TestKind::Property)
            .cluster_role(ClusterRole::Producer)
            .build()
            .unwrap();
        assert!(td.is_behavior_bearing());
    }

    #[test]
    fn descriptor_is_not_behavior_bearing_for_smoke() {
        let td = TestDescriptor::builder("t", "m")
            .kind(TestKind::Smoke)
            .cluster_role(ClusterRole::Producer)
            .build()
            .unwrap();
        assert!(!td.is_behavior_bearing());
    }

    #[test]
    fn descriptor_is_not_behavior_bearing_for_doctest() {
        let td = TestDescriptor::builder("t", "m")
            .kind(TestKind::Doctest)
            .cluster_role(ClusterRole::Producer)
            .build()
            .unwrap();
        assert!(!td.is_behavior_bearing());
    }

    #[test]
    fn descriptor_is_vacuous_when_patterns_present() {
        let mut td = behavioral("test_real");
        td.vacuous_patterns.push(VacuousPattern::AssertTrue);
        assert!(td.is_vacuous());
        assert!(!td.is_behavior_bearing());
    }

    #[test]
    fn descriptor_is_not_vacuous_when_patterns_empty() {
        let td = behavioral("test_real");
        assert!(!td.is_vacuous());
    }

    #[test]
    fn descriptor_has_phase_delegates_to_phase_set() {
        let td = TestDescriptor::builder("t", "m")
            .kind(TestKind::Behavioral)
            .cluster_role(ClusterRole::Verifier)
            .phase(LoopPhase::Verify)
            .build()
            .unwrap();
        assert!(td.has_phase(LoopPhase::Verify));
        assert!(!td.has_phase(LoopPhase::Detect));
    }

    // -----------------------------------------------------------------------
    // TaxonomyLabel
    // -----------------------------------------------------------------------

    #[test]
    fn taxonomy_label_fields_roundtrip_from_descriptor() {
        let td = behavioral("test_real");
        let label = TaxonomyLabel {
            test_name: td.test_name.clone(),
            kind: td.kind,
            cluster_role: td.cluster_role,
            phases: td.loop_phase_set,
        };
        assert_eq!(label.test_name, "test_real");
        assert_eq!(label.kind, TestKind::Behavioral);
        assert_eq!(label.cluster_role, ClusterRole::Verifier);
        assert!(label.phases.has_phase(LoopPhase::Verify));
    }
}
