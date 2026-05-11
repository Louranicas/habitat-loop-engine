# M008 receipt_sha_verifier — receipt_sha_verifier.rs

> **File:** `crates/hle-verifier/src/receipt_sha_verifier.rs` | **LOC:** ~320 | **Tests:** ~40
> **Role:** Independent verifier recomputes receipt hashes; cannot import executor mutation paths

---

## Types at a Glance

| Type | Kind | Copy | Hash | Const | Purpose |
|---|---|---|---|---|---|
| `ReceiptShaVerifier` | struct | No | No | No | Stateless entry point for hash recomputation |
| `VerifyInput` | struct | No | No | No | The fields needed to recompute a hash independently |
| `VerifyOutcome` | enum | No | No | No | `Matched` or `Mismatch` result of recomputation |
| `VerifierToken` | struct | No | No | No | Zero-sized proof-of-verifier-crate; required by M009 |

---

## ReceiptShaVerifier

```rust
pub struct ReceiptShaVerifier {
    _private: (),
}
```

`ReceiptShaVerifier` has no mutable state. It is constructed once and shared
freely. Its only job is to recompute a `ReceiptHash` from raw input fields and
compare it against the stored hash — fully independently of the executor path.

This struct lives in `hle-verifier`, which MUST NOT take a dependency on
`hle-executor`. The Cargo workspace enforces this. Importing any executor mutation
type in this file is a compile error and an architectural violation of HLE-UP-001.

| Method | Signature | Notes |
|---|---|---|
| `new` | `fn() -> Self` | `#[must_use]` |
| `verify` | `fn(&self, input: &VerifyInput) -> Result<VerifyOutcome, HleError>` | `#[must_use]` — recomputes hash; errors E2031 if artifact lookup fails |
| `verify_and_token` | `fn(&self, input: &VerifyInput) -> Result<(VerifyOutcome, VerifierToken), HleError>` | `#[must_use]` — same as `verify` but also returns a `VerifierToken` when outcome is `Matched` |
| `verify_batch` | `fn(&self, inputs: &[VerifyInput]) -> Vec<Result<VerifyOutcome, HleError>>` | `#[must_use]` — runs each independently; one error does not abort the batch |

**Traits implemented:** `Debug`, `Clone`

---

## VerifyInput

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyInput {
    /// The stored hash to verify against (from ReceiptsStore or ClaimStore).
    pub stored_hash: ReceiptHash,
    /// Workflow identifier used during original hashing.
    pub workflow: String,
    /// Step identifier used during original hashing.
    pub step_id: String,
    /// Verdict string used during original hashing.
    pub verdict: String,
    /// Manifest SHA-256 anchor (`^Manifest_sha256`) from HARNESS_CONTRACT.md.
    pub manifest_sha256: String,
    /// Framework SHA-256 anchor (`^Framework_sha256`) from HARNESS_CONTRACT.md.
    pub framework_sha256: String,
}
```

| Method | Signature | Notes |
|---|---|---|
| `new` | `fn(stored_hash, workflow, step_id, verdict, manifest_sha256, framework_sha256) -> Result<Self, HleError>` | `#[must_use]` — validates non-empty fields; errors E2031 on empty |
| `from_stored_receipt` | `fn(receipt: &StoredReceipt) -> Self` | `#[must_use]` — convenience constructor from M007 type |

**Traits implemented:** `Display` ("VerifyInput(stored=3a7f9c…@demo/s1)")

---

## VerifyOutcome

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyOutcome {
    /// Recomputed hash matches stored hash. Evidence is authentic.
    Matched {
        hash: ReceiptHash,
    },
    /// Recomputed hash does not match stored hash. Evidence is suspect.
    Mismatch {
        stored: ReceiptHash,
        recomputed: ReceiptHash,
    },
}
```

| Method | Signature | Notes |
|---|---|---|
| `is_matched` | `const fn(&self) -> bool` | `#[must_use]` |
| `stored_hash` | `fn(&self) -> ReceiptHash` | `#[must_use]` — returns the hash from the `stored` field in either variant |
| `as_hle_result` | `fn(&self) -> Result<ReceiptHash, HleError>` | `#[must_use]` — `Ok(hash)` if Matched; `Err(HleError E2030)` if Mismatch |

**Traits implemented:** `Display` ("Matched(3a7f9c…)" or "Mismatch(stored=…, recomputed=…)")

---

## VerifierToken

```rust
/// Zero-sized proof that the calling code is running inside the `hle-verifier`
/// crate and has completed a successful hash verification. Consumed by
/// `FinalClaimEvaluator::promote()` to prevent executor self-promotion.
///
/// Construction is private to this module. Callers receive one only via
/// `ReceiptShaVerifier::verify_and_token()` when `VerifyOutcome::Matched`.
#[derive(Debug)]
pub struct VerifierToken {
    /// The hash that was verified. Carried to prevent token reuse
    /// across different receipts.
    pub(crate) verified_hash: ReceiptHash,
    _private: (),
}
```

`VerifierToken` cannot be constructed outside `receipt_sha_verifier.rs`. The
`_private: ()` field combined with no public constructor enforces this at the Rust
privacy boundary. `pub(crate)` on `verified_hash` allows `final_claim_evaluator.rs`
(same crate) to read it for cross-checking but not to forge a token.

`FinalClaimEvaluator::promote()` accepts `VerifierToken` by value (not by reference)
so each token can only be used once per promotion call.

---

## Internal Hash Recomputation Logic

```
ReceiptShaVerifier::verify(input):
  1. Construct ReceiptHashFields {
       workflow: input.workflow,
       step_id: input.step_id,
       verdict: input.verdict,
       manifest_sha256: input.manifest_sha256,
       framework_sha256: input.framework_sha256,
     }
  2. recomputed = ReceiptHash::from_fields(&fields)?
  3. if recomputed == input.stored_hash → VerifyOutcome::Matched { hash: recomputed }
     else                               → VerifyOutcome::Mismatch { stored, recomputed }
```

Step 2 uses exactly the same `ReceiptHash::from_fields` path as the executor used
when creating the receipt. This ensures the verification is a faithful independent
recomputation, not a second-system check with different logic.

---

## Design Notes

- `ReceiptShaVerifier` is deliberately stateless. It holds no cache, no store
  reference, and no connection pool. All input comes through `VerifyInput`; all
  output is a value type. This makes it trivially testable in isolation.
- `verify_and_token()` only issues a `VerifierToken` when the outcome is `Matched`.
  A `Mismatch` outcome returns `Ok((Mismatch { ... }, _))` where the token is
  absent — the caller must check `is_matched()` before passing the token to M009.
  Actually: `verify_and_token` returns `Result<(VerifyOutcome, VerifierToken), HleError>`.
  The `VerifierToken` is only inside the tuple when the inner function sees a match;
  for a mismatch it returns `Err(HleError E2030)` so the token cannot be obtained.
- `verify_batch` runs inputs in order, not in parallel, to avoid requiring `Send`
  bounds on the verifier in contexts where the pool is not `Send`. Callers who need
  parallel verification must spawn tasks externally.
- The crate boundary (`hle-verifier`) is the architectural enforcement of HLE-UP-001.
  No amount of Rust `pub(crate)` gymnastics can make executor mutation types visible
  to this module unless someone edits `Cargo.toml` — which is a reviewable change.
- `VerifierToken::verified_hash` is `pub(crate)` so `final_claim_evaluator.rs` can
  confirm the token was issued for the same hash being promoted. This prevents token
  reuse across different receipts within the same crate.

---

## Cluster Invariants

- **HLE-UP-001 (critical).** This module is in `hle-verifier`. Its `Cargo.toml`
  must NOT list `hle-executor` as a dependency (regular or dev). Verification and
  execution are separate binaries at deployment time. See
  [UP_EXECUTOR_VERIFIER_SPLIT](../../../ai_docs/use_patterns/UP_EXECUTOR_VERIFIER_SPLIT.md).
- **VerifierToken non-forgeable.** The private `_private: ()` field and absence of
  any `pub` constructor ensure that only code inside `hle-verifier` can create a
  `VerifierToken`. This is verified by negative-control tests that attempt (and fail)
  to construct one from outside the crate.
- **Recomputation must use `ReceiptHash::from_fields`.** Verifier logic must not
  implement a separate SHA-256 path. Single-point hashing logic in M005 means a
  hash change in M005 propagates automatically to verification.
- **E2030 for mismatch, not silent accept.** A `Mismatch` outcome must surface as
  an error or explicit enum variant. The verifier must never silently treat a
  mismatched hash as `Matched`.

---

*M008 receipt_sha_verifier Spec v1.0 | 2026-05-10*
