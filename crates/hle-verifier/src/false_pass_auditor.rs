#![forbid(unsafe_code)]

//! M024 False Pass Auditor — flagship HLE-SP-001 detector.
//!
//! End-to-end stack cross-reference: terminal implementation node for
//! M024_FALSE_PASS_AUDITOR.md / L04_VERIFICATION.md / C04_ANTI_PATTERN_INTELLIGENCE (cluster).
//! Spec: ai_specs/modules/c04-anti-pattern-intelligence/M024_FALSE_PASS_AUDITOR.md
//!
//! Walks gate JSON text, locates every PASS claim, and validates that all four
//! required anchor fields are present and well-formed:
//!
//! - `^Verdict` — explicit verdict string
//! - `^Manifest_sha256` — 64-char lowercase hex
//! - `^Framework_sha256` — 64-char lowercase hex
//! - `^Counter_evidence_locator` — non-empty path or URI
//!
//! No external crate dependencies — JSON is parsed with a hand-written scanner
//! sufficient for the stub's needs (no serde).
//!
//! Layer: L04 | Cluster: C04

use std::fmt;

use substrate_types::HleError;

// ---------------------------------------------------------------------------
// AnchorKind — The four required fields
// ---------------------------------------------------------------------------

/// The four anchored fields that every PASS claim must carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnchorKind {
    /// `^Verdict` — explicit PASS/FAIL/AWAITING_HUMAN verdict string.
    Verdict,
    /// `^Manifest_sha256` — 64-char lowercase hex of the scaffold manifest.
    ManifestSha256,
    /// `^Framework_sha256` — 64-char lowercase hex of the framework/source tree.
    FrameworkSha256,
    /// `^Counter_evidence_locator` — pointer to negative-control artifacts.
    CounterEvidenceLocator,
}

impl AnchorKind {
    /// All four anchor kinds.
    pub const ALL: [Self; 4] = [
        Self::Verdict,
        Self::ManifestSha256,
        Self::FrameworkSha256,
        Self::CounterEvidenceLocator,
    ];

    /// The literal JSON field key for this anchor.
    #[must_use]
    pub const fn anchor_key(self) -> &'static str {
        match self {
            Self::Verdict => "^Verdict",
            Self::ManifestSha256 => "^Manifest_sha256",
            Self::FrameworkSha256 => "^Framework_sha256",
            Self::CounterEvidenceLocator => "^Counter_evidence_locator",
        }
    }

    /// Human-readable description of the requirement.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::Verdict => "explicit PASS/FAIL/AWAITING_HUMAN verdict string",
            Self::ManifestSha256 => "64-char lowercase hex SHA-256 of the scaffold manifest",
            Self::FrameworkSha256 => "64-char lowercase hex SHA-256 of the framework source tree",
            Self::CounterEvidenceLocator => {
                "non-empty path or URI pointing to negative-control artifacts"
            }
        }
    }

    /// Error code for a missing or malformed anchor of this kind.
    #[must_use]
    pub const fn error_code(self) -> u32 {
        match self {
            Self::Verdict => 2340,
            Self::ManifestSha256 => 2341,
            Self::FrameworkSha256 => 2342,
            Self::CounterEvidenceLocator => 2343,
        }
    }
}

impl fmt::Display for AnchorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.anchor_key())
    }
}

impl AsRef<str> for AnchorKind {
    fn as_ref(&self) -> &str {
        self.anchor_key()
    }
}

// ---------------------------------------------------------------------------
// MissingAnchor
// ---------------------------------------------------------------------------

/// Description of a single absent or malformed anchor field in a PASS claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingAnchor {
    /// Which anchor is absent.
    pub kind: AnchorKind,
    /// The literal JSON field key (same as `AnchorKind::anchor_key()`).
    pub field_key: &'static str,
    /// Why the anchor is considered missing (absent vs. malformed).
    pub rationale: String,
}

// ---------------------------------------------------------------------------
// AuditSeverity
// ---------------------------------------------------------------------------

/// Severity of a `ClaimFinding`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AuditSeverity {
    /// Sanity-check pass for a fully-anchored claim.
    Low,
    /// One to three anchors missing.
    High,
    /// All four anchors absent.
    Critical,
}

impl fmt::Display for AuditSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Low => f.write_str("LOW"),
            Self::High => f.write_str("HIGH"),
            Self::Critical => f.write_str("CRITICAL"),
        }
    }
}

// ---------------------------------------------------------------------------
// CounterEvidenceLocator / CounterEvidenceResult
// ---------------------------------------------------------------------------

/// Parsed `^Counter_evidence_locator` value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CounterEvidenceLocator {
    /// Raw field value.
    pub raw: String,
    /// Whether the locator resolves to an existing filesystem path.
    pub is_resolvable: bool,
}

/// Result of walking a `^Counter_evidence_locator` value.
#[derive(Debug, Clone)]
pub enum CounterEvidenceResult {
    /// Locator resolved to an existing path.
    Resolved {
        /// Resolved path string.
        path: String,
    },
    /// Locator did not resolve.
    NonResolvable {
        /// Raw locator string.
        raw: String,
        /// Human-readable reason.
        reason: String,
    },
    /// Resolution was skipped (scaffold mode).
    ResolutionSkipped,
}

// ---------------------------------------------------------------------------
// ClaimFinding
// ---------------------------------------------------------------------------

/// Finding for a single PASS claim within the audited gate document.
#[derive(Debug, Clone)]
pub struct ClaimFinding {
    /// JSON path within the gate document that identifies this claim.
    pub claim_path: String,
    /// Verdict string found in the claim, if any.
    pub raw_verdict: Option<String>,
    /// True when this claim asserts PASS (or equivalent).
    pub is_pass_claim: bool,
    /// Anchors that are absent or malformed.
    pub missing_anchors: Vec<MissingAnchor>,
    /// Result of walking `^Counter_evidence_locator`, if present.
    pub counter_evidence: Option<CounterEvidenceResult>,
    /// Severity based on missing anchor count.
    pub severity: AuditSeverity,
}

impl ClaimFinding {
    /// True when this is a PASS claim that is missing one or more anchors.
    #[must_use]
    pub fn is_false_pass(&self) -> bool {
        self.is_pass_claim && !self.missing_anchors.is_empty()
    }

    /// Count of present, valid anchors (0..=4).
    #[must_use]
    pub fn anchor_count(&self) -> usize {
        AnchorKind::ALL
            .len()
            .saturating_sub(self.missing_anchors.len())
    }

    /// True when all four anchors are present and valid.
    #[must_use]
    pub fn is_fully_anchored(&self) -> bool {
        self.missing_anchors.is_empty()
    }
}

// ---------------------------------------------------------------------------
// AuditVerdict
// ---------------------------------------------------------------------------

/// Document-level verdict for one audit run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuditVerdict {
    /// No false-pass claims detected.
    Clean,
    /// One or more false-pass claims found; workflow may continue with advisory annotation.
    Findings,
    /// One or more Critical-severity false-pass claims found; workflow must block.
    Blocked,
}

impl fmt::Display for AuditVerdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Clean => f.write_str("CLEAN"),
            Self::Findings => f.write_str("FINDINGS"),
            Self::Blocked => f.write_str("BLOCKED"),
        }
    }
}

// ---------------------------------------------------------------------------
// AuditConfig
// ---------------------------------------------------------------------------

/// Configuration for `FalsePassAuditor`.
#[derive(Debug, Clone)]
pub struct AuditConfig {
    /// When true, attempt to verify `^Counter_evidence_locator` paths on the filesystem.
    pub resolve_counter_evidence: bool,
    /// Maximum number of claims to audit per invocation (clamped to 1..=10_000).
    pub max_claims: usize,
    /// SHA-256 pattern description (stored for documentation; validation is inline).
    pub sha256_pattern_desc: String,
}

impl AuditConfig {
    /// Default scaffold-mode configuration.
    #[must_use]
    pub fn default_config() -> Self {
        Self {
            resolve_counter_evidence: false,
            max_claims: 1000,
            sha256_pattern_desc: String::from("^[0-9a-f]{64}$"),
        }
    }

    /// Construct with explicit settings.
    #[must_use]
    pub fn new(resolve_counter_evidence: bool, max_claims: usize) -> Self {
        Self {
            resolve_counter_evidence,
            max_claims: max_claims.clamp(1, 10_000),
            sha256_pattern_desc: String::from("^[0-9a-f]{64}$"),
        }
    }
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self::default_config()
    }
}

// ---------------------------------------------------------------------------
// AuditReport
// ---------------------------------------------------------------------------

/// Complete per-claim findings for one audit run.
#[derive(Debug, Clone)]
pub struct AuditReport {
    /// All claims inspected (PASS and non-PASS).
    pub all_findings: Vec<ClaimFinding>,
    /// Subset where `is_false_pass() == true`.
    pub false_pass_findings: Vec<ClaimFinding>,
    /// Document-level verdict.
    pub verdict: AuditVerdict,
    /// Total claims evaluated (capped at `AuditConfig::max_claims`).
    pub claims_evaluated: usize,
    /// Total PASS claims found.
    pub pass_claims_found: usize,
    /// True when evaluation stopped early due to `max_claims` limit.
    pub truncated: bool,
    /// Human-readable rationale. Empty string causes `audit_bytes()` to return `Err(2330)`.
    pub rationale: String,
}

impl AuditReport {
    /// True when no false-pass claims were detected.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.false_pass_findings.is_empty()
    }

    /// Highest severity across all false-pass findings.
    #[must_use]
    pub fn highest_severity(&self) -> Option<AuditSeverity> {
        self.false_pass_findings.iter().map(|f| f.severity).max()
    }

    /// True when the verdict is `Blocked`.
    #[must_use]
    pub fn should_block(&self) -> bool {
        self.verdict == AuditVerdict::Blocked
    }

    /// Count of findings with `Critical` severity.
    #[must_use]
    pub fn critical_count(&self) -> usize {
        self.false_pass_findings
            .iter()
            .filter(|f| f.severity == AuditSeverity::Critical)
            .count()
    }
}

// ---------------------------------------------------------------------------
// Hand-written minimal JSON key/value extractor (no serde)
// ---------------------------------------------------------------------------

/// Extract the string value for a top-level JSON key within an object `{...}`.
/// Returns `None` when the key is absent or the value is not a JSON string.
fn extract_json_string_value<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let key_pattern = format!("\"{key}\"");
    let key_pos = json.find(key_pattern.as_str())?;
    let after_key = &json[key_pos + key_pattern.len()..];
    let after_colon = after_key.trim_start().strip_prefix(':')?.trim_start();
    let after_quote = after_colon.strip_prefix('"')?;
    let end = after_quote.find('"')?;
    Some(&after_quote[..end])
}

/// True when `s` is exactly 64 lowercase hex characters.
fn is_valid_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
}

// ---------------------------------------------------------------------------
// FalsePassAuditor
// ---------------------------------------------------------------------------

/// Flagship HLE-SP-001 detector — walks gate JSON and flags PASS claims missing anchors.
pub struct FalsePassAuditor {
    config: AuditConfig,
}

impl FalsePassAuditor {
    /// Construct with the given config.
    #[must_use]
    pub fn new(config: AuditConfig) -> Self {
        Self { config }
    }

    /// Construct with default scaffold-mode config.
    #[must_use]
    pub fn with_default_config() -> Self {
        Self::new(AuditConfig::default())
    }

    /// Audit gate JSON bytes.
    ///
    /// # Errors
    ///
    /// - `[E2330]` when bytes are not valid UTF-8 or the document is empty.
    /// - `[E2330]` when the generated rationale string would be empty.
    pub fn audit_bytes(&self, gate_json: &[u8]) -> Result<AuditReport, HleError> {
        let text = std::str::from_utf8(gate_json)
            .map_err(|_| HleError::new("[E2330] gate JSON is not valid UTF-8"))?;
        if text.trim().is_empty() {
            return Err(HleError::new("[E2330] gate JSON document is empty"));
        }
        self.audit_text(text)
    }

    /// Convenience entry point accepting `&str`.
    ///
    /// # Errors
    ///
    /// Propagates any error from `audit_bytes`.
    pub fn audit_gate_json(json: &str) -> Result<AuditReport, HleError> {
        FalsePassAuditor::with_default_config().audit_bytes(json.as_bytes())
    }

    /// Evaluate a single JSON object string as a claim.
    ///
    /// # Errors
    ///
    /// Returns `[E2330]` when `claim_json` is empty.
    pub fn audit_claim_object(
        &self,
        claim_json: &str,
        path: &str,
    ) -> Result<ClaimFinding, HleError> {
        if claim_json.trim().is_empty() {
            return Err(HleError::new("[E2330] claim object is empty"));
        }
        Ok(self.evaluate_claim(claim_json, path))
    }

    // ------------------------------------------------------------------
    // Internal
    // ------------------------------------------------------------------

    fn audit_text(&self, text: &str) -> Result<AuditReport, HleError> {
        let claims = extract_claim_objects(text);
        let mut all_findings: Vec<ClaimFinding> = Vec::new();
        let mut claims_evaluated = 0usize;
        let mut truncated = false;

        for (idx, claim_text) in claims.iter().enumerate() {
            if claims_evaluated >= self.config.max_claims {
                truncated = true;
                break;
            }
            let path = format!("$[{idx}]");
            all_findings.push(self.evaluate_claim(claim_text, &path));
            claims_evaluated += 1;
        }

        let pass_claims_found = all_findings.iter().filter(|f| f.is_pass_claim).count();
        let false_pass_findings: Vec<ClaimFinding> = all_findings
            .iter()
            .filter(|f| f.is_false_pass())
            .cloned()
            .collect();

        let verdict = if false_pass_findings
            .iter()
            .any(|f| f.severity == AuditSeverity::Critical)
        {
            AuditVerdict::Blocked
        } else if !false_pass_findings.is_empty() {
            AuditVerdict::Findings
        } else {
            AuditVerdict::Clean
        };

        let rationale = build_audit_rationale(
            claims_evaluated,
            pass_claims_found,
            false_pass_findings.len(),
            truncated,
            verdict,
        );

        if rationale.is_empty() {
            return Err(HleError::new(
                "[E2330] auditor generated empty rationale — evaluation was not performed",
            ));
        }

        Ok(AuditReport {
            all_findings,
            false_pass_findings,
            verdict,
            claims_evaluated,
            pass_claims_found,
            truncated,
            rationale,
        })
    }

    fn evaluate_claim(&self, claim_json: &str, path: &str) -> ClaimFinding {
        let raw_verdict = extract_json_string_value(claim_json, "verdict").map(|s| s.to_owned());

        let is_pass_claim = raw_verdict
            .as_deref()
            .map_or(false, |v| v.eq_ignore_ascii_case("PASS"));

        // Walk ^Counter_evidence_locator before computing missing anchors.
        let counter_evidence_raw =
            extract_json_string_value(claim_json, "^Counter_evidence_locator")
                .map(|s| s.to_owned());

        let counter_evidence = counter_evidence_raw.as_ref().map(|raw| {
            if raw.is_empty() {
                CounterEvidenceResult::NonResolvable {
                    raw: raw.clone(),
                    reason: String::from("locator value is empty string"),
                }
            } else if !self.config.resolve_counter_evidence {
                CounterEvidenceResult::ResolutionSkipped
            } else if std::path::Path::new(raw.as_str()).exists() {
                CounterEvidenceResult::Resolved { path: raw.clone() }
            } else {
                CounterEvidenceResult::NonResolvable {
                    raw: raw.clone(),
                    reason: format!("path '{raw}' does not exist on the local filesystem"),
                }
            }
        });

        let missing_anchors = if is_pass_claim {
            self.compute_missing_anchors(claim_json, counter_evidence.as_ref())
        } else {
            Vec::new()
        };

        let severity = if !is_pass_claim {
            AuditSeverity::Low
        } else if missing_anchors.len() == AnchorKind::ALL.len() {
            AuditSeverity::Critical
        } else if !missing_anchors.is_empty() {
            AuditSeverity::High
        } else {
            AuditSeverity::Low
        };

        ClaimFinding {
            claim_path: path.to_owned(),
            raw_verdict,
            is_pass_claim,
            missing_anchors,
            counter_evidence,
            severity,
        }
    }

    fn compute_missing_anchors(
        &self,
        claim_json: &str,
        counter_evidence: Option<&CounterEvidenceResult>,
    ) -> Vec<MissingAnchor> {
        let mut missing = Vec::new();

        for kind in AnchorKind::ALL {
            let key = kind.anchor_key();
            let value = extract_json_string_value(claim_json, key);

            let absent_rationale: Option<String> = match kind {
                AnchorKind::Verdict => {
                    if value.is_none() {
                        Some(format!(
                            "[E{}] '^Verdict' field absent from PASS claim",
                            kind.error_code()
                        ))
                    } else {
                        None
                    }
                }
                AnchorKind::ManifestSha256 | AnchorKind::FrameworkSha256 => match value {
                    None => Some(format!(
                        "[E{}] '{key}' field absent from PASS claim",
                        kind.error_code()
                    )),
                    Some(v) if !is_valid_sha256_hex(v) => Some(format!(
                        "[E{}] '{key}' value is not a valid 64-char lowercase hex SHA-256",
                        kind.error_code()
                    )),
                    _ => None,
                },
                AnchorKind::CounterEvidenceLocator => match counter_evidence {
                    None => Some(format!(
                        "[E{}] '^Counter_evidence_locator' absent from PASS claim",
                        kind.error_code()
                    )),
                    Some(CounterEvidenceResult::NonResolvable { reason, .. }) => Some(format!(
                        "[E{}] '^Counter_evidence_locator' non-resolvable: {reason}",
                        kind.error_code()
                    )),
                    Some(
                        CounterEvidenceResult::Resolved { .. }
                        | CounterEvidenceResult::ResolutionSkipped,
                    ) => None,
                },
            };

            if let Some(rationale) = absent_rationale {
                missing.push(MissingAnchor {
                    kind,
                    field_key: key,
                    rationale,
                });
            }
        }

        missing
    }
}

impl Default for FalsePassAuditor {
    fn default() -> Self {
        Self::with_default_config()
    }
}

// ---------------------------------------------------------------------------
// Claim object extractor (stub — no serde)
// ---------------------------------------------------------------------------

/// Extract JSON object strings from the gate document.
fn extract_claim_objects(text: &str) -> Vec<String> {
    let trimmed = text.trim();
    if trimmed.starts_with('[') {
        extract_array_objects(trimmed)
    } else if trimmed.starts_with('{') {
        vec![trimmed.to_owned()]
    } else {
        vec![trimmed.to_owned()]
    }
}

fn extract_array_objects(array_text: &str) -> Vec<String> {
    let mut objects: Vec<String> = Vec::new();
    let mut depth = 0i32;
    let mut start: Option<usize> = None;

    for (i, ch) in array_text.char_indices() {
        match ch {
            '{' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    if let Some(s) = start {
                        objects.push(array_text[s..=i].to_owned());
                        start = None;
                    }
                }
            }
            _ => {}
        }
    }
    objects
}

fn build_audit_rationale(
    claims_evaluated: usize,
    pass_claims_found: usize,
    false_pass_count: usize,
    truncated: bool,
    verdict: AuditVerdict,
) -> String {
    let truncation_note = if truncated {
        " [TRUNCATED — stopped at max_claims limit]"
    } else {
        ""
    };
    format!(
        "HLE-SP-001 audit: verdict={verdict}, claims_evaluated={claims_evaluated}, \
         pass_claims_found={pass_claims_found}, false_pass_count={false_pass_count}{truncation_note}",
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{
        AnchorKind, AuditConfig, AuditSeverity, AuditVerdict, FalsePassAuditor, MissingAnchor,
    };

    const FULL_SHA: &str = "3a7f9c1b2d4e6f8a0c2e4b6d8f0a2c4e6b8d0f2a4c6e8b0d2f4a6c8e0b2d4f6a";
    const FULL_SHA2: &str = "1b3d5f7a9c0e2b4d6f8a0c2e4b6d8f0a2c4e6b8d0f2a4c6e8b0d2f4a6c8e0b2d";

    fn fully_anchored_json() -> String {
        format!(
            r#"{{
          "verdict": "PASS",
          "^Verdict": "PASS",
          "^Manifest_sha256": "{FULL_SHA}",
          "^Framework_sha256": "{FULL_SHA2}",
          "^Counter_evidence_locator": "tests/negative_controls/taxonomy_negatives.rs"
        }}"#
        )
    }

    // -----------------------------------------------------------------------
    // AnchorKind — all four absence triggers a finding
    // -----------------------------------------------------------------------

    #[test]
    fn anchor_kind_all_has_four_entries() {
        assert_eq!(AnchorKind::ALL.len(), 4);
    }

    #[test]
    fn anchor_kind_verdict_present() {
        assert!(AnchorKind::ALL.contains(&AnchorKind::Verdict));
    }

    #[test]
    fn anchor_kind_manifest_sha256_present() {
        assert!(AnchorKind::ALL.contains(&AnchorKind::ManifestSha256));
    }

    #[test]
    fn anchor_kind_framework_sha256_present() {
        assert!(AnchorKind::ALL.contains(&AnchorKind::FrameworkSha256));
    }

    #[test]
    fn anchor_kind_counter_evidence_locator_present() {
        assert!(AnchorKind::ALL.contains(&AnchorKind::CounterEvidenceLocator));
    }

    #[test]
    fn anchor_kind_error_codes_are_unique() {
        let mut codes: Vec<u32> = AnchorKind::ALL.iter().map(|k| k.error_code()).collect();
        let original_len = codes.len();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), original_len, "duplicate error codes found");
    }

    #[test]
    fn anchor_kind_display_matches_anchor_key() {
        for kind in AnchorKind::ALL {
            assert_eq!(kind.to_string(), kind.anchor_key());
        }
    }

    #[test]
    fn anchor_kind_as_ref_matches_anchor_key() {
        for kind in AnchorKind::ALL {
            let r: &str = kind.as_ref();
            assert_eq!(r, kind.anchor_key());
        }
    }

    #[test]
    fn anchor_kind_description_nonempty_for_all() {
        for kind in AnchorKind::ALL {
            assert!(
                !kind.description().is_empty(),
                "description empty for {kind:?}"
            );
        }
    }

    // Triggering a finding for each missing anchor individually:

    #[test]
    fn missing_verdict_anchor_triggers_finding() {
        // Has all anchors except ^Verdict.
        let json = format!(
            r#"{{
              "verdict": "PASS",
              "^Manifest_sha256": "{FULL_SHA}",
              "^Framework_sha256": "{FULL_SHA2}",
              "^Counter_evidence_locator": "tests/neg.rs"
            }}"#
        );
        let report = FalsePassAuditor::audit_gate_json(&json).unwrap();
        assert!(!report.is_clean());
        let missing_kinds: Vec<AnchorKind> = report.false_pass_findings[0]
            .missing_anchors
            .iter()
            .map(|m| m.kind)
            .collect();
        assert!(missing_kinds.contains(&AnchorKind::Verdict));
    }

    #[test]
    fn missing_manifest_sha256_anchor_triggers_finding() {
        let json = format!(
            r#"{{
              "verdict": "PASS",
              "^Verdict": "PASS",
              "^Framework_sha256": "{FULL_SHA2}",
              "^Counter_evidence_locator": "tests/neg.rs"
            }}"#
        );
        let report = FalsePassAuditor::audit_gate_json(&json).unwrap();
        assert!(!report.is_clean());
        let missing_kinds: Vec<AnchorKind> = report.false_pass_findings[0]
            .missing_anchors
            .iter()
            .map(|m| m.kind)
            .collect();
        assert!(missing_kinds.contains(&AnchorKind::ManifestSha256));
    }

    #[test]
    fn missing_framework_sha256_anchor_triggers_finding() {
        let json = format!(
            r#"{{
              "verdict": "PASS",
              "^Verdict": "PASS",
              "^Manifest_sha256": "{FULL_SHA}",
              "^Counter_evidence_locator": "tests/neg.rs"
            }}"#
        );
        let report = FalsePassAuditor::audit_gate_json(&json).unwrap();
        assert!(!report.is_clean());
        let missing_kinds: Vec<AnchorKind> = report.false_pass_findings[0]
            .missing_anchors
            .iter()
            .map(|m| m.kind)
            .collect();
        assert!(missing_kinds.contains(&AnchorKind::FrameworkSha256));
    }

    #[test]
    fn missing_counter_evidence_locator_anchor_triggers_finding() {
        let json = format!(
            r#"{{
              "verdict": "PASS",
              "^Verdict": "PASS",
              "^Manifest_sha256": "{FULL_SHA}",
              "^Framework_sha256": "{FULL_SHA2}"
            }}"#
        );
        let report = FalsePassAuditor::audit_gate_json(&json).unwrap();
        assert!(!report.is_clean());
        let missing_kinds: Vec<AnchorKind> = report.false_pass_findings[0]
            .missing_anchors
            .iter()
            .map(|m| m.kind)
            .collect();
        assert!(missing_kinds.contains(&AnchorKind::CounterEvidenceLocator));
    }

    // -----------------------------------------------------------------------
    // AuditVerdict promotion: Clean → Findings → Blocked
    // -----------------------------------------------------------------------

    #[test]
    fn auditor_clean_on_fully_anchored_pass_claim() {
        let json = fully_anchored_json();
        let report = FalsePassAuditor::audit_gate_json(&json).unwrap();
        assert!(
            report.is_clean(),
            "fully anchored PASS must not fire: {:#?}",
            report.false_pass_findings
        );
        assert_eq!(report.verdict, AuditVerdict::Clean);
    }

    #[test]
    fn auditor_clean_on_awaiting_human_claim() {
        let json = r#"{ "verdict": "AWAITING_HUMAN" }"#;
        let report = FalsePassAuditor::audit_gate_json(json).unwrap();
        assert!(report.false_pass_findings.is_empty());
    }

    #[test]
    fn auditor_clean_on_fail_claim() {
        let json = r#"{ "verdict": "FAIL" }"#;
        let report = FalsePassAuditor::audit_gate_json(json).unwrap();
        assert!(report.false_pass_findings.is_empty());
    }

    #[test]
    fn auditor_blocked_when_all_anchors_missing() {
        let json = r#"{ "verdict": "PASS" }"#;
        let report = FalsePassAuditor::audit_gate_json(json).unwrap();
        assert!(!report.is_clean());
        assert_eq!(report.verdict, AuditVerdict::Blocked);
        assert_eq!(report.critical_count(), 1);
    }

    #[test]
    fn auditor_findings_when_counter_evidence_missing() {
        let json = format!(
            r#"{{
              "verdict": "PASS",
              "^Verdict": "PASS",
              "^Manifest_sha256": "{FULL_SHA}",
              "^Framework_sha256": "{FULL_SHA2}"
            }}"#
        );
        let report = FalsePassAuditor::audit_gate_json(&json).unwrap();
        assert!(!report.is_clean());
        assert_eq!(report.verdict, AuditVerdict::Findings);
        let fp = &report.false_pass_findings[0];
        assert_eq!(fp.severity, AuditSeverity::High);
        assert_eq!(fp.missing_anchors.len(), 1);
        assert_eq!(
            fp.missing_anchors[0].kind,
            AnchorKind::CounterEvidenceLocator
        );
    }

    #[test]
    fn auditor_findings_verdict_when_one_anchor_missing() {
        // Only ^Counter_evidence_locator absent → Findings (not Blocked).
        let json = format!(
            r#"{{
              "verdict": "PASS",
              "^Verdict": "PASS",
              "^Manifest_sha256": "{FULL_SHA}",
              "^Framework_sha256": "{FULL_SHA2}"
            }}"#
        );
        let report = FalsePassAuditor::audit_gate_json(&json).unwrap();
        assert_eq!(report.verdict, AuditVerdict::Findings);
    }

    // -----------------------------------------------------------------------
    // Malformed SHA triggers finding
    // -----------------------------------------------------------------------

    #[test]
    fn auditor_findings_when_manifest_sha_malformed() {
        let json = format!(
            r#"{{
              "verdict": "PASS",
              "^Verdict": "PASS",
              "^Manifest_sha256": "tooshort",
              "^Framework_sha256": "{FULL_SHA}",
              "^Counter_evidence_locator": "tests/controls.rs"
            }}"#
        );
        let report = FalsePassAuditor::audit_gate_json(&json).unwrap();
        assert!(!report.is_clean());
        let missing_kinds: Vec<AnchorKind> = report.false_pass_findings[0]
            .missing_anchors
            .iter()
            .map(|m| m.kind)
            .collect();
        assert!(missing_kinds.contains(&AnchorKind::ManifestSha256));
    }

    #[test]
    fn auditor_findings_when_framework_sha_malformed() {
        let json = format!(
            r#"{{
              "verdict": "PASS",
              "^Verdict": "PASS",
              "^Manifest_sha256": "{FULL_SHA}",
              "^Framework_sha256": "uppercase_is_invalid_ABCDEF1234567890abcdef1234567890abcdef1234567890abcd",
              "^Counter_evidence_locator": "tests/controls.rs"
            }}"#
        );
        let report = FalsePassAuditor::audit_gate_json(&json).unwrap();
        assert!(!report.is_clean());
        let missing_kinds: Vec<AnchorKind> = report.false_pass_findings[0]
            .missing_anchors
            .iter()
            .map(|m| m.kind)
            .collect();
        assert!(missing_kinds.contains(&AnchorKind::FrameworkSha256));
    }

    #[test]
    fn valid_64_char_lowercase_hex_sha_passes() {
        // Both SHAs are exactly 64 lowercase hex chars — must not trigger findings.
        let json = fully_anchored_json();
        let report = FalsePassAuditor::audit_gate_json(&json).unwrap();
        assert!(report.is_clean());
    }

    // -----------------------------------------------------------------------
    // receipt-chain walking / receipt-with-all-anchors passes Clean
    // -----------------------------------------------------------------------

    #[test]
    fn receipt_with_all_anchors_passes_clean() {
        let json = fully_anchored_json();
        let auditor = FalsePassAuditor::with_default_config();
        let finding = auditor.audit_claim_object(&json, "$[0]").unwrap();
        assert!(finding.is_fully_anchored());
        assert!(!finding.is_false_pass());
    }

    #[test]
    fn receipt_with_one_missing_flags_false_pass() {
        let json = r#"{
          "verdict": "PASS",
          "^Verdict": "PASS",
          "^Manifest_sha256": "abc"
        }"#;
        let auditor = FalsePassAuditor::with_default_config();
        let finding = auditor.audit_claim_object(json, "$[0]").unwrap();
        assert!(finding.is_false_pass());
    }

    #[test]
    fn receipt_with_tampered_format_flags_false_pass() {
        // ^Manifest_sha256 has uppercase letters — invalid hex.
        let json = format!(
            r#"{{
              "verdict": "PASS",
              "^Verdict": "PASS",
              "^Manifest_sha256": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
              "^Framework_sha256": "{FULL_SHA2}",
              "^Counter_evidence_locator": "tests/neg.rs"
            }}"#
        );
        let auditor = FalsePassAuditor::with_default_config();
        let finding = auditor.audit_claim_object(&json, "$[0]").unwrap();
        assert!(finding.is_false_pass());
    }

    // -----------------------------------------------------------------------
    // Bounded output — truncation at max_claims
    // -----------------------------------------------------------------------

    #[test]
    fn auditor_truncates_at_max_claims() {
        // Build an array of 5 PASS claims but set max_claims = 2.
        let mut objects = String::from("[");
        for i in 0..5 {
            if i > 0 {
                objects.push(',');
            }
            objects.push_str(r#"{ "verdict": "PASS" }"#);
        }
        objects.push(']');

        let auditor = FalsePassAuditor::new(AuditConfig::new(false, 2));
        let report = auditor.audit_bytes(objects.as_bytes()).unwrap();
        assert!(report.truncated, "expected truncated=true");
        assert_eq!(report.claims_evaluated, 2);
    }

    #[test]
    fn auditor_not_truncated_within_limit() {
        let json = r#"[{ "verdict": "PASS" }, { "verdict": "FAIL" }]"#;
        let auditor = FalsePassAuditor::new(AuditConfig::new(false, 100));
        let report = auditor.audit_bytes(json.as_bytes()).unwrap();
        assert!(!report.truncated);
    }

    // -----------------------------------------------------------------------
    // Error conditions
    // -----------------------------------------------------------------------

    #[test]
    fn auditor_rejects_empty_json_string() {
        assert!(FalsePassAuditor::audit_gate_json("").is_err());
        assert!(FalsePassAuditor::audit_gate_json("   ").is_err());
    }

    #[test]
    fn auditor_rejects_non_utf8_bytes() {
        let bad: &[u8] = &[0xFF, 0xFE];
        let auditor = FalsePassAuditor::with_default_config();
        assert!(auditor.audit_bytes(bad).is_err());
    }

    #[test]
    fn audit_claim_object_empty_returns_error() {
        let auditor = FalsePassAuditor::with_default_config();
        assert!(auditor.audit_claim_object("", "$[0]").is_err());
    }

    #[test]
    fn audit_claim_object_whitespace_returns_error() {
        let auditor = FalsePassAuditor::with_default_config();
        assert!(auditor.audit_claim_object("   ", "$[0]").is_err());
    }

    // -----------------------------------------------------------------------
    // Report helpers
    // -----------------------------------------------------------------------

    #[test]
    fn audit_report_rationale_is_nonempty() {
        let json = r#"{ "verdict": "PASS" }"#;
        let report = FalsePassAuditor::audit_gate_json(json).unwrap();
        assert!(!report.rationale.is_empty());
    }

    #[test]
    fn audit_report_rationale_contains_verdict_word() {
        let json = r#"{ "verdict": "PASS" }"#;
        let report = FalsePassAuditor::audit_gate_json(json).unwrap();
        assert!(report.rationale.contains("verdict"));
    }

    #[test]
    fn audit_report_should_block_iff_verdict_blocked() {
        let blocked_json = r#"{ "verdict": "PASS" }"#;
        let blocked_report = FalsePassAuditor::audit_gate_json(blocked_json).unwrap();
        assert!(blocked_report.should_block());

        let full_json = fully_anchored_json();
        let clean_report = FalsePassAuditor::audit_gate_json(&full_json).unwrap();
        assert!(!clean_report.should_block());
    }

    #[test]
    fn audit_report_pass_claims_found_counts_pass_only() {
        let json = r#"{ "verdict": "PASS" }"#;
        let report = FalsePassAuditor::audit_gate_json(json).unwrap();
        assert_eq!(report.pass_claims_found, 1);
        assert_eq!(report.claims_evaluated, 1);
    }

    #[test]
    fn audit_report_pass_claims_found_zero_for_fail() {
        let json = r#"{ "verdict": "FAIL" }"#;
        let report = FalsePassAuditor::audit_gate_json(json).unwrap();
        assert_eq!(report.pass_claims_found, 0);
    }

    #[test]
    fn claim_finding_anchor_count_zero_when_all_missing() {
        let auditor = FalsePassAuditor::with_default_config();
        let finding = auditor
            .audit_claim_object(r#"{ "verdict": "PASS" }"#, "$[0]")
            .unwrap();
        assert_eq!(finding.anchor_count(), 0);
        assert!(!finding.is_fully_anchored());
    }

    #[test]
    fn claim_finding_anchor_count_four_when_fully_anchored() {
        let auditor = FalsePassAuditor::with_default_config();
        let json = fully_anchored_json();
        let finding = auditor.audit_claim_object(&json, "$[0]").unwrap();
        assert_eq!(finding.anchor_count(), 4);
        assert!(finding.is_fully_anchored());
    }

    #[test]
    fn claim_finding_is_false_pass_false_for_non_pass_verdict() {
        let auditor = FalsePassAuditor::with_default_config();
        let finding = auditor
            .audit_claim_object(r#"{ "verdict": "FAIL" }"#, "$[0]")
            .unwrap();
        assert!(!finding.is_false_pass());
    }

    // -----------------------------------------------------------------------
    // Config
    // -----------------------------------------------------------------------

    #[test]
    fn audit_config_max_claims_clamped_to_10000() {
        let cfg = AuditConfig::new(false, 999_999);
        assert_eq!(cfg.max_claims, 10_000);
    }

    #[test]
    fn audit_config_max_claims_minimum_is_1() {
        let cfg = AuditConfig::new(false, 0);
        assert_eq!(cfg.max_claims, 1);
    }

    #[test]
    fn audit_config_default_is_1000_claims() {
        let cfg = AuditConfig::default_config();
        assert_eq!(cfg.max_claims, 1000);
    }

    #[test]
    fn audit_config_sha256_pattern_desc_nonempty() {
        let cfg = AuditConfig::default_config();
        assert!(!cfg.sha256_pattern_desc.is_empty());
    }

    // -----------------------------------------------------------------------
    // Display / ordering
    // -----------------------------------------------------------------------

    #[test]
    fn audit_verdict_clean_display() {
        assert_eq!(AuditVerdict::Clean.to_string(), "CLEAN");
    }

    #[test]
    fn audit_verdict_findings_display() {
        assert_eq!(AuditVerdict::Findings.to_string(), "FINDINGS");
    }

    #[test]
    fn audit_verdict_blocked_display() {
        assert_eq!(AuditVerdict::Blocked.to_string(), "BLOCKED");
    }

    #[test]
    fn audit_severity_ordering_correct() {
        assert!(AuditSeverity::Critical > AuditSeverity::High);
        assert!(AuditSeverity::High > AuditSeverity::Low);
    }

    #[test]
    fn audit_severity_display_low() {
        assert_eq!(AuditSeverity::Low.to_string(), "LOW");
    }

    #[test]
    fn audit_severity_display_high() {
        assert_eq!(AuditSeverity::High.to_string(), "HIGH");
    }

    #[test]
    fn audit_severity_display_critical() {
        assert_eq!(AuditSeverity::Critical.to_string(), "CRITICAL");
    }

    #[test]
    fn audit_report_highest_severity_none_when_clean() {
        let full_json = fully_anchored_json();
        let report = FalsePassAuditor::audit_gate_json(&full_json).unwrap();
        assert_eq!(report.highest_severity(), None);
    }

    #[test]
    fn audit_report_highest_severity_critical_when_all_anchors_missing() {
        let json = r#"{ "verdict": "PASS" }"#;
        let report = FalsePassAuditor::audit_gate_json(json).unwrap();
        assert_eq!(report.highest_severity(), Some(AuditSeverity::Critical));
    }

    // -----------------------------------------------------------------------
    // FalsePassAuditor::default() equivalence
    // -----------------------------------------------------------------------

    #[test]
    fn false_pass_auditor_default_same_as_with_default_config() {
        let a: FalsePassAuditor = Default::default();
        let json = r#"{ "verdict": "PASS" }"#;
        let report = a.audit_bytes(json.as_bytes()).unwrap();
        assert!(!report.is_clean());
    }
}
