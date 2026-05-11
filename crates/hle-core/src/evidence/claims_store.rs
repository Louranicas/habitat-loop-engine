#![forbid(unsafe_code)]

// End-to-end stack cross-reference: terminal implementation node for
// M006_CLAIMS_STORE.md / L01_FOUNDATION.md / C01_EVIDENCE_INTEGRITY (cluster).
// Spec: ai_specs/modules/c01-evidence-integrity/M006_CLAIMS_STORE.md.
//
// STUB: compile-safe skeleton. Uses std::sync::RwLock instead of
// parking_lot::RwLock (no external dep). Replace with parking_lot in the
// production implementation pass once that dependency is added.

use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use std::sync::RwLock;

use substrate_types::HleError;

use crate::evidence::receipt_hash::ReceiptHash;

/// One-way finite state machine for claim lifecycle.
///
/// Transitions are strictly `Provisional → Verified → Final`. Any other
/// direction is blocked by `can_transition_to` and returns `[E2011] InvalidTransition`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ClaimState {
    /// Executor-produced; not yet independently verified.
    Provisional,
    /// Verifier confirmed hash matches artifact; ready for promotion.
    Verified,
    /// `FinalClaimEvaluator` has promoted this claim; immutable.
    Final,
}

impl ClaimState {
    /// Wire-format string for this state.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Provisional => "provisional",
            Self::Verified => "verified",
            Self::Final => "final",
        }
    }

    /// Returns `true` only for `Final`; all other states are non-terminal.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Final)
    }

    /// Returns `true` if and only if the transition `self → next` is valid.
    ///
    /// Valid transitions: `Provisional→Verified`, `Verified→Final`.
    /// All others (same-state, backward, skipping) return `false`.
    #[must_use]
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Provisional, Self::Verified) | (Self::Verified, Self::Final)
        )
    }
}

impl fmt::Display for ClaimState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ClaimState {
    type Err = HleError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "provisional" => Ok(Self::Provisional),
            "verified" => Ok(Self::Verified),
            "final" => Ok(Self::Final),
            other => Err(HleError::new(format!("unknown claim state: {other}"))),
        }
    }
}

/// A single claim record, keyed by `ReceiptHash`.
///
/// Claims are produced by executor-side code at `Provisional` state and
/// advanced by the verifier to `Verified`, then by `FinalClaimEvaluator`
/// to `Final`. Direct construction is allowed; state advancement goes through
/// `ClaimStore::mark_verified` and `FinalClaimEvaluator::promote`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    /// Hash of the receipt fields that produced this claim.
    pub hash: ReceiptHash,
    /// Current state in the one-way FSM.
    pub state: ClaimState,
    /// Workflow identifier this claim belongs to.
    pub workflow: String,
    /// Step identifier within the workflow.
    pub step_id: String,
    /// Executor-supplied verdict string before independent verification.
    pub draft_verdict: String,
    /// Monotonic creation counter (not wall clock; no chrono/SystemTime).
    pub created_at: u64,
    /// Monotonic last-transition counter.
    pub updated_at: u64,
}

impl Claim {
    /// Create a new `Claim` in `Provisional` state.
    ///
    /// Both `created_at` and `updated_at` are set to `tick`.
    #[must_use]
    pub fn new(
        hash: ReceiptHash,
        workflow: impl Into<String>,
        step_id: impl Into<String>,
        draft_verdict: impl Into<String>,
        tick: u64,
    ) -> Self {
        Self {
            hash,
            state: ClaimState::Provisional,
            workflow: workflow.into(),
            step_id: step_id.into(),
            draft_verdict: draft_verdict.into(),
            created_at: tick,
            updated_at: tick,
        }
    }

    /// Produce a new `Claim` with `state` advanced to `next`, or an error.
    ///
    /// # Errors
    ///
    /// Returns `Err` (`[E2011] InvalidTransition`) when
    /// `self.state.can_transition_to(next)` is `false`.
    pub fn transition(&self, next: ClaimState, tick: u64) -> Result<Self, HleError> {
        if !self.state.can_transition_to(next) {
            return Err(HleError::new(format!(
                "[E2011] InvalidTransition: cannot transition {} → {next}",
                self.state,
            )));
        }
        Ok(Self {
            hash: self.hash,
            state: next,
            workflow: self.workflow.clone(),
            step_id: self.step_id.clone(),
            draft_verdict: self.draft_verdict.clone(),
            created_at: self.created_at,
            updated_at: tick,
        })
    }

    /// Returns `true` when this claim is in `Final` state.
    #[must_use]
    pub const fn is_final(&self) -> bool {
        self.state.is_terminal()
    }

    /// Human-readable one-liner for log output.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "Claim({}:{}@{}/{})",
            self.hash, self.state, self.workflow, self.step_id,
        )
    }
}

impl fmt::Display for Claim {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.summary())
    }
}

/// Typestate proof that a `Claim` passed `Provisional → Verified`.
///
/// `VerifiedClaim` can be constructed only via `ClaimStore::mark_verified`.
/// The private `fn new` prevents external construction. M009
/// `FinalClaimEvaluator::promote` accepts `&VerifiedClaim` as proof that the
/// claim has been independently verified before promotion to `Final`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedClaim(Claim);

impl VerifiedClaim {
    /// Private constructor — only `ClaimStore::mark_verified` may call this.
    fn new(claim: Claim) -> Result<Self, HleError> {
        if claim.state != ClaimState::Verified {
            return Err(HleError::new(format!(
                "[E2011] InvalidTransition: VerifiedClaim requires Verified state, got {}",
                claim.state,
            )));
        }
        Ok(Self(claim))
    }

    /// Returns a reference to the inner `Claim`.
    #[must_use]
    pub fn inner(&self) -> &Claim {
        &self.0
    }

    /// Returns the `ReceiptHash` of the verified claim.
    #[must_use]
    pub fn hash(&self) -> ReceiptHash {
        self.0.hash
    }
}

// ── ClaimStore internals ────────────────────────────────────────────────────

struct ClaimStoreInner {
    claims: HashMap<ReceiptHash, Claim>,
    monotonic_tick: u64,
}

impl ClaimStoreInner {
    fn new() -> Self {
        Self {
            claims: HashMap::new(),
            monotonic_tick: 0,
        }
    }

    fn next_tick(&mut self) -> u64 {
        self.monotonic_tick = self.monotonic_tick.saturating_add(1);
        self.monotonic_tick
    }
}

/// In-memory claim graph store with `RwLock` interior mutability.
///
/// `ClaimStore` is executor-side for `insert` and verifier-side for
/// `mark_verified`. Wrap in `Arc<ClaimStore>` for cross-thread sharing.
pub struct ClaimStore {
    inner: RwLock<ClaimStoreInner>,
}

impl fmt::Debug for ClaimStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.count();
        write!(f, "ClaimStore(count={count})")
    }
}

impl ClaimStore {
    /// Create an empty claim store with tick counter at zero.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(ClaimStoreInner::new()),
        }
    }

    /// Insert a new `Provisional` claim keyed by `hash`.
    ///
    /// # Errors
    ///
    /// Returns `Err` (`[E2012] DuplicateClaim`) when a claim with this hash
    /// already exists. Returns `Err` on lock poisoning.
    pub fn insert(
        &self,
        hash: ReceiptHash,
        workflow: impl Into<String>,
        step_id: impl Into<String>,
        draft_verdict: impl Into<String>,
    ) -> Result<(), HleError> {
        let mut guard = self
            .inner
            .write()
            .map_err(|_| HleError::new("[E2012] DuplicateClaim: lock poisoned during insert"))?;
        if guard.claims.contains_key(&hash) {
            return Err(HleError::new(format!(
                "[E2012] DuplicateClaim: hash {hash} already present"
            )));
        }
        let tick = guard.next_tick();
        let claim = Claim::new(hash, workflow, step_id, draft_verdict, tick);
        guard.claims.insert(hash, claim);
        Ok(())
    }

    /// Retrieve a clone of the claim for the given hash.
    ///
    /// # Errors
    ///
    /// Returns `Err` (`[E2010] ClaimNotFound`) when the hash is absent.
    /// Returns `Err` on lock poisoning.
    pub fn get(&self, hash: ReceiptHash) -> Result<Claim, HleError> {
        let guard = self
            .inner
            .read()
            .map_err(|_| HleError::new("[E2010] ClaimNotFound: lock poisoned during get"))?;
        guard
            .claims
            .get(&hash)
            .cloned()
            .ok_or_else(|| HleError::new(format!("[E2010] ClaimNotFound: {hash}")))
    }

    /// Advance the claim at `hash` from `Provisional` to `Verified`.
    ///
    /// Returns a `VerifiedClaim` typestate proof on success.
    ///
    /// # Errors
    ///
    /// Returns `Err` (`[E2010] ClaimNotFound`) if the hash is absent.
    /// Returns `Err` (`[E2011] InvalidTransition`) if the claim is not
    /// in `Provisional` state. Returns `Err` on lock poisoning.
    pub fn mark_verified(&self, hash: ReceiptHash) -> Result<VerifiedClaim, HleError> {
        let mut guard = self.inner.write().map_err(|_| {
            HleError::new("[E2011] InvalidTransition: lock poisoned during mark_verified")
        })?;
        let tick = guard.next_tick();
        let existing = guard
            .claims
            .get(&hash)
            .ok_or_else(|| HleError::new(format!("[E2010] ClaimNotFound: {hash}")))?;
        let updated = existing.transition(ClaimState::Verified, tick)?;
        guard.claims.insert(hash, updated.clone());
        // Drop guard before calling VerifiedClaim::new to avoid re-entrancy.
        drop(guard);
        VerifiedClaim::new(updated)
    }

    /// Return a read-only snapshot of the entire store without holding a lock.
    #[must_use]
    pub fn snapshot(&self) -> ClaimStoreSnapshot {
        let guard = self
            .inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        ClaimStoreSnapshot {
            claims: guard.claims.values().cloned().collect(),
            tick: guard.monotonic_tick,
        }
    }

    /// Total number of claims in the store.
    #[must_use]
    pub fn count(&self) -> usize {
        self.inner.read().map_or(0, |g| g.claims.len())
    }

    /// Number of claims currently in the given state.
    #[must_use]
    pub fn count_by_state(&self, state: ClaimState) -> usize {
        self.inner.read().map_or(0, |g| {
            g.claims.values().filter(|c| c.state == state).count()
        })
    }
}

impl Default for ClaimStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Read-only snapshot of the claim store, cloned from the inner map.
///
/// Provides inspection helpers without requiring callers to hold a lock guard.
#[derive(Debug, Clone)]
pub struct ClaimStoreSnapshot {
    /// All claims at the moment of the snapshot.
    pub claims: Vec<Claim>,
    /// Monotonic tick at the moment of the snapshot.
    pub tick: u64,
}

impl ClaimStoreSnapshot {
    /// Return all claims currently in `state`.
    #[must_use]
    pub fn by_state(&self, state: ClaimState) -> Vec<&Claim> {
        self.claims.iter().filter(|c| c.state == state).collect()
    }

    /// Return all claims belonging to `workflow`.
    #[must_use]
    pub fn by_workflow<'a>(&'a self, workflow: &str) -> Vec<&'a Claim> {
        self.claims
            .iter()
            .filter(|c| c.workflow == workflow)
            .collect()
    }

    /// Find a specific claim by hash; returns `None` if absent.
    #[must_use]
    pub fn find(&self, hash: ReceiptHash) -> Option<&Claim> {
        self.claims.iter().find(|c| c.hash == hash)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::evidence::receipt_hash::ReceiptHash;

    // ── helpers ────────────────────────────────────────────────────────────────

    fn test_hash(discriminator: u8) -> ReceiptHash {
        let mut bytes = [0u8; 32];
        bytes[0] = discriminator;
        bytes[1] = 0xAB; // make it non-zeroed
        ReceiptHash::from_bytes(bytes)
    }

    fn insert_one(store: &ClaimStore, discriminator: u8) -> ReceiptHash {
        let hash = test_hash(discriminator);
        store
            .insert(hash, "demo", "s1", "PASS")
            .expect("insert must succeed");
        hash
    }

    fn insert_and_verify(store: &ClaimStore, discriminator: u8) -> VerifiedClaim {
        let hash = insert_one(store, discriminator);
        store
            .mark_verified(hash)
            .expect("mark_verified must succeed")
    }

    // ── ClaimState transitions — allowed ────────────────────────────────────────

    #[test]
    fn claim_state_transitions_provisional_to_verified() {
        assert!(ClaimState::Provisional.can_transition_to(ClaimState::Verified));
    }

    #[test]
    fn claim_state_transitions_verified_to_final() {
        assert!(ClaimState::Verified.can_transition_to(ClaimState::Final));
    }

    // ── ClaimState transitions — forbidden ─────────────────────────────────────

    #[test]
    fn claim_state_blocks_provisional_to_final() {
        assert!(!ClaimState::Provisional.can_transition_to(ClaimState::Final));
    }

    #[test]
    fn claim_state_blocks_provisional_to_provisional() {
        assert!(!ClaimState::Provisional.can_transition_to(ClaimState::Provisional));
    }

    #[test]
    fn claim_state_blocks_verified_to_verified() {
        assert!(!ClaimState::Verified.can_transition_to(ClaimState::Verified));
    }

    #[test]
    fn claim_state_blocks_verified_to_provisional() {
        assert!(!ClaimState::Verified.can_transition_to(ClaimState::Provisional));
    }

    #[test]
    fn claim_state_blocks_final_to_provisional() {
        assert!(!ClaimState::Final.can_transition_to(ClaimState::Provisional));
    }

    #[test]
    fn claim_state_blocks_final_to_verified() {
        assert!(!ClaimState::Final.can_transition_to(ClaimState::Verified));
    }

    #[test]
    fn claim_state_blocks_final_to_final() {
        assert!(!ClaimState::Final.can_transition_to(ClaimState::Final));
    }

    // ── ClaimState terminal predicate ──────────────────────────────────────────

    #[test]
    fn claim_state_final_is_terminal() {
        assert!(ClaimState::Final.is_terminal());
    }

    #[test]
    fn claim_state_provisional_is_not_terminal() {
        assert!(!ClaimState::Provisional.is_terminal());
    }

    #[test]
    fn claim_state_verified_is_not_terminal() {
        assert!(!ClaimState::Verified.is_terminal());
    }

    // ── ClaimState Display / FromStr ───────────────────────────────────────────

    #[test]
    fn claim_state_parses_from_str() {
        assert_eq!(
            "provisional".parse::<ClaimState>(),
            Ok(ClaimState::Provisional)
        );
        assert_eq!("verified".parse::<ClaimState>(), Ok(ClaimState::Verified));
        assert_eq!("final".parse::<ClaimState>(), Ok(ClaimState::Final));
    }

    #[test]
    fn claim_state_rejects_unknown_str() {
        assert!("unknown".parse::<ClaimState>().is_err());
    }

    #[test]
    fn claim_state_rejects_uppercase_provisional() {
        assert!("Provisional".parse::<ClaimState>().is_err());
    }

    #[test]
    fn claim_state_as_str_provisional_is_stable() {
        assert_eq!(ClaimState::Provisional.as_str(), "provisional");
    }

    #[test]
    fn claim_state_as_str_verified_is_stable() {
        assert_eq!(ClaimState::Verified.as_str(), "verified");
    }

    #[test]
    fn claim_state_as_str_final_is_stable() {
        assert_eq!(ClaimState::Final.as_str(), "final");
    }

    #[test]
    fn claim_state_display_matches_as_str() {
        for state in [
            ClaimState::Provisional,
            ClaimState::Verified,
            ClaimState::Final,
        ] {
            assert_eq!(state.to_string(), state.as_str());
        }
    }

    #[test]
    fn claim_state_parse_roundtrip_for_all_variants() {
        for state in [
            ClaimState::Provisional,
            ClaimState::Verified,
            ClaimState::Final,
        ] {
            let parsed: ClaimState = state.as_str().parse().expect("must parse");
            assert_eq!(parsed, state);
        }
    }

    // ── Claim construction ─────────────────────────────────────────────────────

    #[test]
    fn claim_new_starts_as_provisional() {
        let hash = test_hash(1);
        let claim = Claim::new(hash, "demo", "s1", "PASS", 0);
        assert_eq!(claim.state, ClaimState::Provisional);
    }

    #[test]
    fn claim_new_preserves_hash() {
        let hash = test_hash(2);
        let claim = Claim::new(hash, "demo", "s1", "PASS", 0);
        assert_eq!(claim.hash, hash);
    }

    #[test]
    fn claim_new_preserves_workflow() {
        let hash = test_hash(3);
        let claim = Claim::new(hash, "my-workflow", "s1", "PASS", 100);
        assert_eq!(claim.workflow, "my-workflow");
    }

    #[test]
    fn claim_new_preserves_step_id() {
        let hash = test_hash(4);
        let claim = Claim::new(hash, "wf", "step-99", "PASS", 0);
        assert_eq!(claim.step_id, "step-99");
    }

    #[test]
    fn claim_new_preserves_draft_verdict() {
        let hash = test_hash(5);
        let claim = Claim::new(hash, "wf", "s1", "AWAITING_HUMAN", 0);
        assert_eq!(claim.draft_verdict, "AWAITING_HUMAN");
    }

    #[test]
    fn claim_new_sets_created_at_and_updated_at_to_tick() {
        let hash = test_hash(6);
        let claim = Claim::new(hash, "wf", "s1", "PASS", 77);
        assert_eq!(claim.created_at, 77);
        assert_eq!(claim.updated_at, 77);
    }

    #[test]
    fn claim_is_not_final_when_provisional() {
        let hash = test_hash(7);
        let claim = Claim::new(hash, "wf", "s1", "PASS", 0);
        assert!(!claim.is_final());
    }

    #[test]
    fn claim_summary_contains_workflow_and_step() {
        let hash = test_hash(8);
        let claim = Claim::new(hash, "wf-x", "step-y", "PASS", 0);
        let s = claim.summary();
        assert!(s.contains("wf-x"), "summary: {s}");
        assert!(s.contains("step-y"), "summary: {s}");
    }

    #[test]
    fn claim_display_is_nonempty() {
        let hash = test_hash(9);
        let claim = Claim::new(hash, "wf", "s1", "PASS", 0);
        assert!(!format!("{claim}").is_empty());
    }

    // ── Claim::transition ─────────────────────────────────────────────────────

    #[test]
    fn claim_transition_provisional_to_verified_succeeds() {
        let hash = test_hash(10);
        let claim = Claim::new(hash, "demo", "s1", "PASS", 0);
        let verified = claim
            .transition(ClaimState::Verified, 1)
            .expect("must succeed");
        assert_eq!(verified.state, ClaimState::Verified);
    }

    #[test]
    fn claim_transition_advances_state() {
        let hash = test_hash(11);
        let claim = Claim::new(hash, "demo", "s1", "PASS", 0);
        let verified = claim
            .transition(ClaimState::Verified, 1)
            .expect("must succeed");
        assert_eq!(verified.state, ClaimState::Verified);
    }

    #[test]
    fn claim_transition_preserves_hash() {
        let hash = test_hash(12);
        let claim = Claim::new(hash, "demo", "s1", "PASS", 0);
        let verified = claim
            .transition(ClaimState::Verified, 1)
            .expect("must succeed");
        assert_eq!(verified.hash, hash);
    }

    #[test]
    fn claim_transition_preserves_created_at() {
        let hash = test_hash(13);
        let claim = Claim::new(hash, "demo", "s1", "PASS", 42);
        let verified = claim
            .transition(ClaimState::Verified, 99)
            .expect("must succeed");
        assert_eq!(verified.created_at, 42);
    }

    #[test]
    fn claim_transition_updates_updated_at() {
        let hash = test_hash(14);
        let claim = Claim::new(hash, "demo", "s1", "PASS", 0);
        let verified = claim
            .transition(ClaimState::Verified, 55)
            .expect("must succeed");
        assert_eq!(verified.updated_at, 55);
    }

    #[test]
    fn claim_transition_rejects_backward() {
        let hash = test_hash(15);
        let claim = Claim::new(hash, "demo", "s1", "PASS", 0);
        let verified = claim
            .transition(ClaimState::Verified, 1)
            .expect("must succeed");
        assert!(verified.transition(ClaimState::Provisional, 2).is_err());
    }

    #[test]
    fn claim_transition_rejects_provisional_to_final() {
        let hash = test_hash(16);
        let claim = Claim::new(hash, "demo", "s1", "PASS", 0);
        assert!(claim.transition(ClaimState::Final, 1).is_err());
    }

    #[test]
    fn claim_transition_error_contains_e2011() {
        let hash = test_hash(17);
        let claim = Claim::new(hash, "demo", "s1", "PASS", 0);
        let err = claim.transition(ClaimState::Final, 1).unwrap_err();
        assert!(err.to_string().contains("E2011"), "got: {err}");
    }

    #[test]
    fn claim_full_lifecycle_provisional_verified_final() {
        let hash = test_hash(18);
        let provisional = Claim::new(hash, "demo", "s1", "PASS", 0);
        let verified = provisional
            .transition(ClaimState::Verified, 1)
            .expect("must succeed");
        let finalized = verified
            .transition(ClaimState::Final, 2)
            .expect("must succeed");
        assert_eq!(finalized.state, ClaimState::Final);
        assert!(finalized.is_final());
    }

    // ── VerifiedClaim typestate ────────────────────────────────────────────────

    #[test]
    fn verified_claim_inner_is_in_verified_state() {
        let store = ClaimStore::new();
        let vc = insert_and_verify(&store, 20);
        assert_eq!(vc.inner().state, ClaimState::Verified);
    }

    #[test]
    fn verified_claim_hash_matches_inner_hash() {
        let store = ClaimStore::new();
        let vc = insert_and_verify(&store, 21);
        assert_eq!(vc.hash(), vc.inner().hash);
    }

    // ── ClaimStore::insert / get ───────────────────────────────────────────────

    #[test]
    fn store_insert_and_get_roundtrip() {
        let store = ClaimStore::new();
        let hash = insert_one(&store, 30);
        let claim = store.get(hash).expect("get must succeed");
        assert_eq!(claim.hash, hash);
    }

    #[test]
    fn store_get_returns_provisional_after_insert() {
        let store = ClaimStore::new();
        let hash = insert_one(&store, 31);
        let claim = store.get(hash).expect("get must succeed");
        assert_eq!(claim.state, ClaimState::Provisional);
    }

    #[test]
    fn store_get_missing_hash_returns_e2010() {
        let store = ClaimStore::new();
        let err = store.get(test_hash(200)).unwrap_err();
        assert!(err.to_string().contains("E2010"), "got: {err}");
    }

    #[test]
    fn store_rejects_duplicate_insert() {
        let store = ClaimStore::new();
        let hash = insert_one(&store, 32);
        assert!(store.insert(hash, "demo", "s1", "PASS").is_err());
    }

    #[test]
    fn store_duplicate_insert_returns_e2012() {
        let store = ClaimStore::new();
        let hash = insert_one(&store, 33);
        let err = store.insert(hash, "demo", "s1", "PASS").unwrap_err();
        assert!(err.to_string().contains("E2012"), "got: {err}");
    }

    // ── ClaimStore::mark_verified ──────────────────────────────────────────────

    #[test]
    fn store_mark_verified_returns_verified_claim() {
        let store = ClaimStore::new();
        let hash = insert_one(&store, 40);
        let verified = store
            .mark_verified(hash)
            .expect("mark_verified must succeed");
        assert_eq!(verified.inner().state, ClaimState::Verified);
    }

    #[test]
    fn store_mark_verified_persists_state_change() {
        let store = ClaimStore::new();
        let hash = insert_one(&store, 41);
        let _ = store.mark_verified(hash).expect("must succeed");
        let claim = store.get(hash).expect("get must succeed");
        assert_eq!(claim.state, ClaimState::Verified);
    }

    #[test]
    fn store_mark_verified_missing_hash_returns_e2010() {
        let store = ClaimStore::new();
        let err = store.mark_verified(test_hash(201)).unwrap_err();
        assert!(err.to_string().contains("E2010"), "got: {err}");
    }

    #[test]
    fn store_double_mark_verified_returns_e2011() {
        let store = ClaimStore::new();
        let hash = insert_one(&store, 42);
        let _ = store
            .mark_verified(hash)
            .expect("first mark_verified must succeed");
        let err = store.mark_verified(hash).unwrap_err();
        assert!(err.to_string().contains("E2011"), "got: {err}");
    }

    // ── ClaimStore::count / count_by_state ────────────────────────────────────

    #[test]
    fn store_count_starts_at_zero() {
        assert_eq!(ClaimStore::new().count(), 0);
    }

    #[test]
    fn store_count_reflects_insertions() {
        let store = ClaimStore::new();
        assert_eq!(store.count(), 0);
        let _ = insert_one(&store, 50);
        assert_eq!(store.count(), 1);
        let _ = insert_one(&store, 51);
        assert_eq!(store.count(), 2);
    }

    #[test]
    fn store_count_by_state_provisional_increments_on_insert() {
        let store = ClaimStore::new();
        let _ = insert_one(&store, 52);
        assert_eq!(store.count_by_state(ClaimState::Provisional), 1);
        assert_eq!(store.count_by_state(ClaimState::Verified), 0);
    }

    #[test]
    fn store_count_by_state_updates_after_mark_verified() {
        let store = ClaimStore::new();
        let _ = insert_and_verify(&store, 53);
        assert_eq!(store.count_by_state(ClaimState::Provisional), 0);
        assert_eq!(store.count_by_state(ClaimState::Verified), 1);
    }

    // ── ClaimStore default ─────────────────────────────────────────────────────

    #[test]
    fn claim_store_default_is_empty() {
        let store = ClaimStore::default();
        assert_eq!(store.count(), 0);
    }

    // ── ClaimStoreSnapshot ────────────────────────────────────────────────────

    #[test]
    fn snapshot_find_returns_none_for_missing_hash() {
        let store = ClaimStore::new();
        let snap = store.snapshot();
        assert!(snap.find(test_hash(99)).is_none());
    }

    #[test]
    fn snapshot_find_returns_some_after_insert() {
        let store = ClaimStore::new();
        let hash = insert_one(&store, 60);
        let snap = store.snapshot();
        assert!(snap.find(hash).is_some());
    }

    #[test]
    fn snapshot_by_state_filters_correctly() {
        let store = ClaimStore::new();
        let hash = insert_one(&store, 61);
        let snap = store.snapshot();
        assert_eq!(snap.by_state(ClaimState::Provisional).len(), 1);
        assert_eq!(snap.by_state(ClaimState::Verified).len(), 0);
        drop(hash);
    }

    #[test]
    fn snapshot_by_state_shows_verified_after_mark_verified() {
        let store = ClaimStore::new();
        let _ = insert_and_verify(&store, 62);
        let snap = store.snapshot();
        assert_eq!(snap.by_state(ClaimState::Verified).len(), 1);
        assert_eq!(snap.by_state(ClaimState::Provisional).len(), 0);
    }

    #[test]
    fn snapshot_by_workflow_filters_correctly() {
        let store = ClaimStore::new();
        let h1 = test_hash(63);
        let h2 = test_hash(64);
        store
            .insert(h1, "wf-alpha", "s1", "PASS")
            .expect("insert ok");
        store
            .insert(h2, "wf-beta", "s1", "PASS")
            .expect("insert ok");
        let snap = store.snapshot();
        assert_eq!(snap.by_workflow("wf-alpha").len(), 1);
        assert_eq!(snap.by_workflow("wf-beta").len(), 1);
        assert_eq!(snap.by_workflow("wf-gamma").len(), 0);
    }

    #[test]
    fn snapshot_tick_advances_with_each_operation() {
        let store = ClaimStore::new();
        let snap0 = store.snapshot();
        let _ = insert_one(&store, 65);
        let snap1 = store.snapshot();
        assert!(snap1.tick > snap0.tick);
    }

    #[test]
    fn snapshot_claims_count_matches_store_count() {
        let store = ClaimStore::new();
        let _ = insert_one(&store, 66);
        let _ = insert_one(&store, 67);
        let snap = store.snapshot();
        assert_eq!(snap.claims.len(), store.count());
    }

    // ── ClaimStore debug ──────────────────────────────────────────────────────

    #[test]
    fn claim_store_debug_contains_count() {
        let store = ClaimStore::new();
        let _ = insert_one(&store, 70);
        let debug = format!("{store:?}");
        assert!(debug.contains("count=1"), "debug: {debug}");
    }

    // ── ordering invariant: monotonic tick ────────────────────────────────────

    #[test]
    fn monotonic_tick_advances_on_each_insert() {
        let store = ClaimStore::new();
        let h1 = insert_one(&store, 80);
        let c1 = store.get(h1).expect("get ok");
        let h2 = insert_one(&store, 81);
        let c2 = store.get(h2).expect("get ok");
        assert!(c2.created_at > c1.created_at);
    }

    #[test]
    fn updated_at_advances_on_mark_verified() {
        let store = ClaimStore::new();
        let h = insert_one(&store, 82);
        let before = store.get(h).expect("get ok").updated_at;
        let _ = store.mark_verified(h).expect("verify ok");
        let after = store.get(h).expect("get ok").updated_at;
        assert!(after > before);
    }
}
