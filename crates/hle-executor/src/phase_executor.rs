//! M017 `PhaseExecutor` — phase-aware step sequencer.
//!
//! Runs a sequence of [`ExecutionPhase`] steps using [`LocalRunner`] (M016),
//! emits one JSONL [`Receipt`] per step, and halts on the first [`StepState::Failed`]
//! or [`StepState::AwaitingHuman`] outcome.
//!
//! The verifier (C01/C04) is the sole PASS authority; `PhaseExecutor` only
//! submits draft states.
//!
//! Error codes: 2220–2223.

use std::collections::HashSet;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use substrate_types::{Receipt, StepState};

use crate::bounded::{BoundedString, MAX_RECEIPT_MESSAGE_BYTES, MAX_STEP_LABEL_BYTES};
use crate::local_runner::{LocalRunner, RunnerError};
use crate::retry_policy::RetryPolicy;

// ── PhaseExecutorError ───────────────────────────────────────────────────────

/// Errors produced by M017 [`PhaseExecutor`] infrastructure operations.
///
/// Note: [`StepFailed`][Self::StepFailed] and
/// [`AwaitingHuman`][Self::AwaitingHuman] are used only for
/// [`PhaseSequence`] construction validation failures; step-level
/// `Failed`/`AwaitingHuman` outcomes are encoded in [`ExecutionResult`], not
/// returned as `Err` from [`PhaseExecutor::run_phases`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhaseExecutorError {
    /// `[HLE-2220]` `PhaseSequence` was empty.
    PhaseSequenceEmpty,
    /// `[HLE-2221]` A step failed validation during `PhaseSequence` construction.
    StepFailed {
        /// Phase the step belongs to.
        phase: ExecutionPhase,
        /// Step identifier.
        step_id: String,
        /// Validation failure message.
        message: String,
    },
    /// `[HLE-2222]` A step requires human confirmation (construction-time validation).
    AwaitingHuman {
        /// Phase the step belongs to.
        phase: ExecutionPhase,
        /// Step identifier.
        step_id: String,
    },
    /// `[HLE-2223]` JSONL ledger append failed.
    LedgerWriteFailed {
        /// OS or serialisation error.
        reason: String,
    },
}

impl PhaseExecutorError {
    /// HLE error code: 2220–2223.
    #[must_use]
    pub const fn error_code(&self) -> u32 {
        match self {
            Self::PhaseSequenceEmpty => 2220,
            Self::StepFailed { .. } => 2221,
            Self::AwaitingHuman { .. } => 2222,
            Self::LedgerWriteFailed { .. } => 2223,
        }
    }
}

impl fmt::Display for PhaseExecutorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PhaseSequenceEmpty => f.write_str("[HLE-2220] phase sequence is empty"),
            Self::StepFailed {
                phase,
                step_id,
                message,
            } => write!(
                f,
                "[HLE-2221] step failed: phase={phase} step={step_id} msg={message}"
            ),
            Self::AwaitingHuman { phase, step_id } => {
                write!(f, "[HLE-2222] awaiting human: phase={phase} step={step_id}")
            }
            Self::LedgerWriteFailed { reason } => {
                write!(f, "[HLE-2223] ledger write failed: {reason}")
            }
        }
    }
}

impl std::error::Error for PhaseExecutorError {}

// ── ExecutionPhase ────────────────────────────────────────────────────────────

/// The 8 phases defined in the HLE deployment framework §17.7.
///
/// Phase ordering is significant: Detect must precede Block, Fix precedes
/// Verify, `MetaTest` precedes Receipt, Receipt precedes Persist, Persist
/// precedes Notify.  [`PhaseSequence`] enforces non-decreasing ordering at
/// construction time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ExecutionPhase {
    /// Detect the condition that requires action.
    Detect = 0,
    /// Block further damage while remediation proceeds.
    Block = 1,
    /// Apply the fix.
    Fix = 2,
    /// Verify the fix was successful.
    Verify = 3,
    /// Run meta-tests to confirm no regressions.
    MetaTest = 4,
    /// Emit a verifier receipt.
    Receipt = 5,
    /// Persist the evidence.
    Persist = 6,
    /// Notify stakeholders.
    Notify = 7,
}

impl ExecutionPhase {
    /// Stable wire string for this phase.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Detect => "Detect",
            Self::Block => "Block",
            Self::Fix => "Fix",
            Self::Verify => "Verify",
            Self::MetaTest => "MetaTest",
            Self::Receipt => "Receipt",
            Self::Persist => "Persist",
            Self::Notify => "Notify",
        }
    }

    /// Zero-based index (0..=7).
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Reverse mapping from zero-based index.
    #[must_use]
    pub const fn from_index(i: usize) -> Option<Self> {
        match i {
            0 => Some(Self::Detect),
            1 => Some(Self::Block),
            2 => Some(Self::Fix),
            3 => Some(Self::Verify),
            4 => Some(Self::MetaTest),
            5 => Some(Self::Receipt),
            6 => Some(Self::Persist),
            7 => Some(Self::Notify),
            _ => None,
        }
    }

    /// Returns `true` for phases that are inherently verification-oriented
    /// (`Verify`, `MetaTest`, `Receipt`).
    #[must_use]
    pub const fn is_verification_phase(self) -> bool {
        matches!(self, Self::Verify | Self::MetaTest | Self::Receipt)
    }
}

impl fmt::Display for ExecutionPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── PhaseStep ────────────────────────────────────────────────────────────────

/// One step in a phase-aware execution sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhaseStep {
    /// Which phase this step belongs to.
    pub phase: ExecutionPhase,
    /// Unique identifier within the sequence.
    pub step_id: String,
    /// Human-readable label (bounded to [`MAX_STEP_LABEL_BYTES`]).
    pub label: BoundedString,
    /// Command string passed verbatim to [`LocalRunner`] (M016).
    pub command: String,
    /// Expected outcome — used by the verifier, not by M017 itself.
    pub expected_state: StepState,
    /// When `true`, skip the command and emit [`StepState::AwaitingHuman`].
    pub requires_human: bool,
}

impl PhaseStep {
    /// Construct a normal (non-human) step.
    ///
    /// # Errors
    ///
    /// Returns [`PhaseExecutorError::StepFailed`] when `step_id` or `label`
    /// is empty, or when the label exceeds [`MAX_STEP_LABEL_BYTES`].
    pub fn new(
        phase: ExecutionPhase,
        step_id: impl Into<String>,
        label: impl Into<String>,
        command: impl Into<String>,
        expected_state: StepState,
    ) -> Result<Self, PhaseExecutorError> {
        let step_id = step_id.into();
        let label_str = label.into();
        if step_id.trim().is_empty() {
            return Err(PhaseExecutorError::StepFailed {
                phase,
                step_id: step_id.clone(),
                message: String::from("step_id must not be empty"),
            });
        }
        let bounded_label = BoundedString::new(label_str, MAX_STEP_LABEL_BYTES).map_err(|err| {
            PhaseExecutorError::StepFailed {
                phase,
                step_id: step_id.clone(),
                message: err.to_string(),
            }
        })?;
        Ok(Self {
            phase,
            step_id,
            label: bounded_label,
            command: command.into(),
            expected_state,
            requires_human: false,
        })
    }

    /// Construct a human-confirmation step (no command is run).
    ///
    /// # Errors
    ///
    /// Returns [`PhaseExecutorError::AwaitingHuman`] when `step_id` is empty.
    pub fn awaiting_human(
        phase: ExecutionPhase,
        step_id: impl Into<String>,
        label: impl Into<String>,
    ) -> Result<Self, PhaseExecutorError> {
        let step_id = step_id.into();
        let label_str = label.into();
        if step_id.trim().is_empty() {
            return Err(PhaseExecutorError::AwaitingHuman {
                phase,
                step_id: step_id.clone(),
            });
        }
        let bounded_label = BoundedString::new(label_str, MAX_STEP_LABEL_BYTES).map_err(|err| {
            PhaseExecutorError::StepFailed {
                phase,
                step_id: step_id.clone(),
                message: err.to_string(),
            }
        })?;
        Ok(Self {
            phase,
            step_id,
            label: bounded_label,
            command: String::new(),
            expected_state: StepState::AwaitingHuman,
            requires_human: true,
        })
    }

    /// Returns `true` when this step requires human confirmation.
    #[must_use]
    pub fn is_human_step(&self) -> bool {
        self.requires_human
    }
}

// ── PhaseSequence ─────────────────────────────────────────────────────────────

/// An ordered, validated collection of [`PhaseStep`]s.
///
/// Invariants enforced at construction:
/// 1. At least one step.
/// 2. Phase ordering is non-decreasing.
/// 3. No duplicate `step_id`s.
#[derive(Debug, Clone)]
pub struct PhaseSequence {
    steps: Vec<PhaseStep>,
}

impl PhaseSequence {
    /// Construct and validate a `PhaseSequence`.
    ///
    /// # Errors
    ///
    /// Returns [`PhaseExecutorError::PhaseSequenceEmpty`] when `steps` is empty.
    /// Returns [`PhaseExecutorError::StepFailed`] when phase ordering is violated
    /// or a `step_id` is duplicated.
    pub fn new(steps: Vec<PhaseStep>) -> Result<Self, PhaseExecutorError> {
        if steps.is_empty() {
            return Err(PhaseExecutorError::PhaseSequenceEmpty);
        }
        // Validate non-decreasing phase ordering and unique step_ids.
        let mut prev_phase = steps[0].phase;
        let mut seen_ids: HashSet<&str> = HashSet::new();
        for step in &steps {
            if step.phase < prev_phase {
                return Err(PhaseExecutorError::StepFailed {
                    phase: step.phase,
                    step_id: step.step_id.clone(),
                    message: format!(
                        "phase ordering violated: {:?} follows {:?}",
                        step.phase, prev_phase
                    ),
                });
            }
            if !seen_ids.insert(step.step_id.as_str()) {
                return Err(PhaseExecutorError::StepFailed {
                    phase: step.phase,
                    step_id: step.step_id.clone(),
                    message: format!("duplicate step_id '{}'", step.step_id),
                });
            }
            prev_phase = step.phase;
        }
        Ok(Self { steps })
    }

    /// Ordered steps.
    #[must_use]
    pub fn steps(&self) -> &[PhaseStep] {
        &self.steps
    }

    /// Number of steps.
    #[must_use]
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// Returns `true` when the sequence contains no steps.
    ///
    /// Note: `PhaseSequence::new` rejects empty sequences, so this is always
    /// `false` for a successfully constructed value.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Sorted list of distinct phases present in the sequence.
    #[must_use]
    pub fn phases_present(&self) -> Vec<ExecutionPhase> {
        let mut phases: Vec<ExecutionPhase> = self
            .steps
            .iter()
            .map(|s| s.phase)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        phases.sort();
        phases
    }
}

// ── ExecutionResult ───────────────────────────────────────────────────────────

/// Verdict for a completed [`PhaseSequence`] run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionVerdict {
    /// All steps executed and none failed.
    Completed,
    /// Sequence halted early due to [`HaltReason`].
    Halted,
}

/// Why the sequence halted before the final step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HaltReason {
    /// A step returned [`StepState::Failed`].
    StepFailed,
    /// A step required human confirmation.
    AwaitingHuman,
}

/// Full result of a [`PhaseExecutor::run_phases`] call.
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// One receipt per executed step.
    pub receipts: Vec<Receipt>,
    /// Overall verdict.
    pub verdict: ExecutionVerdict,
    /// Set when the sequence halted before the final step.
    pub halt_reason: Option<HaltReason>,
}

impl ExecutionResult {
    /// Returns `true` when all steps completed without halting.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.verdict == ExecutionVerdict::Completed
    }

    /// Returns `true` when the sequence halted early.
    #[must_use]
    pub fn is_halted(&self) -> bool {
        self.verdict == ExecutionVerdict::Halted
    }

    /// The last receipt in the run, or `None` if no steps executed.
    #[must_use]
    pub fn last_receipt(&self) -> Option<&Receipt> {
        self.receipts.last()
    }

    /// All receipts belonging to `phase`.
    #[must_use]
    pub fn receipts_for_phase(&self, phase: ExecutionPhase) -> Vec<&Receipt> {
        self.receipts
            .iter()
            .filter(|r| r.workflow.starts_with(phase.as_str()))
            .collect()
    }

    /// Total number of executed steps.
    #[must_use]
    pub fn step_count(&self) -> usize {
        self.receipts.len()
    }

    /// Number of steps with [`StepState::Passed`].
    #[must_use]
    pub fn passed_count(&self) -> usize {
        self.receipts
            .iter()
            .filter(|r| r.state == StepState::Passed)
            .count()
    }

    /// Number of steps with [`StepState::Failed`].
    #[must_use]
    pub fn failed_count(&self) -> usize {
        self.receipts
            .iter()
            .filter(|r| r.state == StepState::Failed)
            .count()
    }

    // Private constructors used by PhaseExecutor.

    fn completed(receipts: Vec<Receipt>) -> Self {
        Self {
            receipts,
            verdict: ExecutionVerdict::Completed,
            halt_reason: None,
        }
    }

    fn halted(receipts: Vec<Receipt>, cause: StepState) -> Self {
        let halt_reason = match cause {
            StepState::Failed => Some(HaltReason::StepFailed),
            StepState::AwaitingHuman => Some(HaltReason::AwaitingHuman),
            _ => None,
        };
        Self {
            receipts,
            verdict: ExecutionVerdict::Halted,
            halt_reason,
        }
    }
}

// ── PhaseExecutor ─────────────────────────────────────────────────────────────

/// Executes a [`PhaseSequence`] using a [`LocalRunner`].
///
/// Stops at the first `Failed` or `AwaitingHuman` step.  Emits one JSONL
/// receipt per executed step to the configured ledger path.  The verifier
/// (C01/C04) is the sole PASS authority; `PhaseExecutor` only submits draft
/// states.
#[derive(Debug)]
pub struct PhaseExecutor {
    runner: LocalRunner,
    retry_policy: RetryPolicy,
    ledger_path: PathBuf,
}

impl PhaseExecutor {
    /// Construct a `PhaseExecutor`.
    pub fn new(
        runner: LocalRunner,
        retry_policy: RetryPolicy,
        ledger_path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            runner,
            retry_policy,
            ledger_path: ledger_path.into(),
        }
    }

    /// The configured ledger path.
    #[must_use]
    pub fn ledger_path(&self) -> &Path {
        &self.ledger_path
    }

    /// Run all steps in `sequence` in order.
    ///
    /// Returns `Ok(ExecutionResult)` for both all-pass and step-level
    /// `Failed`/`AwaitingHuman` outcomes.  Returns `Err` only for
    /// infrastructure failures (ledger write, unexpected OS error).
    ///
    /// # Errors
    ///
    /// Returns [`PhaseExecutorError::LedgerWriteFailed`] when the JSONL
    /// ledger cannot be written.
    pub fn run_phases(
        &self,
        sequence: &PhaseSequence,
    ) -> Result<ExecutionResult, PhaseExecutorError> {
        let mut receipts: Vec<Receipt> = Vec::with_capacity(sequence.len());

        for step in sequence.steps() {
            let (draft_state, message) = if step.requires_human {
                (StepState::AwaitingHuman, String::new())
            } else {
                self.run_step_with_retry(step)?
            };

            let receipt = build_receipt(step, draft_state, &message);
            append_jsonl_receipt(&self.ledger_path, &receipt).map_err(|err| {
                PhaseExecutorError::LedgerWriteFailed {
                    reason: err.to_string(),
                }
            })?;

            let should_halt = matches!(draft_state, StepState::Failed | StepState::AwaitingHuman);
            receipts.push(receipt);

            if should_halt {
                return Ok(ExecutionResult::halted(receipts, draft_state));
            }
        }

        Ok(ExecutionResult::completed(receipts))
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn run_step_with_retry(
        &self,
        step: &PhaseStep,
    ) -> Result<(StepState, String), PhaseExecutorError> {
        let mut budget = self.retry_policy.budget();
        loop {
            budget
                .next_attempt()
                .map_err(|err| PhaseExecutorError::StepFailed {
                    phase: step.phase,
                    step_id: step.step_id.clone(),
                    message: err.to_string(),
                })?;

            match self.runner.run(&step.command) {
                Ok(output) => {
                    return Ok((output.to_step_state(), output.combined_message));
                }
                Err(RunnerError::CommandRejected { reason }) => {
                    // Non-retryable — propagate immediately.
                    return Ok((
                        StepState::Failed,
                        format!("[HLE-2210] command rejected: {reason}"),
                    ));
                }
                Err(err) if err.is_retryable() && budget.has_remaining() => {
                    // Transient error with budget remaining — loop.
                }
                Err(err) => {
                    return Ok((StepState::Failed, err.to_string()));
                }
            }
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Build a draft [`Receipt`] for one step, bounding the message to
/// [`MAX_RECEIPT_MESSAGE_BYTES`].
fn build_receipt(step: &PhaseStep, state: StepState, message: &str) -> Receipt {
    let verifier_verdict = match state {
        StepState::Passed => "PASS",
        StepState::AwaitingHuman => "AWAITING_HUMAN",
        _ => "FAIL",
    };
    // Best-effort truncation; ignore error (only errors on zero cap, which is a
    // compile-time constant).
    let bounded_msg = BoundedString::new(message, MAX_RECEIPT_MESSAGE_BYTES).map_or_else(
        |_| message.chars().take(MAX_RECEIPT_MESSAGE_BYTES).collect(),
        super::bounded::BoundedString::into_string,
    );

    Receipt::new(
        step.phase.as_str(),
        &step.step_id,
        state,
        verifier_verdict,
        bounded_msg,
    )
}

/// Append one receipt as a JSONL line to the ledger at `path`.
///
/// Mirrors `substrate_emit::append_jsonl_receipt` without introducing a
/// cross-crate dependency. Includes `receipt_sha256` field for end-to-end
/// cryptographic integrity: `M008` `receipt_sha_verifier` recomputes the
/// digest on read and rejects mismatch with `[E2722]`.
fn append_jsonl_receipt(path: &Path, receipt: &Receipt) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let created_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let receipt_sha = receipt_sha256_hex(
        receipt.workflow.as_str(),
        receipt.step_id.as_str(),
        receipt.verifier_verdict.as_str(),
        receipt.state.as_str(),
        receipt.message.as_str(),
    );
    let line = format!(
        "{{\"schema\":\"hle.receipt.v1\",\"created_unix\":{created_unix},\"phase\":\"{phase}\",\"step_id\":\"{step_id}\",\"state\":\"{state}\",\"verdict\":\"{verdict}\",\"message\":{message:?},\"receipt_sha256\":\"{sha}\"}}\n",
        phase = json_escape(receipt.workflow.as_str()),
        step_id = json_escape(receipt.step_id.as_str()),
        state = receipt.state.as_str(),
        verdict = json_escape(receipt.verifier_verdict.as_str()),
        message = receipt.message,
        sha = receipt_sha,
    );
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(line.as_bytes())
}

/// Compute the SHA-256 hex digest over the canonical receipt bytes:
/// `workflow \x00 step_id \x00 verdict \x00 state \x00 message`.
/// Mirrors `substrate_emit::receipt_sha256_hex` so the runtime path and the
/// substrate path produce identical anchors for the same input.
fn receipt_sha256_hex(
    workflow: &str,
    step_id: &str,
    verdict: &str,
    state: &str,
    message: &str,
) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(workflow.as_bytes());
    hasher.update(b"\x00");
    hasher.update(step_id.as_bytes());
    hasher.update(b"\x00");
    hasher.update(verdict.as_bytes());
    hasher.update(b"\x00");
    hasher.update(state.as_bytes());
    hasher.update(b"\x00");
    hasher.update(message.as_bytes());
    let digest: [u8; 32] = hasher.finalize().into();
    let mut hex = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    hex
}

/// Escape a string for inclusion as a JSON string value.
fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_runner::RunnerConfig;

    fn make_runner() -> LocalRunner {
        LocalRunner::new(RunnerConfig::default_m0()).expect("runner ok")
    }

    fn detect_step(id: &str, cmd: &str) -> PhaseStep {
        PhaseStep::new(
            ExecutionPhase::Detect,
            id,
            "detect label",
            cmd,
            StepState::Passed,
        )
        .expect("valid step")
    }

    // ── ExecutionPhase ────────────────────────────────────────────────────────

    #[test]
    fn phase_detect_index_is_0() {
        assert_eq!(ExecutionPhase::Detect.index(), 0);
    }

    #[test]
    fn phase_notify_index_is_7() {
        assert_eq!(ExecutionPhase::Notify.index(), 7);
    }

    #[test]
    fn phase_from_index_roundtrip() {
        for i in 0..=7_usize {
            let phase = ExecutionPhase::from_index(i).expect("valid index");
            assert_eq!(phase.index(), i);
        }
    }

    #[test]
    fn phase_from_index_8_is_none() {
        assert!(ExecutionPhase::from_index(8).is_none());
    }

    #[test]
    fn verify_is_verification_phase() {
        assert!(ExecutionPhase::Verify.is_verification_phase());
    }

    #[test]
    fn detect_is_not_verification_phase() {
        assert!(!ExecutionPhase::Detect.is_verification_phase());
    }

    #[test]
    fn phase_display_matches_as_str() {
        assert_eq!(ExecutionPhase::Fix.to_string(), "Fix");
    }

    // ── PhaseStep ─────────────────────────────────────────────────────────────

    #[test]
    fn phase_step_new_rejects_empty_step_id() {
        let err = PhaseStep::new(
            ExecutionPhase::Detect,
            "",
            "label",
            "true",
            StepState::Passed,
        );
        assert!(err.is_err());
    }

    #[test]
    fn phase_step_awaiting_human_sets_flag() {
        let step =
            PhaseStep::awaiting_human(ExecutionPhase::Verify, "v1", "confirm").expect("valid step");
        assert!(step.is_human_step());
        assert_eq!(step.expected_state, StepState::AwaitingHuman);
    }

    // ── PhaseSequence ─────────────────────────────────────────────────────────

    #[test]
    fn phase_sequence_rejects_empty_steps() {
        let err = PhaseSequence::new(vec![]);
        assert!(matches!(err, Err(PhaseExecutorError::PhaseSequenceEmpty)));
    }

    #[test]
    fn phase_sequence_rejects_decreasing_phase_order() {
        let s1 = detect_step("s1", "true");
        let s2 = PhaseStep::new(
            ExecutionPhase::Detect,
            "s2",
            "label",
            "true",
            StepState::Passed,
        )
        .expect("valid");
        // Place Notify then Detect — ordering violation.
        let notify = PhaseStep::new(
            ExecutionPhase::Notify,
            "n1",
            "label",
            "true",
            StepState::Passed,
        )
        .expect("valid");
        let err = PhaseSequence::new(vec![notify, s1, s2]);
        assert!(err.is_err());
    }

    #[test]
    fn phase_sequence_rejects_duplicate_step_id() {
        let s1 = detect_step("dup", "true");
        let s2 = detect_step("dup", "true");
        let err = PhaseSequence::new(vec![s1, s2]);
        assert!(err.is_err());
    }

    #[test]
    fn phase_sequence_accepts_valid_steps() {
        let s1 = detect_step("s1", "true");
        let s2 = PhaseStep::new(
            ExecutionPhase::Fix,
            "s2",
            "fix label",
            "true",
            StepState::Passed,
        )
        .expect("valid");
        let seq = PhaseSequence::new(vec![s1, s2]).expect("valid sequence");
        assert_eq!(seq.len(), 2);
        assert!(!seq.is_empty());
    }

    #[test]
    fn phase_sequence_phases_present_sorted() {
        let s1 = detect_step("s1", "true");
        let s2 = PhaseStep::new(ExecutionPhase::Verify, "s2", "v", "true", StepState::Passed)
            .expect("valid");
        let seq = PhaseSequence::new(vec![s1, s2]).expect("valid");
        let phases = seq.phases_present();
        assert_eq!(phases, vec![ExecutionPhase::Detect, ExecutionPhase::Verify]);
    }

    // ── PhaseExecutorError ────────────────────────────────────────────────────

    #[test]
    fn phase_sequence_empty_error_code_is_2220() {
        assert_eq!(PhaseExecutorError::PhaseSequenceEmpty.error_code(), 2220);
    }

    #[test]
    fn step_failed_error_code_is_2221() {
        let e = PhaseExecutorError::StepFailed {
            phase: ExecutionPhase::Detect,
            step_id: String::from("s1"),
            message: String::from("boom"),
        };
        assert_eq!(e.error_code(), 2221);
    }

    #[test]
    fn awaiting_human_error_code_is_2222() {
        let e = PhaseExecutorError::AwaitingHuman {
            phase: ExecutionPhase::Verify,
            step_id: String::from("v1"),
        };
        assert_eq!(e.error_code(), 2222);
    }

    #[test]
    fn ledger_write_failed_error_code_is_2223() {
        let e = PhaseExecutorError::LedgerWriteFailed {
            reason: String::from("io error"),
        };
        assert_eq!(e.error_code(), 2223);
    }

    #[test]
    fn phase_sequence_empty_display_contains_hle_2220() {
        assert!(PhaseExecutorError::PhaseSequenceEmpty
            .to_string()
            .contains("[HLE-2220]"));
    }

    // ── PhaseExecutor integration ─────────────────────────────────────────────

    #[test]
    fn phase_executor_runs_passing_step() {
        let dir = std::env::temp_dir().join("hle-executor-test-pass");
        let ledger = dir.join("ledger.jsonl");
        let runner = make_runner();
        let executor = PhaseExecutor::new(runner, RetryPolicy::NO_RETRY, &ledger);
        let step = detect_step("s1", "true");
        let seq = PhaseSequence::new(vec![step]).expect("valid");
        let result = executor.run_phases(&seq).expect("no infra error");
        assert!(result.is_complete());
        assert_eq!(result.step_count(), 1);
        assert_eq!(result.passed_count(), 1);
    }

    #[test]
    fn phase_executor_halts_on_failing_step() {
        let dir = std::env::temp_dir().join("hle-executor-test-fail");
        let ledger = dir.join("ledger.jsonl");
        let runner = make_runner();
        let executor = PhaseExecutor::new(runner, RetryPolicy::NO_RETRY, &ledger);
        let step = detect_step("s1", "false");
        let seq = PhaseSequence::new(vec![step]).expect("valid");
        let result = executor.run_phases(&seq).expect("no infra error");
        assert!(result.is_halted());
        assert_eq!(result.halt_reason, Some(HaltReason::StepFailed));
    }

    // ── ExecutionPhase — all variants ─────────────────────────────────────────

    #[test]
    fn phase_all_indices_round_trip() {
        for i in 0..=7_usize {
            let phase = ExecutionPhase::from_index(i).expect("valid");
            assert_eq!(phase.index(), i, "round-trip failed for i={i}");
        }
    }

    #[test]
    fn phase_detect_as_str_is_detect() {
        assert_eq!(ExecutionPhase::Detect.as_str(), "Detect");
    }

    #[test]
    fn phase_block_as_str_is_block() {
        assert_eq!(ExecutionPhase::Block.as_str(), "Block");
    }

    #[test]
    fn phase_fix_as_str_is_fix() {
        assert_eq!(ExecutionPhase::Fix.as_str(), "Fix");
    }

    #[test]
    fn phase_verify_as_str_is_verify() {
        assert_eq!(ExecutionPhase::Verify.as_str(), "Verify");
    }

    #[test]
    fn phase_meta_test_as_str_is_metatest() {
        assert_eq!(ExecutionPhase::MetaTest.as_str(), "MetaTest");
    }

    #[test]
    fn phase_receipt_as_str_is_receipt() {
        assert_eq!(ExecutionPhase::Receipt.as_str(), "Receipt");
    }

    #[test]
    fn phase_persist_as_str_is_persist() {
        assert_eq!(ExecutionPhase::Persist.as_str(), "Persist");
    }

    #[test]
    fn phase_notify_as_str_is_notify() {
        assert_eq!(ExecutionPhase::Notify.as_str(), "Notify");
    }

    #[test]
    fn meta_test_is_verification_phase() {
        assert!(ExecutionPhase::MetaTest.is_verification_phase());
    }

    #[test]
    fn receipt_is_verification_phase() {
        assert!(ExecutionPhase::Receipt.is_verification_phase());
    }

    #[test]
    fn fix_is_not_verification_phase() {
        assert!(!ExecutionPhase::Fix.is_verification_phase());
    }

    #[test]
    fn block_is_not_verification_phase() {
        assert!(!ExecutionPhase::Block.is_verification_phase());
    }

    #[test]
    fn phase_ordering_detect_lt_notify() {
        assert!(ExecutionPhase::Detect < ExecutionPhase::Notify);
    }

    // ── PhaseStep — additional ────────────────────────────────────────────────

    #[test]
    fn phase_step_new_rejects_whitespace_only_step_id() {
        let err = PhaseStep::new(
            ExecutionPhase::Fix,
            "   ",
            "label",
            "true",
            StepState::Passed,
        );
        assert!(err.is_err());
    }

    #[test]
    fn phase_step_new_normal_step_not_human() {
        let step = PhaseStep::new(
            ExecutionPhase::Block,
            "b1",
            "block label",
            "true",
            StepState::Passed,
        )
        .expect("valid");
        assert!(!step.is_human_step());
        assert_eq!(step.expected_state, StepState::Passed);
    }

    #[test]
    fn phase_step_awaiting_human_rejects_empty_id() {
        let err = PhaseStep::awaiting_human(ExecutionPhase::Verify, "", "confirm");
        assert!(err.is_err());
    }

    #[test]
    fn phase_step_awaiting_human_has_empty_command() {
        let step =
            PhaseStep::awaiting_human(ExecutionPhase::Notify, "n1", "notify label").expect("valid");
        assert!(step.command.is_empty());
    }

    #[test]
    fn phase_step_label_bounded_to_512_bytes() {
        let long_label = "x".repeat(600);
        let step = PhaseStep::new(
            ExecutionPhase::Detect,
            "d1",
            long_label,
            "true",
            StepState::Passed,
        )
        .expect("long label — truncated, not rejected");
        // The label is bounded; we just verify it didn't error out.
        assert!(step.label.len() <= 512);
    }

    // ── PhaseSequence — additional ────────────────────────────────────────────

    #[test]
    fn phase_sequence_single_step_valid() {
        let s = detect_step("s1", "true");
        let seq = PhaseSequence::new(vec![s]).expect("valid");
        assert_eq!(seq.len(), 1);
        assert!(!seq.is_empty());
    }

    #[test]
    fn phase_sequence_equal_phases_allowed() {
        // Two steps in the same phase is valid (non-decreasing).
        let s1 = detect_step("s1", "true");
        let s2 = detect_step("s2", "true");
        let seq = PhaseSequence::new(vec![s1, s2]).expect("same-phase ok");
        assert_eq!(seq.len(), 2);
    }

    #[test]
    fn phase_sequence_all_8_phases_valid() {
        let steps: Vec<PhaseStep> = (0..=7_usize)
            .map(|i| {
                let phase = ExecutionPhase::from_index(i).expect("valid phase");
                PhaseStep::new(
                    phase,
                    format!("step-{i}"),
                    format!("label {i}"),
                    "true",
                    StepState::Passed,
                )
                .expect("valid step")
            })
            .collect();
        let seq = PhaseSequence::new(steps).expect("all 8 phases valid");
        assert_eq!(seq.len(), 8);
        assert_eq!(seq.phases_present().len(), 8);
    }

    #[test]
    fn phase_sequence_phases_present_deduplicates() {
        let s1 = detect_step("s1", "true");
        let s2 = detect_step("s2", "true");
        let seq = PhaseSequence::new(vec![s1, s2]).expect("ok");
        assert_eq!(seq.phases_present(), vec![ExecutionPhase::Detect]);
    }

    // ── ExecutionResult — additional ──────────────────────────────────────────

    #[test]
    fn execution_result_completed_is_not_halted() {
        let dir = std::env::temp_dir().join("hle-result-completed");
        let ledger = dir.join("ledger.jsonl");
        let runner = make_runner();
        let executor = PhaseExecutor::new(runner, RetryPolicy::NO_RETRY, &ledger);
        let step = detect_step("s1", "true");
        let seq = PhaseSequence::new(vec![step]).expect("ok");
        let result = executor.run_phases(&seq).expect("ok");
        assert!(!result.is_halted());
        assert_eq!(result.halt_reason, None);
    }

    #[test]
    fn execution_result_step_count_matches_completed() {
        let dir = std::env::temp_dir().join("hle-result-step-count");
        let ledger = dir.join("ledger.jsonl");
        let runner = make_runner();
        let executor = PhaseExecutor::new(runner, RetryPolicy::NO_RETRY, &ledger);
        let steps = vec![
            detect_step("s1", "true"),
            detect_step("s2", "true"),
            detect_step("s3", "true"),
        ];
        let seq = PhaseSequence::new(steps).expect("ok");
        let result = executor.run_phases(&seq).expect("ok");
        assert_eq!(result.step_count(), 3);
        assert_eq!(result.passed_count(), 3);
        assert_eq!(result.failed_count(), 0);
    }

    #[test]
    fn execution_result_halts_on_human_step() {
        let dir = std::env::temp_dir().join("hle-result-human-halt");
        let ledger = dir.join("ledger.jsonl");
        let runner = make_runner();
        let executor = PhaseExecutor::new(runner, RetryPolicy::NO_RETRY, &ledger);
        let human = PhaseStep::awaiting_human(ExecutionPhase::Verify, "v1", "confirm")
            .expect("valid human step");
        let seq = PhaseSequence::new(vec![human]).expect("ok");
        let result = executor.run_phases(&seq).expect("no infra error");
        assert!(result.is_halted());
        assert_eq!(result.halt_reason, Some(HaltReason::AwaitingHuman));
    }

    #[test]
    fn execution_result_last_receipt_some_for_completed_run() {
        let dir = std::env::temp_dir().join("hle-result-last-receipt");
        let ledger = dir.join("ledger.jsonl");
        let runner = make_runner();
        let executor = PhaseExecutor::new(runner, RetryPolicy::NO_RETRY, &ledger);
        let step = detect_step("s1", "true");
        let seq = PhaseSequence::new(vec![step]).expect("ok");
        let result = executor.run_phases(&seq).expect("ok");
        assert!(result.last_receipt().is_some());
    }

    #[test]
    fn execution_result_ledger_written_to_disk() {
        let dir = std::env::temp_dir().join("hle-result-ledger-disk");
        let ledger = dir.join("ledger.jsonl");
        // Remove if exists from prior run.
        let _ = std::fs::remove_file(&ledger);
        let runner = make_runner();
        let executor = PhaseExecutor::new(runner, RetryPolicy::NO_RETRY, &ledger);
        let step = detect_step("s1", "true");
        let seq = PhaseSequence::new(vec![step]).expect("ok");
        executor.run_phases(&seq).expect("ok");
        assert!(ledger.exists(), "ledger file should be created on disk");
    }

    #[test]
    fn execution_result_ledger_contains_jsonl_receipt() {
        let dir = std::env::temp_dir().join("hle-result-ledger-json");
        let ledger = dir.join("ledger.jsonl");
        let _ = std::fs::remove_file(&ledger);
        let runner = make_runner();
        let executor = PhaseExecutor::new(runner, RetryPolicy::NO_RETRY, &ledger);
        let step = detect_step("step-json", "true");
        let seq = PhaseSequence::new(vec![step]).expect("ok");
        executor.run_phases(&seq).expect("ok");
        let content = std::fs::read_to_string(&ledger).expect("ledger readable");
        assert!(content.contains("hle.receipt.v1"));
        assert!(content.contains("step-json"));
    }

    #[test]
    fn phase_executor_stops_after_first_failure_not_running_subsequent() {
        let dir = std::env::temp_dir().join("hle-executor-stop-on-fail");
        let ledger = dir.join("ledger.jsonl");
        let runner = make_runner();
        let executor = PhaseExecutor::new(runner, RetryPolicy::NO_RETRY, &ledger);
        // Sequence: fail, then two passing steps. Only the failing step runs.
        let s1 = detect_step("s1", "false");
        let s2 = detect_step("s2", "true");
        let s3 = detect_step("s3", "true");
        let seq = PhaseSequence::new(vec![s1, s2, s3]).expect("ok");
        let result = executor.run_phases(&seq).expect("ok");
        assert!(result.is_halted());
        // Only 1 receipt (the failing step).
        assert_eq!(result.step_count(), 1);
    }

    #[test]
    fn phase_executor_rejected_command_causes_failed_step() {
        let dir = std::env::temp_dir().join("hle-executor-rejected-cmd");
        let ledger = dir.join("ledger.jsonl");
        let runner = make_runner();
        let executor = PhaseExecutor::new(runner, RetryPolicy::NO_RETRY, &ledger);
        // "rm" is blocked — the step should show as Failed.
        let step = detect_step("s1", "rm -rf /");
        let seq = PhaseSequence::new(vec![step]).expect("ok");
        let result = executor.run_phases(&seq).expect("no infra error");
        assert!(result.is_halted());
        assert_eq!(result.halt_reason, Some(HaltReason::StepFailed));
    }
}
