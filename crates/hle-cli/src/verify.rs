//! `M048` — `cli_verify`: `hle verify` command adapter.
//!
//! Reads receipts from a JSONL ledger file, validates each entry via
//! `VerifyInput` (anchor-less M0 mode), persists results via
//! `VerifierResultsStore`, and returns a `VerifyReport`.
//!
//! **Gap-2 promotion (M008 integration):** `verify_ledger` now parses the
//! optional `receipt_sha256` field from each JSONL line. When present the
//! field is recomputed from the canonical byte sequence and compared; a
//! mismatch fails the entire verify run with `[E2722] receipt_sha_mismatch`.
//! When absent a warning is printed to stderr unless `--strict-sha` is set,
//! in which case the receipt is rejected with `[E2723] receipt_missing_sha`.
//!
//! Receipt parsing helpers (`parse_receipt_states`, `json_field`,
//! `parse_json_string_tail`) are `pub` so `main.rs` tests can call them via
//! `super::`.
//!
//! Error codes: 2720-2723.

#![forbid(unsafe_code)]

use std::fmt;
use std::path::{Path, PathBuf};
use substrate_emit::receipt_sha256_hex;
use substrate_types::{HleError, Receipt, StepState};
use substrate_verify::verify_report as substrate_verify_report;

use hle_storage::pool::MemPool;
use hle_storage::verifier_results_store::{Verdict, VerifierResult, VerifierResultsStore};
use hle_verifier::receipt_sha_verifier::VerifyInput;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Top-level verdict over all receipts in a ledger.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[allow(dead_code)] // used in module tests and by `verify_ledger`
pub enum VerifyVerdict {
    /// All receipts verified cleanly.
    AllVerified,
    /// One or more receipts failed SHA or false-pass checks.
    Failed,
    /// Some receipts verified; some were inconclusive (no hard failure).
    PartiallyVerified,
}

impl VerifyVerdict {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AllVerified => "ALL_VERIFIED",
            Self::Failed => "FAILED",
            Self::PartiallyVerified => "PARTIALLY_VERIFIED",
        }
    }
}

impl fmt::Display for VerifyVerdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Per-receipt outcome from the two-pass verification.
#[derive(Debug, PartialEq)]
#[allow(dead_code)] // used in module tests
pub enum ReceiptVerdict {
    Verified,
    HashMismatch {
        step_id: String,
        expected: String,
        actual: String,
    },
    FalsePassDetected {
        step_id: String,
        reason: String,
    },
    Inconclusive {
        step_id: String,
        reason: String,
    },
}

/// Full verification report.
#[derive(Debug)]
#[allow(dead_code)] // used in module tests and by `verify_ledger` / `format_report`
pub struct VerifyReport {
    pub ledger_path: PathBuf,
    pub total_receipts: usize,
    pub verified: usize,
    pub hash_mismatches: usize,
    pub false_passes: usize,
    pub inconclusive: usize,
    pub overall: VerifyVerdict,
    pub per_receipt: Vec<ReceiptVerdict>,
}

impl VerifyReport {
    /// Format as a bounded human-readable string (max 2 KB).
    #[must_use]
    pub fn format_human(&self) -> String {
        let raw = format!(
            "hle verify verdict={} receipts={} verified={} mismatches={} \
             false_passes={} ledger={}",
            self.overall.as_str(),
            self.total_receipts,
            self.verified,
            self.hash_mismatches,
            self.false_passes,
            self.ledger_path.display(),
        );
        truncate_2kb(raw)
    }

    /// Format as `hle.verify.report.v1` JSON (max 2 KB).
    #[must_use]
    pub fn format_json(&self) -> String {
        let ledger = self.ledger_path.display().to_string().replace('"', "\\\"");
        let raw = format!(
            "{{\"schema\":\"hle.verify.report.v1\",\"ledger_path\":\"{ledger}\",\
             \"total_receipts\":{tr},\"verified\":{v},\"hash_mismatches\":{hm},\
             \"false_passes\":{fp},\"inconclusive\":{ic},\"overall\":\"{overall}\"}}",
            ledger = ledger,
            tr = self.total_receipts,
            v = self.verified,
            hm = self.hash_mismatches,
            fp = self.false_passes,
            ic = self.inconclusive,
            overall = self.overall.as_str(),
        );
        truncate_2kb(raw)
    }
}

impl fmt::Display for VerifyReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.format_human())
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Execute the `hle verify` command for the given ledger path.
///
/// Reads receipts from the JSONL ledger, validates each receipt via
/// `VerifyInput` (anchor-less M0 mode), persists results via
/// `VerifierResultsStore` (using `MemPool` for bounded M0 operation), and
/// returns a `VerifyReport`.
///
/// When `strict_sha` is `false` (default):
/// - Receipts without `receipt_sha256` emit a stderr warning but continue.
/// - Receipts with a present but mismatched `receipt_sha256` fail immediately
///   with `[E2722] receipt_sha_mismatch`.
///
/// When `strict_sha` is `true`:
/// - Receipts without `receipt_sha256` fail immediately with
///   `[E2723] receipt_missing_sha`.
///
/// # Errors
///
/// - `HleError` 2720 if the ledger file cannot be read.
/// - `HleError` 2721 if the ledger file is empty (zero non-blank lines).
/// - `HleError` 2722 if a receipt has a `receipt_sha256` that does not match the recompute.
/// - `HleError` 2723 if `strict_sha` is set and a receipt is missing `receipt_sha256`.
#[allow(dead_code)] // used in module tests; main.rs uses parse_receipt_states directly
pub fn verify_ledger(ledger: &Path) -> Result<VerifyReport, HleError> {
    verify_ledger_with_opts(ledger, false)
}

/// Like `verify_ledger` but respects the `strict_sha` flag.
///
/// # Errors
///
/// Same error codes as `verify_ledger` plus `[E2723]` when `strict_sha` is
/// set and a receipt is missing its `receipt_sha256` field.
// This function is intentionally self-contained: all phases (read, parse, per-receipt
// SHA verification, shape verification, verdict aggregation) form one cohesive pipeline.
// Splitting into smaller helpers would break the early-return flow or require complex
// ownership threading. The too-many-lines lint is suppressed here, not globally.
#[allow(clippy::too_many_lines)]
pub fn verify_ledger_with_opts(ledger: &Path, strict_sha: bool) -> Result<VerifyReport, HleError> {
    let text = std::fs::read_to_string(ledger).map_err(|err| {
        HleError::new(format!(
            "[2720] verify ledger read failed {}: {err}",
            ledger.display()
        ))
    })?;

    if text.lines().all(|l| l.trim().is_empty()) {
        return Err(HleError::new(format!(
            "[2721] empty ledger: no receipts to verify ({})",
            ledger.display()
        )));
    }

    let receipts = parse_receipt_states(&text)?;
    if receipts.is_empty() {
        return Err(HleError::new(format!(
            "[2721] empty ledger: no receipts to verify ({})",
            ledger.display()
        )));
    }

    let pool = MemPool::new();
    let store = VerifierResultsStore::new(&pool);
    let (per_receipt, verified_count) =
        process_receipt_lines(&text, &receipts, strict_sha, &store)?;

    let overall_verdict_str: &str = substrate_verify_report(&receipts).unwrap_or("FAIL");
    let overall = if overall_verdict_str == "FAIL" {
        VerifyVerdict::Failed
    } else {
        VerifyVerdict::AllVerified
    };

    Ok(VerifyReport {
        ledger_path: ledger.to_path_buf(),
        total_receipts: receipts.len(),
        verified: verified_count,
        hash_mismatches: 0,
        false_passes: 0,
        inconclusive: 0,
        overall,
        per_receipt,
    })
}

/// Process each receipt line: SHA check + shape verification.
///
/// Returns `(per_receipt_verdicts, verified_count)`.
///
/// # Errors
///
/// Returns `[E2722]` or `[E2723]` on SHA check failure.
fn process_receipt_lines(
    text: &str,
    receipts: &[Receipt],
    strict_sha: bool,
    store: &VerifierResultsStore<'_>,
) -> Result<(Vec<ReceiptVerdict>, usize), HleError> {
    let mut per_receipt: Vec<ReceiptVerdict> = Vec::new();
    let mut verified_count: usize = 0;

    for (line_text, receipt) in text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .zip(receipts.iter())
    {
        // Gap-2: verify receipt_sha256 against canonical recompute.
        check_receipt_sha(line_text, receipt, strict_sha)?;

        match VerifyInput::new(
            hle_core::evidence::receipt_hash::ReceiptHash::zeroed(),
            receipt.workflow.as_str(),
            receipt.step_id.as_str(),
            receipt.verifier_verdict.as_str(),
            "",
            "",
        ) {
            Ok(_input) => {
                verified_count += 1;
                per_receipt.push(ReceiptVerdict::Verified);
                let verdict = map_step_state_to_verdict(receipt.state);
                let result = VerifierResult::new(0, receipt.step_id.as_str(), verdict, "", "m0");
                let _ = store.append(&result);
            }
            Err(_) => {
                per_receipt.push(ReceiptVerdict::Inconclusive {
                    step_id: receipt.step_id.clone(),
                    reason: String::from("empty workflow field"),
                });
            }
        }
    }
    Ok((per_receipt, verified_count))
}

/// Verify `receipt_sha256` for one JSONL receipt line.
///
/// # Errors
///
/// Returns `Err([E2722])` when the digest is present but does not match.
/// Returns `Err([E2723])` when absent and `strict_sha` is `true`.
fn check_receipt_sha(line_text: &str, receipt: &Receipt, strict_sha: bool) -> Result<(), HleError> {
    match json_field_opt(line_text, "receipt_sha256") {
        None if strict_sha => Err(HleError::new(format!(
            "[E2723] receipt_missing_sha: step_id={} has no receipt_sha256 field \
             (--strict-sha requires every receipt to carry a digest)",
            receipt.step_id
        ))),
        None => {
            eprintln!(
                "hle verify: warning: legacy receipt without receipt_sha256 \
                 (step_id={}); consider regenerating the ledger",
                receipt.step_id
            );
            Ok(())
        }
        Some(stored_sha) => {
            let message = substrate_emit::bounded(
                &receipt.message,
                substrate_emit::MAX_RECEIPT_MESSAGE_BYTES,
            );
            let recomputed = receipt_sha256_hex(
                &receipt.workflow,
                &receipt.step_id,
                &receipt.verifier_verdict,
                receipt.state.as_str(),
                &message,
            );
            if stored_sha == recomputed {
                Ok(())
            } else {
                Err(HleError::new(format!(
                    "[E2722] receipt_sha_mismatch: step_id={} stored={} recomputed={}",
                    receipt.step_id, stored_sha, recomputed
                )))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Receipt parsing helpers (moved from main.rs)
// ---------------------------------------------------------------------------

/// Parse a JSONL ledger text into a `Vec<Receipt>`.
///
/// Skips blank lines.  Returns `HleError` for malformed lines.
///
/// Accepts both `"workflow"` (legacy hand-written ledger format) and `"phase"`
/// (`PhaseExecutor` JSONL format) as the workflow/phase identifier field.
pub fn parse_receipt_states(text: &str) -> Result<Vec<Receipt>, HleError> {
    let mut receipts = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        // Accept both "workflow" (manual ledger format) and "phase" (PhaseExecutor format).
        let workflow = json_field(line, "workflow")
            .or_else(|_| json_field(line, "phase"))
            .map_err(|_| HleError::new("missing JSON field: workflow or phase"))?;
        let step_id = json_field(line, "step_id")?;
        let state: StepState = json_field(line, "state")?.parse()?;
        let verdict = json_field(line, "verdict")?;
        let message = json_field(line, "message")?;
        receipts.push(Receipt::new(workflow, step_id, state, verdict, message));
    }
    Ok(receipts)
}

/// Extract a quoted JSON string field value from a compact JSON line.
///
/// Looks for `"<field>":"` and returns the decoded string up to the closing
/// `"` (with JSON escape decoding).
///
/// # Errors
///
/// Returns `HleError` when the field is absent or the value is unterminated.
pub fn json_field(line: &str, field: &str) -> Result<String, HleError> {
    let needle = format!("\"{field}\":\"");
    let start = line
        .find(&needle)
        .ok_or_else(|| HleError::new(format!("missing JSON field: {field}")))?
        + needle.len();
    parse_json_string_tail(&line[start..], field)
}

/// Extract an optional quoted JSON string field value from a compact JSON line.
///
/// Returns `None` when the field is absent; `Some(value)` when present and
/// well-formed. Unlike `json_field`, absent fields are not an error.
#[must_use]
pub fn json_field_opt(line: &str, field: &str) -> Option<String> {
    json_field(line, field).ok()
}

/// Decode a JSON string tail (everything after the opening `"`).
///
/// Stops at the closing `"` (unescaped).  Handles `\"`, `\\`, `\n`, `\r`,
/// `\t` escape sequences.
///
/// # Errors
///
/// Returns `HleError` when the string is unterminated.
pub fn parse_json_string_tail(value: &str, field: &str) -> Result<String, HleError> {
    let mut decoded = String::new();
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            match character {
                '"' => decoded.push('"'),
                '\\' => decoded.push('\\'),
                'n' => decoded.push('\n'),
                'r' => decoded.push('\r'),
                't' => decoded.push('\t'),
                other => decoded.push(other),
            }
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            return Ok(decoded);
        } else {
            decoded.push(character);
        }
    }
    Err(HleError::new(format!("unterminated JSON field: {field}")))
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

#[allow(dead_code)] // used inside verify_ledger which is cfg(test)-visible
fn map_step_state_to_verdict(state: StepState) -> Verdict {
    match state {
        StepState::Passed => Verdict::Pass,
        StepState::AwaitingHuman => Verdict::AwaitingHuman,
        _ => Verdict::Fail,
    }
}

/// Format the report according to the `--json` flag.
#[must_use]
#[allow(dead_code)] // used in module tests
pub fn format_report(report: &VerifyReport, json: bool) -> String {
    if json {
        report.format_json()
    } else {
        report.format_human()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[allow(dead_code)] // used in tests via VerifyReport methods
fn truncate_2kb(s: String) -> String {
    const MAX: usize = 2048;
    if s.len() <= MAX {
        return s;
    }
    let mut out = s[..MAX - 5].to_owned();
    out.push_str("[...]");
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_ledger(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("hle-verify-{name}-{}.jsonl", std::process::id()))
    }

    fn write_ledger(path: &PathBuf, content: &str) {
        std::fs::write(path, content).expect("write temp ledger");
    }

    /// Produce a minimal valid hle.receipt.v1 JSON line for test use.
    fn receipt_line(step_id: &str) -> String {
        format!(
            "{{\"workflow\":\"test\",\"step_id\":\"{step_id}\",\"state\":\"passed\",\"verdict\":\"PASS\",\"message\":\"ok\"}}"
        )
    }

    // -- verify_ledger ----------------------------------------------------------

    #[test]
    fn verify_ledger_ok_on_single_line() {
        let p = temp_ledger("single");
        write_ledger(&p, &format!("{}\n", receipt_line("s1")));
        assert!(verify_ledger(&p).is_ok());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn verify_ledger_errors_2720_on_missing_file() {
        let p = PathBuf::from("/tmp/hle-verify-missing-99999.jsonl");
        let r = verify_ledger(&p);
        assert!(r.is_err());
        assert!(r.err().map_or(false, |e| e.to_string().contains("2720")));
    }

    #[test]
    fn verify_ledger_errors_2721_on_empty_file() {
        let p = temp_ledger("empty");
        write_ledger(&p, "");
        let r = verify_ledger(&p);
        assert!(r.is_err());
        assert!(r.err().map_or(false, |e| e.to_string().contains("2721")));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn verify_ledger_errors_2721_on_whitespace_only() {
        let p = temp_ledger("whitespace");
        write_ledger(&p, "   \n\n  \n");
        let r = verify_ledger(&p);
        assert!(r.is_err());
        assert!(r.err().map_or(false, |e| e.to_string().contains("2721")));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn verify_ledger_report_total_receipts_matches_line_count() {
        let p = temp_ledger("count");
        write_ledger(
            &p,
            &format!("{}\n{}\n", receipt_line("s1"), receipt_line("s2")),
        );
        let r = verify_ledger(&p);
        assert_eq!(r.map(|rep| rep.total_receipts), Ok(2));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn verify_ledger_report_ledger_path_correct() {
        let p = temp_ledger("path");
        write_ledger(&p, &format!("{}\n", receipt_line("s1")));
        let r = verify_ledger(&p);
        assert_eq!(r.map(|rep| rep.ledger_path), Ok(p.clone()));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn verify_ledger_report_overall_all_verified() {
        let p = temp_ledger("overall");
        // A PASS receipt should produce AllVerified overall (no mismatches, substrate PASS).
        write_ledger(&p, &format!("{}\n", receipt_line("s1")));
        let r = verify_ledger(&p);
        assert_eq!(r.map(|rep| rep.overall), Ok(VerifyVerdict::AllVerified));
        let _ = std::fs::remove_file(&p);
    }

    // -- format_report ----------------------------------------------------------

    #[test]
    fn format_report_human_contains_verdict() {
        let rep = VerifyReport {
            ledger_path: PathBuf::from("l.jsonl"),
            total_receipts: 3,
            verified: 3,
            hash_mismatches: 0,
            false_passes: 0,
            inconclusive: 0,
            overall: VerifyVerdict::AllVerified,
            per_receipt: Vec::new(),
        };
        assert!(format_report(&rep, false).contains("ALL_VERIFIED"));
    }

    #[test]
    fn format_report_human_contains_ledger_path() {
        let rep = VerifyReport {
            ledger_path: PathBuf::from("my-ledger.jsonl"),
            total_receipts: 1,
            verified: 1,
            hash_mismatches: 0,
            false_passes: 0,
            inconclusive: 0,
            overall: VerifyVerdict::AllVerified,
            per_receipt: Vec::new(),
        };
        assert!(format_report(&rep, false).contains("my-ledger.jsonl"));
    }

    #[test]
    fn format_report_json_contains_schema_field() {
        let rep = VerifyReport {
            ledger_path: PathBuf::from("l.jsonl"),
            total_receipts: 0,
            verified: 0,
            hash_mismatches: 0,
            false_passes: 0,
            inconclusive: 0,
            overall: VerifyVerdict::Failed,
            per_receipt: Vec::new(),
        };
        assert!(format_report(&rep, true).contains("hle.verify.report.v1"));
    }

    #[test]
    fn format_report_json_contains_overall_verdict() {
        let rep = VerifyReport {
            ledger_path: PathBuf::from("l.jsonl"),
            total_receipts: 1,
            verified: 0,
            hash_mismatches: 1,
            false_passes: 0,
            inconclusive: 0,
            overall: VerifyVerdict::Failed,
            per_receipt: Vec::new(),
        };
        assert!(format_report(&rep, true).contains("FAILED"));
    }

    #[test]
    fn format_human_bounded_under_2kb() {
        let rep = VerifyReport {
            ledger_path: PathBuf::from("l.jsonl"),
            total_receipts: 0,
            verified: 0,
            hash_mismatches: 0,
            false_passes: 0,
            inconclusive: 0,
            overall: VerifyVerdict::AllVerified,
            per_receipt: Vec::new(),
        };
        assert!(rep.format_human().len() <= 2048);
    }

    #[test]
    fn format_json_bounded_under_2kb() {
        let rep = VerifyReport {
            ledger_path: PathBuf::from("l.jsonl"),
            total_receipts: 0,
            verified: 0,
            hash_mismatches: 0,
            false_passes: 0,
            inconclusive: 0,
            overall: VerifyVerdict::AllVerified,
            per_receipt: Vec::new(),
        };
        assert!(rep.format_json().len() <= 2048);
    }

    // -- VerifyVerdict ----------------------------------------------------------

    #[test]
    fn verify_verdict_display_all_verified() {
        assert_eq!(VerifyVerdict::AllVerified.to_string(), "ALL_VERIFIED");
    }

    #[test]
    fn verify_verdict_display_failed() {
        assert_eq!(VerifyVerdict::Failed.to_string(), "FAILED");
    }

    #[test]
    fn verify_verdict_display_partially_verified() {
        assert_eq!(
            VerifyVerdict::PartiallyVerified.to_string(),
            "PARTIALLY_VERIFIED"
        );
    }

    #[test]
    fn verify_verdict_as_str_all_verified() {
        assert_eq!(VerifyVerdict::AllVerified.as_str(), "ALL_VERIFIED");
    }

    // -- ReceiptVerdict variants ------------------------------------------------

    #[test]
    fn receipt_verdict_verified_variant() {
        assert_eq!(ReceiptVerdict::Verified, ReceiptVerdict::Verified);
    }

    #[test]
    fn receipt_verdict_hash_mismatch_contains_step_id() {
        let v = ReceiptVerdict::HashMismatch {
            step_id: "s1".to_owned(),
            expected: "abc".to_owned(),
            actual: "xyz".to_owned(),
        };
        if let ReceiptVerdict::HashMismatch { step_id, .. } = v {
            assert_eq!(step_id, "s1");
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn receipt_verdict_false_pass_contains_step_id() {
        let v = ReceiptVerdict::FalsePassDetected {
            step_id: "s2".to_owned(),
            reason: "unanchored".to_owned(),
        };
        if let ReceiptVerdict::FalsePassDetected { step_id, .. } = v {
            assert_eq!(step_id, "s2");
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn receipt_verdict_inconclusive_contains_step_id() {
        let v = ReceiptVerdict::Inconclusive {
            step_id: "s3".to_owned(),
            reason: "missing fields".to_owned(),
        };
        if let ReceiptVerdict::Inconclusive { step_id, .. } = v {
            assert_eq!(step_id, "s3");
        } else {
            panic!("wrong variant");
        }
    }

    // -- truncate helper --------------------------------------------------------

    #[test]
    fn truncate_2kb_short_unchanged() {
        let s = "abc".to_owned();
        assert_eq!(truncate_2kb(s.clone()), s);
    }

    #[test]
    fn truncate_2kb_long_capped_with_marker() {
        let s = "y".repeat(4096);
        let out = truncate_2kb(s);
        assert!(out.len() <= 2048);
        assert!(out.ends_with("[...]"));
    }

    // -- verify_ledger: multi-receipt counting ----------------------------------

    #[test]
    fn verify_ledger_report_five_receipt_lines() {
        let p = temp_ledger("five");
        let lines = (0..5)
            .map(|i| receipt_line(&format!("s{i}")))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        write_ledger(&p, &lines);
        let r = verify_ledger(&p);
        assert_eq!(r.map(|rep| rep.total_receipts), Ok(5));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn verify_ledger_verified_count_equals_line_count() {
        let p = temp_ledger("verified-eq");
        // 3 PASS receipts → each has parseable fields → format-verified in M0 mode.
        let lines = format!(
            "{}\n{}\n{}\n",
            receipt_line("s1"),
            receipt_line("s2"),
            receipt_line("s3")
        );
        write_ledger(&p, &lines);
        let r = verify_ledger(&p);
        assert_eq!(r.map(|rep| rep.verified), Ok(3));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn verify_ledger_hash_mismatches_zero_for_pass_receipts() {
        let p = temp_ledger("mismatch-zero");
        write_ledger(&p, &format!("{}\n", receipt_line("s1")));
        let r = verify_ledger(&p);
        assert_eq!(r.map(|rep| rep.hash_mismatches), Ok(0));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn verify_ledger_false_passes_always_zero() {
        // false_passes is reserved for future anti-forgery detection; always 0 in M0.
        let p = temp_ledger("fp-zero");
        write_ledger(&p, &format!("{}\n", receipt_line("s1")));
        let r = verify_ledger(&p);
        assert_eq!(r.map(|rep| rep.false_passes), Ok(0));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn verify_ledger_inconclusive_zero_for_valid_receipts() {
        let p = temp_ledger("inc-zero");
        write_ledger(&p, &format!("{}\n", receipt_line("s1")));
        let r = verify_ledger(&p);
        assert_eq!(r.map(|rep| rep.inconclusive), Ok(0));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn verify_ledger_per_receipt_has_one_entry() {
        // per_receipt contains one ReceiptVerdict per processed receipt.
        let p = temp_ledger("per-entry");
        write_ledger(&p, &format!("{}\n", receipt_line("s1")));
        let r = verify_ledger(&p);
        assert_eq!(r.map(|rep| rep.per_receipt.len()), Ok(1));
        let _ = std::fs::remove_file(&p);
    }

    // -- error messages contain paths ------------------------------------------

    #[test]
    fn verify_ledger_error_2720_names_path() {
        let p = PathBuf::from("/tmp/hle-verify-2720-namepath-77777.jsonl");
        let r = verify_ledger(&p);
        assert!(r.err().map_or(false, |e| e
            .to_string()
            .contains("hle-verify-2720-namepath")));
    }

    #[test]
    fn verify_ledger_error_2721_names_path() {
        let p = temp_ledger("2721-path");
        write_ledger(&p, "");
        let r = verify_ledger(&p);
        assert!(r.err().map_or(false, |e| e.to_string().contains("2721")));
        let _ = std::fs::remove_file(&p);
    }

    // -- VerifyReport Display ----------------------------------------------------

    #[test]
    fn verify_report_display_uses_format_human() {
        let rep = VerifyReport {
            ledger_path: PathBuf::from("l.jsonl"),
            total_receipts: 1,
            verified: 1,
            hash_mismatches: 0,
            false_passes: 0,
            inconclusive: 0,
            overall: VerifyVerdict::AllVerified,
            per_receipt: Vec::new(),
        };
        assert_eq!(rep.to_string(), rep.format_human());
    }

    // -- format_human field coverage -------------------------------------------

    #[test]
    fn format_human_contains_receipt_count() {
        let rep = VerifyReport {
            ledger_path: PathBuf::from("l.jsonl"),
            total_receipts: 7,
            verified: 7,
            hash_mismatches: 0,
            false_passes: 0,
            inconclusive: 0,
            overall: VerifyVerdict::AllVerified,
            per_receipt: Vec::new(),
        };
        assert!(rep.format_human().contains("receipts=7"));
    }

    #[test]
    fn format_human_contains_verified_count() {
        let rep = VerifyReport {
            ledger_path: PathBuf::from("l.jsonl"),
            total_receipts: 4,
            verified: 4,
            hash_mismatches: 0,
            false_passes: 0,
            inconclusive: 0,
            overall: VerifyVerdict::AllVerified,
            per_receipt: Vec::new(),
        };
        assert!(rep.format_human().contains("verified=4"));
    }

    #[test]
    fn format_human_contains_mismatch_count() {
        let rep = VerifyReport {
            ledger_path: PathBuf::from("l.jsonl"),
            total_receipts: 2,
            verified: 1,
            hash_mismatches: 1,
            false_passes: 0,
            inconclusive: 0,
            overall: VerifyVerdict::Failed,
            per_receipt: Vec::new(),
        };
        assert!(rep.format_human().contains("mismatches=1"));
    }

    #[test]
    fn format_human_contains_false_passes_count() {
        let rep = VerifyReport {
            ledger_path: PathBuf::from("l.jsonl"),
            total_receipts: 3,
            verified: 2,
            hash_mismatches: 0,
            false_passes: 1,
            inconclusive: 0,
            overall: VerifyVerdict::Failed,
            per_receipt: Vec::new(),
        };
        assert!(rep.format_human().contains("false_passes=1"));
    }

    #[test]
    fn format_human_partially_verified_verdict() {
        let rep = VerifyReport {
            ledger_path: PathBuf::from("l.jsonl"),
            total_receipts: 3,
            verified: 2,
            hash_mismatches: 0,
            false_passes: 0,
            inconclusive: 1,
            overall: VerifyVerdict::PartiallyVerified,
            per_receipt: Vec::new(),
        };
        assert!(rep.format_human().contains("PARTIALLY_VERIFIED"));
    }

    // -- format_json field coverage --------------------------------------------

    #[test]
    fn format_json_contains_total_receipts_field() {
        let rep = VerifyReport {
            ledger_path: PathBuf::from("l.jsonl"),
            total_receipts: 9,
            verified: 9,
            hash_mismatches: 0,
            false_passes: 0,
            inconclusive: 0,
            overall: VerifyVerdict::AllVerified,
            per_receipt: Vec::new(),
        };
        assert!(rep.format_json().contains("\"total_receipts\":9"));
    }

    #[test]
    fn format_json_contains_verified_field() {
        let rep = VerifyReport {
            ledger_path: PathBuf::from("l.jsonl"),
            total_receipts: 6,
            verified: 6,
            hash_mismatches: 0,
            false_passes: 0,
            inconclusive: 0,
            overall: VerifyVerdict::AllVerified,
            per_receipt: Vec::new(),
        };
        assert!(rep.format_json().contains("\"verified\":6"));
    }

    #[test]
    fn format_json_contains_hash_mismatches_field() {
        let rep = VerifyReport {
            ledger_path: PathBuf::from("l.jsonl"),
            total_receipts: 2,
            verified: 1,
            hash_mismatches: 1,
            false_passes: 0,
            inconclusive: 0,
            overall: VerifyVerdict::Failed,
            per_receipt: Vec::new(),
        };
        assert!(rep.format_json().contains("\"hash_mismatches\":1"));
    }

    #[test]
    fn format_json_contains_false_passes_field() {
        let rep = VerifyReport {
            ledger_path: PathBuf::from("l.jsonl"),
            total_receipts: 2,
            verified: 1,
            hash_mismatches: 0,
            false_passes: 1,
            inconclusive: 0,
            overall: VerifyVerdict::Failed,
            per_receipt: Vec::new(),
        };
        assert!(rep.format_json().contains("\"false_passes\":1"));
    }

    #[test]
    fn format_json_contains_inconclusive_field() {
        let rep = VerifyReport {
            ledger_path: PathBuf::from("l.jsonl"),
            total_receipts: 2,
            verified: 1,
            hash_mismatches: 0,
            false_passes: 0,
            inconclusive: 1,
            overall: VerifyVerdict::PartiallyVerified,
            per_receipt: Vec::new(),
        };
        assert!(rep.format_json().contains("\"inconclusive\":1"));
    }

    #[test]
    fn format_json_ledger_path_present() {
        let rep = VerifyReport {
            ledger_path: PathBuf::from("my-run.jsonl"),
            total_receipts: 1,
            verified: 1,
            hash_mismatches: 0,
            false_passes: 0,
            inconclusive: 0,
            overall: VerifyVerdict::AllVerified,
            per_receipt: Vec::new(),
        };
        assert!(rep.format_json().contains("my-run.jsonl"));
    }

    // -- format_report dispatch -------------------------------------------------

    #[test]
    fn format_report_json_true_returns_json_object() {
        let rep = VerifyReport {
            ledger_path: PathBuf::from("l.jsonl"),
            total_receipts: 0,
            verified: 0,
            hash_mismatches: 0,
            false_passes: 0,
            inconclusive: 0,
            overall: VerifyVerdict::AllVerified,
            per_receipt: Vec::new(),
        };
        let out = format_report(&rep, true);
        assert!(out.starts_with('{'));
    }

    #[test]
    fn format_report_json_false_returns_human_string() {
        let rep = VerifyReport {
            ledger_path: PathBuf::from("l.jsonl"),
            total_receipts: 0,
            verified: 0,
            hash_mismatches: 0,
            false_passes: 0,
            inconclusive: 0,
            overall: VerifyVerdict::AllVerified,
            per_receipt: Vec::new(),
        };
        let out = format_report(&rep, false);
        assert!(out.starts_with("hle verify"));
    }

    // -- truncate boundary (exactly at limit) -----------------------------------

    #[test]
    fn truncate_2kb_exactly_at_limit_unchanged() {
        let s = "z".repeat(2048);
        let out = truncate_2kb(s.clone());
        assert_eq!(out.len(), 2048);
    }

    #[test]
    fn truncate_2kb_one_over_limit_gets_marker() {
        let s = "z".repeat(2049);
        let out = truncate_2kb(s);
        assert!(out.len() <= 2048);
        assert!(out.ends_with("[...]"));
    }

    // -- VerifyVerdict PartialEq / Copy ----------------------------------------

    #[test]
    fn verify_verdict_copy_semantics() {
        let v = VerifyVerdict::AllVerified;
        let w = v;
        assert_eq!(v, w);
    }

    #[test]
    fn verify_verdict_all_variants_distinct() {
        assert_ne!(VerifyVerdict::AllVerified, VerifyVerdict::Failed);
        assert_ne!(VerifyVerdict::Failed, VerifyVerdict::PartiallyVerified);
    }

    // -- ReceiptVerdict equality -----------------------------------------------

    #[test]
    fn receipt_verdict_inconclusive_fields_accessible() {
        let v = ReceiptVerdict::Inconclusive {
            step_id: "x".to_owned(),
            reason: "y".to_owned(),
        };
        if let ReceiptVerdict::Inconclusive { reason, .. } = v {
            assert_eq!(reason, "y");
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn receipt_verdict_hash_mismatch_expected_field() {
        let v = ReceiptVerdict::HashMismatch {
            step_id: "s1".to_owned(),
            expected: "abc".to_owned(),
            actual: "xyz".to_owned(),
        };
        if let ReceiptVerdict::HashMismatch { expected, .. } = v {
            assert_eq!(expected, "abc");
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn receipt_verdict_hash_mismatch_actual_field() {
        let v = ReceiptVerdict::HashMismatch {
            step_id: "s1".to_owned(),
            expected: "abc".to_owned(),
            actual: "xyz".to_owned(),
        };
        if let ReceiptVerdict::HashMismatch { actual, .. } = v {
            assert_eq!(actual, "xyz");
        } else {
            panic!("wrong variant");
        }
    }

    // -- ledger with only blank lines ------------------------------------------

    #[test]
    fn verify_ledger_blank_lines_only_errors_2721() {
        let p = temp_ledger("blanks-only2");
        write_ledger(&p, "\n\n\n");
        let r = verify_ledger(&p);
        assert!(r.is_err());
        assert!(r.err().map_or(false, |e| e.to_string().contains("2721")));
        let _ = std::fs::remove_file(&p);
    }

    // ── Gap-2: receipt_sha256 field tests ─────────────────────────────────────

    /// Produce a receipt line that includes a correct `receipt_sha256` field.
    fn receipt_line_with_sha(step_id: &str) -> String {
        use substrate_emit::receipt_sha256_hex;
        // Canonical fields: workflow="test", step_id, verdict="PASS", state="passed", message="ok".
        let sha = receipt_sha256_hex("test", step_id, "PASS", "passed", "ok");
        format!(
            "{{\"workflow\":\"test\",\"step_id\":\"{step_id}\",\"state\":\"passed\",\
             \"verdict\":\"PASS\",\"message\":\"ok\",\"receipt_sha256\":\"{sha}\"}}"
        )
    }

    #[test]
    fn verify_ledger_with_sha_field_passes() {
        let p = temp_ledger("sha-pass");
        write_ledger(&p, &format!("{}\n", receipt_line_with_sha("s1")));
        let r = verify_ledger(&p);
        assert!(r.is_ok(), "expected Ok, got {r:?}");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn verify_ledger_forged_sha_errors_e2722() {
        let p = temp_ledger("sha-forged");
        // Hand-forged line with a valid structure but wrong SHA (all zeros).
        let forged = format!(
            "{{\"workflow\":\"test\",\"step_id\":\"s1\",\"state\":\"passed\",\
             \"verdict\":\"PASS\",\"message\":\"ok\",\
             \"receipt_sha256\":\"{}\"}}",
            "0".repeat(64)
        );
        write_ledger(&p, &format!("{forged}\n"));
        let r = verify_ledger(&p);
        assert!(r.is_err(), "forged receipt must be rejected");
        assert!(
            r.err().map_or(false, |e| e.to_string().contains("E2722")),
            "error must contain E2722"
        );
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn verify_ledger_legacy_receipt_without_sha_warns_but_passes() {
        // A receipt without receipt_sha256 should succeed (backward compat).
        let p = temp_ledger("sha-legacy");
        write_ledger(&p, &format!("{}\n", receipt_line("s1")));
        let r = verify_ledger(&p);
        assert!(
            r.is_ok(),
            "legacy receipt must not fail in default mode: {r:?}"
        );
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn verify_ledger_strict_sha_rejects_missing_field() {
        let p = temp_ledger("sha-strict-missing");
        write_ledger(&p, &format!("{}\n", receipt_line("s1")));
        let r = verify_ledger_with_opts(&p, true);
        assert!(
            r.is_err(),
            "strict mode must reject receipt without receipt_sha256"
        );
        assert!(
            r.err().map_or(false, |e| e.to_string().contains("E2723")),
            "error must contain E2723"
        );
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn verify_ledger_strict_sha_accepts_correct_sha_field() {
        let p = temp_ledger("sha-strict-pass");
        write_ledger(&p, &format!("{}\n", receipt_line_with_sha("s1")));
        let r = verify_ledger_with_opts(&p, true);
        assert!(
            r.is_ok(),
            "strict mode must accept a receipt with correct SHA: {r:?}"
        );
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn verify_ledger_e2722_error_contains_step_id() {
        let p = temp_ledger("sha-e2722-step");
        let forged = format!(
            "{{\"workflow\":\"test\",\"step_id\":\"myStep\",\"state\":\"passed\",\
             \"verdict\":\"PASS\",\"message\":\"ok\",\
             \"receipt_sha256\":\"{}\"}}",
            "a".repeat(64)
        );
        write_ledger(&p, &format!("{forged}\n"));
        let r = verify_ledger(&p);
        assert!(
            r.err().map_or(false, |e| e.to_string().contains("myStep")),
            "error must name the step_id"
        );
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn json_field_opt_returns_none_for_missing_field() {
        let line = r#"{"workflow":"demo","step_id":"s1"}"#;
        assert!(json_field_opt(line, "receipt_sha256").is_none());
    }

    #[test]
    fn json_field_opt_returns_some_for_present_field() {
        let sha = "a".repeat(64);
        let line = format!(r#"{{"workflow":"demo","step_id":"s1","receipt_sha256":"{sha}"}}"#);
        assert_eq!(json_field_opt(&line, "receipt_sha256"), Some(sha));
    }
}
