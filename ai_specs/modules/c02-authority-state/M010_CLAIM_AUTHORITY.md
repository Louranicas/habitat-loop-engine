# M010 claim_authority — claim_authority.rs

> **File:** `crates/hle-core/src/authority/claim_authority.rs` | **LOC:** ~280 | **Tests:** ~45
> **Role:** type-state authority model — compile-time enforcement that only `hle-verifier` issues Final tokens

---

## Types at a Glance

| Type | Kind | Copy | Hash | Const | Purpose |
|---|---|---|---|---|---|
| `Provisional` | marker struct | Yes | Yes | Yes | State tag — executor may hold this |
| `Verified` | marker struct | Yes | Yes | Yes | State tag — verifier intermediate step |
| `Final` | marker struct | No | No | No | State tag — non-Copy to prevent cloning; `pub(crate)` in `hle-verifier` |
| `ClaimAuthority<S>` | generic struct + `PhantomData<S>` | conditional | No | No | Type-state authority token |
| `AuthorityClass` | enum | Yes | Yes | Yes | Runtime label attached to a token |
| `AuthorityError` | enum | No | No | No | Error type for C02 cluster (codes 2100–2199) |

---

## State Markers

```rust
/// Executor-held provisional authority: claim is proposed but not yet verified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Provisional;

/// Intermediate state: verifier has accepted the draft but has not yet issued
/// a final receipt.  Only `hle-verifier` creates this via `ClaimAuthority::verify`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Verified;

/// Final authority: verifier has issued a binding PASS receipt.
/// `Final` is NOT `Copy` and is `pub(crate)` inside `hle-verifier` — the executor
/// crate cannot construct or name this type.
///
/// # Safety boundary
/// Making `Final` non-`Copy` prevents the executor from obtaining a `ClaimAuthority<Final>`
/// by cloning a value passed to it.  The combination of `pub(crate)` visibility and
/// no `Copy` impl is the compile-time FP_SELF_CERTIFICATION guard (HLE-SP-001).
#[derive(Debug)]
pub struct Final; // deliberately NOT Clone/Copy
```

---

## ClaimAuthority\<S\>

```rust
/// Type-state authority token.
///
/// The type parameter `S` is one of `Provisional`, `Verified`, or `Final`.
/// Construction is restricted:
///  - `ClaimAuthority::<Provisional>::new(…)` — public, usable by executor
///  - `ClaimAuthority::<Verified>::verify(…)` — visible inside `hle-verifier` only
///  - `ClaimAuthority::<Final>::finalize(…)` — visible inside `hle-verifier` only
///
/// There is no `impl Clone for ClaimAuthority<Final>` — the token can only
/// move, not be duplicated.
#[derive(Debug)]
pub struct ClaimAuthority<S> {
    workflow_id: String,
    step_id: String,
    class: AuthorityClass,
    _state: std::marker::PhantomData<S>,
}
```

### Methods — `ClaimAuthority<Provisional>`

| Method | Signature | Notes |
|---|---|---|
| `new` | `#[must_use] pub fn new(workflow_id: impl Into<String>, step_id: impl Into<String>, class: AuthorityClass) -> Self` | Public; executor entry point |
| `workflow_id` | `#[must_use] pub fn workflow_id(&self) -> &str` | Read-only accessor |
| `step_id` | `#[must_use] pub fn step_id(&self) -> &str` | Read-only accessor |
| `class` | `#[must_use] pub fn class(&self) -> AuthorityClass` | Copy return |
| `is_provisional` | `#[must_use] pub const fn is_provisional(&self) -> bool` | Always `true` for this variant |
| `is_final` | `#[must_use] pub const fn is_final(&self) -> bool` | Always `false` for this variant |

### Methods — `ClaimAuthority<Verified>` (crate-internal in `hle-verifier`)

| Method | Signature | Notes |
|---|---|---|
| `verify` | `#[must_use] pub(crate) fn verify(provisional: ClaimAuthority<Provisional>) -> Self` | Consumes Provisional; only callable inside `hle-verifier` |
| `workflow_id` | `#[must_use] pub fn workflow_id(&self) -> &str` | |
| `step_id` | `#[must_use] pub fn step_id(&self) -> &str` | |

### Methods — `ClaimAuthority<Final>` (crate-internal in `hle-verifier`)

| Method | Signature | Notes |
|---|---|---|
| `finalize` | `#[must_use] pub(crate) fn finalize(verified: ClaimAuthority<Verified>) -> Self` | Consumes Verified; only callable inside `hle-verifier` |
| `workflow_id` | `#[must_use] pub fn workflow_id(&self) -> &str` | |
| `step_id` | `#[must_use] pub fn step_id(&self) -> &str` | |
| `into_receipt_evidence` | `pub(crate) fn into_receipt_evidence(self) -> (String, String, AuthorityClass)` | Destructures the Final token into (workflow_id, step_id, class) for receipt construction; consumes self so the token cannot be reused |

### Trait Implementations

| Trait | Bounds | Notes |
|---|---|---|
| `Display` | all `S` | `"ClaimAuthority<{state}>(workflow={…}, step={…}, class={…})"` |
| `Clone` | `S: Clone` | Excludes `Final` by design |
| `Copy` | `S: Copy` | Excludes `Final` by design |

---

## AuthorityClass

```rust
/// Runtime classification attached to an authority token.
/// Determines which verifier code path handles the claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum AuthorityClass {
    /// Standard automated step; verifier checks receipt hash.
    Automated = 0,
    /// Step requires a human operator decision before PASS is possible.
    HumanRequired = 1,
    /// Negative-control fixture: expected to produce FAIL; verifier checks rejection.
    NegativeControl = 2,
    /// Rollback record: documents a revert action rather than forward progress.
    Rollback = 3,
}
```

| Method | Signature | Notes |
|---|---|---|
| `as_str` | `#[must_use] pub const fn as_str(self) -> &'static str` | Wire-format label |
| `is_human_required` | `#[must_use] pub const fn is_human_required(self) -> bool` | |
| `is_negative_control` | `#[must_use] pub const fn is_negative_control(self) -> bool` | |

**Traits:** `Display` ("automated" / "human-required" / "negative-control" / "rollback")

---

## AuthorityError

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorityError {
    InvalidTransition { from: String, to: String },   // 2100
    TerminalState { state: String },                   // 2101
    SelfCertification { workflow_id: String },          // 2102
    UnknownWorkflow { workflow_id: String },            // 2103
    StaleEvent { expected: u64, received: u64 },        // 2104
    RollbackUnavailable { from: String },               // 2110
    TokenAlreadyConsumed { step_id: String },           // 2150
    Other(String),                                     // 2199
}

pub type Result<T> = std::result::Result<T, AuthorityError>;
```

| Method | Signature |
|---|---|
| `error_code` | `#[must_use] pub const fn error_code(&self) -> u16` |
| `is_self_certification` | `#[must_use] pub const fn is_self_certification(&self) -> bool` |

**Traits:** `Display`, `std::error::Error`

---

## Design Notes

- `PhantomData<S>` carries zero bytes at runtime. `ClaimAuthority<Provisional>` is exactly
  two `String` fields plus an `AuthorityClass` byte — no heap overhead from the state marker.
- The transition from `Provisional` to `Verified` to `Final` is one-way and move-only.
  There is no `downgrade` method; once authority advances it cannot retreat.
- `Final` deliberately omits `Clone` and `Copy`. This is the single structural guarantee
  against `FP_SELF_CERTIFICATION` (HLE-SP-001): even if an executor crate received a
  `ClaimAuthority<Final>` by some other means, it cannot duplicate or hold it without
  consuming the unique token.
- `pub(crate)` on `verify`, `finalize`, and `Final` itself (re-exported from `hle-verifier`)
  means the Rust module system, not documentation or convention, enforces the boundary.
- `AuthorityClass::NegativeControl` lets the verifier distinguish false-pass fixtures from
  live execution claims without a separate type hierarchy.
- Every `&self` accessor is `#[must_use]`. Callers that ignore the return value of
  `is_provisional()` receive a compiler warning under `clippy::pedantic`.

---

## Cluster Invariants

This module enforces C02 Invariant I1:

> **I1 (No Executor Final):** A value of type `ClaimAuthority<Final>` can only be constructed
> inside `crates/hle-verifier`. The executor crate `crates/hle-executor` can construct
> `ClaimAuthority<Provisional>` and observe `ClaimAuthority<Verified>` passed back from the
> verifier, but it cannot call `finalize(…)` or construct `Final` directly.

See also: `../../ai_docs/use_patterns/UP_EXECUTOR_VERIFIER_SPLIT.md` (HLE-UP-001),
`../../ai_docs/anti_patterns/FP_FALSE_PASS_CLASSES.md` (HLE-SP-001).

---

*M010 CLAIM_AUTHORITY Spec v1.0 | 2026-05-10*
