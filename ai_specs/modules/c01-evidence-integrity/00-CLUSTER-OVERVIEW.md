# C01 Evidence Integrity — Cluster Overview

> **Cluster:** C01_EVIDENCE_INTEGRITY | **Modules:** 5 (M005–M009) | **Layers:** L01 / L02 / L04
> **LOC Estimate:** ~2,050 | **Tests Estimate:** ~195
> **Error Code Range:** 2000–2099

---

## Purpose

C01 is the cryptographic trust foundation of the Habitat Loop Engine. Every claim
that a workflow step passed, every receipt persisted to storage, and every promotion
to `Final` state flows through this cluster's five modules. Without C01, the verifier
has no proof identity and the executor has no hash anchor to certify.

The synergy chain is strictly one-directional:

```
M005 receipt_hash   →  computes canonical ReceiptHash
M006 claims_store   →  stores and transitions claims (Provisional → Verified → Final)
M007 receipts_store →  appends receipts durably, keyed by ReceiptHash
M008 receipt_sha_verifier  →  independently recomputes hashes from stored artifacts
M009 final_claim_evaluator →  sole authority for Verified → Final promotion
```

No module in C01 may both produce an artifact and certify it (see
[HLE-UP-001](../../../ai_docs/use_patterns/UP_EXECUTOR_VERIFIER_SPLIT.md)).

---

## File Map

```
crates/
├── hle-core/
│   └── src/
│       └── evidence/
│           ├── receipt_hash.rs      # M005 — ReceiptHash newtype, hashing helpers
│           └── claims_store.rs      # M006 — ClaimStore, ClaimState FSM
└── hle-storage/
│   └── src/
│       └── receipts_store.rs        # M007 — ReceiptsStore, append-only
└── hle-verifier/
    └── src/
        ├── receipt_sha_verifier.rs  # M008 — ReceiptShaVerifier (independent crate)
        └── final_claim_evaluator.rs # M009 — FinalClaimEvaluator, VerifierToken gate
```

---

## Dependency Graph (Intra-Cluster)

```
M005 receipt_hash          (leaf — no C01 internal deps)
        ↓
M006 claims_store          imports M005::ReceiptHash for claim keying
        ↓
M007 receipts_store        imports M005::ReceiptHash (storage key)
                           reads M006::ClaimState via trait bound
        ↓
M008 receipt_sha_verifier  imports M005::ReceiptHash (recompute target)
        ↓
M009 final_claim_evaluator imports M005::ReceiptHash, M006::ClaimStore + VerifiedClaim
                           consumes M008::VerifierToken to gate promotion
```

M008 and M009 live in `hle-verifier`, a crate that has NO import path to any executor
mutation surface. This enforces HLE-UP-001 at the Cargo dependency graph level.

---

## Cross-Cluster Dependencies

| Dependency | Direction | Reason |
|---|---|---|
| `substrate-types::HleError` | C01 → substrate | unified error type for all clusters |
| `substrate-types::StepState` | C01 → substrate | claim states align with step lifecycle |
| `substrate-types::Receipt` | C01 → substrate | persisted receipt struct |
| C05 `pool` (M021) | C01 M007 → C05 | receipts_store uses the shared DB pool |
| C02 `ClaimAuthority` | C02 → C01 | authority cluster wraps C01 claims with type-state tokens |

C01 does NOT import from C03 (execution), C06 (runbooks), or C07 (bridges). Data flows
outward from C01; nothing flows back in.

---

## Concurrency Architecture

| Module | Sync Strategy | Rationale |
|---|---|---|
| M005 `receipt_hash` | None (pure functions, `Copy` type) | Stateless; callers wrap externally |
| M006 `claims_store` | `parking_lot::RwLock<ClaimStoreInner>` | Read-heavy claim lookups, infrequent transitions |
| M007 `receipts_store` | `parking_lot::Mutex<Connection>` via C05 pool | Append-only WAL SQLite; single writer per connection |
| M008 `receipt_sha_verifier` | None (stateless functions) | Verifier is a pure transform over inputs |
| M009 `final_claim_evaluator` | `parking_lot::RwLock<EvaluatorInner>` | Read promotion log, write Final claims atomically |

---

## Design Principles

1. **Hash identity is the root of all proof.** `ReceiptHash([u8; 32])` is the only type
   accepted as a receipt identifier. No string keys, no integers — only the SHA-256
   digest of the canonical receipt fields.

2. **Claim state transitions are one-way.** `Provisional → Verified → Final` is
   enforced via the `ClaimState` FSM. Reverse transitions compile-error.

3. **Append-only storage, never update.** `ReceiptsStore` exposes `append()` and
   `get()` only. There is no `update()` or `delete()` method on any public surface.

4. **Verifier crate isolation.** M008 and M009 live in `hle-verifier` which must not
   take a dev or regular dependency on `hle-executor` or any executor-side mutation
   surface. Enforced in `Cargo.toml` dependency declarations.

5. **`VerifierToken` as zero-cost type gate.** `FinalClaimEvaluator::promote()` requires
   `PhantomData<VerifierToken>` in its signature. Only code compiled inside `hle-verifier`
   can construct a `VerifierToken`. Executors cannot forge one.

6. **All public methods are `#[must_use]`.** Hash computations, claim queries, and
   evaluation results are consumed or the compiler warns.

7. **No `unwrap`, `expect`, `panic`, `todo`, or `dbg` anywhere.** All fallible paths
   return `Result<_, HleError>`. Enforced by workspace-level clippy deny.

8. **Hex encoding matches schema.** `ReceiptHash::to_hex()` produces the 64-character
   lowercase hex string required by `schemas/receipt.schema.json` pattern
   `^[0-9a-f]{64}$`. Binary and hex round-trip are tested.

---

## Error Strategy

All C01 modules use `substrate_types::HleError` and return `Result<_, HleError>`.

| Code | Variant name | Source module | Trigger |
|---|---|---|---|
| 2000 | `HashInput` | M005 | empty or malformed input to `ReceiptHash::from_fields` |
| 2010 | `ClaimNotFound` | M006 | `claims_store::get()` with unknown hash |
| 2011 | `InvalidTransition` | M006 | attempt to reverse or skip a claim state |
| 2012 | `DuplicateClaim` | M006 | inserting a claim whose hash is already present |
| 2020 | `AppendConflict` | M007 | receipt hash already present in append-only store |
| 2021 | `StorageIo` | M007 | SQLite or pool error during append/get |
| 2030 | `HashMismatch` | M008 | recomputed hash does not match stored hash |
| 2031 | `MissingArtifact` | M008 | referenced artifact not found for recomputation |
| 2040 | `NotVerified` | M009 | attempt to promote a Provisional (not Verified) claim |
| 2041 | `AlreadyFinal` | M009 | claim is already in Final state; re-promotion blocked |

These codes are additive extensions of the base `HleError` message convention. The
numeric prefixes appear in the `HleError` message string as `[E2xxx]` for log filtering.

---

## Quality Gate Results Template

```
cargo check --workspace --all-targets       PASS  0 errors
cargo clippy --workspace -- -D warnings     PASS  0 warnings
cargo clippy --workspace -- -W pedantic     PASS  0 warnings
cargo test --workspace --all-targets        PASS  ~195 tests, 0 failures
Zero-tolerance grep (unsafe/unwrap/expect)  PASS  0 hits in C01 source
```

---

*C01 Evidence Integrity Cluster Overview v1.0 | 2026-05-10*
