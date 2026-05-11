# M009 final_claim_evaluator — final_claim_evaluator.rs

> **File:** `crates/hle-verifier/src/final_claim_evaluator.rs` | **LOC:** ~370 | **Tests:** ~35
> **Role:** Only the verifier can promote Verified claims to Final; type-state token gate enforced

---

## Types at a Glance

| Type | Kind | Copy | Hash | Const | Purpose |
|---|---|---|---|---|---|
| `FinalClaimEvaluator` | struct | No | No | No | Sole authority for Verified → Final promotion |
| `PromotionReceipt` | struct | No | No | No | Immutable proof record emitted by a successful promotion |
| `EvaluatorConfig` | struct | No | No | No | Configuration (duplicate-promotion policy, counters) |
| `PromotionLog` | struct | No | No | No | Bounded in-memory audit log of promotions this session |

---

## FinalClaimEvaluator

```rust
pub struct FinalClaimEvaluator {
    config: EvaluatorConfig,
    inner: parking_lot::RwLock<FinalClaimEvaluatorInner>,
}

struct FinalClaimEvaluatorInner {
    log: PromotionLog,
    promotion_count: u64,
}
```

`FinalClaimEvaluator` is the terminal authority in the C01 synergy chain. Its
`promote()` method is the only place in the entire codebase where a claim may
transition from `Verified` to `Final`. Every other code path is structurally
blocked by the `VerifierToken` type gate.

| Method | Signature | Notes |
|---|---|---|
| `new` | `fn(config: EvaluatorConfig) -> Self` | `#[must_use]` |
| `with_default_config` | `fn() -> Self` | `#[must_use]` — convenience; uses `EvaluatorConfig::default()` |
| `promote` | `fn(&self, claim: &VerifiedClaim, token: VerifierToken) -> Result<PromotionReceipt, HleError>` | `#[must_use]` — the sole Final promotion path; see token gate below |
| `is_promoted` | `fn(&self, hash: ReceiptHash) -> bool` | `#[must_use]` — read lock; checks promotion log |
| `promotion_count` | `fn(&self) -> u64` | `#[must_use]` — read lock; total successful promotions this session |
| `log_snapshot` | `fn(&self) -> Vec<PromotionReceipt>` | `#[must_use]` — clones current log under read lock |

**Traits implemented:** `Debug` (shows promotion_count)

---

## promote() — Type-State Token Gate

```rust
pub fn promote(
    &self,
    claim: &VerifiedClaim,
    token: VerifierToken,
) -> Result<PromotionReceipt, HleError> {
    // 1. Confirm token.verified_hash == claim.hash() (same receipt, not reused token)
    // 2. Check claim state is Verified (defended by VerifiedClaim newtype, but double-check)
    // 3. Check claim is not already promoted (error E2041 if so)
    // 4. Transition claim to Final via ClaimStore (passed in at construction or injected)
    // 5. Append PromotionReceipt to inner log
    // 6. Increment promotion_count
    // 7. Return PromotionReceipt
}
```

The `VerifierToken` argument enforces two things at the type level:

1. **Only `hle-verifier` code can call `promote()` with a valid token.** `VerifierToken`
   has no public constructor outside `hle-verifier`. Executor code cannot forge one.
2. **Token is bound to the specific receipt being promoted.** `token.verified_hash`
   must equal `claim.hash()`. A token issued for receipt A cannot be used to promote
   receipt B even within the same crate.

An alternative type-state formulation using `PhantomData<VerifierToken>` would look like:

```rust
pub struct FinalClaimEvaluator<State = Uninitialized> {
    _state: std::marker::PhantomData<State>,
    inner: parking_lot::RwLock<FinalClaimEvaluatorInner>,
}
pub struct Uninitialized;
pub struct ReadyToPromote;

impl FinalClaimEvaluator<Uninitialized> {
    pub fn arm(self, token: VerifierToken) -> Result<FinalClaimEvaluator<ReadyToPromote>, HleError>;
}
impl FinalClaimEvaluator<ReadyToPromote> {
    pub fn promote(self, claim: &VerifiedClaim) -> Result<PromotionReceipt, HleError>;
}
```

Either formulation is acceptable at implementation time. The chosen pattern must be
documented in the implementation with a `// TYPESTATE GATE: VerifierToken required`
comment directly above `promote()`.

---

## PromotionReceipt

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotionReceipt {
    /// Hash of the promoted claim.
    pub hash: ReceiptHash,
    /// Workflow identifier.
    pub workflow: String,
    /// Step identifier.
    pub step_id: String,
    /// Monotonic counter at promotion time.
    pub promoted_at: u64,
    /// Sequential index of this promotion in the log (1-indexed).
    pub sequence: u64,
}
```

`PromotionReceipt` is `#[must_use]`. It is the only written record that a Final
promotion occurred in this session. Callers that discard it lose the promotion
audit trail.

| Method | Signature | Notes |
|---|---|---|
| `summary` | `fn(&self) -> String` | `#[must_use]` — single-line log-friendly description |

**Traits implemented:** `Display` ("PromotionReceipt(#42:3a7f9c…→Final@demo/s1)")

---

## EvaluatorConfig

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluatorConfig {
    /// Maximum entries to retain in the in-memory promotion log. Clamped 1..=10_000.
    pub log_capacity: usize,
    /// Whether to return E2041 on duplicate promotion attempts (true) or silently return
    /// the previous receipt (false). Default: true (strict).
    pub strict_duplicate_guard: bool,
}
```

| Method | Signature | Notes |
|---|---|---|
| `new` | `const fn(log_capacity: usize, strict_duplicate_guard: bool) -> Self` | `#[must_use]` |
| `default` | — | `log_capacity=1000`, `strict_duplicate_guard=true` |

---

## PromotionLog

```rust
struct PromotionLog {
    entries: std::collections::VecDeque<PromotionReceipt>,
    capacity: usize,
}
```

`PromotionLog` is crate-private. It retains at most `EvaluatorConfig::log_capacity`
entries; oldest entries are evicted when the log is full. This bounds memory growth
for long-running sessions without requiring an external store.

| Method | Signature | Notes |
|---|---|---|
| `push` | `fn(&mut self, receipt: PromotionReceipt)` | Evicts oldest if at capacity |
| `contains` | `fn(&self, hash: ReceiptHash) -> bool` | O(n) scan; log is bounded so this is acceptable |
| `snapshot` | `fn(&self) -> Vec<PromotionReceipt>` | Clones all entries in insertion order |

---

## Design Notes

- `FinalClaimEvaluator` is in `hle-verifier`. Like M008, it must not depend on
  `hle-executor`. The `VerifiedClaim` type comes from `hle-core` (M006), which is
  a neutral vocabulary crate that both sides may import.
- The `VerifierToken` consumed by `promote()` is moved (not borrowed). This makes
  each token single-use at the type level — you cannot call `promote()` twice with
  the same token because Rust's ownership rules prevent it.
- `promote()` checks `token.verified_hash == claim.hash()` first. If they differ,
  it returns `HleError` with code E2030 (hash mismatch) rather than E2040/E2041.
  This catches token confusion bugs early.
- `strict_duplicate_guard = true` (default) means attempting to promote a hash that
  already appears in the promotion log returns E2041. This is the safe default;
  callers that want idempotent re-promotion must opt in to `strict_duplicate_guard = false`.
- `PromotionLog` uses `VecDeque` for O(1) push/evict from both ends. The bounded
  capacity prevents memory growth even if the evaluator is used for thousands of
  promotions in a single session.
- `FinalClaimEvaluator` uses `parking_lot::RwLock` for all interior mutability so
  it can be wrapped in `Arc<FinalClaimEvaluator>` and shared across threads.
- The `promoted_at` field uses a monotonic counter passed from the caller (not
  `std::time::SystemTime`). This keeps C01 free of wall-clock dependencies.

---

## Cluster Invariants

- **Sole promotion authority.** No other module in any cluster may transition a claim
  from `Verified` to `Final`. The `VerifierToken` gate enforces this; if a future
  module needs to promote claims, it must go through `FinalClaimEvaluator::promote()`.
- **HLE-UP-001 (critical).** This module is in `hle-verifier`. `Cargo.toml` for
  `hle-verifier` must not list `hle-executor` as a dependency. The `VerifiedClaim`
  and `ReceiptHash` imports come from `hle-core` (neutral). See
  [UP_EXECUTOR_VERIFIER_SPLIT](../../../ai_docs/use_patterns/UP_EXECUTOR_VERIFIER_SPLIT.md).
- **Token non-reuse.** After a successful `promote()` call, the consumed `VerifierToken`
  is dropped. The same token cannot be used again. This is enforced by Rust move
  semantics, not runtime checks.
- **Final is immutable.** Once `FinalClaimEvaluator::promote()` returns `Ok(...)`,
  no further state transitions are possible on that claim. Attempting E2041 in strict
  mode surfaces the guard.
- **Negative-control test required.** The test suite must include a test that confirms
  `promote()` returns an error when called with a `VerifierToken` whose `verified_hash`
  does not match the supplied `VerifiedClaim::hash()`. This is the primary negative
  control for the token gate.

---

*M009 final_claim_evaluator Spec v1.0 | 2026-05-10*
