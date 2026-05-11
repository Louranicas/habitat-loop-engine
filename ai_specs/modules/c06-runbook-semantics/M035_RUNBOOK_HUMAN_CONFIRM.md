# M035 Runbook Human Confirm — `crates/hle-runbook/src/human_confirm.rs`

> **Layer:** L07 | **Cluster:** C06 Runbook Semantics | **Error Codes:** 2550-2560
> **Role:** `AwaitingHuman` semantics — typed confirmation elicitation without executor self-authorization.
> **LOC target:** ~280 | **Test target:** ≥50

---

## Purpose

M035 defines the boundary between the workflow engine and a human operator during a confirmation-required phase. When M039 `SafetyPolicy::check` determines that a `Hard` or `Safety` class runbook needs explicit approval, the executor invokes the `HumanConfirm` trait from this module. The confirmation contract is strict: this module **elicits** a decision, it never **grants** one. The returned `ConfirmToken` is an unforgeable receipt that the executor checks before proceeding; the token cannot be manufactured by the runbook or the executor itself.

The `AwaitingHuman` use-pattern (`UP_RUNBOOK_AWAITING_HUMAN`) governs which states are reachable:
- `ready_for_review` — all evidence gathered, human decision needed
- `blocked_on_input` — required parameters or scope are missing
- `blocked_on_verifier` — verifier evidence failed or incomplete
- `waiver_requested` — explicit human waiver needed

---

## Types at a Glance

| Type | Kind | Notes |
|------|------|-------|
| `HumanConfirm` | trait | Primary abstraction; implemented by all concrete impls |
| `ConfirmToken` | struct | Unforgeable receipt produced by a successful confirmation |
| `ConfirmRequest` | struct | All context needed to present the decision to a human |
| `ConfirmOutcome` | enum | `Approved` / `Refused` / `Deferred` |
| `AwaitingHumanState` | enum | Current state from UP_RUNBOOK_AWAITING_HUMAN |
| `CliHumanConfirm` | struct | Concrete impl: interactive CLI prompt |
| `NoOpHumanConfirm` | struct | Concrete impl: always approves — for tests only |
| `ConfirmError` | enum | 2-variant error (Timeout, Refused) |

---

## Trait: `HumanConfirm`

```rust
/// Elicits an explicit decision from a human operator before a critical phase runs.
///
/// # Contract
/// - Implementors must NOT auto-approve. A response must come from a human signal.
/// - `NoOpHumanConfirm` is the only auto-approve impl; it is gated behind `#[cfg(test)]`.
/// - The returned `ConfirmToken` must embed the outcome; callers check `token.outcome`.
/// - This trait does not write to the verifier receipt store. Token passing to the
///   executor is the caller's responsibility.
pub trait HumanConfirm: Send + Sync {
    /// Present the confirmation request and wait for a human decision.
    ///
    /// Returns `Ok(ConfirmToken)` for both `Approved` and `Refused` outcomes.
    /// Returns `Err(ConfirmError::Timeout)` if the deadline elapses without response.
    fn confirm(
        &self,
        request: &ConfirmRequest,
    ) -> Result<ConfirmToken, ConfirmError>;

    /// Returns the awaiting-human state this impl represents at call time.
    #[must_use]
    fn awaiting_state(&self, request: &ConfirmRequest) -> AwaitingHumanState;
}
```

---

## Struct: `ConfirmRequest`

```rust
/// All context needed to present a confirmation decision to a human operator.
#[derive(Debug, Clone)]
pub struct ConfirmRequest {
    /// Runbook identifier being executed.
    pub runbook_id: RunbookId,
    /// Phase for which confirmation is requested.
    pub phase_kind: PhaseKind,
    /// Safety class of the runbook (determines urgency framing).
    pub safety_class: SafetyClass,
    /// Human-readable description of the action requiring approval.
    pub action_description: String,
    /// Evidence gathered so far (phase detect/block output).
    pub gathered_evidence: Vec<EvidenceLocator>,
    /// Maximum time to wait for a response before returning `ConfirmError::Timeout`.
    pub deadline: std::time::Duration,
    /// Agent or operator identity requesting confirmation (from ExecutionContext).
    pub requesting_agent: AgentId,
}
```

| Method | Signature | Notes |
|--------|-----------|-------|
| `builder` | `fn(runbook_id: RunbookId, phase_kind: PhaseKind) -> ConfirmRequestBuilder` | Entry point |
| `is_safety_critical` | `fn(&self) -> bool` | `safety_class == SafetyClass::Safety` |
| `has_evidence` | `fn(&self) -> bool` | `!gathered_evidence.is_empty()` |

---

## Struct: `ConfirmToken`

```rust
/// Unforgeable receipt of a human confirmation decision.
///
/// # Invariants
/// - `token_id` is a UUID-v4 string generated at construction time.
/// - `issued_at` is a `Timestamp` from the foundation layer.
/// - `outcome` is sealed at construction; no mutation method exists.
/// - Tokens with `outcome == ConfirmOutcome::Refused` must NOT be used to proceed.
///   The executor is responsible for checking `token.is_approved()`.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfirmToken {
    pub token_id: String,
    pub runbook_id: RunbookId,
    pub phase_kind: PhaseKind,
    pub outcome: ConfirmOutcome,
    pub issued_at: Timestamp,
    pub operator_note: Option<String>,
}
```

| Method | Signature | Notes |
|--------|-----------|-------|
| `is_approved` | `fn(&self) -> bool` | `outcome == ConfirmOutcome::Approved` |
| `is_refused` | `fn(&self) -> bool` | `outcome == ConfirmOutcome::Refused` |
| `is_deferred` | `fn(&self) -> bool` | `outcome == ConfirmOutcome::Deferred` |
| `age` | `fn(&self, now: Timestamp) -> u64` | Ticks since issuance |

**Traits:** `Display` ("ConfirmToken(runbook=X, phase=Y, outcome=Approved, id=Z)")

---

## Enum: `ConfirmOutcome`

```rust
/// The human operator's decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmOutcome {
    /// Operator approved; executor may proceed to the next phase.
    Approved,
    /// Operator explicitly refused; executor must halt the runbook.
    Refused,
    /// Operator deferred; executor parks the runbook in `AwaitingHuman` state.
    Deferred,
}
```

---

## Enum: `AwaitingHumanState`

```rust
/// Current awaiting-human state per UP_RUNBOOK_AWAITING_HUMAN.
///
/// Used by the executor to emit structured status and by M027 (blockers_store)
/// to persist the blocked runbook context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AwaitingHumanState {
    /// All evidence gathered; human decision required to proceed.
    ReadyForReview,
    /// Required parameters, authorization, or scope are absent.
    BlockedOnInput,
    /// Verifier evidence failed or is incomplete; human must resolve.
    BlockedOnVerifier,
    /// An explicit human waiver is needed before proceeding.
    WaiverRequested,
}
```

| Method | Signature | Notes |
|--------|-----------|-------|
| `as_str` | `const fn(&self) -> &'static str` | "ready_for_review" / ... |
| `is_blocker` | `const fn(&self) -> bool` | All variants except `ReadyForReview` return true |

---

## Enum: `ConfirmError`

```rust
/// Errors produced during human confirmation. Error codes 2550-2560.
#[derive(Debug)]
pub enum ConfirmError {
    /// Code 2550 — No response received within the deadline.
    Timeout {
        deadline: std::time::Duration,
        runbook_id: String,
        phase: String,
    },
    /// Code 2560 — Operator explicitly declined (distinct from Refused outcome token).
    /// Used when the confirmation channel itself is rejected (e.g., session terminated).
    ChannelRefused { reason: String },
}
```

`ConfirmError` implements `ErrorClassifier`:
- `Timeout` → code 2550, severity Medium, retryable=true, transient=true
- `ChannelRefused` → code 2560, severity High, retryable=false

---

## Struct: `CliHumanConfirm`

```rust
/// Interactive CLI confirmation impl using stdin/stdout.
///
/// Presents a formatted prompt, reads a single line, and interprets:
/// - "y" / "yes" / "approve" → `ConfirmOutcome::Approved`
/// - "n" / "no" / "refuse"  → `ConfirmOutcome::Refused`
/// - "d" / "defer"          → `ConfirmOutcome::Deferred`
/// - (timeout)              → `Err(ConfirmError::Timeout)`
///
/// # Safety-class framing
/// For `SafetyClass::Safety` runbooks, the prompt includes a mandatory
/// acknowledgment string that the operator must type verbatim.
#[derive(Debug, Default)]
pub struct CliHumanConfirm;
```

`CliHumanConfirm` implements `HumanConfirm`. It reads from stdin with a deadline enforced via `std::sync::mpsc` channel + thread.

---

## Struct: `NoOpHumanConfirm`

```rust
/// Test-only human confirm impl that always approves.
///
/// Available only in `#[cfg(test)]` or when the `test-utils` feature is enabled.
/// Must not appear in production code paths. Clippy lint `unused_qualifications`
/// is configured in `Cargo.toml` to reject `NoOpHumanConfirm` outside test modules.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug, Default)]
pub struct NoOpHumanConfirm;
```

---

## Design Notes

- `HumanConfirm::confirm` signature uses `&ConfirmRequest` (not owned) because the executor may need to call `confirm` multiple times if the operator defers and the runbook is resumed later.
- `ConfirmToken` is not `Copy` to prevent accidental reuse. The executor should consume the token (move semantics) when making the proceed/halt decision.
- The `AwaitingHumanState` enum is the typed representation of the five UP-005 states. It is produced by `awaiting_state` on the trait, not by parsing strings, ensuring the executor always has a well-typed state.
- `CliHumanConfirm` uses a thread-per-confirmation to avoid blocking the async runtime. The thread is spawned with `std::thread::Builder::new().name("hle-human-confirm")`. Deadline enforcement uses `mpsc::channel::recv_timeout`.
- `ConfirmToken::token_id` is generated by `uuid::Uuid::new_v4().to_string()`. The `uuid` crate is the only new dependency introduced by this module (feature: `v4`).

---

## Cluster Invariants (this module)

- `HumanConfirm::confirm` must never return `Ok(token)` where `token.outcome == Approved` without reading a positive human signal. `NoOpHumanConfirm` is the only exception and is test-gated.
- `ConfirmToken` has no `set_outcome` or mutation method. Once issued, the outcome is sealed.
- `ConfirmError::Timeout` is always retryable (`ErrorClassifier::is_retryable() == true`).
- `AwaitingHumanState::as_str()` values must match the UP_RUNBOOK_AWAITING_HUMAN predicate identifiers exactly.

---

*M035 Runbook Human Confirm | C06 Runbook Semantics | Habitat Loop Engine | 2026-05-10*
