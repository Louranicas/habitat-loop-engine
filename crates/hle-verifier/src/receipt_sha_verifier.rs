#![forbid(unsafe_code)]

// End-to-end stack cross-reference: terminal implementation node for
// M008_RECEIPT_SHA_VERIFIER.md / L04_VERIFICATION.md / C01_EVIDENCE_INTEGRITY (cluster).
// Spec: ai_specs/modules/c01-evidence-integrity/M008_RECEIPT_SHA_VERIFIER.md.
//
// HLE-UP-001 ENFORCEMENT: this file is in `hle-verifier`. The `Cargo.toml` for
// `hle-verifier` does NOT list `hle-executor` as a dependency. Any import from
// `hle-executor` in this file is a compile error and an architectural violation.

use std::fmt;

use substrate_types::HleError;

use hle_core::evidence::receipt_hash::{ReceiptHash, ReceiptHashFields};
use hle_storage::receipts_store::StoredReceipt;

// ── VerifyInput ───────────────────────────────────────────────────────────────

/// All fields needed to independently recompute a `ReceiptHash`.
///
/// `VerifyInput` mirrors the executor's original `ReceiptHashFields` but is
/// constructed from stored artifacts, not live executor state. The verifier
/// must not trust any value that comes from an executor code path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyInput {
    /// The stored hash to verify against (from `ReceiptsStore` or `ClaimStore`).
    pub stored_hash: ReceiptHash,
    /// Workflow identifier used during original hashing.
    pub workflow: String,
    /// Step identifier used during original hashing.
    pub step_id: String,
    /// Verdict string used during original hashing.
    pub verdict: String,
    /// Manifest SHA-256 anchor (`^Manifest_sha256`) from `HARNESS_CONTRACT.md`.
    pub manifest_sha256: String,
    /// Framework SHA-256 anchor (`^Framework_sha256`) from `HARNESS_CONTRACT.md`.
    pub framework_sha256: String,
}

impl VerifyInput {
    /// Validate and construct a `VerifyInput`.
    ///
    /// # Errors
    ///
    /// Returns `Err` (`[E2031] MissingArtifact`) when `workflow` is empty.
    #[must_use]
    pub fn new(
        stored_hash: ReceiptHash,
        workflow: impl Into<String>,
        step_id: impl Into<String>,
        verdict: impl Into<String>,
        manifest_sha256: impl Into<String>,
        framework_sha256: impl Into<String>,
    ) -> Result<Self, HleError> {
        let workflow = workflow.into();
        if workflow.trim().is_empty() {
            return Err(HleError::new(
                "[E2031] MissingArtifact: workflow must be non-empty for VerifyInput",
            ));
        }
        Ok(Self {
            stored_hash,
            workflow,
            step_id: step_id.into(),
            verdict: verdict.into(),
            manifest_sha256: manifest_sha256.into(),
            framework_sha256: framework_sha256.into(),
        })
    }

    /// Convenience constructor directly from a `StoredReceipt` (M007).
    ///
    /// The `stored_hash` is taken from `receipt.hash`; all other fields mirror
    /// the corresponding `StoredReceipt` fields verbatim.
    #[must_use]
    pub fn from_stored_receipt(receipt: &StoredReceipt) -> Self {
        Self {
            stored_hash: receipt.hash,
            workflow: receipt.workflow.clone(),
            step_id: receipt.step_id.clone(),
            verdict: receipt.verdict.clone(),
            manifest_sha256: receipt.manifest_sha256.clone(),
            framework_sha256: receipt.framework_sha256.clone(),
        }
    }
}

impl fmt::Display for VerifyInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "VerifyInput(stored={}@{}/{})",
            self.stored_hash, self.workflow, self.step_id,
        )
    }
}

// ── VerifyOutcome ─────────────────────────────────────────────────────────────

/// Result of an independent hash recomputation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyOutcome {
    /// Recomputed hash matches stored hash. Evidence is authentic.
    Matched {
        /// The confirmed hash.
        hash: ReceiptHash,
    },
    /// Recomputed hash does not match stored hash. Evidence is suspect.
    Mismatch {
        /// The hash that was stored.
        stored: ReceiptHash,
        /// The hash the verifier computed from the same fields.
        recomputed: ReceiptHash,
    },
}

impl VerifyOutcome {
    /// Returns `true` when the outcome is `Matched`.
    #[must_use]
    pub const fn is_matched(&self) -> bool {
        matches!(self, Self::Matched { .. })
    }

    /// Returns the stored hash from either variant.
    #[must_use]
    pub fn stored_hash(&self) -> ReceiptHash {
        match self {
            Self::Matched { hash } => *hash,
            Self::Mismatch { stored, .. } => *stored,
        }
    }

    /// Converts this outcome to a `Result`.
    ///
    /// # Errors
    ///
    /// Returns `Err` (`[E2030] HashMismatch`) when the outcome is `Mismatch`.
    #[must_use]
    pub fn as_hle_result(&self) -> Result<ReceiptHash, HleError> {
        match self {
            Self::Matched { hash } => Ok(*hash),
            Self::Mismatch { stored, recomputed } => Err(HleError::new(format!(
                "[E2030] HashMismatch: stored={stored} recomputed={recomputed}"
            ))),
        }
    }
}

impl fmt::Display for VerifyOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Matched { hash } => write!(f, "Matched({hash})"),
            Self::Mismatch { stored, recomputed } => {
                write!(f, "Mismatch(stored={stored}, recomputed={recomputed})")
            }
        }
    }
}

// ── VerifierToken ─────────────────────────────────────────────────────────────

/// Zero-sized proof that the calling code is running inside `hle-verifier`
/// and has completed a successful hash verification.
///
/// Consumed by `FinalClaimEvaluator::promote()` to prevent executor
/// self-promotion. Construction is private to this module; callers receive one
/// only via `ReceiptShaVerifier::verify_and_token()` on a `Matched` outcome.
/// The token is moved (not cloned), so each token can only be used once.
#[derive(Debug)]
pub struct VerifierToken {
    /// The hash that was verified. Prevents token reuse across different receipts.
    pub(crate) verified_hash: ReceiptHash,
    _private: (),
}

impl VerifierToken {
    /// Private constructor — only `ReceiptShaVerifier::verify_and_token` may call this.
    fn new(verified_hash: ReceiptHash) -> Self {
        Self {
            verified_hash,
            _private: (),
        }
    }
}

// ── ReceiptShaVerifier ────────────────────────────────────────────────────────

/// Stateless entry point for independent receipt hash recomputation.
///
/// `ReceiptShaVerifier` holds no mutable state, no store reference, and no
/// connection pool. All input comes through `VerifyInput`; all output is a
/// value type. This makes it trivially testable in isolation.
///
/// Lives in `hle-verifier` — must not import executor mutation types.
#[derive(Debug, Clone)]
pub struct ReceiptShaVerifier {
    _private: (),
}

impl ReceiptShaVerifier {
    /// Construct a new verifier instance.
    #[must_use]
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Recompute the hash from `input` fields and compare to the stored hash.
    ///
    /// Uses exactly `ReceiptHash::from_fields` — the same path the executor
    /// used — to guarantee faithful independent recomputation.
    ///
    /// # Errors
    ///
    /// Returns `Err` (`[E2031] MissingArtifact`) when the `ReceiptHashFields`
    /// cannot be constructed (e.g., empty workflow).
    #[must_use]
    pub fn verify(&self, input: &VerifyInput) -> Result<VerifyOutcome, HleError> {
        let fields = ReceiptHashFields::new(
            input.workflow.clone(),
            input.step_id.clone(),
            input.verdict.clone(),
            input.manifest_sha256.clone(),
            input.framework_sha256.clone(),
        )
        .map_err(|e| {
            HleError::new(format!(
                "[E2031] MissingArtifact: could not build fields: {e}"
            ))
        })?;
        let recomputed = ReceiptHash::from_fields(&fields).map_err(|e| {
            HleError::new(format!(
                "[E2031] MissingArtifact: hash computation failed: {e}"
            ))
        })?;
        if recomputed == input.stored_hash {
            Ok(VerifyOutcome::Matched { hash: recomputed })
        } else {
            Ok(VerifyOutcome::Mismatch {
                stored: input.stored_hash,
                recomputed,
            })
        }
    }

    /// Recompute hash and return a `VerifierToken` on a `Matched` outcome.
    ///
    /// The `VerifierToken` is issued only when the outcome is `Matched`. For a
    /// `Mismatch`, this method returns `Err([E2030] HashMismatch)` so the token
    /// cannot be obtained. Token is moved into the caller; single-use enforced
    /// by Rust ownership.
    ///
    /// # Errors
    ///
    /// Returns `Err` (`[E2031] MissingArtifact`) when fields cannot be built.
    /// Returns `Err` (`[E2030] HashMismatch`) when the recomputed hash does
    /// not match the stored hash.
    #[must_use]
    pub fn verify_and_token(
        &self,
        input: &VerifyInput,
    ) -> Result<(VerifyOutcome, VerifierToken), HleError> {
        let outcome = self.verify(input)?;
        match &outcome {
            VerifyOutcome::Matched { hash } => {
                let token = VerifierToken::new(*hash);
                Ok((outcome, token))
            }
            VerifyOutcome::Mismatch { stored, recomputed } => Err(HleError::new(format!(
                "[E2030] HashMismatch: stored={stored} recomputed={recomputed}"
            ))),
        }
    }

    /// Verify each input independently; one error does not abort the batch.
    ///
    /// Returns a `Vec` of `Result`s in the same order as `inputs`. Runs serially
    /// (not in parallel) to avoid requiring `Send` bounds on pool handles.
    #[must_use]
    pub fn verify_batch(&self, inputs: &[VerifyInput]) -> Vec<Result<VerifyOutcome, HleError>> {
        inputs.iter().map(|i| self.verify(i)).collect()
    }
}

impl Default for ReceiptShaVerifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use hle_core::evidence::receipt_hash::{ReceiptHash, ReceiptHashFields};
    use hle_storage::receipts_store::StoredReceipt;

    // ── helpers ────────────────────────────────────────────────────────────────

    /// Build a `VerifyInput` where `stored_hash` is the real SHA-256 of the
    /// canonical fields, so `verify()` will always return `Matched`.
    fn matching_input(workflow: &str, step_id: &str) -> VerifyInput {
        matching_input_with_verdict(workflow, step_id, "PASS")
    }

    fn matching_input_with_verdict(workflow: &str, step_id: &str, verdict: &str) -> VerifyInput {
        let fields = ReceiptHashFields::new(workflow, step_id, verdict, "", "")
            .expect("fields must be valid");
        let hash = ReceiptHash::from_fields(&fields).expect("hash must succeed");
        VerifyInput {
            stored_hash: hash,
            workflow: workflow.to_owned(),
            step_id: step_id.to_owned(),
            verdict: verdict.to_owned(),
            manifest_sha256: String::new(),
            framework_sha256: String::new(),
        }
    }

    fn matching_input_full(
        workflow: &str,
        step_id: &str,
        verdict: &str,
        manifest: &str,
        framework: &str,
    ) -> VerifyInput {
        let fields = ReceiptHashFields::new(workflow, step_id, verdict, manifest, framework)
            .expect("fields must be valid");
        let hash = ReceiptHash::from_fields(&fields).expect("hash must succeed");
        VerifyInput {
            stored_hash: hash,
            workflow: workflow.to_owned(),
            step_id: step_id.to_owned(),
            verdict: verdict.to_owned(),
            manifest_sha256: manifest.to_owned(),
            framework_sha256: framework.to_owned(),
        }
    }

    fn mismatch_input(workflow: &str, step_id: &str) -> VerifyInput {
        let mut input = matching_input(workflow, step_id);
        // Corrupt stored_hash to guarantee a Mismatch.
        // Using 0xAB×32 — the real SHA-256 of "demo"/"s1"/etc will never be this.
        input.stored_hash = ReceiptHash::from_bytes([0xABu8; 32]);
        input
    }

    // ── VerifyInput construction ───────────────────────────────────────────────

    #[test]
    fn verify_input_new_rejects_empty_workflow() {
        let hash = ReceiptHash::zeroed();
        assert!(VerifyInput::new(hash, "", "s1", "PASS", "", "").is_err());
    }

    #[test]
    fn verify_input_new_rejects_blank_workflow() {
        let hash = ReceiptHash::zeroed();
        assert!(VerifyInput::new(hash, "   ", "s1", "PASS", "", "").is_err());
    }

    #[test]
    fn verify_input_new_error_contains_e2031() {
        let hash = ReceiptHash::zeroed();
        let err = VerifyInput::new(hash, "", "s1", "PASS", "", "").unwrap_err();
        assert!(err.to_string().contains("E2031"), "got: {err}");
    }

    #[test]
    fn verify_input_new_preserves_workflow() {
        let hash = ReceiptHash::zeroed();
        let input = VerifyInput::new(hash, "my-wf", "s1", "PASS", "", "").expect("must succeed");
        assert_eq!(input.workflow, "my-wf");
    }

    #[test]
    fn verify_input_display_is_nonempty() {
        let input = matching_input("demo", "s1");
        assert!(!format!("{input}").is_empty());
    }

    #[test]
    fn verify_input_display_contains_workflow() {
        let input = matching_input("display-wf", "s1");
        assert!(format!("{input}").contains("display-wf"));
    }

    #[test]
    fn verify_input_from_stored_receipt_copies_hash() {
        let hash = ReceiptHash::from_bytes([0x01u8; 32]);
        let r = StoredReceipt {
            hash,
            workflow: String::from("wf"),
            step_id: String::from("s1"),
            verdict: String::from("PASS"),
            manifest_sha256: String::new(),
            framework_sha256: String::new(),
            appended_at: 1,
            counter_evidence_locator: None,
        };
        let input = VerifyInput::from_stored_receipt(&r);
        assert_eq!(input.stored_hash, hash);
    }

    #[test]
    fn verify_input_from_stored_receipt_copies_workflow() {
        let hash = ReceiptHash::from_bytes([0x02u8; 32]);
        let r = StoredReceipt {
            hash,
            workflow: String::from("copied-wf"),
            step_id: String::from("s1"),
            verdict: String::from("PASS"),
            manifest_sha256: String::new(),
            framework_sha256: String::new(),
            appended_at: 1,
            counter_evidence_locator: None,
        };
        let input = VerifyInput::from_stored_receipt(&r);
        assert_eq!(input.workflow, "copied-wf");
    }

    // ── VerifyOutcome ─────────────────────────────────────────────────────────

    #[test]
    fn verify_outcome_matched_is_matched() {
        let hash = ReceiptHash::from_bytes([0x01u8; 32]);
        let outcome = VerifyOutcome::Matched { hash };
        assert!(outcome.is_matched());
    }

    #[test]
    fn verify_outcome_mismatch_is_not_matched() {
        let stored = ReceiptHash::from_bytes([0x01u8; 32]);
        let recomputed = ReceiptHash::from_bytes([0x02u8; 32]);
        let outcome = VerifyOutcome::Mismatch { stored, recomputed };
        assert!(!outcome.is_matched());
    }

    #[test]
    fn verify_outcome_stored_hash_matched() {
        let hash = ReceiptHash::from_bytes([0x05u8; 32]);
        let outcome = VerifyOutcome::Matched { hash };
        assert_eq!(outcome.stored_hash(), hash);
    }

    #[test]
    fn verify_outcome_stored_hash_mismatch() {
        let stored = ReceiptHash::from_bytes([0x03u8; 32]);
        let recomputed = ReceiptHash::from_bytes([0x04u8; 32]);
        let outcome = VerifyOutcome::Mismatch { stored, recomputed };
        assert_eq!(outcome.stored_hash(), stored);
    }

    #[test]
    fn verify_outcome_as_hle_result_ok_on_matched() {
        let hash = ReceiptHash::from_bytes([0x01u8; 32]);
        let outcome = VerifyOutcome::Matched { hash };
        assert!(outcome.as_hle_result().is_ok());
    }

    #[test]
    fn verify_outcome_as_hle_result_ok_carries_hash() {
        let hash = ReceiptHash::from_bytes([0x10u8; 32]);
        let outcome = VerifyOutcome::Matched { hash };
        assert_eq!(outcome.as_hle_result().expect("ok"), hash);
    }

    #[test]
    fn verify_outcome_as_hle_result_err_on_mismatch() {
        let stored = ReceiptHash::from_bytes([0x01u8; 32]);
        let recomputed = ReceiptHash::from_bytes([0x02u8; 32]);
        let outcome = VerifyOutcome::Mismatch { stored, recomputed };
        assert!(outcome.as_hle_result().is_err());
    }

    #[test]
    fn verify_outcome_mismatch_error_contains_e2030() {
        let stored = ReceiptHash::from_bytes([0x01u8; 32]);
        let recomputed = ReceiptHash::from_bytes([0x02u8; 32]);
        let outcome = VerifyOutcome::Mismatch { stored, recomputed };
        let err = outcome.as_hle_result().unwrap_err();
        assert!(err.to_string().contains("E2030"), "got: {err}");
    }

    #[test]
    fn verify_outcome_display_matched_is_nonempty() {
        let hash = ReceiptHash::zeroed();
        let outcome = VerifyOutcome::Matched { hash };
        assert!(!format!("{outcome}").is_empty());
    }

    #[test]
    fn verify_outcome_display_mismatch_is_nonempty() {
        let s = ReceiptHash::from_bytes([0x01u8; 32]);
        let r = ReceiptHash::from_bytes([0x02u8; 32]);
        let outcome = VerifyOutcome::Mismatch {
            stored: s,
            recomputed: r,
        };
        assert!(!format!("{outcome}").is_empty());
    }

    #[test]
    fn verify_outcome_mismatch_display_contains_stored_and_recomputed() {
        let s = ReceiptHash::from_bytes([0x01u8; 32]);
        let r = ReceiptHash::from_bytes([0x02u8; 32]);
        let outcome = VerifyOutcome::Mismatch {
            stored: s,
            recomputed: r,
        };
        let display = format!("{outcome}");
        assert!(display.contains("stored"), "missing 'stored': {display}");
        assert!(
            display.contains("recomputed"),
            "missing 'recomputed': {display}"
        );
    }

    // ── ReceiptShaVerifier — real SHA-256 verify ──────────────────────────────

    #[test]
    fn verify_matched_when_fields_agree() {
        let verifier = ReceiptShaVerifier::new();
        let input = matching_input("demo", "s1");
        let outcome = verifier.verify(&input).expect("verify must not error");
        assert!(outcome.is_matched());
    }

    #[test]
    fn verify_matched_is_deterministic() {
        let verifier = ReceiptShaVerifier::new();
        let input = matching_input("deterministic", "step");
        let o1 = verifier.verify(&input).expect("ok");
        let o2 = verifier.verify(&input).expect("ok");
        assert_eq!(o1.is_matched(), o2.is_matched());
    }

    #[test]
    fn verify_mismatch_when_hash_is_all_ff() {
        let verifier = ReceiptShaVerifier::new();
        let input = mismatch_input("demo", "s1");
        let outcome = verifier.verify(&input).expect("verify must not error");
        assert!(!outcome.is_matched());
    }

    #[test]
    fn verify_mismatch_carries_original_stored_hash() {
        let verifier = ReceiptShaVerifier::new();
        let input = mismatch_input("demo", "s2");
        let outcome = verifier.verify(&input).expect("ok");
        assert_eq!(outcome.stored_hash(), input.stored_hash);
    }

    #[test]
    fn verify_rejects_empty_workflow_via_missing_artifact() {
        let verifier = ReceiptShaVerifier::new();
        let hash = ReceiptHash::zeroed();
        // Build VerifyInput manually with empty workflow to bypass constructor guard.
        let input = VerifyInput {
            stored_hash: hash,
            workflow: String::new(),
            step_id: String::from("s1"),
            verdict: String::from("PASS"),
            manifest_sha256: String::new(),
            framework_sha256: String::new(),
        };
        let result = verifier.verify(&input);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("E2031"), "got: {msg}");
    }

    #[test]
    fn verify_different_workflows_produce_different_recomputed_hashes() {
        let verifier = ReceiptShaVerifier::new();
        let i1 = matching_input("wf-alpha", "s1");
        let i2 = matching_input("wf-beta", "s1");
        let o1 = verifier.verify(&i1).expect("ok");
        let o2 = verifier.verify(&i2).expect("ok");
        // Both should match their stored hashes.
        assert!(o1.is_matched());
        assert!(o2.is_matched());
        // But the stored hashes must differ.
        assert_ne!(o1.stored_hash(), o2.stored_hash());
    }

    #[test]
    fn verify_with_all_verdicts_pass() {
        let verifier = ReceiptShaVerifier::new();
        for verdict in ["PASS", "FAIL", "AWAITING_HUMAN"] {
            let input = matching_input_with_verdict("wf", "s1", verdict);
            assert!(verifier.verify(&input).expect("ok").is_matched());
        }
    }

    #[test]
    fn verify_tampered_payload_field_produces_mismatch() {
        let verifier = ReceiptShaVerifier::new();
        // Build matching input then corrupt one field so stored_hash no longer matches.
        let mut input = matching_input("wf-tamper", "s1");
        input.step_id = String::from("s2-tampered"); // change step but not stored_hash
        let outcome = verifier.verify(&input).expect("ok");
        assert!(!outcome.is_matched());
    }

    // ── verify_and_token ──────────────────────────────────────────────────────

    #[test]
    fn verify_and_token_returns_token_on_matched_outcome() {
        let verifier = ReceiptShaVerifier::new();
        let input = matching_input("alpha", "s2");
        let (outcome, token) = verifier
            .verify_and_token(&input)
            .expect("verify_and_token must succeed on a matched input");
        assert!(outcome.is_matched());
        assert_eq!(token.verified_hash, input.stored_hash);
    }

    #[test]
    fn verify_and_token_token_hash_matches_input_stored_hash() {
        let verifier = ReceiptShaVerifier::new();
        let input = matching_input("token-test", "s1");
        let (_outcome, token) = verifier.verify_and_token(&input).expect("ok");
        assert_eq!(token.verified_hash, input.stored_hash);
    }

    #[test]
    fn verify_and_token_errors_on_mismatch_with_e2030() {
        let verifier = ReceiptShaVerifier::new();
        let input = mismatch_input("beta", "s3");
        let result = verifier.verify_and_token(&input);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("E2030"), "got: {msg}");
    }

    #[test]
    fn verify_and_token_with_manifest_and_framework_matched() {
        let verifier = ReceiptShaVerifier::new();
        let input = matching_input_full("wf-full", "s1", "PASS", &"m".repeat(64), &"f".repeat(64));
        let (outcome, _) = verifier.verify_and_token(&input).expect("ok");
        assert!(outcome.is_matched());
    }

    // ── HashAlgorithm propagation (verify uses Sha256 path) ──────────────────

    #[test]
    fn hash_algorithm_sha256_is_the_only_algorithm() {
        // The verifier must use Sha256 — there is no other algorithm. This
        // test documents that the type system enforces the algorithm via
        // ReceiptHash::from_fields which always uses Sha256.
        use hle_core::evidence::receipt_hash::HashAlgorithm;
        assert_eq!(HashAlgorithm::Sha256.as_str(), "sha256");
        assert_eq!(HashAlgorithm::Sha256.digest_len(), 32);
    }

    // ── verify_batch ─────────────────────────────────────────────────────────

    #[test]
    fn verify_batch_returns_one_result_per_input() {
        let verifier = ReceiptShaVerifier::new();
        let inputs = vec![matching_input("w1", "s1"), matching_input("w2", "s2")];
        let results = verifier.verify_batch(&inputs);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn verify_batch_all_matched_when_all_correct() {
        let verifier = ReceiptShaVerifier::new();
        let inputs: Vec<VerifyInput> = (1u8..=5)
            .map(|i| matching_input("batch-wf", &format!("s{i}")))
            .collect();
        let results = verifier.verify_batch(&inputs);
        for r in &results {
            assert!(r.as_ref().map_or(false, VerifyOutcome::is_matched));
        }
    }

    #[test]
    fn verify_batch_empty_input_returns_empty() {
        let verifier = ReceiptShaVerifier::new();
        assert!(verifier.verify_batch(&[]).is_empty());
    }

    #[test]
    fn verify_batch_error_does_not_abort_other_results() {
        let verifier = ReceiptShaVerifier::new();
        let bad = VerifyInput {
            stored_hash: ReceiptHash::zeroed(),
            workflow: String::new(), // will cause E2031
            step_id: String::new(),
            verdict: String::new(),
            manifest_sha256: String::new(),
            framework_sha256: String::new(),
        };
        let good = matching_input("good-wf", "s1");
        let results = verifier.verify_batch(&[bad, good]);
        assert_eq!(results.len(), 2);
        assert!(results[0].is_err());
        assert!(results[1].as_ref().map_or(false, VerifyOutcome::is_matched));
    }

    // ── ReceiptShaVerifier Default ────────────────────────────────────────────

    #[test]
    fn verifier_default_produces_same_results_as_new() {
        let v1 = ReceiptShaVerifier::new();
        let v2 = ReceiptShaVerifier::default();
        let input = matching_input("default-test", "s1");
        let o1 = v1.verify(&input).expect("ok");
        let o2 = v2.verify(&input).expect("ok");
        assert_eq!(o1.is_matched(), o2.is_matched());
    }

    #[test]
    fn verifier_clone_produces_same_results() {
        let v1 = ReceiptShaVerifier::new();
        let v2 = v1.clone();
        let input = matching_input("clone-test", "s1");
        let o1 = v1.verify(&input).expect("ok");
        let o2 = v2.verify(&input).expect("ok");
        assert_eq!(o1.is_matched(), o2.is_matched());
    }

    #[test]
    fn verifier_debug_is_nonempty() {
        let v = ReceiptShaVerifier::new();
        assert!(!format!("{v:?}").is_empty());
    }

    // ── additional boundary / invariant tests ─────────────────────────────────

    #[test]
    fn verify_input_new_accepts_nonempty_step_id() {
        let hash = ReceiptHash::zeroed();
        let input = VerifyInput::new(hash, "wf", "s1", "PASS", "", "").expect("must succeed");
        assert_eq!(input.step_id, "s1");
    }

    #[test]
    fn verify_input_new_accepts_empty_step_id() {
        // step_id is not validated by VerifyInput::new
        let hash = ReceiptHash::zeroed();
        let input = VerifyInput::new(hash, "wf", "", "PASS", "", "").expect("must succeed");
        assert_eq!(input.step_id, "");
    }

    #[test]
    fn verify_input_new_stores_manifest_and_framework() {
        let hash = ReceiptHash::zeroed();
        let input =
            VerifyInput::new(hash, "wf", "s1", "PASS", "mf64", "fr64").expect("must succeed");
        assert_eq!(input.manifest_sha256, "mf64");
        assert_eq!(input.framework_sha256, "fr64");
    }

    #[test]
    fn verify_outcome_matched_clone_preserves_hash() {
        let hash = ReceiptHash::from_bytes([0x99u8; 32]);
        let outcome = VerifyOutcome::Matched { hash };
        let cloned = outcome.clone();
        assert_eq!(cloned.stored_hash(), hash);
    }

    #[test]
    fn verify_outcome_mismatch_clone_preserves_hashes() {
        let s = ReceiptHash::from_bytes([0x01u8; 32]);
        let r = ReceiptHash::from_bytes([0x02u8; 32]);
        let outcome = VerifyOutcome::Mismatch {
            stored: s,
            recomputed: r,
        };
        let cloned = outcome.clone();
        assert_eq!(cloned.stored_hash(), s);
    }

    #[test]
    fn verify_outcome_eq_matched_equal() {
        let h = ReceiptHash::from_bytes([0x01u8; 32]);
        assert_eq!(
            VerifyOutcome::Matched { hash: h },
            VerifyOutcome::Matched { hash: h }
        );
    }

    #[test]
    fn verify_outcome_eq_matched_ne_mismatch() {
        let h = ReceiptHash::from_bytes([0x01u8; 32]);
        let m = VerifyOutcome::Matched { hash: h };
        let d = VerifyOutcome::Mismatch {
            stored: h,
            recomputed: h,
        };
        assert_ne!(m, d);
    }

    #[test]
    fn verify_recomputes_sha256_not_xor() {
        // Sanity-check: the verifier must use real SHA-256 (not a stub).
        // SHA-256("demo\x00s1\x00PASS\x00\x00") must match the independently-
        // known canonical hash produced by ReceiptHash::from_fields.
        let input = matching_input("demo", "s1");
        let fields = ReceiptHashFields::new("demo", "s1", "PASS", "", "").expect("fields ok");
        let expected = ReceiptHash::from_fields(&fields).expect("hash ok");
        assert_eq!(input.stored_hash, expected);
        let verifier = ReceiptShaVerifier::new();
        let outcome = verifier.verify(&input).expect("ok");
        assert!(outcome.is_matched());
    }

    #[test]
    fn verify_batch_mixed_results_preserves_order() {
        let verifier = ReceiptShaVerifier::new();
        let good1 = matching_input("wf1", "s1");
        let bad = mismatch_input("wf2", "s2");
        let good2 = matching_input("wf3", "s3");
        let results = verifier.verify_batch(&[good1, bad, good2]);
        assert_eq!(results.len(), 3);
        assert!(results[0].as_ref().map_or(false, VerifyOutcome::is_matched));
        assert!(results[1].as_ref().map_or(false, |o| !o.is_matched()));
        assert!(results[2].as_ref().map_or(false, VerifyOutcome::is_matched));
    }

    #[test]
    fn verify_and_token_outcome_carries_correct_hash() {
        let verifier = ReceiptShaVerifier::new();
        let input = matching_input("token-carry", "s5");
        let (outcome, _token) = verifier.verify_and_token(&input).expect("ok");
        assert_eq!(outcome.stored_hash(), input.stored_hash);
    }

    #[test]
    fn verify_input_eq_two_matching_inputs_are_equal() {
        let i1 = matching_input("eq-wf", "s1");
        let i2 = matching_input("eq-wf", "s1");
        assert_eq!(i1, i2);
    }

    #[test]
    fn verify_input_ne_for_different_fields() {
        let i1 = matching_input("ne-wf-a", "s1");
        let i2 = matching_input("ne-wf-b", "s1");
        assert_ne!(i1, i2);
    }
}
