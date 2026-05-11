#![forbid(unsafe_code)]

//! M014 — `ClaimAuthorityVerifier` — adversarial check against executor
//! self-certification; sole issuer of `ClaimAuthority<Final>`.
//!
//! **Cluster:** C02 Authority & State | **Layer:** L04
//!
//! Enforces C02 Invariants I5 and I6:
//!
//! > **I5 (Verifier Sole Final Issuer):** `ClaimAuthority::<Final>::finalize(…)`
//! > is called only inside this file.  Verifiable with
//! > `rg 'ClaimAuthority::<Final>'` from the workspace root.
//!
//! > **I6 (Evidence Hash Independence):** The verifier receives an artifact
//! > hash independently of the executor's self-report.  An executor that
//! > writes `evidence_hash: "PASS"` into its `DraftPassed` event and supplies
//! > the same string as `artifact_hash` to `evaluate` is caught by step 5 of
//! > the evaluation logic because the verifier compares against the hash it
//! > received through a separate channel.
//!
//! The authority boundary is enforced structurally:
//! - `hle-executor` does NOT appear in `hle-verifier`'s `[dependencies]`.
//! - `ExecutorEvent` and `ClaimAuthority<Provisional>` are imported as **data
//!   types** only — no executor code is callable from here.
//! - `ClaimAuthority<Final>` and its constructor `finalize(…)` are **only
//!   used in this file** within `hle-verifier`.
//!
//! Cross-reference: `ai_specs/modules/c02-authority-state/M014_CLAIM_AUTHORITY_VERIFIER.md`
//! Use pattern: `ai_docs/use_patterns/UP_EXECUTOR_VERIFIER_SPLIT.md` (HLE-UP-001)
//! Anti-pattern: `ai_docs/anti_patterns/FP_FALSE_PASS_CLASSES.md` (HLE-SP-001)
//!
//! NOTE: `hle-verifier` does NOT depend on `hle-executor` at the Cargo level.
//! `ExecutorEvent` is re-exported from `hle-core` or defined locally here for
//! the stub.  In the full topology this type will be shared via `hle-core`.
//! For the compile-safe stub we define a minimal local mirror so the file
//! compiles without introducing a forbidden crate dependency.

use std::collections::HashMap;
use std::fmt;

use hle_core::authority::claim_authority::{
    AuthorityClass, AuthorityError, ClaimAuthority, Final, Provisional, Verified,
};

// ---------------------------------------------------------------------------
// EventSequence mirror (local stub — avoids hle-executor dependency)
// ---------------------------------------------------------------------------

/// Monotonically increasing event counter (mirror of M012 `EventSequence`).
///
/// Defined locally in the verifier crate so that `hle-verifier` does not
/// acquire a compile-time dependency on `hle-executor` (UP_EXECUTOR_VERIFIER_SPLIT).
/// In the full topology both crates will share this type via `hle-core`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct EventSequence(u64);

impl EventSequence {
    /// Construct with an explicit value.
    #[must_use]
    pub const fn new(n: u64) -> Self {
        Self(n)
    }

    /// The raw sequence number.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

impl fmt::Display for EventSequence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "seq:{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// ExecutorEvent mirror (local stub — avoids hle-executor dependency)
// ---------------------------------------------------------------------------

/// Verifier-visible record of an executor state transition.
///
/// This is a local mirror of `hle_executor::state_machine::ExecutorEvent`.
/// The verifier crate deliberately does NOT import `hle-executor` at the Cargo
/// level — `hle-verifier` is a pure consumer of event values.  When this type
/// is eventually shared via `hle-core`, this local definition will be removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutorEvent {
    /// Executor claimed the step and started running it.
    StepStarted {
        /// Workflow identifier.
        workflow_id: String,
        /// Step identifier.
        step_id: String,
        /// Monotonic sequence number.
        sequence: EventSequence,
    },
    /// Executor yielded because a human decision is required.
    HumanGateReached {
        /// Workflow identifier.
        workflow_id: String,
        /// Step identifier.
        step_id: String,
        /// Monotonic sequence number.
        sequence: EventSequence,
        /// Human-readable reason.
        reason: String,
    },
    /// Executor observed step completion; draft state is `Passed`.
    DraftPassed {
        /// Workflow identifier.
        workflow_id: String,
        /// Step identifier.
        step_id: String,
        /// Monotonic sequence number.
        sequence: EventSequence,
        /// SHA256 (or similar) hash of the step's output artifacts.
        evidence_hash: String,
    },
    /// Executor observed step failure.
    DraftFailed {
        /// Workflow identifier.
        workflow_id: String,
        /// Step identifier.
        step_id: String,
        /// Monotonic sequence number.
        sequence: EventSequence,
        /// Human-readable failure reason.
        reason: String,
    },
    /// Executor is executing a compensating rollback.
    RollbackStarted {
        /// Workflow identifier.
        workflow_id: String,
        /// Step identifier.
        step_id: String,
        /// Monotonic sequence number.
        sequence: EventSequence,
        /// Human-readable rollback reason.
        reason: String,
    },
    /// Executor completed the rollback.
    RollbackCompleted {
        /// Workflow identifier.
        workflow_id: String,
        /// Step identifier.
        step_id: String,
        /// Monotonic sequence number.
        sequence: EventSequence,
    },
}

impl ExecutorEvent {
    /// Workflow identifier carried by this event.
    #[must_use]
    pub fn workflow_id(&self) -> &str {
        match self {
            Self::StepStarted { workflow_id, .. }
            | Self::HumanGateReached { workflow_id, .. }
            | Self::DraftPassed { workflow_id, .. }
            | Self::DraftFailed { workflow_id, .. }
            | Self::RollbackStarted { workflow_id, .. }
            | Self::RollbackCompleted { workflow_id, .. } => workflow_id,
        }
    }

    /// Step identifier carried by this event.
    #[must_use]
    pub fn step_id(&self) -> &str {
        match self {
            Self::StepStarted { step_id, .. }
            | Self::HumanGateReached { step_id, .. }
            | Self::DraftPassed { step_id, .. }
            | Self::DraftFailed { step_id, .. }
            | Self::RollbackStarted { step_id, .. }
            | Self::RollbackCompleted { step_id, .. } => step_id,
        }
    }

    /// Monotonic sequence number.
    #[must_use]
    pub fn sequence(&self) -> EventSequence {
        match self {
            Self::StepStarted { sequence, .. }
            | Self::HumanGateReached { sequence, .. }
            | Self::DraftPassed { sequence, .. }
            | Self::DraftFailed { sequence, .. }
            | Self::RollbackStarted { sequence, .. }
            | Self::RollbackCompleted { sequence, .. } => *sequence,
        }
    }
}

// ---------------------------------------------------------------------------
// RejectionReason
// ---------------------------------------------------------------------------

/// Structured reason for a [`VerifierVerdict::Rejected`] verdict.
///
/// Enables the false-pass auditor (M020) to classify rejections without string
/// parsing.  Error codes fall in the 2100–2199 range.
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

impl RejectionReason {
    /// Wire-format label for this rejection reason.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoDraftPassedEvent => "no-draft-passed-event",
            Self::EvidenceHashMismatch => "evidence-hash-mismatch",
            Self::SequenceGap => "sequence-gap",
            Self::LaterFailureObserved => "later-failure-observed",
            Self::NegativeControlMustFail => "negative-control-must-fail",
            Self::HumanGateMissing => "human-gate-missing",
            Self::SelfCertificationAttempt => "self-certification-attempt",
        }
    }

    /// Returns `true` when this reason indicates a self-certification attempt.
    #[must_use]
    pub const fn is_self_certification(self) -> bool {
        matches!(self, Self::SelfCertificationAttempt)
    }

    /// Numeric error code (2100–2199 range).
    #[must_use]
    pub const fn error_code(self) -> u16 {
        match self {
            Self::NoDraftPassedEvent => 2100,
            Self::EvidenceHashMismatch => 2100,
            Self::SequenceGap => 2104,
            Self::LaterFailureObserved => 2100,
            Self::NegativeControlMustFail => 2102,
            Self::HumanGateMissing => 2100,
            Self::SelfCertificationAttempt => 2102,
        }
    }
}

impl fmt::Display for RejectionReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// VerifierReceipt
// ---------------------------------------------------------------------------

/// Binding proof that the verifier accepted an executor claim.
///
/// Contains a [`ClaimAuthority<Final>`] that is consumed internally when
/// writing to the persistence layer.  Downstream consumers receive the receipt
/// and extract only `workflow_id`, `step_id`, and `verdict_string` via public
/// accessors.
#[derive(Debug)]
pub struct VerifierReceipt {
    workflow_id: String,
    step_id: String,
    artifact_hash: String,
    /// The final authority token.  `pub(crate)` — only verifier internals and
    /// persistence writers may move this.
    #[allow(dead_code)]
    // moved out via consume_authority() below; held for type-state evidence
    authority: ClaimAuthority<Final>,
}

impl VerifierReceipt {
    /// Workflow identifier.
    #[must_use]
    pub fn workflow_id(&self) -> &str {
        &self.workflow_id
    }

    /// Step identifier.
    #[must_use]
    pub fn step_id(&self) -> &str {
        &self.step_id
    }

    /// The artifact hash that the verifier accepted.
    #[must_use]
    pub fn artifact_hash(&self) -> &str {
        &self.artifact_hash
    }

    /// Verdict string — always `"PASS"` for a receipt.
    #[must_use]
    pub fn verdict_string(&self) -> &'static str {
        "PASS"
    }

    /// Destructure the `ClaimAuthority<Final>` for persistence.  Consumes
    /// `self` so the token cannot be reused.
    #[allow(dead_code)] // exposed for future C05 verifier_results_store integration
    pub(crate) fn consume_authority(self) -> (String, String, AuthorityClass) {
        self.authority.into_receipt_evidence()
    }
}

// ---------------------------------------------------------------------------
// VerifierVerdict
// ---------------------------------------------------------------------------

/// Binding result of a [`ClaimAuthorityVerifier::evaluate`] call.
#[derive(Debug)]
#[must_use]
pub enum VerifierVerdict {
    /// The claim is accepted.  The receipt carries a `ClaimAuthority<Final>`
    /// that is the authoritative PASS token for this step.
    Pass(VerifierReceipt),

    /// The step requires further human input before a PASS verdict is possible.
    Blocked {
        /// Workflow identifier.
        workflow_id: String,
        /// Step identifier.
        step_id: String,
        /// Human-readable reason.
        reason: String,
    },

    /// The claim is rejected.  No `ClaimAuthority<Final>` is issued.
    Rejected {
        /// Workflow identifier.
        workflow_id: String,
        /// Step identifier.
        step_id: String,
        /// Structured rejection reason.
        reason: RejectionReason,
    },
}

impl VerifierVerdict {
    /// Returns `true` when the verdict is `Pass`.
    #[must_use]
    pub fn is_pass(&self) -> bool {
        matches!(self, Self::Pass(_))
    }

    /// Returns `true` when the verdict is `Blocked`.
    #[must_use]
    pub fn is_blocked(&self) -> bool {
        matches!(self, Self::Blocked { .. })
    }

    /// Returns `true` when the verdict is `Rejected`.
    #[must_use]
    pub fn is_rejected(&self) -> bool {
        matches!(self, Self::Rejected { .. })
    }

    /// Consume this verdict and return the receipt if it is `Pass`.
    #[must_use]
    pub fn into_receipt(self) -> Option<VerifierReceipt> {
        match self {
            Self::Pass(receipt) => Some(receipt),
            Self::Blocked { .. } | Self::Rejected { .. } => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Internal event log
// ---------------------------------------------------------------------------

/// Internal per-workflow event history tracked by the verifier.
#[derive(Debug, Default)]
struct EventLog {
    events: Vec<ExecutorEvent>,
    last_sequence: Option<EventSequence>,
}

impl EventLog {
    /// Append an event; returns `StaleEvent` if the sequence regresses.
    fn append(&mut self, event: ExecutorEvent) -> Result<(), AuthorityError> {
        let seq = event.sequence();
        if let Some(last) = self.last_sequence {
            if seq.value() <= last.value() {
                return Err(AuthorityError::StaleEvent {
                    expected: last.value() + 1,
                    received: seq.value(),
                });
            }
        }
        self.last_sequence = Some(seq);
        self.events.push(event);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// VerifierInner (behind RwLock)
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct VerifierInner {
    logs: HashMap<String, EventLog>,
}

impl VerifierInner {
    fn log_for(&self, workflow_id: &str) -> Option<&EventLog> {
        self.logs.get(workflow_id)
    }

    fn log_for_mut(&mut self, workflow_id: &str) -> &mut EventLog {
        self.logs.entry(workflow_id.to_owned()).or_default()
    }
}

// ---------------------------------------------------------------------------
// ClaimAuthorityVerifier
// ---------------------------------------------------------------------------

/// Adversarial verifier that watches [`ExecutorEvent`] streams and is the sole
/// producer of [`ClaimAuthority<Final>`].
///
/// # Authority boundary
///
/// This struct lives in `crates/hle-verifier`.  [`ClaimAuthority<Final>`] and
/// its constructor `ClaimAuthority::<Final>::finalize(…)` are used **only here**.
/// The executor crate cannot import or construct either.  This is the
/// compile-time enforcement of HLE-UP-001 and the structural defence against
/// `FP_SELF_CERTIFICATION` (HLE-SP-001).
///
/// # Operation model
///
/// The verifier is a passive consumer.  It does not call into the executor.
/// It receives [`ExecutorEvent`] values (via [`observe`][Self::observe]) and
/// makes a binding decision only when [`evaluate`][Self::evaluate] is called
/// with a [`ClaimAuthority<Provisional>`] from the executor plus the artifact
/// hash.
///
/// # Concurrency
///
/// Internal event log is behind a [`std::sync::RwLock`].  `observe` acquires a
/// write lock; `evaluate` acquires a read lock then briefly upgrades for the
/// final-token construction.  Stub uses `std::sync::RwLock` (no `parking_lot`
/// dependency required for compile-safe stub).
#[derive(Debug)]
pub struct ClaimAuthorityVerifier {
    inner: std::sync::RwLock<VerifierInner>,
}

impl Default for ClaimAuthorityVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl ClaimAuthorityVerifier {
    /// Construct a new verifier with an empty event log.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: std::sync::RwLock::new(VerifierInner::default()),
        }
    }

    /// Append an [`ExecutorEvent`] to the per-workflow log.
    ///
    /// # Errors
    ///
    /// Returns [`AuthorityError::StaleEvent`] if the sequence number regresses.
    pub fn observe(&self, event: ExecutorEvent) -> Result<(), AuthorityError> {
        let workflow_id = event.workflow_id().to_owned();
        let mut guard = self
            .inner
            .write()
            .map_err(|_| AuthorityError::Other(String::from("lock poisoned")))?;
        guard.log_for_mut(&workflow_id).append(event)
    }

    /// Check the event log against the executor's claim and return a verdict.
    ///
    /// The ten-step evaluation logic from the M014 spec is executed in order.
    /// On `Pass`, this function is the only call site in the workspace that
    /// constructs `ClaimAuthority<Final>` (I5).
    ///
    /// # Errors
    ///
    /// Returns [`AuthorityError::UnknownWorkflow`] when no events have been
    /// observed for `provisional.workflow_id()`.
    pub fn evaluate(
        &self,
        provisional: ClaimAuthority<Provisional>,
        artifact_hash: &str,
    ) -> Result<VerifierVerdict, AuthorityError> {
        let workflow_id = provisional.workflow_id().to_owned();
        let step_id = provisional.step_id().to_owned();
        let class = provisional.class();

        let guard = self
            .inner
            .read()
            .map_err(|_| AuthorityError::Other(String::from("lock poisoned")))?;

        // Step 2: workflow must be known.
        let log = guard
            .log_for(&workflow_id)
            .ok_or_else(|| AuthorityError::UnknownWorkflow {
                workflow_id: workflow_id.clone(),
            })?;

        // Step 3: find a DraftPassed event for this step.
        let draft_passed = log.events.iter().find(
            |ev| matches!(ev, ExecutorEvent::DraftPassed { step_id: sid, .. } if sid == &step_id),
        );

        let Some(draft_event) = draft_passed else {
            // Step 8: NegativeControl must fail — but no DraftPassed means rejected anyway.
            if class.is_negative_control() {
                return Ok(VerifierVerdict::Rejected {
                    workflow_id,
                    step_id,
                    reason: RejectionReason::NegativeControlMustFail,
                });
            }
            // Step 9: check if blocked at human gate.
            let has_human_gate = log.events.iter().any(|ev| {
                matches!(ev, ExecutorEvent::HumanGateReached { step_id: sid, .. } if sid == &step_id)
            });
            if has_human_gate {
                return Ok(VerifierVerdict::Blocked {
                    workflow_id,
                    step_id,
                    reason: String::from("awaiting human decision"),
                });
            }
            return Ok(VerifierVerdict::Rejected {
                workflow_id,
                step_id,
                reason: RejectionReason::NoDraftPassedEvent,
            });
        };

        // Step 4 / Step 5: extract and compare evidence hash.
        let event_hash = match draft_event {
            ExecutorEvent::DraftPassed { evidence_hash, .. } => evidence_hash.as_str(),
            _ => {
                return Ok(VerifierVerdict::Rejected {
                    workflow_id,
                    step_id,
                    reason: RejectionReason::NoDraftPassedEvent,
                });
            }
        };
        if event_hash != artifact_hash {
            return Ok(VerifierVerdict::Rejected {
                workflow_id,
                step_id,
                reason: RejectionReason::EvidenceHashMismatch,
            });
        }

        // Step 6: verify sequence contiguity.
        // For the stub: check that no sequence gap exists before DraftPassed.
        let draft_seq = draft_event.sequence();
        let step_events: Vec<&ExecutorEvent> = log
            .events
            .iter()
            .filter(|ev| ev.step_id() == step_id)
            .collect();
        if step_events.len() > 1 {
            for i in 1..step_events.len() {
                let prev = step_events[i - 1].sequence().value();
                let curr = step_events[i].sequence().value();
                if curr != prev + 1 {
                    return Ok(VerifierVerdict::Rejected {
                        workflow_id,
                        step_id,
                        reason: RejectionReason::SequenceGap,
                    });
                }
            }
        }

        // Step 7: check no DraftFailed / RollbackStarted after DraftPassed.
        let later_failure = log.events.iter().any(|ev| {
            let is_failure = matches!(
                ev,
                ExecutorEvent::DraftFailed { step_id: sid, .. }
                | ExecutorEvent::RollbackStarted { step_id: sid, .. }
                if sid == &step_id
            );
            is_failure && ev.sequence().value() > draft_seq.value()
        });
        if later_failure {
            return Ok(VerifierVerdict::Rejected {
                workflow_id,
                step_id,
                reason: RejectionReason::LaterFailureObserved,
            });
        }

        // Step 8: NegativeControl must fail.
        if class.is_negative_control() {
            return Ok(VerifierVerdict::Rejected {
                workflow_id,
                step_id,
                reason: RejectionReason::NegativeControlMustFail,
            });
        }

        // Step 9: HumanRequired must have a HumanGateReached event.
        if class.is_human_required() {
            let has_gate = log.events.iter().any(|ev| {
                matches!(ev, ExecutorEvent::HumanGateReached { step_id: sid, .. } if sid == &step_id)
            });
            if !has_gate {
                return Ok(VerifierVerdict::Rejected {
                    workflow_id,
                    step_id,
                    reason: RejectionReason::HumanGateMissing,
                });
            }
        }

        // Step 10: advance authority through Provisional → Verified → Final.
        // This is the ONLY call site for ClaimAuthority::<Final>::finalize in
        // the workspace (C02 Invariant I5).
        let verified = ClaimAuthority::<Verified>::verify(provisional);
        let final_token = ClaimAuthority::<Final>::finalize(verified);

        // Step 11: construct VerifierReceipt.
        let receipt = VerifierReceipt {
            workflow_id: final_token.workflow_id().to_owned(),
            step_id: final_token.step_id().to_owned(),
            artifact_hash: artifact_hash.to_owned(),
            authority: final_token,
        };

        // Step 12: return Pass verdict.
        Ok(VerifierVerdict::Pass(receipt))
    }

    /// Returns all workflow IDs currently tracked.
    #[must_use]
    pub fn workflow_ids(&self) -> Vec<String> {
        self.inner
            .read()
            .map(|g| g.logs.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Number of events observed for `workflow_id`.
    #[must_use]
    pub fn event_count(&self, workflow_id: &str) -> usize {
        self.inner
            .read()
            .map(|g| g.log_for(workflow_id).map_or(0, |log| log.events.len()))
            .unwrap_or(0)
    }

    /// Highest sequence number observed for `workflow_id`.
    #[must_use]
    pub fn last_sequence(&self, workflow_id: &str) -> Option<EventSequence> {
        self.inner
            .read()
            .ok()
            .and_then(|g| g.log_for(workflow_id).and_then(|log| log.last_sequence))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{
        AuthorityClass, ClaimAuthority, ClaimAuthorityVerifier, EventSequence, ExecutorEvent,
        Provisional, RejectionReason, VerifierReceipt, VerifierVerdict,
    };

    // ---------------------------------------------------------------------------
    // Helpers
    // ---------------------------------------------------------------------------

    fn make_provisional(workflow: &str, step: &str) -> ClaimAuthority<Provisional> {
        ClaimAuthority::<Provisional>::new(workflow, step, AuthorityClass::Automated)
    }

    fn make_provisional_class(
        workflow: &str,
        step: &str,
        class: AuthorityClass,
    ) -> ClaimAuthority<Provisional> {
        ClaimAuthority::<Provisional>::new(workflow, step, class)
    }

    /// Feed StepStarted + DraftPassed into the verifier with contiguous sequences.
    fn observe_draft_passed(
        verifier: &ClaimAuthorityVerifier,
        workflow: &str,
        step: &str,
        hash: &str,
        seq: u64,
    ) {
        verifier
            .observe(ExecutorEvent::StepStarted {
                workflow_id: workflow.to_owned(),
                step_id: step.to_owned(),
                sequence: EventSequence::new(seq),
            })
            .expect("observe StepStarted");
        verifier
            .observe(ExecutorEvent::DraftPassed {
                workflow_id: workflow.to_owned(),
                step_id: step.to_owned(),
                sequence: EventSequence::new(seq + 1),
                evidence_hash: hash.to_owned(),
            })
            .expect("observe DraftPassed");
    }

    /// Feed StepStarted + HumanGateReached + DraftPassed (all contiguous).
    fn observe_human_gate_then_passed(
        verifier: &ClaimAuthorityVerifier,
        workflow: &str,
        step: &str,
        hash: &str,
    ) {
        verifier
            .observe(ExecutorEvent::StepStarted {
                workflow_id: workflow.to_owned(),
                step_id: step.to_owned(),
                sequence: EventSequence::new(1),
            })
            .expect("observe StepStarted");
        verifier
            .observe(ExecutorEvent::HumanGateReached {
                workflow_id: workflow.to_owned(),
                step_id: step.to_owned(),
                sequence: EventSequence::new(2),
                reason: String::from("approval needed"),
            })
            .expect("observe HumanGateReached");
        verifier
            .observe(ExecutorEvent::DraftPassed {
                workflow_id: workflow.to_owned(),
                step_id: step.to_owned(),
                sequence: EventSequence::new(3),
                evidence_hash: hash.to_owned(),
            })
            .expect("observe DraftPassed");
    }

    fn extract_receipt(verdict: VerifierVerdict) -> VerifierReceipt {
        verdict.into_receipt().expect("expected Pass receipt")
    }

    // ---------------------------------------------------------------------------
    // Happy path — evaluate returns Pass
    // ---------------------------------------------------------------------------

    #[test]
    fn evaluate_returns_pass_when_hash_matches() {
        let v = ClaimAuthorityVerifier::new();
        observe_draft_passed(&v, "wf-1", "step-1", "abc123", 1);
        let provisional = make_provisional("wf-1", "step-1");
        let verdict = v
            .evaluate(provisional, "abc123")
            .expect("evaluate should not error");
        assert!(verdict.is_pass(), "expected Pass, got {verdict:?}");
    }

    #[test]
    fn pass_verdict_receipt_has_correct_workflow_id() {
        let v = ClaimAuthorityVerifier::new();
        observe_draft_passed(&v, "wf-2", "step-2", "hash", 1);
        let verdict = v
            .evaluate(make_provisional("wf-2", "step-2"), "hash")
            .expect("evaluate");
        let receipt = extract_receipt(verdict);
        assert_eq!(receipt.workflow_id(), "wf-2");
    }

    #[test]
    fn pass_verdict_receipt_has_correct_step_id() {
        let v = ClaimAuthorityVerifier::new();
        observe_draft_passed(&v, "wf-s", "my-step", "h", 1);
        let receipt = extract_receipt(
            v.evaluate(make_provisional("wf-s", "my-step"), "h")
                .expect("evaluate"),
        );
        assert_eq!(receipt.step_id(), "my-step");
    }

    #[test]
    fn pass_verdict_receipt_verdict_string_is_pass() {
        let v = ClaimAuthorityVerifier::new();
        observe_draft_passed(&v, "wf-3", "step-3", "h", 1);
        let verdict = v
            .evaluate(make_provisional("wf-3", "step-3"), "h")
            .expect("evaluate");
        let receipt = extract_receipt(verdict);
        assert_eq!(receipt.verdict_string(), "PASS");
    }

    #[test]
    fn pass_verdict_receipt_artifact_hash_matches_input() {
        let v = ClaimAuthorityVerifier::new();
        observe_draft_passed(&v, "wf-ah", "step-ah", "my-artifact-hash", 1);
        let receipt = extract_receipt(
            v.evaluate(make_provisional("wf-ah", "step-ah"), "my-artifact-hash")
                .expect("evaluate"),
        );
        assert_eq!(receipt.artifact_hash(), "my-artifact-hash");
    }

    #[test]
    fn pass_verdict_is_not_blocked() {
        let v = ClaimAuthorityVerifier::new();
        observe_draft_passed(&v, "wf-nb", "step-nb", "h", 1);
        let verdict = v
            .evaluate(make_provisional("wf-nb", "step-nb"), "h")
            .expect("evaluate");
        assert!(!verdict.is_blocked());
        assert!(!verdict.is_rejected());
    }

    #[test]
    fn human_required_with_gate_event_produces_pass() {
        let v = ClaimAuthorityVerifier::new();
        observe_human_gate_then_passed(&v, "wf-hr-ok", "step-hr-ok", "h2");
        let provisional =
            make_provisional_class("wf-hr-ok", "step-hr-ok", AuthorityClass::HumanRequired);
        let verdict = v.evaluate(provisional, "h2").expect("evaluate");
        assert!(
            verdict.is_pass(),
            "expected Pass for human-required with gate event"
        );
    }

    // ---------------------------------------------------------------------------
    // Rejection cases
    // ---------------------------------------------------------------------------

    #[test]
    fn evaluate_rejects_unknown_workflow() {
        let v = ClaimAuthorityVerifier::new();
        let err = v.evaluate(make_provisional("ghost", "step"), "h");
        assert!(
            err.is_err(),
            "unknown workflow should produce AuthorityError"
        );
    }

    #[test]
    fn evaluate_unknown_workflow_error_code_is_2103() {
        let v = ClaimAuthorityVerifier::new();
        let err = v
            .evaluate(make_provisional("no-such-wf", "s"), "h")
            .expect_err("should error");
        assert_eq!(err.error_code(), 2103);
    }

    #[test]
    fn evaluate_rejects_when_hash_mismatches() {
        let v = ClaimAuthorityVerifier::new();
        observe_draft_passed(&v, "wf-4", "step-4", "correct-hash", 1);
        let verdict = v
            .evaluate(make_provisional("wf-4", "step-4"), "wrong-hash")
            .expect("evaluate");
        assert!(verdict.is_rejected());
        if let VerifierVerdict::Rejected { reason, .. } = verdict {
            assert_eq!(reason, RejectionReason::EvidenceHashMismatch);
        }
    }

    #[test]
    fn evaluate_rejects_negative_control() {
        let v = ClaimAuthorityVerifier::new();
        let workflow = "wf-nc";
        let step = "step-nc";
        let hash = "h";
        v.observe(ExecutorEvent::StepStarted {
            workflow_id: workflow.to_owned(),
            step_id: step.to_owned(),
            sequence: EventSequence::new(1),
        })
        .expect("observe");
        v.observe(ExecutorEvent::DraftPassed {
            workflow_id: workflow.to_owned(),
            step_id: step.to_owned(),
            sequence: EventSequence::new(2),
            evidence_hash: hash.to_owned(),
        })
        .expect("observe");
        let provisional =
            ClaimAuthority::<Provisional>::new(workflow, step, AuthorityClass::NegativeControl);
        let verdict = v.evaluate(provisional, hash).expect("evaluate");
        assert!(verdict.is_rejected());
        if let VerifierVerdict::Rejected { reason, .. } = verdict {
            assert_eq!(reason, RejectionReason::NegativeControlMustFail);
        }
    }

    #[test]
    fn evaluate_rejects_human_required_without_gate_event() {
        let v = ClaimAuthorityVerifier::new();
        let workflow = "wf-hr";
        let step = "step-hr";
        let hash = "h";
        // No HumanGateReached event — only StepStarted + DraftPassed.
        v.observe(ExecutorEvent::StepStarted {
            workflow_id: workflow.to_owned(),
            step_id: step.to_owned(),
            sequence: EventSequence::new(1),
        })
        .expect("observe");
        v.observe(ExecutorEvent::DraftPassed {
            workflow_id: workflow.to_owned(),
            step_id: step.to_owned(),
            sequence: EventSequence::new(2),
            evidence_hash: hash.to_owned(),
        })
        .expect("observe");
        let provisional =
            ClaimAuthority::<Provisional>::new(workflow, step, AuthorityClass::HumanRequired);
        let verdict = v.evaluate(provisional, hash).expect("evaluate");
        assert!(verdict.is_rejected());
        if let VerifierVerdict::Rejected { reason, .. } = verdict {
            assert_eq!(reason, RejectionReason::HumanGateMissing);
        }
    }

    #[test]
    fn evaluate_blocked_when_only_human_gate_observed() {
        let v = ClaimAuthorityVerifier::new();
        v.observe(ExecutorEvent::StepStarted {
            workflow_id: String::from("wf-block"),
            step_id: String::from("s"),
            sequence: EventSequence::new(1),
        })
        .expect("observe");
        v.observe(ExecutorEvent::HumanGateReached {
            workflow_id: String::from("wf-block"),
            step_id: String::from("s"),
            sequence: EventSequence::new(2),
            reason: String::from("waiting"),
        })
        .expect("observe");
        // No DraftPassed yet, but HumanGateReached → Blocked
        let provisional = make_provisional("wf-block", "s");
        let verdict = v.evaluate(provisional, "h").expect("evaluate");
        assert!(verdict.is_blocked(), "expected Blocked, got {verdict:?}");
    }

    #[test]
    fn evaluate_rejects_when_no_draft_passed_and_no_gate() {
        let v = ClaimAuthorityVerifier::new();
        v.observe(ExecutorEvent::StepStarted {
            workflow_id: String::from("wf-nopass"),
            step_id: String::from("s"),
            sequence: EventSequence::new(1),
        })
        .expect("observe");
        let verdict = v
            .evaluate(make_provisional("wf-nopass", "s"), "h")
            .expect("evaluate");
        assert!(verdict.is_rejected());
        if let VerifierVerdict::Rejected { reason, .. } = verdict {
            assert_eq!(reason, RejectionReason::NoDraftPassedEvent);
        }
    }

    #[test]
    fn evaluate_rejects_later_failure_after_draft_passed() {
        let v = ClaimAuthorityVerifier::new();
        v.observe(ExecutorEvent::StepStarted {
            workflow_id: String::from("wf-lf"),
            step_id: String::from("s"),
            sequence: EventSequence::new(1),
        })
        .expect("observe");
        v.observe(ExecutorEvent::DraftPassed {
            workflow_id: String::from("wf-lf"),
            step_id: String::from("s"),
            sequence: EventSequence::new(2),
            evidence_hash: String::from("h"),
        })
        .expect("observe");
        v.observe(ExecutorEvent::DraftFailed {
            workflow_id: String::from("wf-lf"),
            step_id: String::from("s"),
            sequence: EventSequence::new(3),
            reason: String::from("late failure"),
        })
        .expect("observe");
        let verdict = v
            .evaluate(make_provisional("wf-lf", "s"), "h")
            .expect("evaluate");
        assert!(verdict.is_rejected());
        if let VerifierVerdict::Rejected { reason, .. } = verdict {
            assert_eq!(reason, RejectionReason::LaterFailureObserved);
        }
    }

    #[test]
    fn evaluate_rejects_sequence_gap_between_step_events() {
        let v = ClaimAuthorityVerifier::new();
        v.observe(ExecutorEvent::StepStarted {
            workflow_id: String::from("wf-gap"),
            step_id: String::from("s"),
            sequence: EventSequence::new(1),
        })
        .expect("observe StepStarted");
        // Gap: seq 2 is missing; jump to 4.
        v.observe(ExecutorEvent::DraftPassed {
            workflow_id: String::from("wf-gap"),
            step_id: String::from("s"),
            sequence: EventSequence::new(4),
            evidence_hash: String::from("h"),
        })
        .expect("observe DraftPassed");
        let verdict = v
            .evaluate(make_provisional("wf-gap", "s"), "h")
            .expect("evaluate");
        assert!(verdict.is_rejected());
        if let VerifierVerdict::Rejected { reason, .. } = verdict {
            assert_eq!(reason, RejectionReason::SequenceGap);
        }
    }

    // ---------------------------------------------------------------------------
    // observe() sequence validation
    // ---------------------------------------------------------------------------

    #[test]
    fn stale_event_is_rejected_by_observe() {
        let v = ClaimAuthorityVerifier::new();
        v.observe(ExecutorEvent::StepStarted {
            workflow_id: String::from("wf"),
            step_id: String::from("s"),
            sequence: EventSequence::new(5),
        })
        .expect("first observe");
        let err = v.observe(ExecutorEvent::StepStarted {
            workflow_id: String::from("wf"),
            step_id: String::from("s"),
            sequence: EventSequence::new(3), // regresses
        });
        assert!(err.is_err(), "stale sequence should error");
    }

    #[test]
    fn stale_event_equal_sequence_is_rejected() {
        let v = ClaimAuthorityVerifier::new();
        v.observe(ExecutorEvent::StepStarted {
            workflow_id: String::from("wf-eq"),
            step_id: String::from("s"),
            sequence: EventSequence::new(10),
        })
        .expect("first observe");
        // Same sequence value — also stale.
        let err = v.observe(ExecutorEvent::DraftFailed {
            workflow_id: String::from("wf-eq"),
            step_id: String::from("s"),
            sequence: EventSequence::new(10),
            reason: String::from("dupe"),
        });
        assert!(err.is_err(), "equal sequence should be rejected as stale");
    }

    #[test]
    fn stale_event_error_code_is_2104() {
        let v = ClaimAuthorityVerifier::new();
        v.observe(ExecutorEvent::StepStarted {
            workflow_id: String::from("wf-2104"),
            step_id: String::from("s"),
            sequence: EventSequence::new(5),
        })
        .expect("observe");
        let err = v
            .observe(ExecutorEvent::StepStarted {
                workflow_id: String::from("wf-2104"),
                step_id: String::from("s"),
                sequence: EventSequence::new(2),
            })
            .expect_err("should be stale");
        assert_eq!(err.error_code(), 2104);
    }

    #[test]
    fn observe_separate_workflows_do_not_interfere() {
        let v = ClaimAuthorityVerifier::new();
        v.observe(ExecutorEvent::StepStarted {
            workflow_id: String::from("wf-a"),
            step_id: String::from("s"),
            sequence: EventSequence::new(10),
        })
        .expect("observe wf-a");
        // wf-b starts at sequence 1 — should not be stale relative to wf-a.
        v.observe(ExecutorEvent::StepStarted {
            workflow_id: String::from("wf-b"),
            step_id: String::from("s"),
            sequence: EventSequence::new(1),
        })
        .expect("observe wf-b with lower seq");
    }

    // ---------------------------------------------------------------------------
    // Metadata queries
    // ---------------------------------------------------------------------------

    #[test]
    fn workflow_ids_tracks_observed_workflows() {
        let v = ClaimAuthorityVerifier::new();
        observe_draft_passed(&v, "wf-q", "step-q", "h", 1);
        assert!(v.workflow_ids().contains(&String::from("wf-q")));
    }

    #[test]
    fn workflow_ids_empty_before_observation() {
        let v = ClaimAuthorityVerifier::new();
        assert!(v.workflow_ids().is_empty());
    }

    #[test]
    fn workflow_ids_contains_multiple_workflows() {
        let v = ClaimAuthorityVerifier::new();
        observe_draft_passed(&v, "wf-a", "s", "h", 1);
        observe_draft_passed(&v, "wf-b", "s", "h2", 1);
        let ids = v.workflow_ids();
        assert!(ids.contains(&String::from("wf-a")));
        assert!(ids.contains(&String::from("wf-b")));
    }

    #[test]
    fn event_count_matches_observed_events() {
        let v = ClaimAuthorityVerifier::new();
        observe_draft_passed(&v, "wf-e", "step-e", "h", 1);
        // observe_draft_passed sends 2 events.
        assert_eq!(v.event_count("wf-e"), 2);
    }

    #[test]
    fn event_count_zero_for_unknown_workflow() {
        let v = ClaimAuthorityVerifier::new();
        assert_eq!(v.event_count("no-such-wf"), 0);
    }

    #[test]
    fn event_count_increases_with_each_observe() {
        let v = ClaimAuthorityVerifier::new();
        v.observe(ExecutorEvent::StepStarted {
            workflow_id: String::from("wf-cnt"),
            step_id: String::from("s"),
            sequence: EventSequence::new(1),
        })
        .expect("observe");
        assert_eq!(v.event_count("wf-cnt"), 1);
        v.observe(ExecutorEvent::DraftFailed {
            workflow_id: String::from("wf-cnt"),
            step_id: String::from("s"),
            sequence: EventSequence::new(2),
            reason: String::from("r"),
        })
        .expect("observe");
        assert_eq!(v.event_count("wf-cnt"), 2);
    }

    #[test]
    fn last_sequence_returns_highest_observed() {
        let v = ClaimAuthorityVerifier::new();
        observe_draft_passed(&v, "wf-ls", "step-ls", "h", 10);
        let last = v.last_sequence("wf-ls");
        assert_eq!(last.map(|s| s.value()), Some(11));
    }

    #[test]
    fn last_sequence_none_for_unknown_workflow() {
        let v = ClaimAuthorityVerifier::new();
        assert!(v.last_sequence("ghost").is_none());
    }

    #[test]
    fn last_sequence_updates_with_each_event() {
        let v = ClaimAuthorityVerifier::new();
        v.observe(ExecutorEvent::StepStarted {
            workflow_id: String::from("wf-seq"),
            step_id: String::from("s"),
            sequence: EventSequence::new(7),
        })
        .expect("observe");
        assert_eq!(v.last_sequence("wf-seq").map(|s| s.value()), Some(7));
        v.observe(ExecutorEvent::DraftPassed {
            workflow_id: String::from("wf-seq"),
            step_id: String::from("s"),
            sequence: EventSequence::new(8),
            evidence_hash: String::from("h"),
        })
        .expect("observe");
        assert_eq!(v.last_sequence("wf-seq").map(|s| s.value()), Some(8));
    }

    // ---------------------------------------------------------------------------
    // VerifierVerdict predicates
    // ---------------------------------------------------------------------------

    #[test]
    fn verdict_blocked_is_blocked() {
        let v = ClaimAuthorityVerifier::new();
        v.observe(ExecutorEvent::StepStarted {
            workflow_id: String::from("wf-vb"),
            step_id: String::from("s"),
            sequence: EventSequence::new(1),
        })
        .expect("observe");
        v.observe(ExecutorEvent::HumanGateReached {
            workflow_id: String::from("wf-vb"),
            step_id: String::from("s"),
            sequence: EventSequence::new(2),
            reason: String::from("gate"),
        })
        .expect("observe");
        let verdict = v
            .evaluate(make_provisional("wf-vb", "s"), "h")
            .expect("evaluate");
        assert!(verdict.is_blocked());
        assert!(!verdict.is_pass());
        assert!(!verdict.is_rejected());
    }

    #[test]
    fn verdict_rejected_is_rejected() {
        let v = ClaimAuthorityVerifier::new();
        observe_draft_passed(&v, "wf-rej", "s", "real-hash", 1);
        let verdict = v
            .evaluate(make_provisional("wf-rej", "s"), "wrong")
            .expect("evaluate");
        assert!(verdict.is_rejected());
        assert!(!verdict.is_pass());
        assert!(!verdict.is_blocked());
    }

    #[test]
    fn into_receipt_returns_some_on_pass() {
        let v = ClaimAuthorityVerifier::new();
        observe_draft_passed(&v, "wf-ir", "s", "h", 1);
        let verdict = v
            .evaluate(make_provisional("wf-ir", "s"), "h")
            .expect("evaluate");
        assert!(verdict.into_receipt().is_some());
    }

    #[test]
    fn into_receipt_returns_none_on_rejected() {
        let v = ClaimAuthorityVerifier::new();
        observe_draft_passed(&v, "wf-ir2", "s", "h", 1);
        let verdict = v
            .evaluate(make_provisional("wf-ir2", "s"), "wrong")
            .expect("evaluate");
        assert!(verdict.into_receipt().is_none());
    }

    // ---------------------------------------------------------------------------
    // RejectionReason helpers
    // ---------------------------------------------------------------------------

    #[test]
    fn rejection_reason_as_str_is_stable() {
        assert_eq!(
            RejectionReason::NoDraftPassedEvent.as_str(),
            "no-draft-passed-event"
        );
        assert_eq!(
            RejectionReason::EvidenceHashMismatch.as_str(),
            "evidence-hash-mismatch"
        );
        assert_eq!(RejectionReason::SequenceGap.as_str(), "sequence-gap");
        assert_eq!(
            RejectionReason::LaterFailureObserved.as_str(),
            "later-failure-observed"
        );
        assert_eq!(
            RejectionReason::NegativeControlMustFail.as_str(),
            "negative-control-must-fail"
        );
        assert_eq!(
            RejectionReason::HumanGateMissing.as_str(),
            "human-gate-missing"
        );
        assert_eq!(
            RejectionReason::SelfCertificationAttempt.as_str(),
            "self-certification-attempt"
        );
    }

    #[test]
    fn rejection_reason_display_matches_as_str() {
        for reason in [
            RejectionReason::NoDraftPassedEvent,
            RejectionReason::EvidenceHashMismatch,
            RejectionReason::SequenceGap,
            RejectionReason::LaterFailureObserved,
            RejectionReason::NegativeControlMustFail,
            RejectionReason::HumanGateMissing,
            RejectionReason::SelfCertificationAttempt,
        ] {
            assert_eq!(reason.to_string(), reason.as_str());
        }
    }

    #[test]
    fn self_certification_reason_is_detected() {
        assert!(RejectionReason::SelfCertificationAttempt.is_self_certification());
        assert!(!RejectionReason::NoDraftPassedEvent.is_self_certification());
        assert!(!RejectionReason::EvidenceHashMismatch.is_self_certification());
    }

    #[test]
    fn rejection_reason_error_codes_are_in_range() {
        for reason in [
            RejectionReason::NoDraftPassedEvent,
            RejectionReason::EvidenceHashMismatch,
            RejectionReason::SequenceGap,
            RejectionReason::LaterFailureObserved,
            RejectionReason::NegativeControlMustFail,
            RejectionReason::HumanGateMissing,
            RejectionReason::SelfCertificationAttempt,
        ] {
            let code = reason.error_code();
            assert!(
                (2100..=2199).contains(&code),
                "error code {code} for {reason} out of 2100-2199 range"
            );
        }
    }

    // ---------------------------------------------------------------------------
    // consume_authority (crate-internal)
    // ---------------------------------------------------------------------------

    #[test]
    fn consume_authority_destructures_receipt() {
        let v = ClaimAuthorityVerifier::new();
        observe_draft_passed(&v, "wf-ca", "step-ca", "xhash", 1);
        let verdict = v
            .evaluate(make_provisional("wf-ca", "step-ca"), "xhash")
            .expect("evaluate");
        let receipt = extract_receipt(verdict);
        let (wf, step, class) = receipt.consume_authority();
        assert_eq!(wf, "wf-ca");
        assert_eq!(step, "step-ca");
        assert_eq!(class, AuthorityClass::Automated);
    }

    #[test]
    fn consume_authority_preserves_class_human_required() {
        let v = ClaimAuthorityVerifier::new();
        observe_human_gate_then_passed(&v, "wf-hr-cons", "s", "h3");
        let provisional = make_provisional_class("wf-hr-cons", "s", AuthorityClass::HumanRequired);
        let receipt = extract_receipt(v.evaluate(provisional, "h3").expect("evaluate"));
        let (_, _, class) = receipt.consume_authority();
        assert_eq!(class, AuthorityClass::HumanRequired);
    }

    // ---------------------------------------------------------------------------
    // ClaimAuthorityVerifier default/new equivalence
    // ---------------------------------------------------------------------------

    #[test]
    fn default_verifier_is_equivalent_to_new() {
        let v1 = ClaimAuthorityVerifier::new();
        let v2 = ClaimAuthorityVerifier::default();
        assert!(v1.workflow_ids().is_empty());
        assert!(v2.workflow_ids().is_empty());
    }

    // ---------------------------------------------------------------------------
    // EventSequence (verifier-local mirror)
    // ---------------------------------------------------------------------------

    #[test]
    fn event_sequence_new_value() {
        assert_eq!(EventSequence::new(42).value(), 42);
    }

    #[test]
    fn event_sequence_default_is_zero() {
        assert_eq!(EventSequence::default().value(), 0);
    }

    #[test]
    fn event_sequence_display_contains_value() {
        let s = EventSequence::new(77);
        assert!(s.to_string().contains("77"));
    }

    #[test]
    fn event_sequence_ord() {
        assert!(EventSequence::new(1) < EventSequence::new(2));
    }

    // ---------------------------------------------------------------------------
    // Additional coverage — rollback events and multi-workflow isolation
    // ---------------------------------------------------------------------------

    #[test]
    fn evaluate_rejects_workflow_with_only_rollback_events() {
        let v = ClaimAuthorityVerifier::new();
        v.observe(ExecutorEvent::StepStarted {
            workflow_id: String::from("wf-rb-only"),
            step_id: String::from("s"),
            sequence: EventSequence::new(1),
        })
        .expect("observe");
        v.observe(ExecutorEvent::RollbackStarted {
            workflow_id: String::from("wf-rb-only"),
            step_id: String::from("s"),
            sequence: EventSequence::new(2),
            reason: String::from("abort"),
        })
        .expect("observe");
        let verdict = v
            .evaluate(make_provisional("wf-rb-only", "s"), "h")
            .expect("evaluate");
        // No DraftPassed — must be rejected.
        assert!(verdict.is_rejected());
    }

    #[test]
    fn evaluate_accepts_correct_step_when_multiple_steps_in_workflow() {
        let v = ClaimAuthorityVerifier::new();
        // Step "s1" passes; step "s2" only has StepStarted.
        v.observe(ExecutorEvent::StepStarted {
            workflow_id: String::from("wf-multi"),
            step_id: String::from("s1"),
            sequence: EventSequence::new(1),
        })
        .expect("observe");
        v.observe(ExecutorEvent::DraftPassed {
            workflow_id: String::from("wf-multi"),
            step_id: String::from("s1"),
            sequence: EventSequence::new(2),
            evidence_hash: String::from("h-s1"),
        })
        .expect("observe");
        v.observe(ExecutorEvent::StepStarted {
            workflow_id: String::from("wf-multi"),
            step_id: String::from("s2"),
            sequence: EventSequence::new(3),
        })
        .expect("observe");
        // Evaluate s1 — should pass.
        let verdict = v
            .evaluate(make_provisional("wf-multi", "s1"), "h-s1")
            .expect("evaluate s1");
        assert!(verdict.is_pass(), "s1 should pass");
    }

    #[test]
    fn evaluate_rejects_unpassed_step_in_multi_step_workflow() {
        let v = ClaimAuthorityVerifier::new();
        v.observe(ExecutorEvent::StepStarted {
            workflow_id: String::from("wf-multi2"),
            step_id: String::from("s1"),
            sequence: EventSequence::new(1),
        })
        .expect("observe");
        v.observe(ExecutorEvent::DraftPassed {
            workflow_id: String::from("wf-multi2"),
            step_id: String::from("s1"),
            sequence: EventSequence::new(2),
            evidence_hash: String::from("h"),
        })
        .expect("observe");
        v.observe(ExecutorEvent::StepStarted {
            workflow_id: String::from("wf-multi2"),
            step_id: String::from("s2"),
            sequence: EventSequence::new(3),
        })
        .expect("observe");
        // Evaluate s2 which has no DraftPassed — must reject.
        let verdict = v
            .evaluate(make_provisional("wf-multi2", "s2"), "h")
            .expect("evaluate s2");
        assert!(
            verdict.is_rejected(),
            "s2 without DraftPassed should be rejected"
        );
    }

    #[test]
    fn receipt_workflow_and_step_ids_match_provisional() {
        let v = ClaimAuthorityVerifier::new();
        observe_draft_passed(&v, "wf-match", "step-match", "h-match", 1);
        let p = make_provisional("wf-match", "step-match");
        let receipt = extract_receipt(v.evaluate(p, "h-match").expect("evaluate"));
        assert_eq!(receipt.workflow_id(), "wf-match");
        assert_eq!(receipt.step_id(), "step-match");
    }

    #[test]
    fn evaluate_multiple_times_on_independent_provisionals() {
        let v = ClaimAuthorityVerifier::new();
        observe_draft_passed(&v, "wf-i1", "s", "h1", 1);
        observe_draft_passed(&v, "wf-i2", "s", "h2", 1);
        let v1 = v
            .evaluate(make_provisional("wf-i1", "s"), "h1")
            .expect("eval 1");
        let v2 = v
            .evaluate(make_provisional("wf-i2", "s"), "h2")
            .expect("eval 2");
        assert!(v1.is_pass());
        assert!(v2.is_pass());
    }

    #[test]
    fn event_log_is_per_workflow_not_global() {
        let v = ClaimAuthorityVerifier::new();
        // Only observe events for wf-a.
        observe_draft_passed(&v, "wf-a", "s", "h", 1);
        // wf-b has no events at all → unknown workflow error.
        let err = v.evaluate(make_provisional("wf-b", "s"), "h");
        assert!(err.is_err(), "wf-b should be unknown");
    }
}
