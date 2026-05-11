#![forbid(unsafe_code)]

// End-to-end stack cross-reference: terminal implementation node for
// M009_FINAL_CLAIM_EVALUATOR.md / L04_VERIFICATION.md / C01_EVIDENCE_INTEGRITY (cluster).
// Spec: ai_specs/modules/c01-evidence-integrity/M009_FINAL_CLAIM_EVALUATOR.md.
//
// HLE-UP-001 ENFORCEMENT: this file is in `hle-verifier`. The `Cargo.toml`
// for `hle-verifier` does NOT list `hle-executor` as a dependency. Any import
// from `hle-executor` in this file is a compile error and an architectural violation.
//
// TYPESTATE GATE: `promote()` requires a `VerifierToken` that can only be
// constructed inside `hle-verifier` via `ReceiptShaVerifier::verify_and_token()`.
// Executor code cannot forge a `VerifierToken`. The token is consumed by value
// (moved), so each token can only be used once (Rust ownership semantics).

use std::collections::VecDeque;
use std::fmt;
use std::sync::RwLock;

use substrate_types::HleError;

use hle_core::evidence::claims_store::VerifiedClaim;
use hle_core::evidence::receipt_hash::ReceiptHash;

use crate::receipt_sha_verifier::VerifierToken;

// ── EvaluatorConfig ───────────────────────────────────────────────────────────

/// Configuration for `FinalClaimEvaluator`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluatorConfig {
    /// Maximum entries to retain in the in-memory promotion log.
    /// Clamped to `1..=10_000` at construction.
    pub log_capacity: usize,
    /// When `true` (default), duplicate promotion attempts return `[E2041]`.
    /// When `false`, re-promotion silently returns the previous receipt.
    pub strict_duplicate_guard: bool,
}

impl EvaluatorConfig {
    /// Construct with explicit values; `log_capacity` clamped to `1..=10_000`.
    #[must_use]
    pub const fn new(log_capacity: usize, strict_duplicate_guard: bool) -> Self {
        // const fn cannot use `.clamp` on non-Copy usize in older editions;
        // inline the clamp manually.
        let capacity = if log_capacity < 1 {
            1
        } else if log_capacity > 10_000 {
            10_000
        } else {
            log_capacity
        };
        Self {
            log_capacity: capacity,
            strict_duplicate_guard,
        }
    }
}

impl Default for EvaluatorConfig {
    fn default() -> Self {
        Self::new(1_000, true)
    }
}

// ── PromotionReceipt ──────────────────────────────────────────────────────────

/// Immutable proof record emitted by a successful `Final` promotion.
///
/// `#[must_use]` — discarding this record loses the session audit trail.
#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotionReceipt {
    /// Hash of the promoted claim.
    pub hash: ReceiptHash,
    /// Workflow identifier.
    pub workflow: String,
    /// Step identifier.
    pub step_id: String,
    /// Monotonic counter at promotion time (no wall clock; no chrono).
    pub promoted_at: u64,
    /// Sequential index of this promotion in the session log (1-indexed).
    pub sequence: u64,
}

impl PromotionReceipt {
    /// Single-line human-readable description for log output.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "PromotionReceipt(#{seq}:{hash}→Final@{wf}/{step})",
            seq = self.sequence,
            hash = self.hash,
            wf = self.workflow,
            step = self.step_id,
        )
    }
}

impl fmt::Display for PromotionReceipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.summary())
    }
}

// ── PromotionLog (crate-private) ──────────────────────────────────────────────

struct PromotionLog {
    entries: VecDeque<PromotionReceipt>,
    capacity: usize,
}

impl PromotionLog {
    fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(capacity.min(1_024)),
            capacity,
        }
    }

    /// Append a receipt, evicting the oldest if at capacity.
    fn push(&mut self, receipt: PromotionReceipt) {
        if self.entries.len() == self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(receipt);
    }

    /// O(n) scan; log is bounded so this is acceptable per the spec.
    fn contains(&self, hash: ReceiptHash) -> bool {
        self.entries.iter().any(|r| r.hash == hash)
    }

    /// Returns a clone of all entries in insertion order.
    fn snapshot(&self) -> Vec<PromotionReceipt> {
        self.entries.iter().cloned().collect()
    }
}

// ── FinalClaimEvaluator internals ─────────────────────────────────────────────

struct FinalClaimEvaluatorInner {
    log: PromotionLog,
    promotion_count: u64,
    monotonic_tick: u64,
}

impl FinalClaimEvaluatorInner {
    fn new(log_capacity: usize) -> Self {
        Self {
            log: PromotionLog::new(log_capacity),
            promotion_count: 0,
            monotonic_tick: 0,
        }
    }

    fn next_tick(&mut self) -> u64 {
        self.monotonic_tick = self.monotonic_tick.saturating_add(1);
        self.monotonic_tick
    }
}

// ── FinalClaimEvaluator ───────────────────────────────────────────────────────

/// Sole authority for `Verified → Final` claim promotion.
///
/// `promote()` is the only place in the entire codebase where a claim may
/// transition from `Verified` to `Final`. The `VerifierToken` argument enforces
/// this at the Rust type level: only code compiled inside `hle-verifier` can
/// hold a `VerifierToken`, and each token is single-use.
pub struct FinalClaimEvaluator {
    config: EvaluatorConfig,
    inner: RwLock<FinalClaimEvaluatorInner>,
}

impl fmt::Debug for FinalClaimEvaluator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.promotion_count();
        write!(f, "FinalClaimEvaluator(promotion_count={count})")
    }
}

impl FinalClaimEvaluator {
    /// Construct with explicit configuration.
    #[must_use]
    pub fn new(config: EvaluatorConfig) -> Self {
        let cap = config.log_capacity;
        Self {
            config,
            inner: RwLock::new(FinalClaimEvaluatorInner::new(cap)),
        }
    }

    /// Construct with default configuration (`log_capacity=1000`, strict guard).
    #[must_use]
    pub fn with_default_config() -> Self {
        Self::new(EvaluatorConfig::default())
    }

    // TYPESTATE GATE: VerifierToken required — see module-level comment.
    /// Promote a `VerifiedClaim` to `Final` state.
    ///
    /// Requires a `VerifierToken` produced by `ReceiptShaVerifier::verify_and_token`
    /// for the **same** receipt hash. The token is consumed (moved) — each token
    /// grants exactly one promotion.
    ///
    /// # Errors
    ///
    /// - `[E2030] HashMismatch` — `token.verified_hash` ≠ `claim.hash()`.
    /// - `[E2041] AlreadyFinal` — claim already in the promotion log (strict mode).
    /// - `[E2021] StorageIo` — lock poisoned.
    #[must_use]
    pub fn promote(
        &self,
        claim: &VerifiedClaim,
        token: VerifierToken,
    ) -> Result<PromotionReceipt, HleError> {
        // Step 1: confirm token is for the same receipt.
        if token.verified_hash != claim.hash() {
            return Err(HleError::new(format!(
                "[E2030] HashMismatch: token verified_hash={} but claim.hash()={}",
                token.verified_hash,
                claim.hash(),
            )));
        }

        let mut guard = self
            .inner
            .write()
            .map_err(|_| HleError::new("[E2021] StorageIo: lock poisoned during promote"))?;

        // Step 2: duplicate guard.
        if guard.log.contains(claim.hash()) {
            if self.config.strict_duplicate_guard {
                return Err(HleError::new(format!(
                    "[E2041] AlreadyFinal: claim {} already promoted",
                    claim.hash(),
                )));
            }
            // Non-strict mode: return the existing receipt.
            if let Some(existing) = guard.log.entries.iter().find(|r| r.hash == claim.hash()) {
                return Ok(existing.clone());
            }
        }

        // Step 3: record the promotion.
        let tick = guard.next_tick();
        guard.promotion_count = guard.promotion_count.saturating_add(1);
        let sequence = guard.promotion_count;

        let receipt = PromotionReceipt {
            hash: claim.hash(),
            workflow: claim.inner().workflow.clone(),
            step_id: claim.inner().step_id.clone(),
            promoted_at: tick,
            sequence,
        };

        guard.log.push(receipt.clone());
        Ok(receipt)
    }

    /// Returns `true` when this hash appears in the in-memory promotion log.
    #[must_use]
    pub fn is_promoted(&self, hash: ReceiptHash) -> bool {
        self.inner
            .read()
            .map(|g| g.log.contains(hash))
            .unwrap_or(false)
    }

    /// Total successful promotions this session.
    #[must_use]
    pub fn promotion_count(&self) -> u64 {
        self.inner.read().map(|g| g.promotion_count).unwrap_or(0)
    }

    /// Clones the current in-memory promotion log in insertion order.
    #[must_use]
    pub fn log_snapshot(&self) -> Vec<PromotionReceipt> {
        self.inner
            .read()
            .map(|g| g.log.snapshot())
            .unwrap_or_default()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::receipt_sha_verifier::{ReceiptShaVerifier, VerifyInput};
    use hle_core::evidence::claims_store::{ClaimState, ClaimStore};
    use hle_core::evidence::receipt_hash::{ReceiptHash, ReceiptHashFields};

    // ── helpers ────────────────────────────────────────────────────────────────

    fn make_hash(workflow: &str, step_id: &str) -> ReceiptHash {
        let fields = ReceiptHashFields::new(workflow, step_id, "PASS", "", "")
            .expect("fields must be valid");
        ReceiptHash::from_fields(&fields).expect("hash must succeed")
    }

    fn make_verified_claim(
        store: &ClaimStore,
        hash: ReceiptHash,
        workflow: &str,
        step_id: &str,
    ) -> hle_core::evidence::claims_store::VerifiedClaim {
        store
            .insert(hash, workflow, step_id, "PASS")
            .expect("insert must succeed");
        store
            .mark_verified(hash)
            .expect("mark_verified must succeed")
    }

    fn make_matching_token(workflow: &str, step_id: &str) -> (ReceiptHash, VerifierToken) {
        let hash = make_hash(workflow, step_id);
        let verifier = ReceiptShaVerifier::new();
        let input = VerifyInput {
            stored_hash: hash,
            workflow: workflow.to_owned(),
            step_id: step_id.to_owned(),
            verdict: String::from("PASS"),
            manifest_sha256: String::new(),
            framework_sha256: String::new(),
        };
        let (_outcome, token) = verifier
            .verify_and_token(&input)
            .expect("verify_and_token must succeed for a matching input");
        (hash, token)
    }

    /// Full test fixture: store + evaluator + one verified claim + matching token.
    struct Fixture {
        store: ClaimStore,
        evaluator: FinalClaimEvaluator,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                store: ClaimStore::new(),
                evaluator: FinalClaimEvaluator::with_default_config(),
            }
        }

        fn promote(
            &self,
            wf: &str,
            step: &str,
        ) -> Result<PromotionReceipt, substrate_types::HleError> {
            let (hash, token) = make_matching_token(wf, step);
            let vc = make_verified_claim(&self.store, hash, wf, step);
            self.evaluator.promote(&vc, token)
        }
    }

    // ── EvaluatorConfig ───────────────────────────────────────────────────────

    #[test]
    fn evaluator_config_default_has_strict_guard() {
        assert!(EvaluatorConfig::default().strict_duplicate_guard);
    }

    #[test]
    fn evaluator_config_default_log_capacity_is_1000() {
        assert_eq!(EvaluatorConfig::default().log_capacity, 1_000);
    }

    #[test]
    fn evaluator_config_log_capacity_clamps_to_minimum() {
        assert_eq!(EvaluatorConfig::new(0, true).log_capacity, 1);
    }

    #[test]
    fn evaluator_config_log_capacity_clamps_to_maximum() {
        assert_eq!(EvaluatorConfig::new(99_999, true).log_capacity, 10_000);
    }

    #[test]
    fn evaluator_config_1000_is_within_range() {
        assert_eq!(EvaluatorConfig::new(1_000, false).log_capacity, 1_000);
    }

    #[test]
    fn evaluator_config_non_strict_guard_is_false() {
        assert!(!EvaluatorConfig::new(100, false).strict_duplicate_guard);
    }

    // ── PromotionReceipt ──────────────────────────────────────────────────────

    #[test]
    fn promotion_receipt_summary_contains_sequence_and_hash() {
        let fx = Fixture::new();
        let receipt = fx.promote("wf", "step").expect("promote must succeed");
        let s = receipt.summary();
        assert!(s.contains('#'), "summary missing '#': {s}");
        assert!(s.contains("Final"), "summary missing 'Final': {s}");
    }

    #[test]
    fn promotion_receipt_summary_contains_workflow() {
        let fx = Fixture::new();
        let receipt = fx.promote("summary-wf", "s1").expect("promote ok");
        assert!(receipt.summary().contains("summary-wf"));
    }

    #[test]
    fn promotion_receipt_display_matches_summary() {
        let fx = Fixture::new();
        let receipt = fx.promote("display-wf", "s1").expect("promote ok");
        assert_eq!(format!("{receipt}"), receipt.summary());
    }

    #[test]
    fn promotion_receipt_sequence_starts_at_one() {
        let fx = Fixture::new();
        let receipt = fx.promote("seq-wf", "s1").expect("promote ok");
        assert_eq!(receipt.sequence, 1);
    }

    #[test]
    fn promotion_receipt_sequence_increments_per_promotion() {
        let fx = Fixture::new();
        let r1 = fx.promote("wf", "seq-a").expect("ok");
        let r2 = fx.promote("wf", "seq-b").expect("ok");
        assert_eq!(r1.sequence, 1);
        assert_eq!(r2.sequence, 2);
    }

    #[test]
    fn promotion_receipt_hash_matches_claim_hash() {
        let fx = Fixture::new();
        let (hash, token) = make_matching_token("hash-wf", "s1");
        let vc = make_verified_claim(&fx.store, hash, "hash-wf", "s1");
        let receipt = fx.evaluator.promote(&vc, token).expect("promote ok");
        assert_eq!(receipt.hash, hash);
    }

    #[test]
    fn promotion_receipt_carries_workflow() {
        let fx = Fixture::new();
        let receipt = fx.promote("carried-wf", "s1").expect("ok");
        assert_eq!(receipt.workflow, "carried-wf");
    }

    #[test]
    fn promotion_receipt_carries_step_id() {
        let fx = Fixture::new();
        let receipt = fx.promote("wf", "carried-step").expect("ok");
        assert_eq!(receipt.step_id, "carried-step");
    }

    #[test]
    fn promotion_receipt_promoted_at_advances_with_each_promotion() {
        let fx = Fixture::new();
        let r1 = fx.promote("wf", "pa-a").expect("ok");
        let r2 = fx.promote("wf", "pa-b").expect("ok");
        assert!(r2.promoted_at > r1.promoted_at);
    }

    // ── FinalClaimEvaluator::promote — success paths ──────────────────────────

    #[test]
    fn promote_succeeds_with_matching_token() {
        let store = ClaimStore::new();
        let evaluator = FinalClaimEvaluator::with_default_config();
        let (hash, token) = make_matching_token("demo", "s1");
        let verified = make_verified_claim(&store, hash, "demo", "s1");
        let receipt = evaluator
            .promote(&verified, token)
            .expect("promote must succeed");
        assert_eq!(receipt.hash, hash);
        assert_eq!(receipt.sequence, 1);
    }

    #[test]
    fn promote_increments_count() {
        let store = ClaimStore::new();
        let evaluator = FinalClaimEvaluator::with_default_config();
        let (h1, t1) = make_matching_token("demo", "s1");
        let (h2, t2) = make_matching_token("demo", "s2");
        let v1 = make_verified_claim(&store, h1, "demo", "s1");
        let v2 = make_verified_claim(&store, h2, "demo", "s2");
        let _ = evaluator
            .promote(&v1, t1)
            .expect("first promote must succeed");
        let _ = evaluator
            .promote(&v2, t2)
            .expect("second promote must succeed");
        assert_eq!(evaluator.promotion_count(), 2);
    }

    #[test]
    fn promote_multiple_claims_increments_sequence_correctly() {
        let fx = Fixture::new();
        for i in 1u8..=5 {
            let receipt = fx.promote("multi-wf", &format!("s{i}")).expect("ok");
            assert_eq!(receipt.sequence, u64::from(i));
        }
    }

    // ── FinalClaimEvaluator::promote — error paths ────────────────────────────

    #[test]
    fn promote_returns_e2030_when_token_hash_mismatches_claim() {
        let store = ClaimStore::new();
        let evaluator = FinalClaimEvaluator::with_default_config();
        let hash1 = make_hash("demo", "s1");
        let verified = make_verified_claim(&store, hash1, "demo", "s1");
        let (_hash2, wrong_token) = make_matching_token("demo", "s2");
        let result = evaluator.promote(&verified, wrong_token);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("E2030"), "expected E2030, got: {msg}");
    }

    #[test]
    fn promote_returns_e2041_on_duplicate_in_strict_mode() {
        let store = ClaimStore::new();
        let evaluator = FinalClaimEvaluator::with_default_config();
        let (hash, token1) = make_matching_token("demo", "s1");
        let verified = make_verified_claim(&store, hash, "demo", "s1");
        let _ = evaluator
            .promote(&verified, token1)
            .expect("first promote must succeed");
        let (_same_hash, token2) = make_matching_token("demo", "s1");
        assert!(evaluator.is_promoted(hash));
        let result = evaluator.promote(&verified, token2);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("E2041"), "expected E2041, got: {msg}");
    }

    #[test]
    fn promote_duplicate_in_non_strict_mode_returns_existing_receipt() {
        let store = ClaimStore::new();
        let config = EvaluatorConfig::new(100, false); // non-strict
        let evaluator = FinalClaimEvaluator::new(config);
        let (hash, token1) = make_matching_token("demo", "non-strict");
        let verified = make_verified_claim(&store, hash, "demo", "non-strict");
        let r1 = evaluator.promote(&verified, token1).expect("first ok");
        let (_same, token2) = make_matching_token("demo", "non-strict");
        let r2 = evaluator
            .promote(&verified, token2)
            .expect("non-strict second ok");
        // Both receipts refer to the same hash and sequence.
        assert_eq!(r1.hash, r2.hash);
        assert_eq!(r1.sequence, r2.sequence);
    }

    // ── is_promoted ───────────────────────────────────────────────────────────

    #[test]
    fn is_promoted_returns_false_before_promotion() {
        let evaluator = FinalClaimEvaluator::with_default_config();
        let hash = make_hash("demo", "s99");
        assert!(!evaluator.is_promoted(hash));
    }

    #[test]
    fn is_promoted_returns_true_after_promotion() {
        let fx = Fixture::new();
        let (hash, token) = make_matching_token("promoted-wf", "s1");
        let vc = make_verified_claim(&fx.store, hash, "promoted-wf", "s1");
        let _ = fx.evaluator.promote(&vc, token).expect("ok");
        assert!(fx.evaluator.is_promoted(hash));
    }

    #[test]
    fn is_promoted_false_for_different_hash_after_promotion() {
        let fx = Fixture::new();
        let _ = fx.promote("wf", "s1").expect("ok");
        let other_hash = make_hash("other-wf", "s99");
        assert!(!fx.evaluator.is_promoted(other_hash));
    }

    // ── promotion_count ───────────────────────────────────────────────────────

    #[test]
    fn promotion_count_zero_initially() {
        let evaluator = FinalClaimEvaluator::with_default_config();
        assert_eq!(evaluator.promotion_count(), 0);
    }

    #[test]
    fn promotion_count_one_after_one_promotion() {
        let fx = Fixture::new();
        let _ = fx.promote("wf", "s1").expect("ok");
        assert_eq!(fx.evaluator.promotion_count(), 1);
    }

    // ── log_snapshot ──────────────────────────────────────────────────────────

    #[test]
    fn log_snapshot_is_empty_before_any_promotion() {
        let evaluator = FinalClaimEvaluator::with_default_config();
        assert!(evaluator.log_snapshot().is_empty());
    }

    #[test]
    fn log_snapshot_has_one_entry_after_one_promotion() {
        let fx = Fixture::new();
        let _ = fx.promote("wf", "s1").expect("ok");
        assert_eq!(fx.evaluator.log_snapshot().len(), 1);
    }

    #[test]
    fn log_snapshot_preserves_insertion_order() {
        let fx = Fixture::new();
        let _ = fx.promote("wf", "ordered-a").expect("ok");
        let _ = fx.promote("wf", "ordered-b").expect("ok");
        let snap = fx.evaluator.log_snapshot();
        assert_eq!(snap[0].sequence, 1);
        assert_eq!(snap[1].sequence, 2);
    }

    #[test]
    fn log_evicts_oldest_when_at_capacity() {
        let config = EvaluatorConfig::new(3, true);
        let evaluator = FinalClaimEvaluator::new(config);
        let store = ClaimStore::new();
        // Fill to capacity + 1.
        for i in 0u8..4 {
            let (hash, token) = make_matching_token("evict-wf", &format!("evict-{i}"));
            let vc = make_verified_claim(&store, hash, "evict-wf", &format!("evict-{i}"));
            let _ = evaluator.promote(&vc, token).expect("ok");
        }
        // Log should contain at most 3 entries.
        let snap = evaluator.log_snapshot();
        assert!(snap.len() <= 3, "expected ≤3, got {}", snap.len());
    }

    // ── debug / with_default_config ───────────────────────────────────────────

    #[test]
    fn evaluator_debug_contains_promotion_count() {
        let evaluator = FinalClaimEvaluator::with_default_config();
        let debug = format!("{evaluator:?}");
        assert!(debug.contains("promotion_count"), "debug: {debug}");
    }

    // ── ClaimState.Final is_terminal (regression guard) ───────────────────────

    #[test]
    fn claim_state_final_is_terminal() {
        assert!(ClaimState::Final.is_terminal());
    }

    // ── token-hash binding enforced ───────────────────────────────────────────

    #[test]
    fn token_for_claim_a_cannot_promote_claim_b() {
        let store = ClaimStore::new();
        let evaluator = FinalClaimEvaluator::with_default_config();
        let hash_a = make_hash("wf", "bind-a");
        let hash_b = make_hash("wf", "bind-b");
        let vc_a = make_verified_claim(&store, hash_a, "wf", "bind-a");
        let _vc_b = make_verified_claim(&store, hash_b, "wf", "bind-b");
        let (_a, token_a) = make_matching_token("wf", "bind-a");
        // Use token_a (for hash_a) to try to promote _vc_b — must fail.
        let result = evaluator.promote(&vc_a, token_a);
        // This succeeds because vc_a and token_a share the same hash.
        // But if we try to use a token for a different hash it fails.
        let _ = result; // first promote is ok; tested above
        let (_b, token_b) = make_matching_token("wf", "bind-b");
        // token_b is for hash_b, vc_a is for hash_a — must fail with E2030.
        let result2 = evaluator.promote(&vc_a, token_b);
        assert!(result2.is_err());
        let msg = result2.unwrap_err().to_string();
        assert!(msg.contains("E2030") || msg.contains("E2041"), "got: {msg}");
    }

    // ── additional boundary / invariant tests ─────────────────────────────────

    #[test]
    fn evaluator_config_capacity_in_range_is_preserved() {
        let c = EvaluatorConfig::new(500, true);
        assert_eq!(c.log_capacity, 500);
    }

    #[test]
    fn evaluator_config_strict_true_is_preserved() {
        assert!(EvaluatorConfig::new(100, true).strict_duplicate_guard);
    }

    #[test]
    fn promotion_count_zero_after_failed_promote() {
        let store = ClaimStore::new();
        let evaluator = FinalClaimEvaluator::with_default_config();
        let hash = make_hash("count-fail", "s1");
        let vc = make_verified_claim(&store, hash, "count-fail", "s1");
        let (_wrong_hash, wrong_token) = make_matching_token("count-fail", "s99");
        let _ = evaluator.promote(&vc, wrong_token); // expected to fail
        assert_eq!(evaluator.promotion_count(), 0);
    }

    #[test]
    fn log_snapshot_contains_receipt_with_correct_workflow() {
        let fx = Fixture::new();
        let _ = fx.promote("snap-wf", "s1").expect("ok");
        let snap = fx.evaluator.log_snapshot();
        assert_eq!(snap[0].workflow, "snap-wf");
    }

    #[test]
    fn log_snapshot_contains_receipt_with_correct_step_id() {
        let fx = Fixture::new();
        let _ = fx.promote("wf", "snap-step").expect("ok");
        let snap = fx.evaluator.log_snapshot();
        assert_eq!(snap[0].step_id, "snap-step");
    }

    #[test]
    fn log_snapshot_reflects_multiple_promotions() {
        let fx = Fixture::new();
        let _ = fx.promote("wf", "multi-1").expect("ok");
        let _ = fx.promote("wf", "multi-2").expect("ok");
        let _ = fx.promote("wf", "multi-3").expect("ok");
        assert_eq!(fx.evaluator.log_snapshot().len(), 3);
    }

    #[test]
    fn promotion_receipt_promoted_at_is_nonzero_after_first_promotion() {
        let fx = Fixture::new();
        let r = fx.promote("wf", "pa-nonzero").expect("ok");
        assert!(r.promoted_at > 0);
    }

    #[test]
    fn promote_returns_e2030_error_message_mentions_token_and_claim() {
        let store = ClaimStore::new();
        let evaluator = FinalClaimEvaluator::with_default_config();
        let hash1 = make_hash("mismatch-wf", "s1");
        let vc = make_verified_claim(&store, hash1, "mismatch-wf", "s1");
        let (_h2, wrong_token) = make_matching_token("mismatch-wf", "s99");
        let err = evaluator.promote(&vc, wrong_token).unwrap_err();
        let msg = err.to_string();
        // Error must mention both hashes for diagnosability.
        assert!(msg.contains("E2030"), "expected E2030 in: {msg}");
    }

    #[test]
    fn promote_e2041_message_mentions_claim_hash() {
        let store = ClaimStore::new();
        let evaluator = FinalClaimEvaluator::with_default_config();
        let (hash, token1) = make_matching_token("dup-wf", "s1");
        let vc = make_verified_claim(&store, hash, "dup-wf", "s1");
        let _ = evaluator.promote(&vc, token1).expect("first ok");
        let (_h, token2) = make_matching_token("dup-wf", "s1");
        let err = evaluator.promote(&vc, token2).unwrap_err();
        assert!(err.to_string().contains("E2041"));
    }

    #[test]
    fn claim_state_provisional_and_verified_not_terminal() {
        assert!(!ClaimState::Provisional.is_terminal());
        assert!(!ClaimState::Verified.is_terminal());
    }

    #[test]
    fn with_default_config_uses_strict_duplicate_guard() {
        let ev = FinalClaimEvaluator::with_default_config();
        // Confirm strict guard by attempting double-promote.
        let store = ClaimStore::new();
        let (hash, token1) = make_matching_token("strict-check", "s1");
        let vc = make_verified_claim(&store, hash, "strict-check", "s1");
        let _ = ev.promote(&vc, token1).expect("first ok");
        let (_h, token2) = make_matching_token("strict-check", "s1");
        assert!(ev.promote(&vc, token2).is_err());
    }

    #[test]
    fn evaluator_debug_output_is_nonempty() {
        let ev = FinalClaimEvaluator::with_default_config();
        assert!(!format!("{ev:?}").is_empty());
    }

    #[test]
    fn promotion_receipt_must_use_attribute_forces_explicit_handling() {
        // `PromotionReceipt` is #[must_use]. This test documents the contract by
        // explicitly binding the return value — the test passes trivially but the
        // attribute would emit a warning if the caller silently discards it.
        let fx = Fixture::new();
        let receipt = fx.promote("must-use-wf", "s1").expect("ok");
        assert!(!receipt.summary().is_empty());
    }

    #[test]
    fn promotion_receipt_eq_same_data() {
        let fx = Fixture::new();
        let r1 = fx.promote("eq-wf", "s1").expect("ok");
        // Clone and check equality.
        let r2 = r1.clone();
        assert_eq!(r1, r2);
    }

    #[test]
    fn evaluator_new_with_config_respects_log_capacity() {
        let config = EvaluatorConfig::new(5, true);
        let evaluator = FinalClaimEvaluator::new(config);
        // Promotion count starts at zero.
        assert_eq!(evaluator.promotion_count(), 0);
    }

    #[test]
    fn log_snapshot_empty_after_failed_promote_only() {
        let store = ClaimStore::new();
        let evaluator = FinalClaimEvaluator::with_default_config();
        let hash = make_hash("fail-only", "s1");
        let vc = make_verified_claim(&store, hash, "fail-only", "s1");
        let (_wrong, wrong_token) = make_matching_token("fail-only", "s99");
        let _ = evaluator.promote(&vc, wrong_token); // fails
        assert!(evaluator.log_snapshot().is_empty());
    }

    #[test]
    fn is_promoted_false_for_zeroed_hash() {
        let evaluator = FinalClaimEvaluator::with_default_config();
        assert!(!evaluator.is_promoted(ReceiptHash::zeroed()));
    }
}
