# M039 Runbook Safety Policy — `crates/hle-runbook/src/safety_policy.rs`

> **Layer:** L07 | **Cluster:** C06 Runbook Semantics | **Error Codes:** 2590-2591
> **Role:** Safety gate and policy enforcement — the final check before any runbook phase executes.
> **LOC target:** ~300 | **Test target:** ≥50

---

## Purpose

M039 is the last line of defence before a runbook phase executes. The `SafetyPolicy::check` method examines the runbook's declared `safety_class`, the current `ExecutionContext` (traversal count, elevation token, agent identity), and the specific phase being requested. It returns `Ok(())` only when all safety conditions are satisfied. Any violation returns `Err(SafetyViolation)` with error code 2590 or 2591.

The module enforces four distinct safety rules:

1. **Traversal guard.** Non-idempotent runbooks halt after `max_traversals` passes.
2. **Safety-class elevation.** `Hard` and `Safety` class runbooks require a valid `ConfirmToken` before the `Fix` phase.
3. **Authority guard.** `Safety` class runbooks additionally require `ClaimAuthority` elevation.
4. **Phase ordering guard.** A `Fix` phase may not execute before `Detect` has completed with `passed: true`.

These rules are checked in order; the first violation short-circuits and returns an error.

---

## Types at a Glance

| Type | Kind | Notes |
|------|------|-------|
| `SafetyPolicy` | struct | Primary entry point; configurable per deployment |
| `ExecutionContext` | struct (imported from C02) | Current execution state and elevation tokens |
| `SafetyViolation` | enum | 2-variant violation type (2590, 2591) |
| `PolicyConfig` | struct | Per-deployment safety configuration |
| `SafetyCheckResult` | enum | `Permitted` / `RequiresConfirm` / `Denied` — pre-check result |
| `TraversalGuard` | struct | Isolated traversal count checker |

---

## Imported from C02 (Authority State)

```rust
// These types are imported from C02; they are not defined here.
use crate::executor::{ExecutionContext, ClaimAuthority};
```

`ExecutionContext` is the typed snapshot of execution state that the executor passes to M039 on every phase transition. `ClaimAuthority` is the authority token that grants permission for `Safety` class operations. M039 consumes these types; it does not define them.

---

## Struct: `PolicyConfig`

```rust
/// Per-deployment safety policy configuration.
///
/// Use `PolicyConfig::default()` for standard production settings.
/// Override individual fields for testing or operator-adjusted deployments.
#[derive(Debug, Clone)]
pub struct PolicyConfig {
    /// When false, traversal guard is disabled (for testing only).
    /// Default: true.
    pub enforce_traversal_guard: bool,
    /// When false, elevation checks are disabled (for testing only).
    /// Default: true.
    pub enforce_elevation_checks: bool,
    /// When false, phase ordering guard is disabled.
    /// Default: true.
    pub enforce_phase_ordering: bool,
    /// Maximum ticks a `ConfirmToken` remains valid after issuance.
    /// Tokens older than this are rejected as stale.
    /// Default: 300 (5 minutes at 1 tick/second).
    pub confirm_token_max_age_ticks: u64,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            enforce_traversal_guard: true,
            enforce_elevation_checks: true,
            enforce_phase_ordering: true,
            confirm_token_max_age_ticks: 300,
        }
    }
}
```

---

## Struct: `SafetyPolicy`

```rust
/// Safety gate enforced before every runbook phase transition.
///
/// All check methods are `&self` — the policy holds no mutable state.
/// Construct once with `SafetyPolicy::new(config)` and share via `Arc<SafetyPolicy>`.
#[derive(Debug, Clone)]
pub struct SafetyPolicy {
    config: PolicyConfig,
}
```

---

## Method Table

| Method | Signature | Notes |
|--------|-----------|-------|
| `new` | `fn(config: PolicyConfig) -> Self` | |
| `default` | `fn() -> Self` | `PolicyConfig::default()` |
| `check` | `fn(&self, runbook: &Runbook, phase: PhaseKind, context: &ExecutionContext) -> Result<(), SafetyViolation>` | Primary gate — must return `Ok(())` for execution to proceed |
| `pre_check` | `fn(&self, runbook: &Runbook, context: &ExecutionContext) -> SafetyCheckResult` | Lightweight check before phase is known — used by CLI to present status |
| `requires_confirm_for_phase` | `fn(&self, runbook: &Runbook, phase: PhaseKind) -> bool` | True for Hard/Safety class Fix phases |
| `traversal_guard` | `fn(&self) -> TraversalGuard` | Returns isolated traversal checker |

---

## `check` Execution Order

```
SafetyPolicy::check(runbook, phase, context)
  │
  ├─ Rule 1: Traversal guard (if enforce_traversal_guard)
  │   if !runbook.idempotent && context.traversal_count >= runbook.max_traversals:
  │       return Err(SafetyViolation::TraversalExceeded { ... })   // code 2590
  │
  ├─ Rule 2: Safety-class elevation for Fix phase (if enforce_elevation_checks)
  │   if phase == PhaseKind::Fix && runbook.safety_class.requires_explicit_confirm():
  │       if context.confirm_token.is_none():
  │           return Err(SafetyViolation::ElevationDenied { reason: "no confirm token" })  // 2591
  │       if confirm_token.age(context.now) > config.confirm_token_max_age_ticks:
  │           return Err(SafetyViolation::ElevationDenied { reason: "stale token" })        // 2591
  │       if confirm_token.outcome != ConfirmOutcome::Approved:
  │           return Err(SafetyViolation::ElevationDenied { reason: "token refused/deferred" }) // 2591
  │
  ├─ Rule 3: ClaimAuthority for Safety class (if enforce_elevation_checks)
  │   if runbook.safety_class == SafetyClass::Safety:
  │       if context.claim_authority.is_none() || !context.claim_authority.is_elevated():
  │           return Err(SafetyViolation::ElevationDenied { reason: "no authority elevation" }) // 2591
  │
  ├─ Rule 4: Phase ordering (if enforce_phase_ordering)
  │   if phase == PhaseKind::Fix && !context.phases_passed.contains(&PhaseKind::Detect):
  │       return Err(SafetyViolation::PolicyViolation {
  │           reason: "Fix phase requires Detect to have passed first" })  // 2590
  │
  └─ Ok(())
```

---

## Enum: `SafetyViolation`

```rust
/// Safety violation types. Error codes 2590-2591.
#[derive(Debug)]
pub enum SafetyViolation {
    /// Code 2590 — The runbook operation exceeds its declared safety class boundary.
    PolicyViolation {
        runbook_id: String,
        phase: String,
        reason: String,
    },
    /// Code 2590 — Traversal count exceeded max_traversals on non-idempotent runbook.
    TraversalExceeded {
        runbook_id: String,
        traversal_count: u32,
        max_traversals: u32,
    },
    /// Code 2591 — Elevation was requested but not granted.
    ElevationDenied {
        runbook_id: String,
        phase: String,
        reason: String,
    },
}
```

`SafetyViolation` implements `ErrorClassifier`:
- `PolicyViolation` → code 2590, severity Critical, retryable=false
- `TraversalExceeded` → code 2590, severity Critical, retryable=false
- `ElevationDenied` → code 2591, severity Critical, retryable=false

All three implement `std::error::Error` and `Display`.

---

## Enum: `SafetyCheckResult`

```rust
/// Pre-check result (before phase is known) — used by CLI for status display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafetyCheckResult {
    /// All known checks pass; execution may proceed.
    Permitted,
    /// The runbook requires human confirmation before the next phase.
    RequiresConfirm { safety_class: SafetyClass },
    /// The runbook is blocked — traversal or authority violation detected.
    Denied { reason: &'static str },
}
```

---

## Struct: `TraversalGuard`

```rust
/// Isolated traversal count checker. Extracted from SafetyPolicy for use
/// in contexts where the full runbook is not available (e.g., CLI status).
#[derive(Debug, Clone)]
pub struct TraversalGuard {
    config: PolicyConfig,
}

impl TraversalGuard {
    /// Returns true when traversal_count has reached or exceeded max_traversals
    /// for a non-idempotent runbook.
    #[must_use]
    pub fn is_exceeded(
        &self,
        idempotent: bool,
        traversal_count: u32,
        max_traversals: u32,
    ) -> bool;

    /// Remaining traversals before the guard triggers.
    /// Returns None for idempotent runbooks (no limit).
    #[must_use]
    pub fn remaining(
        &self,
        idempotent: bool,
        traversal_count: u32,
        max_traversals: u32,
    ) -> Option<u32>;
}
```

---

## Compile-Time Safety: `const fn` Usage

```rust
// Demonstrating that SafetyClass const fn methods are usable in static contexts.
// These assertions are documentation anchors; runtime tests cover the behavior.
const SOFT_NEEDS_ELEVATION: bool = SafetyClass::Soft.requires_elevation();
const HARD_NEEDS_ELEVATION: bool = SafetyClass::Hard.requires_elevation();
const SAFETY_NEEDS_ELEVATION: bool = SafetyClass::Safety.requires_elevation();
// SOFT_NEEDS_ELEVATION == false, HARD_NEEDS_ELEVATION == true, SAFETY_NEEDS_ELEVATION == true
```

These `const fn` properties are defined on `SafetyClass` in M032. M039 uses them in `check` for clarity and compile-time verifiability.

---

## Design Notes

- `SafetyPolicy::check` is designed to be called by the executor immediately before every phase transition — not just before the Fix phase. The traversal and phase-ordering checks are relevant for all phases; only the elevation check is phase-specific.
- `PolicyConfig::confirm_token_max_age_ticks` defaults to 300. This assumes the ticker increments at roughly 1 Hz (1 tick per second), making 300 ticks a 5-minute window. Operators can override this in `PolicyConfig` for slower or faster tick rates.
- M039 does not invoke M035 (`HumanConfirm`). It only checks whether a valid `ConfirmToken` already exists in `ExecutionContext`. M035 is invoked by the executor in response to M039 returning `Err(ElevationDenied { reason: "no confirm token" })`.
- The phase ordering rule (Rule 4) is deliberately limited to `Detect → Fix`. It does not enforce a full DAG ordering of all 5 phases because Framework §17.8 runbooks are linear by definition. If the spec evolves to support non-linear phase graphs, Rule 4 would need to consult the `PhaseAffinityTable`.
- `SafetyPolicy` is not a singleton. Multiple policies with different `PolicyConfig` instances may coexist (e.g., strict policy for production paths, permissive policy for test scaffolds). The executor holds a reference to the policy active for its context.

---

## Cluster Invariants (this module)

- `SafetyPolicy::check` returns `Ok(())` for `SafetyClass::Soft` runbooks with `idempotent: true` and `traversal_count < max_traversals` regardless of the phase, when no other rule fires.
- `SafetyPolicy::check` returns `Err(ElevationDenied)` (code 2591) for any `SafetyClass::Hard` or `SafetyClass::Safety` runbook attempting the `Fix` phase without a valid, non-stale, Approved `ConfirmToken`.
- `PolicyConfig::enforce_traversal_guard = false` is only permitted in test contexts. The production `PolicyConfig::default()` always sets this to `true`.
- INV-C06-04: `SafetyPolicy::check` returns `Ok(())` for `Soft` class runbooks unconditionally on the elevation dimension (Rules 2 and 3 do not fire for `Soft`). This is enforced by `SafetyClass::requires_elevation() == false` for `Soft`.
- INV-C06-07: A runbook with `idempotent: false` and `max_traversals: 1` is blocked by `SafetyPolicy::check` after the first traversal. The test `non_idempotent_max_one_blocks_on_second_traversal` enforces this.

---

*M039 Runbook Safety Policy | C06 Runbook Semantics | Habitat Loop Engine | 2026-05-10*
