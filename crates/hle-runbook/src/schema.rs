#![forbid(unsafe_code)]

//! M032 — Typed Rust mirror of the Framework §17.8 TOML runbook definition schema.
//!
//! **Cluster:** C06 Runbook Semantics | **Layer:** L07 | **Error codes:** 2500-2510
//!
//! `schema.rs` is the vocabulary layer for all of C06.  Every other runbook
//! module operates on the types defined here.  The TOML file on disk is a
//! serialisation format; the structs in this module are canonical.
//!
//! No business logic lives here — only type definitions, constructors, and
//! pure derived methods.

use std::collections::HashMap;
use std::fmt;

use substrate_types::HleError;

// ── RunbookError ──────────────────────────────────────────────────────────────

/// Schema-level error for M032 constructors.  Codes 2500–2510.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunbookError {
    /// Code 2510 — Required field missing or invariant violated.
    Validation {
        /// Name of the field that violated the constraint.
        field: &'static str,
        /// Human-readable reason.
        reason: String,
    },
}

impl RunbookError {
    /// Numeric error code.
    #[must_use]
    pub const fn error_code(&self) -> u16 {
        match self {
            Self::Validation { .. } => 2510,
        }
    }
}

impl fmt::Display for RunbookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation { field, reason } => {
                write!(f, "[2510 RunbookValidation] field '{field}': {reason}")
            }
        }
    }
}

impl std::error::Error for RunbookError {}

impl From<RunbookError> for HleError {
    fn from(e: RunbookError) -> Self {
        HleError::new(e.to_string())
    }
}

// ── RunbookId ─────────────────────────────────────────────────────────────────

/// Validated runbook identifier — must match `[a-z0-9_-]+`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RunbookId(String);

impl RunbookId {
    /// Construct and validate a runbook identifier.
    ///
    /// # Errors
    ///
    /// Returns `RunbookError::Validation` when `raw` is empty or contains
    /// characters outside `[a-z0-9_-]`.
    pub fn new(raw: impl Into<String>) -> Result<Self, RunbookError> {
        let s = raw.into();
        if s.is_empty() {
            return Err(RunbookError::Validation {
                field: "id",
                reason: "runbook id must not be empty".into(),
            });
        }
        if !s
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        {
            return Err(RunbookError::Validation {
                field: "id",
                reason: format!("runbook id '{s}' contains characters outside [a-z0-9_-]"),
            });
        }
        Ok(Self(s))
    }

    /// Return the underlying string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RunbookId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// ── FixtureId ─────────────────────────────────────────────────────────────────

/// Validated fixture identifier for M038 replay fixtures.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FixtureId(String);

impl FixtureId {
    /// Construct and validate a fixture identifier.
    ///
    /// # Errors
    ///
    /// Returns `RunbookError::Validation` when `raw` is empty or contains
    /// characters outside `[a-z0-9_-]`.
    pub fn new(raw: impl Into<String>) -> Result<Self, RunbookError> {
        let s = raw.into();
        if s.is_empty() {
            return Err(RunbookError::Validation {
                field: "fixture_id",
                reason: "fixture id must not be empty".into(),
            });
        }
        if !s
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        {
            return Err(RunbookError::Validation {
                field: "fixture_id",
                reason: format!("fixture id '{s}' contains characters outside [a-z0-9_-]"),
            });
        }
        Ok(Self(s))
    }

    /// Return the underlying string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FixtureId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// ── PhaseKind ─────────────────────────────────────────────────────────────────

/// Canonical runbook phase identifiers per Framework §17.8.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PhaseKind {
    /// Detect: identify that an incident is occurring.
    Detect,
    /// Block: prevent the incident from spreading or worsening.
    Block,
    /// Fix: apply the corrective action.
    Fix,
    /// Verify: confirm the fix resolved the incident.
    Verify,
    /// `MetaTest`: validate the runbook itself via replay fixtures.
    MetaTest,
}

impl PhaseKind {
    /// Stable wire string for this phase kind.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Detect => "detect",
            Self::Block => "block",
            Self::Fix => "fix",
            Self::Verify => "verify",
            Self::MetaTest => "meta_test",
        }
    }

    /// Parse from a wire string; case-insensitive.
    #[must_use]
    pub fn parse_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "detect" => Some(Self::Detect),
            "block" => Some(Self::Block),
            "fix" => Some(Self::Fix),
            "verify" => Some(Self::Verify),
            "meta_test" | "metatest" => Some(Self::MetaTest),
            _ => None,
        }
    }

    /// Execution order: `Detect=0, Block=1, Fix=2, Verify=3, MetaTest=4`.
    #[must_use]
    pub const fn execution_order(self) -> u8 {
        match self {
            Self::Detect => 0,
            Self::Block => 1,
            Self::Fix => 2,
            Self::Verify => 3,
            Self::MetaTest => 4,
        }
    }

    /// All variants in execution order.
    #[must_use]
    pub fn all() -> [Self; 5] {
        [
            Self::Detect,
            Self::Block,
            Self::Fix,
            Self::Verify,
            Self::MetaTest,
        ]
    }
}

impl fmt::Display for PhaseKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── SafetyClass ───────────────────────────────────────────────────────────────

/// Safety tier for a runbook — governs M039 enforcement.
///
/// - `Soft`: may auto-execute if idempotent and confidence is high.
/// - `Hard`: requires explicit operator confirmation (M035) before Fix phase.
/// - `Safety`: requires explicit authority elevation + operator confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SafetyClass {
    /// Auto-executable when `idempotent == true` and confidence is high.
    Soft,
    /// Requires explicit operator confirmation before the Fix phase.
    Hard,
    /// Requires authority elevation plus operator confirmation.
    Safety,
}

impl SafetyClass {
    /// Stable wire string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Soft => "soft",
            Self::Hard => "hard",
            Self::Safety => "safety",
        }
    }

    /// Parse from a wire string; case-insensitive.
    #[must_use]
    pub fn parse_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "soft" => Some(Self::Soft),
            "hard" => Some(Self::Hard),
            "safety" => Some(Self::Safety),
            _ => None,
        }
    }

    /// Returns `true` for `Hard` and `Safety` — both require authority elevation.
    #[must_use]
    pub const fn requires_elevation(self) -> bool {
        matches!(self, Self::Hard | Self::Safety)
    }

    /// Returns `true` for `Hard` and `Safety` — both require explicit confirmation.
    #[must_use]
    pub const fn requires_explicit_confirm(self) -> bool {
        matches!(self, Self::Hard | Self::Safety)
    }
}

impl fmt::Display for SafetyClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// Compile-time proofs that const fn is usable in static assertions.
const _SOFT_NEEDS_ELEVATION: bool = SafetyClass::Soft.requires_elevation();
const _HARD_NEEDS_ELEVATION: bool = SafetyClass::Hard.requires_elevation();
const _SAFETY_NEEDS_ELEVATION: bool = SafetyClass::Safety.requires_elevation();
// Values: false, true, true — verified by tests.

// ── OperationalMode ───────────────────────────────────────────────────────────

/// Operational mode gate used by `ModeApplicability::applies_in`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationalMode {
    /// Scaffold-only (documentation / dry-run) mode.
    Scaffold,
    /// Local one-shot M0 mode.
    LocalM0,
    /// Production (future-authorised) mode.
    Production,
}

// ── ModeApplicability ─────────────────────────────────────────────────────────

/// Operational mode gates for a runbook.
///
/// A runbook is applicable only when the relevant gate flag is `true`.
/// The zero-value (all false) means the runbook applies in no mode — callers
/// must be explicit.  Use [`ModeApplicability::all`] to permit all modes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModeApplicability {
    /// Applicable in scaffold-only mode.
    pub scaffold: bool,
    /// Applicable in local-M0 (one-shot) mode.
    pub local_m0: bool,
    /// Applicable in production (future-authorised) mode.
    pub production: bool,
}

impl ModeApplicability {
    /// All three flags set to `true`.
    #[must_use]
    pub fn all() -> Self {
        Self {
            scaffold: true,
            local_m0: true,
            production: true,
        }
    }

    /// Only `scaffold = true`.
    #[must_use]
    pub fn scaffold_only() -> Self {
        Self {
            scaffold: true,
            local_m0: false,
            production: false,
        }
    }

    /// Only `local_m0 = true`.
    #[must_use]
    pub fn local_m0_only() -> Self {
        Self {
            scaffold: false,
            local_m0: true,
            production: false,
        }
    }

    /// Return `true` when the given `mode` flag is enabled on this struct.
    #[must_use]
    pub fn applies_in(&self, mode: &OperationalMode) -> bool {
        match mode {
            OperationalMode::Scaffold => self.scaffold,
            OperationalMode::LocalM0 => self.local_m0,
            OperationalMode::Production => self.production,
        }
    }
}

// ── EvidenceLocator ───────────────────────────────────────────────────────────

/// Points to evidence required or attached for a runbook phase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceLocator {
    /// Relative path within the runbook directory.
    FilePath(String),
    /// Inline content stored directly in the runbook TOML.
    Inline(String),
    /// Receipt ID from the persistence ledger.
    ReceiptId(String),
}

// ── Probe ─────────────────────────────────────────────────────────────────────

/// A single observable check within a runbook phase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    /// Unique probe identifier within the phase.
    pub id: String,
    /// Human-readable description of what this probe observes.
    pub description: String,
    /// Command or script to run.  `None` means manual observation required.
    pub command: Option<String>,
    /// Expected exit code.  `None` means any non-error exit is acceptable.
    pub expected_exit_code: Option<i32>,
}

impl Probe {
    /// Construct a manual (no-command) probe.
    #[must_use]
    pub fn manual(id: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            description: description.into(),
            command: None,
            expected_exit_code: None,
        }
    }

    /// Returns `true` when this probe requires manual observation (no command).
    #[must_use]
    pub fn is_manual(&self) -> bool {
        self.command.is_none()
    }
}

// ── Phase ─────────────────────────────────────────────────────────────────────

/// One phase of a runbook (detect / block / fix / verify / `meta_test`).
///
/// A phase is empty-safe: `probes` may be empty for stub phases.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Phase {
    /// Optional trigger condition evaluated before probes run.
    pub trigger: Option<String>,
    /// Ordered list of observable checks.
    pub probes: Vec<Probe>,
    /// Predicate string that, when true, advances the phase to PASS.
    pub pass_predicate: Option<String>,
    /// Predicate string that, when true, fails the phase immediately.
    pub fail_predicate: Option<String>,
    /// Evidence items required before the phase may be marked complete.
    pub evidence_required: Vec<EvidenceLocator>,
}

impl Phase {
    /// Returns `true` when the phase has no probes and no required evidence.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.probes.is_empty() && self.evidence_required.is_empty()
    }

    /// Returns `true` when at least one evidence item is required.
    #[must_use]
    pub fn requires_evidence(&self) -> bool {
        !self.evidence_required.is_empty()
    }

    /// Number of probes in this phase.
    #[must_use]
    pub fn probe_count(&self) -> usize {
        self.probes.len()
    }
}

// ── Runbook ───────────────────────────────────────────────────────────────────

/// Root runbook document — typed mirror of Framework §17.8 TOML `[runbook]` header.
///
/// # Invariants
/// - `id` is non-empty and matches `[a-z0-9_-]+`.
/// - `max_traversals >= 1`.
/// - `phases` contains at least one entry.
/// - `safety_class` governs which operations M039 permits without elevation.
#[derive(Debug, Clone, PartialEq)]
pub struct Runbook {
    /// Validated runbook identifier.
    pub id: RunbookId,
    /// One-line title.
    pub title: String,
    /// Free-form cross-reference to a habitat session or incident record.
    pub habitat_history: Option<String>,
    /// Incident fingerprint used for M038 replay fixture matching.
    pub failure_signature: Option<String>,
    /// Operational mode gates.
    pub mode_applicability: ModeApplicability,
    /// Path to the canonical schematic (relative to the runbook directory).
    pub canonical_schematic: Option<String>,
    /// Self-referential canonical runbook path.
    pub canonical_runbook: Option<String>,
    /// Maximum allowed execution passes before the executor halts.
    pub max_traversals: u32,
    /// When `true`, re-running is safe; when `false`, M039 enforces the traversal guard.
    pub idempotent: bool,
    /// Safety tier; governs M039 policy enforcement.
    pub safety_class: SafetyClass,
    /// Phases keyed by kind; ordering is produced by `ordered_phases()`.
    pub phases: HashMap<PhaseKind, Phase>,
}

impl Runbook {
    /// Entry point for the builder API.
    #[must_use]
    pub fn builder(id: impl Into<String>, title: impl Into<String>) -> RunbookBuilder {
        RunbookBuilder::new(id, title)
    }

    /// Return the phase for the given `kind`, if present.
    #[must_use]
    pub fn phase(&self, kind: PhaseKind) -> Option<&Phase> {
        self.phases.get(&kind)
    }

    /// Return `true` when the given phase kind is present.
    #[must_use]
    pub fn has_phase(&self, kind: PhaseKind) -> bool {
        self.phases.contains_key(&kind)
    }

    /// Return all phases sorted by `PhaseKind::execution_order`.
    #[must_use]
    pub fn ordered_phases(&self) -> Vec<(PhaseKind, &Phase)> {
        let mut pairs: Vec<(PhaseKind, &Phase)> =
            self.phases.iter().map(|(&k, v)| (k, v)).collect();
        pairs.sort_by_key(|(k, _)| k.execution_order());
        pairs
    }

    /// Returns `true` when this runbook may auto-execute
    /// (`safety_class == Soft && idempotent`).
    #[must_use]
    pub fn is_safe_for_auto_execution(&self) -> bool {
        self.safety_class == SafetyClass::Soft && self.idempotent
    }
}

// ── RunbookBuilder ────────────────────────────────────────────────────────────

/// Builder for [`Runbook`] — validates all invariants in `build()`.
#[derive(Debug)]
pub struct RunbookBuilder {
    id: String,
    title: String,
    habitat_history: Option<String>,
    failure_signature: Option<String>,
    mode_applicability: ModeApplicability,
    canonical_schematic: Option<String>,
    canonical_runbook: Option<String>,
    max_traversals: u32,
    idempotent: bool,
    safety_class: SafetyClass,
    phases: HashMap<PhaseKind, Phase>,
}

impl RunbookBuilder {
    /// Create a builder with required fields; defaults: `max_traversals=3`,
    /// `idempotent=true`, `safety_class=Soft`.
    #[must_use]
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            habitat_history: None,
            failure_signature: None,
            mode_applicability: ModeApplicability::default(),
            canonical_schematic: None,
            canonical_runbook: None,
            max_traversals: 3,
            idempotent: true,
            safety_class: SafetyClass::Soft,
            phases: HashMap::new(),
        }
    }

    /// Set habitat history cross-reference.
    #[must_use]
    pub fn habitat_history(mut self, h: impl Into<String>) -> Self {
        self.habitat_history = Some(h.into());
        self
    }

    /// Set the failure signature.
    #[must_use]
    pub fn failure_signature(mut self, s: impl Into<String>) -> Self {
        self.failure_signature = Some(s.into());
        self
    }

    /// Set mode applicability gates.
    #[must_use]
    pub fn mode_applicability(mut self, m: ModeApplicability) -> Self {
        self.mode_applicability = m;
        self
    }

    /// Set maximum traversal count.
    #[must_use]
    pub fn max_traversals(mut self, n: u32) -> Self {
        self.max_traversals = n;
        self
    }

    /// Set idempotency flag.
    #[must_use]
    pub fn idempotent(mut self, v: bool) -> Self {
        self.idempotent = v;
        self
    }

    /// Set safety class.
    #[must_use]
    pub fn safety_class(mut self, c: SafetyClass) -> Self {
        self.safety_class = c;
        self
    }

    /// Add a phase (overwrites if the kind was already present).
    #[must_use]
    pub fn add_phase(mut self, kind: PhaseKind, phase: Phase) -> Self {
        self.phases.insert(kind, phase);
        self
    }

    /// Validate all invariants and return a `Runbook`.
    ///
    /// # Errors
    ///
    /// Returns `RunbookError::Validation` (code 2510) when:
    /// - `id` is invalid.
    /// - `title` is empty.
    /// - `max_traversals == 0`.
    /// - `phases` is empty.
    pub fn build(self) -> Result<Runbook, RunbookError> {
        let id = RunbookId::new(&self.id)?;

        if self.title.trim().is_empty() {
            return Err(RunbookError::Validation {
                field: "title",
                reason: "title must not be empty".into(),
            });
        }

        if self.max_traversals == 0 {
            return Err(RunbookError::Validation {
                field: "max_traversals",
                reason: "max_traversals must be >= 1".into(),
            });
        }

        if self.phases.is_empty() {
            return Err(RunbookError::Validation {
                field: "phases",
                reason: "runbook must contain at least one phase".into(),
            });
        }

        Ok(Runbook {
            id,
            title: self.title,
            habitat_history: self.habitat_history,
            failure_signature: self.failure_signature,
            mode_applicability: self.mode_applicability,
            canonical_schematic: self.canonical_schematic,
            canonical_runbook: self.canonical_runbook,
            max_traversals: self.max_traversals,
            idempotent: self.idempotent,
            safety_class: self.safety_class,
            phases: self.phases,
        })
    }
}

// ── AgentId ───────────────────────────────────────────────────────────────────

/// Identifies the agent or operator acting within the runbook context.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AgentId(String);

impl AgentId {
    /// Construct from a raw identifier string.
    #[must_use]
    pub fn new(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    /// System agent used for automated actions (e.g., replay harness attachments).
    #[must_use]
    pub fn system() -> Self {
        Self("system".into())
    }

    /// Return the underlying string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Unix millisecond timestamp alias used across C06.
pub type Timestamp = u64;

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{
        AgentId, EvidenceLocator, FixtureId, ModeApplicability, OperationalMode, Phase, PhaseKind,
        Probe, Runbook, RunbookBuilder, RunbookError, RunbookId, SafetyClass,
    };

    // ── RunbookId ──

    #[test]
    fn runbook_id_accepts_valid_id() {
        assert!(RunbookId::new("s112-bridge-breaker").is_ok());
    }

    #[test]
    fn runbook_id_accepts_underscore() {
        assert!(RunbookId::new("my_runbook").is_ok());
    }

    #[test]
    fn runbook_id_rejects_empty() {
        assert!(RunbookId::new("").is_err());
    }

    #[test]
    fn runbook_id_rejects_uppercase() {
        assert!(RunbookId::new("MyRunbook").is_err());
    }

    #[test]
    fn runbook_id_rejects_space() {
        assert!(RunbookId::new("my runbook").is_err());
    }

    #[test]
    fn runbook_id_as_str_matches_input() {
        let id = RunbookId::new("test-id").expect("valid");
        assert_eq!(id.as_str(), "test-id");
    }

    #[test]
    fn runbook_id_display_equals_as_str() {
        let id = RunbookId::new("test-id").expect("valid");
        assert_eq!(id.to_string(), id.as_str());
    }

    // ── FixtureId ──

    #[test]
    fn fixture_id_accepts_valid_id() {
        assert!(FixtureId::new("s112_bridge_breaker").is_ok());
    }

    #[test]
    fn fixture_id_rejects_empty() {
        assert!(FixtureId::new("").is_err());
    }

    #[test]
    fn fixture_id_accepts_hyphen() {
        // Fixture IDs allow [a-z0-9_-]; hyphens are valid.
        assert!(FixtureId::new("my-fixture").is_ok());
        assert!(FixtureId::new("fix-001").is_ok());
    }

    #[test]
    fn fixture_id_rejects_uppercase() {
        assert!(FixtureId::new("My-Fixture").is_err());
    }

    #[test]
    fn fixture_id_rejects_space() {
        assert!(FixtureId::new("my fixture").is_err());
    }

    // ── PhaseKind ──

    #[test]
    fn phase_kind_as_str_stable() {
        assert_eq!(PhaseKind::Detect.as_str(), "detect");
        assert_eq!(PhaseKind::Block.as_str(), "block");
        assert_eq!(PhaseKind::Fix.as_str(), "fix");
        assert_eq!(PhaseKind::Verify.as_str(), "verify");
        assert_eq!(PhaseKind::MetaTest.as_str(), "meta_test");
    }

    #[test]
    fn phase_kind_from_str_roundtrip() {
        for kind in PhaseKind::all() {
            assert_eq!(PhaseKind::parse_str(kind.as_str()), Some(kind));
        }
    }

    #[test]
    fn phase_kind_from_str_case_insensitive() {
        assert_eq!(PhaseKind::parse_str("DETECT"), Some(PhaseKind::Detect));
    }

    #[test]
    fn phase_kind_execution_order_is_monotone() {
        let all = PhaseKind::all();
        for i in 0..all.len() - 1 {
            assert!(all[i].execution_order() < all[i + 1].execution_order());
        }
    }

    #[test]
    fn phase_kind_all_returns_five_variants() {
        assert_eq!(PhaseKind::all().len(), 5);
    }

    // ── SafetyClass ──

    #[test]
    fn safety_class_as_str_stable() {
        assert_eq!(SafetyClass::Soft.as_str(), "soft");
        assert_eq!(SafetyClass::Hard.as_str(), "hard");
        assert_eq!(SafetyClass::Safety.as_str(), "safety");
    }

    #[test]
    fn safety_class_from_str_roundtrip() {
        for &cls in &[SafetyClass::Soft, SafetyClass::Hard, SafetyClass::Safety] {
            assert_eq!(SafetyClass::parse_str(cls.as_str()), Some(cls));
        }
    }

    #[test]
    fn soft_does_not_require_elevation() {
        assert!(!SafetyClass::Soft.requires_elevation());
    }

    #[test]
    fn hard_requires_elevation() {
        assert!(SafetyClass::Hard.requires_elevation());
    }

    #[test]
    fn safety_requires_elevation() {
        assert!(SafetyClass::Safety.requires_elevation());
    }

    #[test]
    fn soft_does_not_require_explicit_confirm() {
        assert!(!SafetyClass::Soft.requires_explicit_confirm());
    }

    #[test]
    fn hard_requires_explicit_confirm() {
        assert!(SafetyClass::Hard.requires_explicit_confirm());
    }

    #[test]
    fn const_fn_elevation_values_are_correct() {
        // These compile-time assertions verify the const fn results.
        assert!(!super::_SOFT_NEEDS_ELEVATION);
        assert!(super::_HARD_NEEDS_ELEVATION);
        assert!(super::_SAFETY_NEEDS_ELEVATION);
    }

    // ── ModeApplicability ──

    #[test]
    fn mode_applicability_default_is_all_false() {
        let m = ModeApplicability::default();
        assert!(!m.scaffold && !m.local_m0 && !m.production);
    }

    #[test]
    fn mode_applicability_all_is_all_true() {
        let m = ModeApplicability::all();
        assert!(m.scaffold && m.local_m0 && m.production);
    }

    #[test]
    fn mode_applicability_scaffold_only() {
        let m = ModeApplicability::scaffold_only();
        assert!(m.scaffold);
        assert!(!m.local_m0);
    }

    #[test]
    fn mode_applicability_applies_in_scaffold() {
        let m = ModeApplicability::scaffold_only();
        assert!(m.applies_in(&OperationalMode::Scaffold));
        assert!(!m.applies_in(&OperationalMode::LocalM0));
    }

    // ── Phase ──

    #[test]
    fn empty_phase_is_empty() {
        let p = Phase::default();
        assert!(p.is_empty());
    }

    #[test]
    fn phase_with_probe_is_not_empty() {
        let mut p = Phase::default();
        p.probes.push(Probe::manual("p1", "check it"));
        assert!(!p.is_empty());
    }

    #[test]
    fn phase_probe_count_matches() {
        let mut p = Phase::default();
        p.probes.push(Probe::manual("p1", "check one"));
        p.probes.push(Probe::manual("p2", "check two"));
        assert_eq!(p.probe_count(), 2);
    }

    #[test]
    fn phase_requires_evidence_when_set() {
        let mut p = Phase::default();
        p.evidence_required
            .push(EvidenceLocator::Inline("note".into()));
        assert!(p.requires_evidence());
    }

    // ── Runbook / RunbookBuilder ──

    fn minimal_runbook() -> Runbook {
        RunbookBuilder::new("test-rb", "Test Runbook")
            .add_phase(PhaseKind::Detect, Phase::default())
            .build()
            .expect("valid runbook")
    }

    #[test]
    fn builder_produces_valid_runbook() {
        let rb = minimal_runbook();
        assert_eq!(rb.id.as_str(), "test-rb");
        assert_eq!(rb.title, "Test Runbook");
    }

    #[test]
    fn builder_rejects_empty_title() {
        let result = RunbookBuilder::new("test-rb", "")
            .add_phase(PhaseKind::Detect, Phase::default())
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn builder_rejects_zero_max_traversals() {
        let result = RunbookBuilder::new("test-rb", "T")
            .max_traversals(0)
            .add_phase(PhaseKind::Detect, Phase::default())
            .build();
        assert!(result.is_err());
        assert_eq!(result.err().map(|e| e.error_code()), Some(2510));
    }

    #[test]
    fn builder_rejects_empty_phases() {
        let result = RunbookBuilder::new("test-rb", "T").build();
        assert!(result.is_err());
    }

    #[test]
    fn runbook_has_phase_returns_true_for_present() {
        let rb = minimal_runbook();
        assert!(rb.has_phase(PhaseKind::Detect));
    }

    #[test]
    fn runbook_has_phase_returns_false_for_absent() {
        let rb = minimal_runbook();
        assert!(!rb.has_phase(PhaseKind::Fix));
    }

    #[test]
    fn runbook_ordered_phases_sorted_by_execution_order() {
        let rb = RunbookBuilder::new("x", "X")
            .add_phase(PhaseKind::Verify, Phase::default())
            .add_phase(PhaseKind::Detect, Phase::default())
            .add_phase(PhaseKind::Fix, Phase::default())
            .build()
            .expect("valid");
        let ordered: Vec<PhaseKind> = rb.ordered_phases().into_iter().map(|(k, _)| k).collect();
        assert_eq!(
            ordered,
            vec![PhaseKind::Detect, PhaseKind::Fix, PhaseKind::Verify]
        );
    }

    #[test]
    fn soft_idempotent_is_safe_for_auto() {
        let rb = RunbookBuilder::new("x", "X")
            .safety_class(SafetyClass::Soft)
            .idempotent(true)
            .add_phase(PhaseKind::Detect, Phase::default())
            .build()
            .expect("valid");
        assert!(rb.is_safe_for_auto_execution());
    }

    #[test]
    fn hard_is_not_safe_for_auto() {
        let rb = RunbookBuilder::new("x", "X")
            .safety_class(SafetyClass::Hard)
            .add_phase(PhaseKind::Detect, Phase::default())
            .build()
            .expect("valid");
        assert!(!rb.is_safe_for_auto_execution());
    }

    #[test]
    fn agent_id_system_is_system() {
        assert_eq!(AgentId::system().as_str(), "system");
    }

    #[test]
    fn runbook_error_display_contains_code() {
        let e = RunbookError::Validation {
            field: "id",
            reason: "bad".into(),
        };
        assert!(e.to_string().contains("2510"));
    }

    // ── Additional coverage to reach ≥50 ──────────────────────────────────

    #[test]
    fn runbook_id_accepts_digits_only() {
        assert!(RunbookId::new("123").is_ok());
    }

    #[test]
    fn runbook_id_accepts_single_char() {
        assert!(RunbookId::new("a").is_ok());
    }

    #[test]
    fn runbook_id_rejects_dot() {
        assert!(RunbookId::new("my.runbook").is_err());
    }

    #[test]
    fn runbook_id_error_code_is_2510() {
        let e = RunbookId::new("").unwrap_err();
        assert_eq!(e.error_code(), 2510);
    }

    #[test]
    fn fixture_id_accepts_digits_and_hyphens() {
        assert!(FixtureId::new("001-alpha").is_ok());
    }

    #[test]
    fn fixture_id_as_str_matches_input() {
        let id = FixtureId::new("my-fixture").expect("valid");
        assert_eq!(id.as_str(), "my-fixture");
    }

    #[test]
    fn fixture_id_display_equals_as_str() {
        let id = FixtureId::new("my-fixture").expect("valid");
        assert_eq!(id.to_string(), id.as_str());
    }

    #[test]
    fn fixture_id_error_code_is_2510() {
        let e = FixtureId::new("").unwrap_err();
        assert_eq!(e.error_code(), 2510);
    }

    #[test]
    fn phase_kind_metatest_alias_parses() {
        assert_eq!(PhaseKind::parse_str("metatest"), Some(PhaseKind::MetaTest));
    }

    #[test]
    fn phase_kind_unknown_returns_none() {
        assert!(PhaseKind::parse_str("unknown_phase").is_none());
    }

    #[test]
    fn phase_kind_display_equals_as_str() {
        for k in PhaseKind::all() {
            assert_eq!(k.to_string(), k.as_str());
        }
    }

    #[test]
    fn safety_class_parse_unknown_returns_none() {
        assert!(SafetyClass::parse_str("unknown").is_none());
    }

    #[test]
    fn safety_class_display_equals_as_str() {
        for &c in &[SafetyClass::Soft, SafetyClass::Hard, SafetyClass::Safety] {
            assert_eq!(c.to_string(), c.as_str());
        }
    }

    #[test]
    fn safety_class_ordering_soft_lt_hard_lt_safety() {
        assert!(SafetyClass::Soft < SafetyClass::Hard);
        assert!(SafetyClass::Hard < SafetyClass::Safety);
    }

    #[test]
    fn mode_applicability_local_m0_only() {
        let m = ModeApplicability::local_m0_only();
        assert!(!m.scaffold);
        assert!(m.local_m0);
        assert!(!m.production);
    }

    #[test]
    fn mode_applicability_applies_in_production() {
        let m = ModeApplicability::all();
        assert!(m.applies_in(&OperationalMode::Production));
    }

    #[test]
    fn mode_applicability_not_applies_when_flag_false() {
        let m = ModeApplicability::scaffold_only();
        assert!(!m.applies_in(&OperationalMode::Production));
    }

    #[test]
    fn evidence_locator_file_path_variant() {
        let e = EvidenceLocator::FilePath("a/b.toml".into());
        assert!(matches!(e, EvidenceLocator::FilePath(_)));
    }

    #[test]
    fn evidence_locator_inline_variant() {
        let e = EvidenceLocator::Inline("content".into());
        assert!(matches!(e, EvidenceLocator::Inline(_)));
    }

    #[test]
    fn evidence_locator_receipt_id_variant() {
        let e = EvidenceLocator::ReceiptId("rec-001".into());
        assert!(matches!(e, EvidenceLocator::ReceiptId(_)));
    }

    #[test]
    fn probe_manual_has_no_command() {
        let p = Probe::manual("p1", "desc");
        assert!(p.is_manual());
        assert!(p.command.is_none());
    }

    #[test]
    fn probe_with_command_is_not_manual() {
        let p = Probe {
            id: "p2".into(),
            description: "desc".into(),
            command: Some("/usr/bin/check".into()),
            expected_exit_code: Some(0),
        };
        assert!(!p.is_manual());
    }

    #[test]
    fn phase_with_evidence_is_not_empty() {
        let mut p = Phase::default();
        p.evidence_required
            .push(EvidenceLocator::Inline("x".into()));
        assert!(!p.is_empty());
        assert!(p.requires_evidence());
    }

    #[test]
    fn phase_probe_count_zero_for_default() {
        assert_eq!(Phase::default().probe_count(), 0);
    }

    #[test]
    fn builder_invalid_id_returns_2510() {
        let result = RunbookBuilder::new("UPPER_CASE", "title")
            .add_phase(PhaseKind::Detect, Phase::default())
            .build();
        assert!(result.is_err());
        assert_eq!(result.err().map(|e| e.error_code()), Some(2510));
    }

    #[test]
    fn builder_whitespace_title_returns_error() {
        let result = RunbookBuilder::new("rb", "   ")
            .add_phase(PhaseKind::Detect, Phase::default())
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn runbook_safety_class_is_set_correctly() {
        let rb = RunbookBuilder::new("rb", "T")
            .safety_class(SafetyClass::Safety)
            .add_phase(PhaseKind::Detect, Phase::default())
            .build()
            .expect("valid");
        assert_eq!(rb.safety_class, SafetyClass::Safety);
    }

    #[test]
    fn runbook_max_traversals_default_is_3() {
        let rb = RunbookBuilder::new("rb", "T")
            .add_phase(PhaseKind::Detect, Phase::default())
            .build()
            .expect("valid");
        assert_eq!(rb.max_traversals, 3);
    }

    #[test]
    fn runbook_idempotent_default_is_true() {
        let rb = RunbookBuilder::new("rb", "T")
            .add_phase(PhaseKind::Detect, Phase::default())
            .build()
            .expect("valid");
        assert!(rb.idempotent);
    }

    #[test]
    fn non_idempotent_soft_is_not_safe_for_auto() {
        let rb = RunbookBuilder::new("rb", "T")
            .safety_class(SafetyClass::Soft)
            .idempotent(false)
            .add_phase(PhaseKind::Detect, Phase::default())
            .build()
            .expect("valid");
        assert!(!rb.is_safe_for_auto_execution());
    }

    #[test]
    fn agent_id_display_equals_as_str() {
        let a = AgentId::new("my-agent");
        assert_eq!(a.to_string(), a.as_str());
    }

    #[test]
    fn agent_id_custom_str() {
        let a = AgentId::new("zen-agent");
        assert_eq!(a.as_str(), "zen-agent");
    }

    #[test]
    fn runbook_error_from_impl() {
        use substrate_types::HleError;
        let e = RunbookError::Validation {
            field: "f",
            reason: "r".into(),
        };
        let _: HleError = e.into();
    }
}
