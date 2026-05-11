# M014 claim_authority_verifier — claim_authority_verifier.rs

> **File:** `crates/hle-verifier/src/claim_authority_verifier.rs` | **LOC:** ~430 | **Tests:** ~60
> **Role:** adversarial check against executor self-certification — sole issuer of `ClaimAuthority<Final>`

---

## Types at a Glance

| Type | Kind | Copy | Hash | Const | Purpose |
|---|---|---|---|---|---|
| `ClaimAuthorityVerifier` | struct | No | No | No | Stateful adversarial consumer of `ExecutorEvent` streams |
| `VerifierVerdict` | enum | No | No | No | Output of a verification decision: `Pass`, `Blocked`, or `Rejected` |
| `VerifierReceipt` | struct | No | No | No | Binding record produced on `VerifierVerdict::Pass`; carries `ClaimAuthority<Final>` |
| `RejectionReason` | enum | Yes | Yes | Yes | Structured reason for a `VerifierVerdict::Rejected` verdict |
| `EventLog` | struct | No | No | No | Internal per-workflow event history tracked by the verifier |

---

## ClaimAuthorityVerifier

```rust
/// Adversarial verifier that watches `ExecutorEvent` streams and is the sole
/// producer of `ClaimAuthority<Final>`.
///
/// # Authority boundary
/// This struct lives in `crates/hle-verifier`.  `ClaimAuthority<Final>` and its
/// constructor `ClaimAuthority::<Final>::finalize(…)` are both `pub(crate)` here.
/// The executor crate cannot import or construct either.  This is the compile-time
/// enforcement of HLE-UP-001 and the structural defence against HLE-SP-001
/// (`FP_SELF_CERTIFICATION`).
///
/// # Operation model
/// The verifier is a passive consumer.  It does not call into the executor.
/// It receives `ExecutorEvent` values (via `observe`) and makes a binding decision
/// only when `evaluate` is called with a `ClaimAuthority<Provisional>` from the
/// executor plus a receipt of the artifacts the executor claims to have produced.
///
/// # Concurrency
/// Internal event log is behind `parking_lot::RwLock`.  `observe` acquires a write
/// lock; `evaluate` acquires a read lock first then briefly upgrades.
#[derive(Debug)]
pub struct ClaimAuthorityVerifier {
    inner: parking_lot::RwLock<VerifierInner>,
}
```

### Methods

| Method | Signature | Notes |
|---|---|---|
| `new` | `#[must_use] pub fn new() -> Self` | Empty event log; no workflows tracked yet |
| `observe` | `pub fn observe(&self, event: ExecutorEvent) -> Result<(), AuthorityError>` | Appends event to per-workflow log; returns `StaleEvent` if sequence number regresses |
| `evaluate` | `pub fn evaluate(&self, provisional: ClaimAuthority<Provisional>, artifact_hash: &str) -> Result<VerifierVerdict, AuthorityError>` | Checks event log against claim; returns verdict |
| `workflow_ids` | `#[must_use] pub fn workflow_ids(&self) -> Vec<String>` | Returns all workflow IDs currently tracked |
| `event_count` | `#[must_use] pub fn event_count(&self, workflow_id: &str) -> usize` | Number of events observed for a workflow |
| `last_sequence` | `#[must_use] pub fn last_sequence(&self, workflow_id: &str) -> Option<EventSequence>` | Highest sequence number observed |

---

## evaluate — Core Logic (spec-level pseudocode)

```
fn evaluate(
    &self,
    provisional: ClaimAuthority<Provisional>,
    artifact_hash: &str,
) -> Result<VerifierVerdict, AuthorityError>
```

**Steps performed (in order):**

1. Acquire read lock on `inner`.
2. Look up `EventLog` for `provisional.workflow_id()` — return
   `AuthorityError::UnknownWorkflow` if absent.
3. Check that the event log contains at least one `ExecutorEvent::DraftPassed` for
   `provisional.step_id()` — return `RejectionReason::NoDraftPassedEvent` if absent.
4. Extract the `evidence_hash` from the matching `DraftPassed` event.
5. Compare `evidence_hash == artifact_hash` — return
   `RejectionReason::EvidenceHashMismatch` if different.
6. Verify event sequence is contiguous (no gaps from sequence 0 through last observed) —
   return `RejectionReason::SequenceGap` if discontinuous.
7. Verify no `DraftFailed` or `RollbackStarted` event follows the `DraftPassed` event —
   return `RejectionReason::LaterFailureObserved` if present.
8. Check `provisional.class()` — if `AuthorityClass::NegativeControl`, return
   `VerifierVerdict::Rejected(RejectionReason::NegativeControlMustFail)`.
9. If `provisional.class()` is `AuthorityClass::HumanRequired` and no
   `HumanGateReached` event is present, return
   `RejectionReason::HumanGateMissing`.
10. Advance authority: `ClaimAuthority::<Verified>::verify(provisional)` then
    `ClaimAuthority::<Final>::finalize(verified)`.
11. Construct `VerifierReceipt` embedding the `ClaimAuthority<Final>`.
12. Return `VerifierVerdict::Pass(receipt)`.

---

## VerifierVerdict

```rust
/// Binding result of a `ClaimAuthorityVerifier::evaluate` call.
#[derive(Debug)]
#[must_use]
pub enum VerifierVerdict {
    /// The claim is accepted.  The receipt carries a `ClaimAuthority<Final>`
    /// that is the authoritative PASS token for this step.
    Pass(VerifierReceipt),

    /// The step requires further human input before a PASS verdict is possible.
    /// The verifier has observed a `HumanGateReached` event but no subsequent
    /// `DraftPassed` event with a matching step id.
    Blocked {
        workflow_id: String,
        step_id: String,
        reason: String,
    },

    /// The claim is rejected.  No `ClaimAuthority<Final>` is issued.
    /// The structured `RejectionReason` enables the false-pass auditor (M020)
    /// to classify the rejection without string parsing.
    Rejected {
        workflow_id: String,
        step_id: String,
        reason: RejectionReason,
    },
}
```

| Method | Signature | Notes |
|---|---|---|
| `is_pass` | `#[must_use] pub fn is_pass(&self) -> bool` | |
| `is_blocked` | `#[must_use] pub fn is_blocked(&self) -> bool` | |
| `is_rejected` | `#[must_use] pub fn is_rejected(&self) -> bool` | |
| `into_receipt` | `pub fn into_receipt(self) -> Option<VerifierReceipt>` | Consumes self; returns Some only on Pass |

---

## VerifierReceipt

```rust
/// Binding proof that the verifier accepted an executor claim.
///
/// Contains a `ClaimAuthority<Final>` that is `pub(crate)` to this crate —
/// downstream consumers receive the receipt and extract only the `workflow_id`,
/// `step_id`, and `verdict_string` via public accessors.  The `Final` token
/// itself is consumed internally when writing to the persistence layer.
#[derive(Debug)]
pub struct VerifierReceipt {
    workflow_id: String,
    step_id: String,
    artifact_hash: String,
    // pub(crate) — only verifier internals and persistence writers may move this
    authority: ClaimAuthority<Final>,
}
```

| Method | Signature | Notes |
|---|---|---|
| `workflow_id` | `#[must_use] pub fn workflow_id(&self) -> &str` | |
| `step_id` | `#[must_use] pub fn step_id(&self) -> &str` | |
| `artifact_hash` | `#[must_use] pub fn artifact_hash(&self) -> &str` | |
| `verdict_string` | `#[must_use] pub fn verdict_string(&self) -> &'static str` | Always `"PASS"` |
| `consume_authority` | `pub(crate) fn consume_authority(self) -> (String, String, AuthorityClass)` | Destructures `ClaimAuthority<Final>` for persistence; consumes self |

---

## RejectionReason

```rust
/// Structured reason for a `VerifierVerdict::Rejected`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RejectionReason {
    /// No `DraftPassed` event in the observed log for this step.
    NoDraftPassedEvent,
    /// The evidence hash in the event does not match the supplied artifact hash.
    EvidenceHashMismatch,
    /// Event sequence numbers are not contiguous starting from 0.
    SequenceGap,
    /// A failure or rollback event follows the `DraftPassed` in the log.
    LaterFailureObserved,
    /// The claim is a `NegativeControl` fixture; it must not receive PASS.
    NegativeControlMustFail,
    /// A `HumanRequired` step lacks a `HumanGateReached` event.
    HumanGateMissing,
    /// The executor attempted to claim `Final` authority without going through
    /// the verifier.  This is the `FP_SELF_CERTIFICATION` (HLE-SP-001) trigger.
    SelfCertificationAttempt,
}
```

| Method | Signature |
|---|---|
| `as_str` | `#[must_use] pub const fn as_str(self) -> &'static str` |
| `is_self_certification` | `#[must_use] pub const fn is_self_certification(self) -> bool` |
| `error_code` | `#[must_use] pub const fn error_code(self) -> u16` — range 2100–2199 |

**Traits:** `Display`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`

---

## Design Notes

- `ClaimAuthorityVerifier` is an **event consumer only**. It has no method that calls
  back into executor code. The only coupling is the `ExecutorEvent` type defined in
  `hle-executor` — but `hle-verifier` imports it as a data type, not a callable service.
  This preserves the one-way dependency arrow: executor → (emits events) → verifier.
- The `VerifierReceipt` deliberately hides the `ClaimAuthority<Final>` behind `pub(crate)`
  so that code outside `hle-verifier` cannot observe or move the final token. The only
  operation available on the receipt to external callers is reading string accessors.
  Persistence writers inside the verifier crate call `consume_authority` to destructure it.
- `RejectionReason::SelfCertificationAttempt` is never returned by normal code paths in
  this spec — it is the variant that a future dynamic detector (part of C04 anti-pattern
  intelligence) would inject if it observed a receipt claiming `ClaimAuthority<Final>` whose
  origin was not this module. The enum variant exists to make the taxonomy complete.
- `VerifierVerdict` is `#[must_use]`. Callers cannot call `evaluate` and ignore the result
  without a compiler warning under `clippy::pedantic`.
- The `observe` / `evaluate` split maps cleanly to streaming architectures: the verifier
  can consume a channel of `ExecutorEvent` values continuously, then `evaluate` is called
  once per step claim — it never needs to poll the executor for state.
- `parking_lot::RwLock` is used rather than `std::sync::RwLock` for consistent,
  non-poisoning semantics. Read locks during `evaluate` do not block concurrent `observe`
  calls from other workflow runs.

---

## Cluster Invariants

This module enforces C02 Invariants I5 and I6:

> **I5 (Verifier Sole Final Issuer):** `ClaimAuthority::<Final>::finalize(…)` is called only
> inside `claim_authority_verifier.rs`.  No other file in the workspace imports or constructs
> `Final`.  This is verifiable with `rg 'ClaimAuthority::<Final>'` from the workspace root.
>
> **I6 (Evidence Hash Independence):** The verifier recomputes or receives the artifact hash
> independently of the executor's self-report.  An executor that writes `evidence_hash: "PASS"`
> into its `DraftPassed` event and supplies the same string as `artifact_hash` to `evaluate`
> will be caught by step 5 in the evaluation logic, because the verifier holds the hash
> computed from the actual artifact bytes, not from the event.

See also:
- `../../ai_docs/use_patterns/UP_EXECUTOR_VERIFIER_SPLIT.md` (HLE-UP-001)
- `../../ai_docs/anti_patterns/FP_FALSE_PASS_CLASSES.md` (HLE-SP-001)
- `crates/substrate-verify/src/lib.rs` — `verify_step` is the substrate-layer authority
  gate that M014 builds on top of.

---

*M014 CLAIM_AUTHORITY_VERIFIER Spec v1.0 | 2026-05-10*
