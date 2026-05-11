#![forbid(unsafe_code)]

//! M037 — Pure TOML skeleton generator for Framework §17.8 runbooks.
//!
//! **Cluster:** C06 Runbook Semantics | **Layer:** L07 | **Error code:** 2575
//!
//! `scaffold()` is a pure function — it takes a description of what incident
//! the runbook should address and returns a TOML string that is valid for the
//! M033 parser.  No I/O, no state.
//!
//! The output is intentionally verbose: placeholder comments explain every
//! field so an operator can fill in the blanks without referencing the spec.

use std::fmt;
use std::fmt::Write as _;

use crate::schema::{OperationalMode, PhaseKind, SafetyClass};

// ── ScaffoldError ─────────────────────────────────────────────────────────────

/// Error produced by the scaffold generator.  Error code 2575.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScaffoldError {
    /// Code 2575 — `ScaffoldInput` invariant violated.
    Validation {
        /// Field that failed validation.
        field: &'static str,
        /// Human-readable reason.
        reason: String,
    },
}

impl ScaffoldError {
    /// Numeric error code.
    #[must_use]
    pub const fn error_code(&self) -> u16 {
        2575
    }
}

impl fmt::Display for ScaffoldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation { field, reason } => {
                write!(f, "[2575 ScaffoldValidation] field '{field}': {reason}")
            }
        }
    }
}

impl std::error::Error for ScaffoldError {}

// ── IncidentSignature ─────────────────────────────────────────────────────────

/// The family of incident that the runbook targets.
///
/// Each variant influences which phases the scaffold pre-populates and what
/// probe examples are generated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IncidentSignature {
    /// Service or process is unreachable.
    ServiceDown,
    /// Observed metric exceeds a defined threshold.
    MetricThresholdBreach,
    /// Elevated error rate on an HTTP or gRPC endpoint.
    HighErrorRate,
    /// Disk, memory, or CPU resource exhausted.
    ResourceExhaustion,
    /// Certificate is expired or about to expire.
    CertificateExpiry,
    /// Deployment changed configuration in a way that broke a service.
    ConfigurationDrift,
    /// Security anomaly detected (suspicious access, unexpected egress, etc.).
    SecurityAnomaly,
    /// A database is unavailable or has lost quorum.
    DatabaseUnavailable,
}

impl IncidentSignature {
    /// Return the canonical string tag for this signature.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::ServiceDown => "service_down",
            Self::MetricThresholdBreach => "metric_threshold_breach",
            Self::HighErrorRate => "high_error_rate",
            Self::ResourceExhaustion => "resource_exhaustion",
            Self::CertificateExpiry => "certificate_expiry",
            Self::ConfigurationDrift => "configuration_drift",
            Self::SecurityAnomaly => "security_anomaly",
            Self::DatabaseUnavailable => "database_unavailable",
        }
    }

    /// Return all defined signatures.
    #[must_use]
    pub fn all() -> [Self; 8] {
        [
            Self::ServiceDown,
            Self::MetricThresholdBreach,
            Self::HighErrorRate,
            Self::ResourceExhaustion,
            Self::CertificateExpiry,
            Self::ConfigurationDrift,
            Self::SecurityAnomaly,
            Self::DatabaseUnavailable,
        ]
    }

    /// Return the suggested safety class for this incident signature.
    #[must_use]
    pub const fn suggested_safety_class(&self) -> SafetyClass {
        match self {
            Self::ServiceDown
            | Self::HighErrorRate
            | Self::MetricThresholdBreach
            | Self::ResourceExhaustion
            | Self::ConfigurationDrift
            | Self::DatabaseUnavailable => SafetyClass::Hard,
            Self::CertificateExpiry => SafetyClass::Soft,
            Self::SecurityAnomaly => SafetyClass::Safety,
        }
    }

    /// Return the suggested `OperationalMode` for this signature.
    #[must_use]
    pub const fn suggested_mode(&self) -> OperationalMode {
        match self {
            // SecurityAnomaly warrants production-level authority.
            Self::SecurityAnomaly => OperationalMode::Production,
            // Everything else defaults to local one-shot M0 mode.
            _ => OperationalMode::LocalM0,
        }
    }

    /// Human-readable incident title.
    #[must_use]
    pub const fn title(&self) -> &'static str {
        match self {
            Self::ServiceDown => "Service Down — Runbook",
            Self::MetricThresholdBreach => "Metric Threshold Breach — Runbook",
            Self::HighErrorRate => "High Error Rate — Runbook",
            Self::ResourceExhaustion => "Resource Exhaustion — Runbook",
            Self::CertificateExpiry => "Certificate Expiry — Runbook",
            Self::ConfigurationDrift => "Configuration Drift — Runbook",
            Self::SecurityAnomaly => "Security Anomaly — Runbook",
            Self::DatabaseUnavailable => "Database Unavailable — Runbook",
        }
    }
}

impl fmt::Display for IncidentSignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── ScaffoldPhaseSpec ─────────────────────────────────────────────────────────

/// Per-phase customisation for the scaffold output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScaffoldPhaseSpec {
    /// The phase to include.
    pub kind: PhaseKind,
    /// Custom description comment for this phase.  Defaults to a generic
    /// placeholder when `None`.
    pub description: Option<String>,
    /// Whether to include a probe stub for this phase.
    pub include_probe_stub: bool,
}

impl ScaffoldPhaseSpec {
    /// Create a spec for `kind` with defaults.
    #[must_use]
    pub fn new(kind: PhaseKind) -> Self {
        Self {
            kind,
            description: None,
            include_probe_stub: true,
        }
    }

    /// Override the description comment.
    #[must_use]
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Disable the probe stub for this phase.
    #[must_use]
    pub fn no_probe_stub(mut self) -> Self {
        self.include_probe_stub = false;
        self
    }
}

// ── ScaffoldInput ─────────────────────────────────────────────────────────────

/// All inputs that drive skeleton generation.
#[derive(Debug, Clone)]
pub struct ScaffoldInput {
    /// Unique runbook identifier (TOML `id` field).
    pub id: String,
    /// Incident signature — sets defaults for safety class, mode, probes.
    pub signature: IncidentSignature,
    /// Custom title.  Defaults to `signature.title()` when `None`.
    pub title: Option<String>,
    /// Override safety class.  Defaults to `signature.suggested_safety_class()`.
    pub safety_class: Option<SafetyClass>,
    /// Override operational mode.  Defaults to `signature.suggested_mode()`.
    pub mode: Option<OperationalMode>,
    /// Explicit list of phases to include (in order).  Defaults to all 5
    /// canonical phases when empty.
    pub phases: Vec<ScaffoldPhaseSpec>,
}

impl ScaffoldInput {
    /// Create a minimal input from an incident signature.
    ///
    /// All optional fields take their defaults.
    #[must_use]
    pub fn from_signature(id: impl Into<String>, signature: IncidentSignature) -> Self {
        Self {
            id: id.into(),
            signature,
            title: None,
            safety_class: None,
            mode: None,
            phases: Vec::new(),
        }
    }

    /// Validate the input.
    ///
    /// # Errors
    ///
    /// Returns [`ScaffoldError::Validation`] when `id` is empty.
    pub fn validate(&self) -> Result<(), ScaffoldError> {
        if self.id.trim().is_empty() {
            return Err(ScaffoldError::Validation {
                field: "id",
                reason: "runbook id must not be empty".into(),
            });
        }
        Ok(())
    }

    /// Return the effective title.
    #[must_use]
    pub fn effective_title(&self) -> &str {
        self.title
            .as_deref()
            .unwrap_or_else(|| self.signature.title())
    }

    /// Return the effective safety class.
    #[must_use]
    pub fn effective_safety_class(&self) -> SafetyClass {
        self.safety_class
            .unwrap_or_else(|| self.signature.suggested_safety_class())
    }

    /// Return the effective operational mode.
    #[must_use]
    pub fn effective_mode(&self) -> OperationalMode {
        self.mode.unwrap_or_else(|| self.signature.suggested_mode())
    }

    /// Return the effective phase list (defaulting to all 5 canonical phases).
    #[must_use]
    pub fn effective_phases(&self) -> Vec<ScaffoldPhaseSpec> {
        if self.phases.is_empty() {
            PhaseKind::all()
                .iter()
                .map(|&k| ScaffoldPhaseSpec::new(k))
                .collect()
        } else {
            self.phases.clone()
        }
    }
}

// ── ScaffoldOptions ───────────────────────────────────────────────────────────

/// Presentation options for the generated TOML skeleton.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScaffoldOptions {
    /// When `true`, emit probe stub comments for each phase.
    pub include_probe_stubs: bool,
    /// When `true`, emit a `# Framework §17.8` header comment block.
    pub include_framework_comment: bool,
    /// Number of spaces per indentation level (applied inside `[phases]` table arrays).
    pub indent_width: usize,
}

impl Default for ScaffoldOptions {
    fn default() -> Self {
        Self {
            include_probe_stubs: true,
            include_framework_comment: true,
            indent_width: 2,
        }
    }
}

// ── RunbookToml ───────────────────────────────────────────────────────────────

/// Newtype wrapping the generated TOML skeleton string.
///
/// This type signals that the contained string was produced by `scaffold()`.
/// It does not guarantee that the TOML is valid for M033 (operators must
/// fill in required fields before the parser accepts it), but it does ensure
/// all mandatory TOML keys are present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunbookToml(String);

impl RunbookToml {
    /// Return the underlying TOML string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume the wrapper and return the owned TOML string.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for RunbookToml {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// ── scaffold() ────────────────────────────────────────────────────────────────

/// Return the wire string for an `OperationalMode`.
#[must_use]
const fn operational_mode_str(mode: OperationalMode) -> &'static str {
    match mode {
        OperationalMode::Scaffold => "scaffold",
        OperationalMode::LocalM0 => "local_m0",
        OperationalMode::Production => "production",
    }
}

/// Generate a TOML runbook skeleton from `input` using `options`.
///
/// This is a pure function — it has no side effects and will produce the same
/// output for the same inputs.
///
/// # Errors
///
/// Returns [`ScaffoldError::Validation`] when `input.validate()` fails.
pub fn scaffold(
    input: &ScaffoldInput,
    options: &ScaffoldOptions,
) -> Result<RunbookToml, ScaffoldError> {
    input.validate()?;

    let mut out = String::with_capacity(2048);
    let indent = " ".repeat(options.indent_width);

    if options.include_framework_comment {
        out.push_str("# Framework §17.8 Runbook Skeleton\n");
        out.push_str("# Generated by M037 scaffold — fill in all TODO fields before use.\n");
        out.push('\n');
    }

    // ── Header section ────────────────────────────────────────────────────────
    out.push_str("[runbook]\n");
    push_kv_str(&mut out, "id", &input.id);
    push_kv_str(&mut out, "title", input.effective_title());
    push_kv_str(
        &mut out,
        "safety_class",
        input.effective_safety_class().as_str(),
    );
    push_kv_str(
        &mut out,
        "operational_mode",
        operational_mode_str(input.effective_mode()),
    );
    push_kv_str(&mut out, "incident_signature", input.signature.as_str());
    out.push_str("# version = \"1\"\n");
    out.push('\n');

    // ── Phases section ────────────────────────────────────────────────────────
    for spec in input.effective_phases() {
        let desc = spec
            .description
            .as_deref()
            .unwrap_or_else(|| default_phase_description(spec.kind));

        out.push_str("[[phases]]\n");
        push_kv_str(&mut out, "kind", spec.kind.as_str());
        // writeln! on String is infallible; ignore the unused-result lint.
        let _ = writeln!(out, "# {desc}");
        let _ = writeln!(
            out,
            "{indent}# timeout_secs = 300  # optional — overrides executor default"
        );

        if options.include_probe_stubs && spec.include_probe_stub {
            out.push('\n');
            let _ = writeln!(out, "{indent}[[phases.probes]]");
            let _ = writeln!(
                out,
                "{indent}# kind = \"http\"  # one of: http | script | sql | noop"
            );
            let _ = writeln!(out, "{indent}# target = \"http://localhost:8080/health\"");
            let _ = writeln!(out, "{indent}# expected_status = 200  # for http probes");
            let _ = writeln!(out, "{indent}# command = [\"/usr/bin/systemctl\", \"is-active\", \"my-service\"]  # for script probes");
        }

        out.push('\n');
    }

    // ── Meta-test fixtures section ─────────────────────────────────────────────
    out.push_str("[meta_test]\n");
    out.push_str("# fixture_id = \"TODO — reference an M038 fixture id\"\n");
    out.push('\n');

    Ok(RunbookToml(out))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Emit `key = "value"\n` into `out`.
fn push_kv_str(out: &mut String, key: &str, value: &str) {
    out.push_str(key);
    out.push_str(" = \"");
    // Escape backslash and double-quote for TOML basic strings.
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push_str("\"\n");
}

/// Return a generic description comment for a phase kind.
#[must_use]
const fn default_phase_description(kind: PhaseKind) -> &'static str {
    match kind {
        PhaseKind::Detect => "Probe the system to detect the incident condition.",
        PhaseKind::Block => "Confirm the spread path is closed before proceeding.",
        PhaseKind::Fix => "Apply the remediation action (requires human confirmation).",
        PhaseKind::Verify => "Re-probe to confirm the incident condition is resolved.",
        PhaseKind::MetaTest => "Run the M038 replay fixture to validate the runbook end-to-end.",
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{
        scaffold, IncidentSignature, ScaffoldError, ScaffoldInput, ScaffoldOptions,
        ScaffoldPhaseSpec,
    };
    use crate::schema::{OperationalMode, PhaseKind, SafetyClass};

    fn default_input() -> ScaffoldInput {
        ScaffoldInput::from_signature("rb-001", IncidentSignature::ServiceDown)
    }

    fn default_options() -> ScaffoldOptions {
        ScaffoldOptions::default()
    }

    // ── IncidentSignature ─────────────────────────────────────────────────────

    #[test]
    fn all_signatures_have_non_empty_as_str() {
        for sig in IncidentSignature::all() {
            assert!(
                !sig.as_str().is_empty(),
                "signature {sig:?} has empty as_str"
            );
        }
    }

    #[test]
    fn all_signatures_have_non_empty_title() {
        for sig in IncidentSignature::all() {
            assert!(!sig.title().is_empty(), "signature {sig:?} has empty title");
        }
    }

    #[test]
    fn service_down_suggests_hard_class() {
        assert_eq!(
            IncidentSignature::ServiceDown.suggested_safety_class(),
            SafetyClass::Hard
        );
    }

    #[test]
    fn certificate_expiry_suggests_soft_class() {
        assert_eq!(
            IncidentSignature::CertificateExpiry.suggested_safety_class(),
            SafetyClass::Soft
        );
    }

    #[test]
    fn security_anomaly_suggests_safety_class() {
        assert_eq!(
            IncidentSignature::SecurityAnomaly.suggested_safety_class(),
            SafetyClass::Safety
        );
    }

    #[test]
    fn security_anomaly_suggests_production_mode() {
        assert_eq!(
            IncidentSignature::SecurityAnomaly.suggested_mode(),
            OperationalMode::Production
        );
    }

    #[test]
    fn service_down_suggests_local_m0_mode() {
        assert_eq!(
            IncidentSignature::ServiceDown.suggested_mode(),
            OperationalMode::LocalM0
        );
    }

    #[test]
    fn all_returns_eight_signatures() {
        assert_eq!(IncidentSignature::all().len(), 8);
    }

    #[test]
    fn incident_signature_display() {
        assert_eq!(IncidentSignature::ServiceDown.to_string(), "service_down");
    }

    // ── ScaffoldInput ─────────────────────────────────────────────────────────

    #[test]
    fn empty_id_fails_validation() {
        let input = ScaffoldInput::from_signature("", IncidentSignature::ServiceDown);
        let err = input.validate().unwrap_err();
        assert_eq!(err.error_code(), 2575);
        assert!(matches!(err, ScaffoldError::Validation { field: "id", .. }));
    }

    #[test]
    fn whitespace_only_id_fails_validation() {
        let input = ScaffoldInput::from_signature("   ", IncidentSignature::ServiceDown);
        assert!(input.validate().is_err());
    }

    #[test]
    fn valid_id_passes_validation() {
        assert!(default_input().validate().is_ok());
    }

    #[test]
    fn effective_title_uses_signature_when_no_override() {
        let input = default_input();
        assert_eq!(
            input.effective_title(),
            IncidentSignature::ServiceDown.title()
        );
    }

    #[test]
    fn effective_title_uses_override_when_set() {
        let mut input = default_input();
        input.title = Some("My Custom Title".into());
        assert_eq!(input.effective_title(), "My Custom Title");
    }

    #[test]
    fn effective_safety_class_uses_signature_default() {
        let input = default_input();
        assert_eq!(
            input.effective_safety_class(),
            IncidentSignature::ServiceDown.suggested_safety_class()
        );
    }

    #[test]
    fn effective_safety_class_uses_override() {
        let mut input = default_input();
        input.safety_class = Some(SafetyClass::Soft);
        assert_eq!(input.effective_safety_class(), SafetyClass::Soft);
    }

    #[test]
    fn effective_phases_defaults_to_five() {
        let input = default_input();
        assert_eq!(input.effective_phases().len(), 5);
    }

    #[test]
    fn effective_phases_uses_custom_list() {
        let mut input = default_input();
        input.phases = vec![ScaffoldPhaseSpec::new(PhaseKind::Detect)];
        assert_eq!(input.effective_phases().len(), 1);
    }

    // ── ScaffoldOptions ───────────────────────────────────────────────────────

    #[test]
    fn default_options_include_probe_stubs() {
        assert!(ScaffoldOptions::default().include_probe_stubs);
    }

    #[test]
    fn default_options_include_framework_comment() {
        assert!(ScaffoldOptions::default().include_framework_comment);
    }

    #[test]
    fn default_options_indent_width_is_two() {
        assert_eq!(ScaffoldOptions::default().indent_width, 2);
    }

    // ── scaffold() ────────────────────────────────────────────────────────────

    #[test]
    fn scaffold_empty_id_returns_error() {
        let input = ScaffoldInput::from_signature("", IncidentSignature::ServiceDown);
        let err = scaffold(&input, &default_options()).unwrap_err();
        assert_eq!(err.error_code(), 2575);
    }

    #[test]
    fn scaffold_produces_runbook_toml() {
        let toml = scaffold(&default_input(), &default_options()).expect("scaffold ok");
        let s = toml.as_str();
        assert!(s.contains("[runbook]"));
        assert!(s.contains("id = \"rb-001\""));
    }

    #[test]
    fn scaffold_contains_five_phase_sections() {
        let toml = scaffold(&default_input(), &default_options()).expect("ok");
        let count = toml.as_str().matches("[[phases]]").count();
        assert_eq!(count, 5, "expected 5 [[phases]] sections, got {count}");
    }

    #[test]
    fn scaffold_contains_detect_phase() {
        let toml = scaffold(&default_input(), &default_options()).expect("ok");
        assert!(toml.as_str().contains("kind = \"detect\""));
    }

    #[test]
    fn scaffold_contains_fix_phase() {
        let toml = scaffold(&default_input(), &default_options()).expect("ok");
        assert!(toml.as_str().contains("kind = \"fix\""));
    }

    #[test]
    fn scaffold_without_framework_comment() {
        let mut opts = default_options();
        opts.include_framework_comment = false;
        let toml = scaffold(&default_input(), &opts).expect("ok");
        assert!(!toml.as_str().contains("Framework §17.8"));
    }

    #[test]
    fn scaffold_with_framework_comment() {
        let toml = scaffold(&default_input(), &default_options()).expect("ok");
        assert!(toml.as_str().contains("Framework §17.8"));
    }

    #[test]
    fn scaffold_without_probe_stubs() {
        let mut opts = default_options();
        opts.include_probe_stubs = false;
        let toml = scaffold(&default_input(), &opts).expect("ok");
        assert!(!toml.as_str().contains("[[phases.probes]]"));
    }

    #[test]
    fn scaffold_with_probe_stubs() {
        let toml = scaffold(&default_input(), &default_options()).expect("ok");
        assert!(toml.as_str().contains("[[phases.probes]]"));
    }

    #[test]
    fn scaffold_contains_safety_class() {
        let toml = scaffold(&default_input(), &default_options()).expect("ok");
        // ServiceDown → Hard
        assert!(toml.as_str().contains("safety_class = \"hard\""));
    }

    #[test]
    fn scaffold_contains_meta_test_section() {
        let toml = scaffold(&default_input(), &default_options()).expect("ok");
        assert!(toml.as_str().contains("[meta_test]"));
    }

    #[test]
    fn scaffold_into_string_unwraps() {
        let toml = scaffold(&default_input(), &default_options()).expect("ok");
        let s: String = toml.into_string();
        assert!(s.contains("[runbook]"));
    }

    #[test]
    fn scaffold_display_equals_as_str() {
        let toml = scaffold(&default_input(), &default_options()).expect("ok");
        assert_eq!(toml.to_string(), toml.as_str());
    }

    #[test]
    fn scaffold_single_phase_input() {
        let mut input =
            ScaffoldInput::from_signature("rb-detect", IncidentSignature::HighErrorRate);
        input.phases = vec![ScaffoldPhaseSpec::new(PhaseKind::Detect).description("Custom detect")];
        let toml = scaffold(&input, &default_options()).expect("ok");
        let s = toml.as_str();
        assert_eq!(s.matches("[[phases]]").count(), 1);
        assert!(s.contains("Custom detect"));
    }

    #[test]
    fn scaffold_security_anomaly_production_mode() {
        let input = ScaffoldInput::from_signature("rb-sec", IncidentSignature::SecurityAnomaly);
        let toml = scaffold(&input, &default_options()).expect("ok");
        assert!(toml.as_str().contains("operational_mode = \"production\""));
    }

    #[test]
    fn scaffold_safety_class_override() {
        let mut input = default_input();
        input.safety_class = Some(SafetyClass::Safety);
        let toml = scaffold(&input, &default_options()).expect("ok");
        assert!(toml.as_str().contains("safety_class = \"safety\""));
    }

    #[test]
    fn scaffold_custom_title_override() {
        let mut input = default_input();
        input.title = Some("Redis Replication Failover".into());
        let toml = scaffold(&input, &default_options()).expect("ok");
        assert!(toml.as_str().contains("Redis Replication Failover"));
    }

    #[test]
    fn scaffold_title_with_quotes_is_escaped() {
        let mut input = default_input();
        input.title = Some("\"Quoted\" Title".into());
        let toml = scaffold(&input, &default_options()).expect("ok");
        // Backslash-escaped quote must appear in TOML.
        assert!(toml.as_str().contains("\\\"Quoted\\\""));
    }

    #[test]
    fn scaffold_phase_no_probe_stub() {
        let mut input = default_input();
        input.phases = vec![ScaffoldPhaseSpec::new(PhaseKind::Detect).no_probe_stub()];
        let toml = scaffold(&input, &default_options()).expect("ok");
        assert!(!toml.as_str().contains("[[phases.probes]]"));
    }

    #[test]
    fn scaffold_error_display_contains_code() {
        let err = ScaffoldError::Validation {
            field: "id",
            reason: "empty".into(),
        };
        assert!(err.to_string().contains("2575"));
        assert_eq!(err.error_code(), 2575);
    }

    #[test]
    fn incident_signature_all_unique_as_str() {
        let sigs = IncidentSignature::all();
        let mut seen = std::collections::HashSet::new();
        for sig in &sigs {
            assert!(seen.insert(sig.as_str()), "duplicate tag: {}", sig.as_str());
        }
    }

    #[test]
    fn scaffold_phase_spec_new_defaults() {
        let spec = ScaffoldPhaseSpec::new(PhaseKind::Fix);
        assert_eq!(spec.kind, PhaseKind::Fix);
        assert!(spec.description.is_none());
        assert!(spec.include_probe_stub);
    }

    #[test]
    fn scaffold_contains_incident_signature_field() {
        let toml = scaffold(&default_input(), &default_options()).expect("ok");
        assert!(toml
            .as_str()
            .contains("incident_signature = \"service_down\""));
    }

    #[test]
    fn scaffold_database_unavailable_signature() {
        let input = ScaffoldInput::from_signature("rb-db", IncidentSignature::DatabaseUnavailable);
        let toml = scaffold(&input, &default_options()).expect("ok");
        assert!(toml.as_str().contains("database_unavailable"));
    }

    #[test]
    fn scaffold_effective_phases_sorted_by_execution_order() {
        let input = default_input();
        let phases = input.effective_phases();
        for pair in phases.windows(2) {
            assert!(
                pair[0].kind.execution_order() < pair[1].kind.execution_order(),
                "phases not in execution order"
            );
        }
    }

    // ── Additional scaffold tests to reach ≥50 ───────────────────────────────

    #[test]
    fn incident_signature_high_error_rate_suggests_hard() {
        assert_eq!(
            IncidentSignature::HighErrorRate.suggested_safety_class(),
            SafetyClass::Hard
        );
    }

    #[test]
    fn incident_signature_resource_exhaustion_suggests_hard() {
        assert_eq!(
            IncidentSignature::ResourceExhaustion.suggested_safety_class(),
            SafetyClass::Hard
        );
    }

    #[test]
    fn incident_signature_config_drift_suggests_hard() {
        assert_eq!(
            IncidentSignature::ConfigurationDrift.suggested_safety_class(),
            SafetyClass::Hard
        );
    }

    #[test]
    fn incident_signature_database_unavailable_suggests_hard() {
        assert_eq!(
            IncidentSignature::DatabaseUnavailable.suggested_safety_class(),
            SafetyClass::Hard
        );
    }

    #[test]
    fn all_non_security_signatures_suggest_local_m0_mode() {
        for sig in IncidentSignature::all() {
            if sig != IncidentSignature::SecurityAnomaly {
                assert_eq!(
                    sig.suggested_mode(),
                    OperationalMode::LocalM0,
                    "{sig:?} should suggest LocalM0"
                );
            }
        }
    }

    #[test]
    fn scaffold_contains_id_field() {
        let toml = scaffold(&default_input(), &default_options()).expect("ok");
        assert!(toml.as_str().contains("id = \"rb-001\""));
    }

    #[test]
    fn scaffold_contains_verify_phase() {
        let toml = scaffold(&default_input(), &default_options()).expect("ok");
        assert!(toml.as_str().contains("kind = \"verify\""));
    }

    #[test]
    fn scaffold_contains_block_phase() {
        let toml = scaffold(&default_input(), &default_options()).expect("ok");
        assert!(toml.as_str().contains("kind = \"block\""));
    }

    #[test]
    fn scaffold_contains_meta_test_phase() {
        let toml = scaffold(&default_input(), &default_options()).expect("ok");
        assert!(toml.as_str().contains("kind = \"meta_test\""));
    }

    #[test]
    fn scaffold_phase_spec_description_method() {
        let spec = ScaffoldPhaseSpec::new(PhaseKind::Detect).description("Custom probe");
        assert_eq!(spec.description.as_deref(), Some("Custom probe"));
    }

    #[test]
    fn scaffold_phase_spec_no_probe_stub_sets_flag() {
        let spec = ScaffoldPhaseSpec::new(PhaseKind::Fix).no_probe_stub();
        assert!(!spec.include_probe_stub);
    }

    #[test]
    fn effective_mode_uses_override_when_set() {
        let mut input = default_input();
        input.mode = Some(OperationalMode::Scaffold);
        assert_eq!(input.effective_mode(), OperationalMode::Scaffold);
    }

    #[test]
    fn effective_mode_uses_signature_default_when_none() {
        let input = default_input();
        assert_eq!(
            input.effective_mode(),
            IncidentSignature::ServiceDown.suggested_mode()
        );
    }

    #[test]
    fn scaffold_error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ScaffoldError>();
    }

    #[test]
    fn runbook_toml_as_str_is_same_as_display() {
        let toml = scaffold(&default_input(), &default_options()).expect("ok");
        let via_display = toml.to_string();
        assert_eq!(toml.as_str(), via_display.as_str());
    }

    #[test]
    fn scaffold_indent_width_zero_still_produces_output() {
        let opts = ScaffoldOptions {
            indent_width: 0,
            ..ScaffoldOptions::default()
        };
        let toml = scaffold(&default_input(), &opts).expect("ok");
        assert!(toml.as_str().contains("[runbook]"));
    }

    #[test]
    fn scaffold_resource_exhaustion_signature() {
        let input = ScaffoldInput::from_signature("rb-res", IncidentSignature::ResourceExhaustion);
        let toml = scaffold(&input, &default_options()).expect("ok");
        assert!(toml.as_str().contains("resource_exhaustion"));
    }

    #[test]
    fn scaffold_high_error_rate_signature() {
        let input = ScaffoldInput::from_signature("rb-err", IncidentSignature::HighErrorRate);
        let toml = scaffold(&input, &default_options()).expect("ok");
        assert!(toml.as_str().contains("high_error_rate"));
    }
}
