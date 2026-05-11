# M006 claims_store — claims_store.rs

> **File:** `crates/hle-core/src/evidence/claims_store.rs` | **LOC:** ~420 | **Tests:** ~45
> **Role:** Claim graph store with Provisional / Verified / Final state anchors

---

## Types at a Glance

| Type | Kind | Copy | Hash | Const | Purpose |
|---|---|---|---|---|---|
| `ClaimState` | enum | Yes | Yes | Yes | One-way FSM: Provisional → Verified → Final |
| `Claim` | struct | No | No | No | A single claim record keyed by ReceiptHash |
| `VerifiedClaim` | newtype(`Claim`) | No | No | No | Proof that a claim passed Provisional → Verified |
| `ClaimStore` | struct | No | No | No | In-memory store with RwLock interior mutability |
| `ClaimStoreSnapshot` | struct | No | No | No | Read-only clone for inspection without holding the lock |

---

## ClaimState

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ClaimState {
    /// Executor-produced; not yet independently verified.
    Provisional,
    /// Verifier confirmed hash matches artifact; ready for promotion.
    Verified,
    /// FinalClaimEvaluator has promoted this claim; immutable.
    Final,
}
```

| Method | Signature | Notes |
|---|---|---|
| `as_str` | `const fn(self) -> &'static str` | `#[must_use]` — `"provisional"` / `"verified"` / `"final"` |
| `is_terminal` | `const fn(self) -> bool` | `#[must_use]` — `true` for `Final` only |
| `can_transition_to` | `const fn(self, next: Self) -> bool` | `#[must_use]` — `Provisional→Verified` and `Verified→Final` only; all others `false` |

**Traits implemented:** `Display` ("provisional" / "verified" / "final"), `FromStr`
(parses the same wire strings; errors via `HleError` for unknown values)

---

## Claim

```rust
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
    /// Executor-supplied verdict string before verification.
    pub draft_verdict: String,
    /// Monotonic creation counter (not wall clock; no chrono/SystemTime).
    pub created_at: u64,
    /// Monotonic last-transition counter.
    pub updated_at: u64,
}
```

| Method | Signature | Notes |
|---|---|---|
| `new` | `fn(hash, workflow, step_id, draft_verdict, tick) -> Self` | `#[must_use]` — creates Provisional; both timestamps = tick |
| `transition` | `fn(&self, next: ClaimState, tick: u64) -> Result<Self, HleError>` | `#[must_use]` — validates via `ClaimState::can_transition_to`; error E2011 on invalid |
| `is_final` | `const fn(&self) -> bool` | `#[must_use]` |
| `summary` | `fn(&self) -> String` | `#[must_use]` — human-readable one-liner for logs |

**Traits implemented:** `Display` ("Claim(3a7f9c…:verified@demo/s1)")

---

## VerifiedClaim

```rust
/// A `Claim` whose state is confirmed `Verified`. Constructible only inside
/// this module; callers receive one via `ClaimStore::mark_verified`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedClaim(Claim);
```

`VerifiedClaim` is a module-private constructor newtype. The inner `Claim` is always
in state `Verified` — this is guaranteed by the only constructor path:

```rust
impl VerifiedClaim {
    // Private — only ClaimStore::mark_verified can call this.
    fn new(claim: Claim) -> Result<Self, HleError> { ... }

    #[must_use]
    pub fn inner(&self) -> &Claim { &self.0 }

    #[must_use]
    pub fn hash(&self) -> ReceiptHash { self.0.hash }
}
```

M009 `FinalClaimEvaluator::promote()` accepts `&VerifiedClaim` as its input. This
ensures only claims that have passed verification can reach Final state.

---

## ClaimStore

```rust
pub struct ClaimStore {
    inner: parking_lot::RwLock<ClaimStoreInner>,
}

struct ClaimStoreInner {
    claims: std::collections::HashMap<ReceiptHash, Claim>,
    monotonic_tick: u64,
}
```

| Method | Signature | Notes |
|---|---|---|
| `new` | `fn() -> Self` | `#[must_use]` — empty store, tick=0 |
| `insert` | `fn(&self, hash: ReceiptHash, workflow, step_id, draft_verdict) -> Result<(), HleError>` | Acquires write lock; errors E2012 if hash already present |
| `get` | `fn(&self, hash: ReceiptHash) -> Result<Claim, HleError>` | `#[must_use]` — acquires read lock; errors E2010 if not found |
| `mark_verified` | `fn(&self, hash: ReceiptHash) -> Result<VerifiedClaim, HleError>` | `#[must_use]` — write lock; transitions Provisional→Verified; errors E2011 on wrong state |
| `snapshot` | `fn(&self) -> ClaimStoreSnapshot` | `#[must_use]` — clones entire inner map under read lock |
| `count` | `fn(&self) -> usize` | `#[must_use]` — read lock; total claim count |
| `count_by_state` | `fn(&self, state: ClaimState) -> usize` | `#[must_use]` — read lock; filtered count |

**Traits implemented:** `Debug` (shows count, not contents)

---

## ClaimStoreSnapshot

```rust
#[derive(Debug, Clone)]
pub struct ClaimStoreSnapshot {
    pub claims: Vec<Claim>,
    pub tick: u64,
}
```

| Method | Signature | Notes |
|---|---|---|
| `by_state` | `fn(&self, state: ClaimState) -> Vec<&Claim>` | `#[must_use]` |
| `by_workflow` | `fn(&self, workflow: &str) -> Vec<&Claim>` | `#[must_use]` |
| `find` | `fn(&self, hash: ReceiptHash) -> Option<&Claim>` | `#[must_use]` |

---

## Design Notes

- The `ClaimState` FSM is strictly one-way: `can_transition_to` returns `false`
  for any backward or same-state transition. There is no `demote()` method on any
  type in this module.
- `ClaimStore` uses `parking_lot::RwLock` for all interior mutability. Trait
  methods are `&self` so the store is `Arc<ClaimStore>`-compatible for cross-thread
  sharing.
- Scoped lock guards are always dropped before any call that could re-acquire the
  lock. No method holds a lock across an `await` point — this module is sync-only.
- Monotonic tick counters use `u64` incremented atomically inside `ClaimStoreInner`.
  There is no `chrono`, no `SystemTime`, and no wall clock.
- `VerifiedClaim` is the typestate proof that transitions into M009's
  `FinalClaimEvaluator`. Constructing one outside `mark_verified` is impossible
  without access to the private constructor — the Rust module system enforces this.
- `DuplicateClaim` (E2012) is a hard error, not a no-op. The store does not silently
  accept duplicate hashes to prevent replay of executor artifacts.
- `ClaimStoreSnapshot` returns owned data, not references through the lock. This
  avoids RAII anti-patterns where callers inadvertently hold lock guards.

---

## Cluster Invariants

- **State monotonicity:** once a claim reaches `Final`, no method in C01 can alter
  its state. Downstream clusters (C02) may wrap a `Final` claim but cannot regress it.
- **HLE-UP-001:** `ClaimStore` is in `hle-core`. It does not import from `hle-verifier`
  or `hle-executor`. The `VerifiedClaim` newtype exists so M009 (in `hle-verifier`) can
  accept proof without the verifier importing executor internals. See
  [UP_EXECUTOR_VERIFIER_SPLIT](../../../ai_docs/use_patterns/UP_EXECUTOR_VERIFIER_SPLIT.md).
- `ClaimStore::insert` must be called only by executor-side code. Verifier-side code
  calls `mark_verified` and reads via `get`.

---

*M006 claims_store Spec v1.0 | 2026-05-10*
