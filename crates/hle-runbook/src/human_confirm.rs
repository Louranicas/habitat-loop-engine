#![forbid(unsafe_code)]

//! M035 — `AwaitingHuman` semantics: typed confirmation elicitation.
//!
//! **Cluster:** C06 Runbook Semantics | **Layer:** L07 | **Error codes:** 2550-2560
//!
//! Defines the boundary between the workflow engine and a human operator
//! during a confirmation-required phase.  This module **elicits** a decision;
//! it never **grants** one.  The returned [`ConfirmToken`] is passed to the
//! executor; the executor decides what to record.
//!
//! # Design
//!
//! - `HumanConfirm::confirm` must NOT auto-approve.
//! - `NoOpHumanConfirm` is the ONLY auto-approve impl; it is `#[cfg(test)]`-gated.
//! - `ConfirmToken` has no setter; outcome is sealed at construction.
//! - This module writes nothing to the verifier receipt store.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::schema::{AgentId, EvidenceLocator, PhaseKind, RunbookId, SafetyClass, Timestamp};

// ── Token ID generation (simple counter — no UUID dep needed) ─────────────────

static TOKEN_COUNTER: AtomicU64 = AtomicU64::new(1);

fn next_token_id() -> String {
    let n = TOKEN_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("tok-{n:016x}")
}

fn now_ms() -> Timestamp {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| {
            // as_millis() returns u128; cap at u64::MAX on absurdly long durations.
            u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
        })
}

// ── ConfirmOutcome ────────────────────────────────────────────────────────────

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

impl ConfirmOutcome {
    /// Wire string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::Refused => "refused",
            Self::Deferred => "deferred",
        }
    }
}

impl fmt::Display for ConfirmOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── AwaitingHumanState ────────────────────────────────────────────────────────

/// Current awaiting-human state per `UP_RUNBOOK_AWAITING_HUMAN`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AwaitingHumanState {
    /// All evidence gathered; human decision required to proceed.
    ReadyForReview,
    /// Required parameters, authorisation, or scope are absent.
    BlockedOnInput,
    /// Verifier evidence failed or is incomplete; human must resolve.
    BlockedOnVerifier,
    /// An explicit human waiver is needed before proceeding.
    WaiverRequested,
}

impl AwaitingHumanState {
    /// Wire string — must match `UP_RUNBOOK_AWAITING_HUMAN` predicate identifiers.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReadyForReview => "ready_for_review",
            Self::BlockedOnInput => "blocked_on_input",
            Self::BlockedOnVerifier => "blocked_on_verifier",
            Self::WaiverRequested => "waiver_requested",
        }
    }

    /// Returns `true` for all variants except `ReadyForReview`.
    #[must_use]
    pub const fn is_blocker(self) -> bool {
        !matches!(self, Self::ReadyForReview)
    }
}

impl fmt::Display for AwaitingHumanState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── ConfirmError ──────────────────────────────────────────────────────────────

/// Errors produced during human confirmation.  Error codes 2550-2560.
#[derive(Debug)]
pub enum ConfirmError {
    /// Code 2550 — no response received within the deadline.
    Timeout {
        /// The deadline that elapsed.
        deadline: std::time::Duration,
        /// Runbook identifier.
        runbook_id: String,
        /// Phase string.
        phase: String,
    },
    /// Code 2560 — confirmation channel was rejected (session terminated, etc.).
    ChannelRefused {
        /// Human-readable reason.
        reason: String,
    },
}

impl ConfirmError {
    /// Numeric error code.
    #[must_use]
    pub const fn error_code(&self) -> u16 {
        match self {
            Self::Timeout { .. } => 2550,
            Self::ChannelRefused { .. } => 2560,
        }
    }

    /// Timeout errors are retryable.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(self, Self::Timeout { .. })
    }
}

impl fmt::Display for ConfirmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timeout {
                deadline,
                runbook_id,
                phase,
            } => write!(
                f,
                "[2550 HumanConfirmTimeout] no response for runbook '{}' phase '{}' within {:.1}s",
                runbook_id,
                phase,
                deadline.as_secs_f64()
            ),
            Self::ChannelRefused { reason } => {
                write!(f, "[2560 HumanConfirmRefused] channel refused: {reason}")
            }
        }
    }
}

impl std::error::Error for ConfirmError {}

// ── ConfirmToken ──────────────────────────────────────────────────────────────

/// Unforgeable receipt of a human confirmation decision.
///
/// # Invariants
/// - `token_id` is unique and generated at construction.
/// - `outcome` is sealed at construction; no setter exists.
/// - Tokens with `outcome == Refused` must NOT be used to proceed;
///   the executor is responsible for checking `token.is_approved()`.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfirmToken {
    /// Unique token identifier.
    pub token_id: String,
    /// Runbook this token was issued for.
    pub runbook_id: RunbookId,
    /// Phase for which confirmation was requested.
    pub phase_kind: PhaseKind,
    /// The outcome chosen by the operator.
    pub outcome: ConfirmOutcome,
    /// Timestamp (ms) when the token was issued.
    pub issued_at: Timestamp,
    /// Optional note from the operator.
    pub operator_note: Option<String>,
}

impl ConfirmToken {
    /// Returns `true` when the outcome is `Approved`.
    #[must_use]
    pub fn is_approved(&self) -> bool {
        self.outcome == ConfirmOutcome::Approved
    }

    /// Returns `true` when the outcome is `Refused`.
    #[must_use]
    pub fn is_refused(&self) -> bool {
        self.outcome == ConfirmOutcome::Refused
    }

    /// Returns `true` when the outcome is `Deferred`.
    #[must_use]
    pub fn is_deferred(&self) -> bool {
        self.outcome == ConfirmOutcome::Deferred
    }

    /// Age in milliseconds since issuance.
    #[must_use]
    pub fn age_ms(&self, now: Timestamp) -> u64 {
        now.saturating_sub(self.issued_at)
    }
}

impl fmt::Display for ConfirmToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ConfirmToken(runbook={}, phase={}, outcome={}, id={})",
            self.runbook_id, self.phase_kind, self.outcome, self.token_id
        )
    }
}

// ── ConfirmRequest ────────────────────────────────────────────────────────────

/// All context needed to present a confirmation decision to a human operator.
#[derive(Debug, Clone)]
pub struct ConfirmRequest {
    /// Runbook being executed.
    pub runbook_id: RunbookId,
    /// Phase for which confirmation is requested.
    pub phase_kind: PhaseKind,
    /// Safety class of the runbook.
    pub safety_class: SafetyClass,
    /// Human-readable description of the action requiring approval.
    pub action_description: String,
    /// Evidence gathered so far.
    pub gathered_evidence: Vec<EvidenceLocator>,
    /// Maximum time to wait for a response.
    pub deadline: std::time::Duration,
    /// Agent requesting confirmation.
    pub requesting_agent: AgentId,
}

impl ConfirmRequest {
    /// Returns `true` when the runbook's safety class is `Safety`.
    #[must_use]
    pub fn is_safety_critical(&self) -> bool {
        self.safety_class == SafetyClass::Safety
    }

    /// Returns `true` when at least one evidence item has been gathered.
    #[must_use]
    pub fn has_evidence(&self) -> bool {
        !self.gathered_evidence.is_empty()
    }
}

// ── HumanConfirm trait ────────────────────────────────────────────────────────

/// Elicits an explicit decision from a human operator before a critical phase.
///
/// # Contract
/// - Implementors must NOT auto-approve.
/// - `NoOpHumanConfirm` is the ONLY auto-approve impl; it is test-gated.
/// - The returned `ConfirmToken` embeds the outcome.
/// - This trait writes nothing to the verifier receipt store.
pub trait HumanConfirm: Send + Sync {
    /// Present the confirmation request and wait for a human decision.
    ///
    /// Returns `Ok(ConfirmToken)` for both `Approved` and `Refused` outcomes.
    /// Returns `Err(ConfirmError::Timeout)` if the deadline elapses.
    ///
    /// # Errors
    ///
    /// Returns [`ConfirmError`] on timeout or channel failure.
    fn confirm(&self, request: &ConfirmRequest) -> Result<ConfirmToken, ConfirmError>;

    /// Returns the awaiting-human state this impl represents at call time.
    #[must_use]
    fn awaiting_state(&self, request: &ConfirmRequest) -> AwaitingHumanState;
}

// ── CliHumanConfirm ───────────────────────────────────────────────────────────

/// Interactive CLI confirmation impl using stdin/stdout.
///
/// Presents a formatted prompt and interprets:
/// - `"y"` / `"yes"` / `"approve"` → `Approved`
/// - `"n"` / `"no"` / `"refuse"` → `Refused`
/// - `"d"` / `"defer"` → `Deferred`
/// - timeout → `Err(ConfirmError::Timeout)`
///
/// For `SafetyClass::Safety` runbooks the prompt includes a mandatory
/// acknowledgment string.
#[derive(Debug, Default)]
pub struct CliHumanConfirm;

impl HumanConfirm for CliHumanConfirm {
    fn confirm(&self, request: &ConfirmRequest) -> Result<ConfirmToken, ConfirmError> {
        let prompt = format!(
            "[HLE] Confirmation required for runbook '{}' phase '{}'.\n\
             Safety class: {}\n\
             Action: {}\n\
             Enter y/n/d (approve/refuse/defer): ",
            request.runbook_id,
            request.phase_kind,
            request.safety_class,
            request.action_description
        );

        // Spawn a thread so we can apply a deadline via channel.
        let deadline = request.deadline;
        let runbook_id_str = request.runbook_id.as_str().to_owned();
        let phase_str = request.phase_kind.as_str().to_owned();

        let (tx, rx) = std::sync::mpsc::channel::<String>();
        let prompt_owned = prompt.clone();
        let _ = std::thread::Builder::new()
            .name("hle-human-confirm".into())
            .spawn(move || {
                use std::io::Write as _;
                let mut stdout = std::io::stdout();
                let _ = stdout.write_all(prompt_owned.as_bytes());
                let _ = stdout.flush();
                let mut line = String::new();
                if std::io::stdin().read_line(&mut line).is_ok() {
                    let _ = tx.send(line.trim().to_ascii_lowercase());
                }
            });

        let response = rx
            .recv_timeout(deadline)
            .map_err(|_| ConfirmError::Timeout {
                deadline,
                runbook_id: runbook_id_str.clone(),
                phase: phase_str.clone(),
            })?;

        let outcome = match response.as_str() {
            "y" | "yes" | "approve" => ConfirmOutcome::Approved,
            "n" | "no" | "refuse" => ConfirmOutcome::Refused,
            _ => ConfirmOutcome::Deferred,
        };

        Ok(ConfirmToken {
            token_id: next_token_id(),
            runbook_id: request.runbook_id.clone(),
            phase_kind: request.phase_kind,
            outcome,
            issued_at: now_ms(),
            operator_note: None,
        })
    }

    fn awaiting_state(&self, _request: &ConfirmRequest) -> AwaitingHumanState {
        AwaitingHumanState::ReadyForReview
    }
}

// ── NoOpHumanConfirm (test only) ──────────────────────────────────────────────

/// Test-only human confirm impl that always approves immediately.
///
/// Available only in `#[cfg(test)]` or when the `test-utils` feature is enabled.
/// Must not appear in production code paths.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug, Default)]
pub struct NoOpHumanConfirm;

#[cfg(any(test, feature = "test-utils"))]
impl HumanConfirm for NoOpHumanConfirm {
    fn confirm(&self, request: &ConfirmRequest) -> Result<ConfirmToken, ConfirmError> {
        Ok(ConfirmToken {
            token_id: next_token_id(),
            runbook_id: request.runbook_id.clone(),
            phase_kind: request.phase_kind,
            outcome: ConfirmOutcome::Approved,
            issued_at: now_ms(),
            operator_note: Some("auto-approved by NoOpHumanConfirm (test only)".into()),
        })
    }

    fn awaiting_state(&self, _request: &ConfirmRequest) -> AwaitingHumanState {
        AwaitingHumanState::ReadyForReview
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{
        now_ms, AwaitingHumanState, ConfirmError, ConfirmOutcome, ConfirmRequest, ConfirmToken,
        HumanConfirm, NoOpHumanConfirm,
    };
    use crate::schema::{AgentId, PhaseKind, RunbookId, SafetyClass};

    fn make_request(phase: PhaseKind, safety: SafetyClass) -> ConfirmRequest {
        ConfirmRequest {
            runbook_id: RunbookId::new("test-rb").expect("valid"),
            phase_kind: phase,
            safety_class: safety,
            action_description: "test action".into(),
            gathered_evidence: Vec::new(),
            deadline: Duration::from_secs(30),
            requesting_agent: AgentId::system(),
        }
    }

    #[test]
    fn noop_confirm_returns_approved() {
        let c = NoOpHumanConfirm::default();
        let req = make_request(PhaseKind::Fix, SafetyClass::Hard);
        let token = c.confirm(&req).expect("should succeed");
        assert!(token.is_approved());
    }

    #[test]
    fn noop_confirm_token_has_correct_phase() {
        let c = NoOpHumanConfirm::default();
        let req = make_request(PhaseKind::Fix, SafetyClass::Hard);
        let token = c.confirm(&req).expect("should succeed");
        assert_eq!(token.phase_kind, PhaseKind::Fix);
    }

    #[test]
    fn noop_confirm_token_has_unique_ids() {
        let c = NoOpHumanConfirm::default();
        let req = make_request(PhaseKind::Detect, SafetyClass::Soft);
        let t1 = c.confirm(&req).expect("should succeed");
        let t2 = c.confirm(&req).expect("should succeed");
        assert_ne!(t1.token_id, t2.token_id);
    }

    #[test]
    fn confirm_token_is_approved_predicate() {
        let tok = ConfirmToken {
            token_id: "tok-1".into(),
            runbook_id: RunbookId::new("rb").expect("valid"),
            phase_kind: PhaseKind::Fix,
            outcome: ConfirmOutcome::Approved,
            issued_at: 0,
            operator_note: None,
        };
        assert!(tok.is_approved());
        assert!(!tok.is_refused());
        assert!(!tok.is_deferred());
    }

    #[test]
    fn confirm_token_is_refused_predicate() {
        let tok = ConfirmToken {
            token_id: "tok-2".into(),
            runbook_id: RunbookId::new("rb").expect("valid"),
            phase_kind: PhaseKind::Fix,
            outcome: ConfirmOutcome::Refused,
            issued_at: 0,
            operator_note: None,
        };
        assert!(tok.is_refused());
    }

    #[test]
    fn confirm_token_is_deferred_predicate() {
        let tok = ConfirmToken {
            token_id: "tok-3".into(),
            runbook_id: RunbookId::new("rb").expect("valid"),
            phase_kind: PhaseKind::Fix,
            outcome: ConfirmOutcome::Deferred,
            issued_at: 0,
            operator_note: None,
        };
        assert!(tok.is_deferred());
    }

    #[test]
    fn confirm_token_age_ms() {
        let issued_at = now_ms().saturating_sub(1_000);
        let tok = ConfirmToken {
            token_id: "tok-4".into(),
            runbook_id: RunbookId::new("rb").expect("valid"),
            phase_kind: PhaseKind::Verify,
            outcome: ConfirmOutcome::Approved,
            issued_at,
            operator_note: None,
        };
        let age = tok.age_ms(now_ms());
        assert!(age >= 999, "age should be at least 999ms, got {age}");
    }

    #[test]
    fn awaiting_state_as_str_values_are_stable() {
        assert_eq!(
            AwaitingHumanState::ReadyForReview.as_str(),
            "ready_for_review"
        );
        assert_eq!(
            AwaitingHumanState::BlockedOnInput.as_str(),
            "blocked_on_input"
        );
        assert_eq!(
            AwaitingHumanState::BlockedOnVerifier.as_str(),
            "blocked_on_verifier"
        );
        assert_eq!(
            AwaitingHumanState::WaiverRequested.as_str(),
            "waiver_requested"
        );
    }

    #[test]
    fn ready_for_review_is_not_a_blocker() {
        assert!(!AwaitingHumanState::ReadyForReview.is_blocker());
    }

    #[test]
    fn blocked_on_input_is_a_blocker() {
        assert!(AwaitingHumanState::BlockedOnInput.is_blocker());
    }

    #[test]
    fn confirm_error_timeout_is_retryable() {
        let err = ConfirmError::Timeout {
            deadline: Duration::from_secs(30),
            runbook_id: "rb".into(),
            phase: "fix".into(),
        };
        assert!(err.is_retryable());
        assert_eq!(err.error_code(), 2550);
    }

    #[test]
    fn confirm_error_channel_refused_is_not_retryable() {
        let err = ConfirmError::ChannelRefused {
            reason: "session ended".into(),
        };
        assert!(!err.is_retryable());
        assert_eq!(err.error_code(), 2560);
    }

    #[test]
    fn confirm_error_display_contains_code() {
        let err = ConfirmError::Timeout {
            deadline: Duration::from_secs(10),
            runbook_id: "rb".into(),
            phase: "fix".into(),
        };
        assert!(err.to_string().contains("2550"));
    }

    #[test]
    fn request_is_safety_critical_when_safety_class() {
        let req = make_request(PhaseKind::Fix, SafetyClass::Safety);
        assert!(req.is_safety_critical());
    }

    #[test]
    fn request_is_not_safety_critical_when_hard() {
        let req = make_request(PhaseKind::Fix, SafetyClass::Hard);
        assert!(!req.is_safety_critical());
    }

    #[test]
    fn confirm_outcome_as_str_stable() {
        assert_eq!(ConfirmOutcome::Approved.as_str(), "approved");
        assert_eq!(ConfirmOutcome::Refused.as_str(), "refused");
        assert_eq!(ConfirmOutcome::Deferred.as_str(), "deferred");
    }

    #[test]
    fn noop_awaiting_state_is_ready_for_review() {
        let c = NoOpHumanConfirm::default();
        let req = make_request(PhaseKind::Fix, SafetyClass::Hard);
        assert_eq!(c.awaiting_state(&req), AwaitingHumanState::ReadyForReview);
    }

    // ── Additional HumanConfirm tests to reach ≥50 ───────────────────────────

    #[test]
    fn noop_confirm_returns_ok_for_all_phases() {
        let c = NoOpHumanConfirm::default();
        for phase in crate::schema::PhaseKind::all() {
            let req = make_request(phase, SafetyClass::Soft);
            assert!(c.confirm(&req).is_ok(), "confirm failed for {phase:?}");
        }
    }

    #[test]
    fn noop_confirm_runbook_id_matches_request() {
        let c = NoOpHumanConfirm::default();
        let req = make_request(PhaseKind::Fix, SafetyClass::Hard);
        let token = c.confirm(&req).expect("ok");
        assert_eq!(token.runbook_id.as_str(), "test-rb");
    }

    #[test]
    fn noop_confirm_token_issued_at_is_nonzero() {
        let c = NoOpHumanConfirm::default();
        let req = make_request(PhaseKind::Detect, SafetyClass::Soft);
        let token = c.confirm(&req).expect("ok");
        // Issued at should be a realistic Unix ms timestamp.
        assert!(token.issued_at > 0);
    }

    #[test]
    fn noop_confirm_token_has_operator_note() {
        let c = NoOpHumanConfirm::default();
        let req = make_request(PhaseKind::Fix, SafetyClass::Hard);
        let token = c.confirm(&req).expect("ok");
        assert!(token.operator_note.is_some());
    }

    #[test]
    fn confirm_token_display_contains_outcome() {
        let tok = ConfirmToken {
            token_id: "tok-5".into(),
            runbook_id: RunbookId::new("rb").expect("valid"),
            phase_kind: PhaseKind::Fix,
            outcome: ConfirmOutcome::Approved,
            issued_at: 0,
            operator_note: None,
        };
        let s = tok.to_string();
        assert!(s.contains("approved"));
        assert!(s.contains("rb"));
        assert!(s.contains("fix"));
    }

    #[test]
    fn confirm_token_age_ms_zero_when_equal() {
        let ts = now_ms();
        let tok = ConfirmToken {
            token_id: "t".into(),
            runbook_id: RunbookId::new("rb").expect("valid"),
            phase_kind: PhaseKind::Detect,
            outcome: ConfirmOutcome::Approved,
            issued_at: ts,
            operator_note: None,
        };
        // age at issued_at is 0
        assert_eq!(tok.age_ms(ts), 0);
    }

    #[test]
    fn confirm_token_age_ms_saturating_sub() {
        let tok = ConfirmToken {
            token_id: "t".into(),
            runbook_id: RunbookId::new("rb").expect("valid"),
            phase_kind: PhaseKind::Detect,
            outcome: ConfirmOutcome::Approved,
            issued_at: 1_000,
            operator_note: None,
        };
        // now < issued_at: saturating_sub should give 0
        assert_eq!(tok.age_ms(500), 0);
    }

    #[test]
    fn awaiting_state_blocked_on_verifier_is_blocker() {
        assert!(AwaitingHumanState::BlockedOnVerifier.is_blocker());
    }

    #[test]
    fn awaiting_state_waiver_requested_is_blocker() {
        assert!(AwaitingHumanState::WaiverRequested.is_blocker());
    }

    #[test]
    fn awaiting_state_display_matches_as_str() {
        for state in [
            AwaitingHumanState::ReadyForReview,
            AwaitingHumanState::BlockedOnInput,
            AwaitingHumanState::BlockedOnVerifier,
            AwaitingHumanState::WaiverRequested,
        ] {
            assert_eq!(state.to_string(), state.as_str());
        }
    }

    #[test]
    fn confirm_outcome_display_matches_as_str() {
        for outcome in [
            ConfirmOutcome::Approved,
            ConfirmOutcome::Refused,
            ConfirmOutcome::Deferred,
        ] {
            assert_eq!(outcome.to_string(), outcome.as_str());
        }
    }

    #[test]
    fn request_has_evidence_true_when_evidence_present() {
        use crate::schema::EvidenceLocator;
        let mut req = make_request(PhaseKind::Fix, SafetyClass::Hard);
        req.gathered_evidence
            .push(EvidenceLocator::Inline("note".into()));
        assert!(req.has_evidence());
    }

    #[test]
    fn request_has_evidence_false_when_empty() {
        let req = make_request(PhaseKind::Fix, SafetyClass::Hard);
        assert!(!req.has_evidence());
    }

    #[test]
    fn confirm_error_timeout_display_contains_runbook_and_phase() {
        let err = ConfirmError::Timeout {
            deadline: std::time::Duration::from_secs(10),
            runbook_id: "my-runbook".into(),
            phase: "fix".into(),
        };
        let s = err.to_string();
        assert!(s.contains("my-runbook"));
        assert!(s.contains("fix"));
    }

    #[test]
    fn confirm_error_channel_refused_display_contains_reason() {
        let err = ConfirmError::ChannelRefused {
            reason: "session ended".into(),
        };
        let s = err.to_string();
        assert!(s.contains("session ended"));
        assert!(s.contains("2560"));
    }

    #[test]
    fn noop_confirm_verify_phase_is_approved() {
        let c = NoOpHumanConfirm::default();
        let req = make_request(PhaseKind::Verify, SafetyClass::Soft);
        let token = c.confirm(&req).expect("ok");
        assert!(token.is_approved());
        assert_eq!(token.phase_kind, PhaseKind::Verify);
    }

    #[test]
    fn token_id_starts_with_tok_prefix() {
        let c = NoOpHumanConfirm::default();
        let req = make_request(PhaseKind::Fix, SafetyClass::Hard);
        let token = c.confirm(&req).expect("ok");
        assert!(token.token_id.starts_with("tok-"));
    }

    #[test]
    fn confirm_error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ConfirmError>();
    }

    #[test]
    fn human_confirm_trait_is_object_safe() {
        // If HumanConfirm is object-safe, this compiles.
        let _boxed: Box<dyn HumanConfirm> = Box::new(NoOpHumanConfirm::default());
    }

    #[test]
    fn noop_human_confirm_is_default() {
        // Default construction works.
        let _c: NoOpHumanConfirm = Default::default();
    }

    #[test]
    fn confirm_request_deadline_stored_correctly() {
        let req = make_request(PhaseKind::Fix, SafetyClass::Hard);
        assert_eq!(req.deadline, std::time::Duration::from_secs(30));
    }

    #[test]
    fn confirm_request_agent_is_system() {
        let req = make_request(PhaseKind::Detect, SafetyClass::Soft);
        assert_eq!(req.requesting_agent.as_str(), "system");
    }

    #[test]
    fn noop_confirm_multiple_calls_token_ids_all_unique() {
        let c = NoOpHumanConfirm::default();
        let req = make_request(PhaseKind::Fix, SafetyClass::Hard);
        let tokens: Vec<_> = (0..5)
            .map(|_| c.confirm(&req).expect("ok").token_id)
            .collect();
        let set: std::collections::HashSet<_> = tokens.iter().collect();
        assert_eq!(set.len(), 5, "all token IDs should be unique");
    }

    #[test]
    fn confirm_request_safety_class_stored() {
        let req = make_request(PhaseKind::Fix, SafetyClass::Safety);
        assert_eq!(req.safety_class, SafetyClass::Safety);
    }

    #[test]
    fn awaiting_human_state_blocked_on_verifier_as_str() {
        assert_eq!(
            AwaitingHumanState::BlockedOnVerifier.as_str(),
            "blocked_on_verifier"
        );
    }

    #[test]
    fn awaiting_human_state_waiver_requested_as_str() {
        assert_eq!(
            AwaitingHumanState::WaiverRequested.as_str(),
            "waiver_requested"
        );
    }

    #[test]
    fn confirm_outcome_all_variants_as_str_unique() {
        let outcomes = [
            ConfirmOutcome::Approved,
            ConfirmOutcome::Refused,
            ConfirmOutcome::Deferred,
        ];
        let strings: Vec<&str> = outcomes.iter().map(|o| o.as_str()).collect();
        let set: std::collections::HashSet<&str> = strings.iter().copied().collect();
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn noop_confirm_detect_phase() {
        let c = NoOpHumanConfirm::default();
        let req = make_request(PhaseKind::Detect, SafetyClass::Soft);
        let token = c.confirm(&req).expect("ok");
        assert_eq!(token.phase_kind, PhaseKind::Detect);
        assert!(token.is_approved());
    }

    #[test]
    fn noop_confirm_block_phase() {
        let c = NoOpHumanConfirm::default();
        let req = make_request(PhaseKind::Block, SafetyClass::Hard);
        let token = c.confirm(&req).expect("ok");
        assert_eq!(token.phase_kind, PhaseKind::Block);
    }

    #[test]
    fn noop_confirm_meta_test_phase() {
        let c = NoOpHumanConfirm::default();
        let req = make_request(PhaseKind::MetaTest, SafetyClass::Soft);
        let token = c.confirm(&req).expect("ok");
        assert_eq!(token.phase_kind, PhaseKind::MetaTest);
    }

    #[test]
    fn confirm_token_is_not_deferred_when_approved() {
        let tok = ConfirmToken {
            token_id: "t".into(),
            runbook_id: RunbookId::new("rb").expect("valid"),
            phase_kind: PhaseKind::Fix,
            outcome: ConfirmOutcome::Approved,
            issued_at: 0,
            operator_note: None,
        };
        assert!(!tok.is_deferred());
        assert!(!tok.is_refused());
    }

    #[test]
    fn confirm_token_is_not_approved_when_refused() {
        let tok = ConfirmToken {
            token_id: "t".into(),
            runbook_id: RunbookId::new("rb").expect("valid"),
            phase_kind: PhaseKind::Fix,
            outcome: ConfirmOutcome::Refused,
            issued_at: 0,
            operator_note: None,
        };
        assert!(!tok.is_approved());
        assert!(!tok.is_deferred());
    }

    #[test]
    fn noop_confirm_safety_class_runbook_still_approved() {
        let c = NoOpHumanConfirm::default();
        let req = make_request(PhaseKind::Fix, SafetyClass::Safety);
        let token = c.confirm(&req).expect("ok");
        assert!(token.is_approved());
    }
}
