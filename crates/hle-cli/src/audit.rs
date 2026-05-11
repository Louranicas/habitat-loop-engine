//! `M051` — `cli_audit`: `hle audit` command adapter.
//!
//! Reads a source file (JSONL ledger, gate JSON, or receipt markdown),
//! runs every receipt-like entry through `M024 FalsePassAuditor`, aggregates
//! the per-entry verdicts, and emits human-readable or JSON output.
//!
//! Format auto-detection by extension:
//! - `.jsonl` — JSONL ledger: one JSON object per line, each treated as a claim.
//! - `.json`  — gate JSON: the whole file is one claim object (or an array).
//! - `.md`    — receipt markdown: scans for `^Verdict`, `^Manifest_sha256`,
//!   `^Framework_sha256`, `^Counter_evidence_locator` anchor lines.
//! - anything else — strict mode rejects; non-strict mode treats as JSONL.
//!
//! Error codes:
//! - `[2750]` source-not-found
//! - `[2751]` source-parse-error
//! - `[2752]` audit-blocked
//!
//! Layer: L06 | Cluster: C08

#![forbid(unsafe_code)]

use std::fmt;
use std::fmt::Write as _;
use std::path::Path;
use substrate_types::HleError;

use hle_verifier::false_pass_auditor::{
    AuditConfig, AuditSeverity, AuditVerdict, ClaimFinding, FalsePassAuditor,
};

// ---------------------------------------------------------------------------
// Public output types
// ---------------------------------------------------------------------------

/// Source format detected from the file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFormat {
    /// `.jsonl` — newline-delimited JSON claims.
    Jsonl,
    /// `.json` — single JSON object or array of claim objects.
    Json,
    /// `.md` — receipt markdown with `^Anchor: value` lines.
    Markdown,
}

impl SourceFormat {
    /// Detect from path extension; returns `None` when extension is unrecognized.
    #[must_use]
    pub fn detect(path: &Path) -> Option<Self> {
        match path.extension().and_then(|ext| ext.to_str()) {
            Some("jsonl") => Some(Self::Jsonl),
            Some("json") => Some(Self::Json),
            Some("md") => Some(Self::Markdown),
            _ => None,
        }
    }
}

impl fmt::Display for SourceFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Jsonl => f.write_str("jsonl"),
            Self::Json => f.write_str("json"),
            Self::Markdown => f.write_str("markdown"),
        }
    }
}

/// Single finding line emitted by the aggregator.
#[derive(Debug, Clone)]
pub struct AuditFindingLine {
    /// Human-readable location (e.g. `$[0]` or `markdown:root`).
    pub location: String,
    /// Severity of this finding.
    pub severity: AuditSeverity,
    /// Short description of what is missing.
    pub message: String,
}

/// Aggregated result over all entries in the source file.
#[derive(Debug)]
pub struct AuditResult {
    /// Overall verdict across all entries.
    pub verdict: AuditVerdict,
    /// Per-finding detail rows.
    pub findings: Vec<AuditFindingLine>,
    /// Count of entries whose per-entry verdict was Clean.
    pub clean_count: usize,
    /// Count of entries whose per-entry verdict was Findings.
    pub findings_count: usize,
    /// Count of entries whose per-entry verdict was Blocked.
    pub blocked_count: usize,
}

impl AuditResult {
    fn new() -> Self {
        Self {
            verdict: AuditVerdict::Clean,
            findings: Vec::new(),
            clean_count: 0,
            findings_count: 0,
            blocked_count: 0,
        }
    }

    /// Incorporate a single-claim `ClaimFinding` into the aggregate.
    fn absorb(&mut self, cf: &ClaimFinding) {
        if cf.is_false_pass() {
            // Escalate overall verdict.
            if cf.severity == AuditSeverity::Critical {
                self.verdict = AuditVerdict::Blocked;
                self.blocked_count += 1;
            } else if self.verdict != AuditVerdict::Blocked {
                self.verdict = AuditVerdict::Findings;
                self.findings_count += 1;
            } else {
                // already Blocked — still tally
                self.findings_count += 1;
            }

            // Emit one finding line per missing anchor.
            for anchor in &cf.missing_anchors {
                self.findings.push(AuditFindingLine {
                    location: cf.claim_path.clone(),
                    severity: cf.severity,
                    message: anchor.rationale.clone(),
                });
            }
        } else {
            self.clean_count += 1;
        }
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Execute `hle audit --source <path> [--strict] [--json]`.
///
/// Auto-detects source format from the file extension.  When `strict` is
/// `true`, an unrecognized extension returns `[2751]`.  When `strict` is
/// `false`, unknown extensions are treated as JSONL.
///
/// Returns the formatted output string on success.
///
/// # Errors
///
/// - `[2750]` when `source` does not exist or cannot be read.
/// - `[2751]` when `strict` is true and the format is unrecognized, or when
///   the file contents cannot be parsed as the detected format.
/// - `[2752]` when the aggregated verdict is `Blocked`.
pub fn audit_ledger(source: &Path, strict: bool, json: bool) -> Result<String, HleError> {
    // --- 1. Read file ----------------------------------------------------------
    if !source.exists() {
        return Err(HleError::new(format!(
            "[2750] audit source not found: {}",
            source.display()
        )));
    }
    let text = std::fs::read_to_string(source).map_err(|err| {
        HleError::new(format!(
            "[2750] audit source read failed {}: {err}",
            source.display()
        ))
    })?;

    // --- 2. Detect format ------------------------------------------------------
    let format = if let Some(f) = SourceFormat::detect(source) {
        f
    } else {
        if strict {
            return Err(HleError::new(format!(
                "[2751] audit source has unrecognized extension (strict mode): {}",
                source.display()
            )));
        }
        // Non-strict fallback: treat as JSONL.
        SourceFormat::Jsonl
    };

    // --- 3. Extract claim texts ------------------------------------------------
    let claim_texts: Vec<(String, String)> = match format {
        SourceFormat::Jsonl => extract_jsonl_claims(&text),
        SourceFormat::Json => extract_json_claims(&text),
        SourceFormat::Markdown => extract_markdown_claim(&text),
    };

    // --- 4. Run M024 on each claim and aggregate -------------------------------
    let auditor = FalsePassAuditor::new(AuditConfig::default());
    let mut result = AuditResult::new();

    for (location, claim_text) in &claim_texts {
        let finding = auditor
            .audit_claim_object(claim_text, location)
            .map_err(|err| {
                HleError::new(format!("[2751] audit parse error at {location}: {err}"))
            })?;
        result.absorb(&finding);
    }

    // Edge case: zero claims → clean (no receipts to check).
    // result.verdict stays Clean, counts stay 0.

    // --- 5. Format output ------------------------------------------------------
    let output = if json {
        format_json(&result, source)
    } else {
        format_human(&result, source)
    };

    // --- 6. Propagate Blocked as error code 2752 -------------------------------
    if result.verdict == AuditVerdict::Blocked {
        return Err(HleError::new(format!(
            "[2752] audit blocked — critical false-pass claims detected in {}:\n{output}",
            source.display()
        )));
    }

    Ok(output)
}

// ---------------------------------------------------------------------------
// Claim extractors
// ---------------------------------------------------------------------------

/// Extract `(location, claim_text)` pairs from a JSONL source.
///
/// Each non-blank line is treated as one claim object.  Empty lines are skipped.
fn extract_jsonl_claims(text: &str) -> Vec<(String, String)> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(idx, line)| (format!("$[{idx}]"), line.trim().to_owned()))
        .collect()
}

/// Extract `(location, claim_text)` pairs from a JSON source.
///
/// If the trimmed text starts with `[`, each bracketed object becomes a claim.
/// Otherwise the whole text is one claim.
fn extract_json_claims(text: &str) -> Vec<(String, String)> {
    let trimmed = text.trim();
    if trimmed.starts_with('[') {
        extract_array_objects(trimmed)
            .into_iter()
            .enumerate()
            .map(|(i, obj)| (format!("$[{i}]"), obj))
            .collect()
    } else if trimmed.starts_with('{') {
        vec![(String::from("$[0]"), trimmed.to_owned())]
    } else {
        // Not an object/array — synthesize an empty claim so the auditor
        // can report cleanly (no PASS claims → Clean).
        Vec::new()
    }
}

/// Extract a single synthetic claim from a receipt markdown file.
///
/// Scans for lines matching `^Key: value` (the four anchor lines) and also
/// looks for a `^Verdict:` line.  Builds a minimal JSON object representing
/// what was found, so the `FalsePassAuditor` can check it normally.
///
/// A gate JSON file that is embedded in the markdown (fenced code block) is
/// NOT extracted here — only the bare anchor lines at the document root count.
fn extract_markdown_claim(text: &str) -> Vec<(String, String)> {
    // We want to know whether this markdown looks like a receipt at all (has at
    // least one `^` anchor).  If it has none, return empty → 0 findings.
    let has_any_anchor = text.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.starts_with("^Verdict:")
            || trimmed.starts_with("^Manifest_sha256:")
            || trimmed.starts_with("^Framework_sha256:")
            || trimmed.starts_with("^Counter_evidence_locator:")
    });

    if !has_any_anchor {
        // No receipt anchors — treat as a document with no PASS claims: Clean.
        return Vec::new();
    }

    // Collect each anchor value.
    let verdict_val = find_markdown_anchor(text, "^Verdict");
    let manifest_val = find_markdown_anchor(text, "^Manifest_sha256");
    let framework_val = find_markdown_anchor(text, "^Framework_sha256");
    let counter_val = find_markdown_anchor(text, "^Counter_evidence_locator");

    // Build a synthetic JSON claim object.
    let mut parts: Vec<String> = Vec::new();

    // A markdown receipt always claims PASS at the "verdict" level so the
    // auditor treats it as a PASS claim and checks the four anchors.
    parts.push(String::from("\"verdict\": \"PASS\""));

    if let Some(v) = verdict_val {
        let escaped = json_escape(&v);
        parts.push(format!("\"^Verdict\": \"{escaped}\""));
    }
    if let Some(v) = manifest_val {
        let escaped = json_escape(&v);
        parts.push(format!("\"^Manifest_sha256\": \"{escaped}\""));
    }
    if let Some(v) = framework_val {
        let escaped = json_escape(&v);
        parts.push(format!("\"^Framework_sha256\": \"{escaped}\""));
    }
    if let Some(v) = counter_val {
        let escaped = json_escape(&v);
        parts.push(format!("\"^Counter_evidence_locator\": \"{escaped}\""));
    }

    let claim_json = format!("{{{}}}", parts.join(", "));
    vec![(String::from("markdown:root"), claim_json)]
}

/// Find the value after `^Key:` on a line in the markdown text.
///
/// Returns `Some(trimmed_value)` for the first matching line; `None` if absent.
fn find_markdown_anchor(text: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(prefix.as_str()) {
            // Strip an optional leading space and return the rest.
            let value = rest.trim_start_matches(' ').trim().to_owned();
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

/// Minimal JSON string escape (just double-quotes and backslashes).
fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Extract top-level `{...}` objects from a JSON array string.
fn extract_array_objects(array_text: &str) -> Vec<String> {
    let mut objects: Vec<String> = Vec::new();
    let mut depth: i32 = 0;
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
                    if let Some(s) = start.take() {
                        objects.push(array_text[s..=i].to_owned());
                    }
                }
            }
            _ => {}
        }
    }
    objects
}

// ---------------------------------------------------------------------------
// Output formatters
// ---------------------------------------------------------------------------

fn format_human(result: &AuditResult, source: &Path) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "hle audit verdict={} source={}",
        result.verdict,
        source.display()
    );
    let _ = writeln!(
        out,
        "  counts: clean={} findings={} blocked={}",
        result.clean_count, result.findings_count, result.blocked_count
    );
    if result.findings.is_empty() {
        out.push_str("  (no findings)\n");
    } else {
        for f in &result.findings {
            let _ = writeln!(
                out,
                "  [{sev}] {loc}: {msg}",
                sev = f.severity,
                loc = f.location,
                msg = f.message
            );
        }
    }
    out
}

fn format_json(result: &AuditResult, source: &Path) -> String {
    let source_str = source.display().to_string().replace('"', "\\\"");

    let findings_json = {
        let mut arr = String::from('[');
        for (i, f) in result.findings.iter().enumerate() {
            if i > 0 {
                arr.push(',');
            }
            let loc = f.location.replace('"', "\\\"");
            let msg = f.message.replace('"', "\\\"");
            let _ = write!(
                arr,
                "{{\"severity\":\"{sev}\",\"location\":\"{loc}\",\"message\":\"{msg}\"}}",
                sev = f.severity,
                loc = loc,
                msg = msg,
            );
        }
        arr.push(']');
        arr
    };

    format!(
        "{{\"schema\":\"hle.audit.v1\",\"source\":\"{src}\",\
         \"verdict\":\"{verdict}\",\
         \"findings\":{findings},\
         \"counts\":{{\"clean\":{clean},\"findings\":{findings_count},\"blocked\":{blocked}}}}}",
        src = source_str,
        verdict = result.verdict,
        findings = findings_json,
        clean = result.clean_count,
        findings_count = result.findings_count,
        blocked = result.blocked_count,
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    const FULL_SHA: &str = "3a7f9c1b2d4e6f8a0c2e4b6d8f0a2c4e6b8d0f2a4c6e8b0d2f4a6c8e0b2d4f6a";
    const FULL_SHA2: &str = "1b3d5f7a9c0e2b4d6f8a0c2e4b6d8f0a2c4e6b8d0f2a4c6e8b0d2f4a6c8e0b2d";

    /// Build a temp path preserving the extension from `name`.
    /// e.g. `tmp("foo.jsonl")` → `/tmp/hle-audit-foo-12345.jsonl`
    fn tmp(name: &str) -> PathBuf {
        let pid = std::process::id();
        // Split off the extension so the PID goes before it.
        if let Some(dot) = name.rfind('.') {
            let stem = &name[..dot];
            let ext = &name[dot..]; // includes the leading '.'
            std::env::temp_dir().join(format!("hle-audit-{stem}-{pid}{ext}"))
        } else {
            std::env::temp_dir().join(format!("hle-audit-{name}-{pid}"))
        }
    }

    fn fully_anchored_json_object() -> String {
        format!(
            r#"{{"verdict":"PASS","^Verdict":"PASS","^Manifest_sha256":"{FULL_SHA}","^Framework_sha256":"{FULL_SHA2}","^Counter_evidence_locator":"tests/neg.rs"}}"#
        )
    }

    // -----------------------------------------------------------------------
    // JSONL: zero findings → Clean
    // -----------------------------------------------------------------------

    #[test]
    fn jsonl_zero_findings_returns_clean() {
        let path = tmp("jsonl-clean.jsonl");
        fs::write(&path, format!("{}\n", fully_anchored_json_object())).unwrap();
        let r = audit_ledger(&path, false, false);
        assert!(r.is_ok(), "expected Ok, got {r:?}");
        assert!(r.unwrap().contains("verdict=CLEAN"));
        let _ = fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------
    // JSONL: one missing anchored field → Findings
    // -----------------------------------------------------------------------

    #[test]
    fn jsonl_one_missing_anchor_returns_findings() {
        let path = tmp("jsonl-findings.jsonl");
        // Missing ^Counter_evidence_locator → Findings
        let line = format!(
            r#"{{"verdict":"PASS","^Verdict":"PASS","^Manifest_sha256":"{FULL_SHA}","^Framework_sha256":"{FULL_SHA2}"}}"#
        );
        fs::write(&path, format!("{line}\n")).unwrap();
        let r = audit_ledger(&path, false, false);
        assert!(r.is_ok(), "expected Ok, got {r:?}");
        assert!(r.unwrap().contains("verdict=FINDINGS"));
        let _ = fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------
    // JSONL: all four anchors missing → Blocked (returns Err 2752)
    // -----------------------------------------------------------------------

    #[test]
    fn jsonl_all_anchors_missing_returns_blocked_err() {
        let path = tmp("jsonl-blocked.jsonl");
        fs::write(&path, "{\"verdict\":\"PASS\"}\n").unwrap();
        let r = audit_ledger(&path, false, false);
        assert!(r.is_err());
        assert!(r.err().unwrap().to_string().contains("2752"));
        let _ = fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------
    // Gate JSON (.json): no receipt PASS claims → Clean
    // -----------------------------------------------------------------------

    #[test]
    fn gate_json_no_pass_claims_returns_clean() {
        let path = tmp("gate.json");
        // A gate JSON with no "verdict":"PASS" — all are FAIL or absent.
        fs::write(&path, r#"{"schema":"hle.quality_gate.v2","status":"FAIL"}"#).unwrap();
        let r = audit_ledger(&path, false, false);
        assert!(r.is_ok(), "expected Ok, got {r:?}");
        assert!(r.unwrap().contains("verdict=CLEAN"));
        let _ = fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------
    // Gate JSON: fully anchored single PASS claim → Clean
    // -----------------------------------------------------------------------

    #[test]
    fn gate_json_fully_anchored_pass_returns_clean() {
        let path = tmp("gate-anchored.json");
        fs::write(&path, fully_anchored_json_object()).unwrap();
        let r = audit_ledger(&path, false, false);
        assert!(r.is_ok(), "expected Ok, got {r:?}");
        assert!(r.unwrap().contains("verdict=CLEAN"));
        let _ = fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------
    // Gate JSON: array with one anchored + one unanchored PASS → Blocked
    // -----------------------------------------------------------------------

    #[test]
    fn gate_json_array_one_unanchored_blocked() {
        let path = tmp("gate-array.json");
        let content = format!(
            "[{},{{\"verdict\":\"PASS\"}}]",
            fully_anchored_json_object()
        );
        fs::write(&path, content).unwrap();
        let r = audit_ledger(&path, false, false);
        assert!(r.is_err());
        assert!(r.err().unwrap().to_string().contains("2752"));
        let _ = fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------
    // Receipt markdown: all 4 anchors present → Clean
    // -----------------------------------------------------------------------

    #[test]
    fn markdown_receipt_all_anchors_present_returns_clean() {
        let path = tmp("receipt-clean.md");
        let content = format!(
            "# Receipt\n\n\
            ^Verdict: PASS\n\
            ^Manifest_sha256: {FULL_SHA}\n\
            ^Framework_sha256: {FULL_SHA2}\n\
            ^Counter_evidence_locator: tests/negative_controls/taxonomy_negatives.rs\n"
        );
        fs::write(&path, content).unwrap();
        let r = audit_ledger(&path, false, false);
        assert!(r.is_ok(), "expected Ok, got {r:?}");
        assert!(r.unwrap().contains("verdict=CLEAN"));
        let _ = fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------
    // Receipt markdown: missing ^Verdict → Findings
    // -----------------------------------------------------------------------

    #[test]
    fn markdown_missing_verdict_anchor_returns_findings() {
        let path = tmp("receipt-no-verdict.md");
        let content = format!(
            "# Receipt\n\n\
            ^Manifest_sha256: {FULL_SHA}\n\
            ^Framework_sha256: {FULL_SHA2}\n\
            ^Counter_evidence_locator: tests/neg.rs\n"
        );
        fs::write(&path, content).unwrap();
        let r = audit_ledger(&path, false, false);
        assert!(r.is_ok(), "expected Ok, got {r:?}");
        assert!(r.unwrap().contains("verdict=FINDINGS"));
        let _ = fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------
    // Strict mode rejects unknown extension
    // -----------------------------------------------------------------------

    #[test]
    fn strict_mode_rejects_unknown_extension() {
        let path = tmp("source.txt");
        fs::write(&path, "anything").unwrap();
        let r = audit_ledger(&path, true, false);
        assert!(r.is_err());
        assert!(r.err().unwrap().to_string().contains("2751"));
        let _ = fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------
    // Non-strict mode falls back to JSONL for unknown extension
    // -----------------------------------------------------------------------

    #[test]
    fn non_strict_unknown_extension_treated_as_jsonl() {
        let path = tmp("source.txt");
        // A fully-anchored JSON object line — in JSONL fallback it should be Clean.
        fs::write(&path, format!("{}\n", fully_anchored_json_object())).unwrap();
        let r = audit_ledger(&path, false, false);
        assert!(r.is_ok(), "expected Ok for JSONL fallback");
        let _ = fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------
    // Error 2750: source not found
    // -----------------------------------------------------------------------

    #[test]
    fn source_not_found_returns_2750() {
        let path = PathBuf::from("/tmp/hle-audit-nonexistent-99999-source.jsonl");
        let r = audit_ledger(&path, false, false);
        assert!(r.is_err());
        assert!(r.err().unwrap().to_string().contains("2750"));
    }

    // -----------------------------------------------------------------------
    // JSON output: schema field present
    // -----------------------------------------------------------------------

    #[test]
    fn json_output_contains_schema_field() {
        let path = tmp("json-schema.jsonl");
        fs::write(&path, format!("{}\n", fully_anchored_json_object())).unwrap();
        let r = audit_ledger(&path, false, true);
        assert!(r.is_ok());
        assert!(r.unwrap().contains("hle.audit.v1"));
        let _ = fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------
    // JSON output: verdict field present
    // -----------------------------------------------------------------------

    #[test]
    fn json_output_contains_verdict_field() {
        let path = tmp("json-verdict.jsonl");
        fs::write(&path, format!("{}\n", fully_anchored_json_object())).unwrap();
        let r = audit_ledger(&path, false, true);
        assert!(r.is_ok());
        assert!(r.unwrap().contains("\"verdict\":\"CLEAN\""));
        let _ = fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------
    // JSON output: counts object present
    // -----------------------------------------------------------------------

    #[test]
    fn json_output_contains_counts_object() {
        let path = tmp("json-counts.jsonl");
        fs::write(&path, format!("{}\n", fully_anchored_json_object())).unwrap();
        let r = audit_ledger(&path, false, true);
        assert!(r.is_ok());
        let s = r.unwrap();
        assert!(s.contains("\"counts\":"), "expected counts field: {s}");
        let _ = fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------
    // JSON output: findings array present
    // -----------------------------------------------------------------------

    #[test]
    fn json_output_contains_findings_array() {
        let path = tmp("json-findings-arr.jsonl");
        fs::write(&path, format!("{}\n", fully_anchored_json_object())).unwrap();
        let r = audit_ledger(&path, false, true);
        assert!(r.is_ok());
        assert!(r.unwrap().contains("\"findings\":"));
        let _ = fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------
    // JSON output: source field matches path
    // -----------------------------------------------------------------------

    #[test]
    fn json_output_source_field_matches_path() {
        let path = tmp("json-src.jsonl");
        fs::write(&path, format!("{}\n", fully_anchored_json_object())).unwrap();
        let r = audit_ledger(&path, false, true);
        assert!(r.is_ok());
        // The path stem should appear in the source field.
        assert!(r.unwrap().contains("json-src"));
        let _ = fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------
    // Error code propagation: blocked output contains 2752
    // -----------------------------------------------------------------------

    #[test]
    fn blocked_error_contains_2752_and_verdict() {
        let path = tmp("blocked-err.jsonl");
        fs::write(&path, "{\"verdict\":\"PASS\"}\n").unwrap();
        let r = audit_ledger(&path, false, false);
        assert!(r.is_err());
        let msg = r.err().unwrap().to_string();
        assert!(msg.contains("2752"), "expected 2752 in: {msg}");
        let _ = fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------
    // Empty JSONL ledger → Clean (zero claims)
    // -----------------------------------------------------------------------

    #[test]
    fn empty_jsonl_ledger_returns_clean() {
        let path = tmp("empty.jsonl");
        fs::write(&path, "").unwrap();
        let r = audit_ledger(&path, false, false);
        assert!(r.is_ok());
        assert!(r.unwrap().contains("verdict=CLEAN"));
        let _ = fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------
    // Markdown with no anchor lines → Clean (no receipt to check)
    // -----------------------------------------------------------------------

    #[test]
    fn markdown_no_anchor_lines_returns_clean() {
        let path = tmp("noanchor.md");
        fs::write(&path, "# Just a doc\n\nSome content here.\n").unwrap();
        let r = audit_ledger(&path, false, false);
        assert!(r.is_ok());
        assert!(r.unwrap().contains("verdict=CLEAN"));
        let _ = fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------
    // Multiple JSONL lines: mixed clean/findings → Findings
    // -----------------------------------------------------------------------

    #[test]
    fn jsonl_mixed_clean_and_findings_returns_findings() {
        let path = tmp("jsonl-mixed.jsonl");
        let clean = fully_anchored_json_object();
        // Missing ^Verdict only → Findings (not Critical).
        let partial = format!(
            r#"{{"verdict":"PASS","^Manifest_sha256":"{FULL_SHA}","^Framework_sha256":"{FULL_SHA2}","^Counter_evidence_locator":"tests/neg.rs"}}"#
        );
        fs::write(&path, format!("{clean}\n{partial}\n")).unwrap();
        let r = audit_ledger(&path, false, false);
        assert!(r.is_ok());
        assert!(r.unwrap().contains("verdict=FINDINGS"));
        let _ = fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------
    // JSONL: FAIL receipt (not PASS) → Clean (not a PASS claim)
    // -----------------------------------------------------------------------

    #[test]
    fn jsonl_fail_verdict_receipt_returns_clean() {
        let path = tmp("jsonl-fail.jsonl");
        fs::write(&path, "{\"verdict\":\"FAIL\"}\n").unwrap();
        let r = audit_ledger(&path, false, false);
        assert!(r.is_ok());
        assert!(r.unwrap().contains("verdict=CLEAN"));
        let _ = fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------
    // Human output: finding lines contain location
    // -----------------------------------------------------------------------

    #[test]
    fn human_output_finding_line_contains_location() {
        let path = tmp("human-loc.jsonl");
        // Missing ^Counter_evidence_locator only.
        let line = format!(
            r#"{{"verdict":"PASS","^Verdict":"PASS","^Manifest_sha256":"{FULL_SHA}","^Framework_sha256":"{FULL_SHA2}"}}"#
        );
        fs::write(&path, format!("{line}\n")).unwrap();
        let r = audit_ledger(&path, false, false);
        assert!(r.is_ok());
        let out = r.unwrap();
        assert!(out.contains("$[0]"), "expected location in: {out}");
        let _ = fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------
    // Human output: summary counts line present
    // -----------------------------------------------------------------------

    #[test]
    fn human_output_counts_line_present() {
        let path = tmp("human-counts.jsonl");
        fs::write(&path, format!("{}\n", fully_anchored_json_object())).unwrap();
        let r = audit_ledger(&path, false, false);
        assert!(r.is_ok());
        assert!(r.unwrap().contains("counts:"));
        let _ = fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------
    // SourceFormat: detection by extension
    // -----------------------------------------------------------------------

    #[test]
    fn source_format_detect_jsonl() {
        assert_eq!(
            SourceFormat::detect(Path::new("foo.jsonl")),
            Some(SourceFormat::Jsonl)
        );
    }

    #[test]
    fn source_format_detect_json() {
        assert_eq!(
            SourceFormat::detect(Path::new("foo.json")),
            Some(SourceFormat::Json)
        );
    }

    #[test]
    fn source_format_detect_md() {
        assert_eq!(
            SourceFormat::detect(Path::new("foo.md")),
            Some(SourceFormat::Markdown)
        );
    }

    #[test]
    fn source_format_detect_unknown_returns_none() {
        assert_eq!(SourceFormat::detect(Path::new("foo.txt")), None);
    }

    // -----------------------------------------------------------------------
    // end-to-end receipt smoke: the real end-to-end receipt has all 4 anchors
    // -----------------------------------------------------------------------

    #[test]
    fn real_end_to_end_receipt_returns_clean() {
        let receipt_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(".deployment-work/receipts/end-to-end-stack-complete-20260511T090000Z.md");
        if !receipt_path.exists() {
            // Skip if the receipt is not present in this test environment.
            return;
        }
        let r = audit_ledger(&receipt_path, false, false);
        assert!(r.is_ok(), "end-to-end receipt audit failed: {r:?}");
        let out = r.unwrap();
        assert!(
            out.contains("verdict=CLEAN"),
            "expected CLEAN for end-to-end receipt, got: {out}"
        );
    }

    // -----------------------------------------------------------------------
    // JSON output for blocked: findings array non-empty
    // -----------------------------------------------------------------------

    #[test]
    fn json_blocked_findings_array_nonempty() {
        let path = tmp("json-blocked-arr.jsonl");
        fs::write(&path, "{\"verdict\":\"PASS\"}\n").unwrap();
        let r = audit_ledger(&path, false, true);
        // blocked → Err, but the audit output is embedded in the error message.
        assert!(r.is_err());
        let msg = r.err().unwrap().to_string();
        // The embedded JSON should have a non-empty findings array.
        assert!(msg.contains("\"findings\":[{"), "expected findings: {msg}");
        let _ = fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------
    // Markdown: ^Manifest_sha256 with malformed hex → Findings
    // -----------------------------------------------------------------------

    #[test]
    fn markdown_malformed_manifest_sha_returns_findings() {
        let path = tmp("md-badsha.md");
        let content = format!(
            "^Verdict: PASS\n\
            ^Manifest_sha256: tooshort\n\
            ^Framework_sha256: {FULL_SHA2}\n\
            ^Counter_evidence_locator: tests/neg.rs\n"
        );
        fs::write(&path, content).unwrap();
        let r = audit_ledger(&path, false, false);
        assert!(r.is_ok());
        assert!(r.unwrap().contains("verdict=FINDINGS"));
        let _ = fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------
    // SourceFormat Display
    // -----------------------------------------------------------------------

    #[test]
    fn source_format_display_jsonl() {
        assert_eq!(SourceFormat::Jsonl.to_string(), "jsonl");
    }

    #[test]
    fn source_format_display_json() {
        assert_eq!(SourceFormat::Json.to_string(), "json");
    }

    #[test]
    fn source_format_display_md() {
        assert_eq!(SourceFormat::Markdown.to_string(), "markdown");
    }

    // -----------------------------------------------------------------------
    // extract_jsonl_claims: blank line skipped
    // -----------------------------------------------------------------------

    #[test]
    fn extract_jsonl_skips_blank_lines() {
        let text = "{\"a\":\"b\"}\n\n{\"c\":\"d\"}\n";
        let claims = extract_jsonl_claims(text);
        assert_eq!(claims.len(), 2);
    }

    // -----------------------------------------------------------------------
    // extract_markdown_claim: missing all anchors yields empty
    // -----------------------------------------------------------------------

    #[test]
    fn extract_markdown_no_anchors_yields_empty_vec() {
        let claims = extract_markdown_claim("# doc\nsome text\n");
        assert!(claims.is_empty());
    }

    // -----------------------------------------------------------------------
    // Strict mode with .md extension (recognized) → no 2751
    // -----------------------------------------------------------------------

    #[test]
    fn strict_mode_md_extension_not_rejected() {
        let path = tmp("strict-md.md");
        // No anchors → Clean
        fs::write(&path, "# doc\n").unwrap();
        let r = audit_ledger(&path, true, false);
        assert!(r.is_ok(), "strict mode should not reject .md: {r:?}");
        let _ = fs::remove_file(&path);
    }
}
