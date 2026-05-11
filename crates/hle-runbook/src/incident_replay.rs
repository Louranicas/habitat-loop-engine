#![forbid(unsafe_code)]

//! M038 — Deterministic incident replay fixtures for meta-test phases.
//!
//! **Cluster:** C06 Runbook Semantics | **Layer:** L07 | **Error code:** 2580
//!
//! Every `IncidentFixture` in [`IncidentReplayRegistry::standard()`] is fully
//! deterministic — given the same `ReplayInput`, it produces the same
//! `VerifyTrace`.  There are no random seeds, no I/O calls, and no external
//! dependencies in this module.
//!
//! INV-C06-06: All 8 fixtures in the standard registry must have a non-empty
//! `expected_trace`.

use std::collections::HashMap;
use std::fmt;

use crate::schema::{FixtureId, PhaseKind, RunbookError};

// ── ReplayError ───────────────────────────────────────────────────────────────

/// Error produced during replay.  Error code 2580.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayError {
    /// Code 2580 — Fixture not found in registry.
    NotFound {
        /// ID that was not found.
        id: String,
    },
    /// Code 2581 — Trace did not match expected outcome.
    TraceMismatch {
        /// Human-readable mismatch description.
        reason: String,
    },
    /// Code 2582 — Input invariant violated.
    InvalidInput {
        /// Field that failed.
        field: &'static str,
        /// Reason.
        reason: String,
    },
}

impl ReplayError {
    /// Numeric error code.
    #[must_use]
    pub const fn error_code(&self) -> u16 {
        match self {
            Self::NotFound { .. } => 2580,
            Self::TraceMismatch { .. } => 2581,
            Self::InvalidInput { .. } => 2582,
        }
    }
}

impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { id } => write!(f, "[2580 ReplayNotFound] fixture '{id}' not found"),
            Self::TraceMismatch { reason } => {
                write!(f, "[2581 ReplayTraceMismatch] {reason}")
            }
            Self::InvalidInput { field, reason } => {
                write!(f, "[2582 ReplayInvalidInput] field '{field}': {reason}")
            }
        }
    }
}

impl std::error::Error for ReplayError {}

// ── ProbeOutcome ──────────────────────────────────────────────────────────────

/// The result of a probe execution within a replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeOutcome {
    /// Probe returned the expected value / status.
    Pass,
    /// Probe returned an unexpected value / status.
    Fail {
        /// Concise reason for the failure.
        reason: String,
    },
    /// Probe timed out before returning.
    Timeout,
    /// Probe was skipped (e.g. conditional gate not met).
    Skipped,
}

impl ProbeOutcome {
    /// Return `true` when the outcome is `Pass`.
    #[must_use]
    pub const fn is_pass(&self) -> bool {
        matches!(self, Self::Pass)
    }

    /// Return `true` when the outcome is `Fail`.
    #[must_use]
    pub const fn is_fail(&self) -> bool {
        matches!(self, Self::Fail { .. })
    }
}

impl fmt::Display for ProbeOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pass => f.write_str("pass"),
            Self::Fail { reason } => write!(f, "fail({reason})"),
            Self::Timeout => f.write_str("timeout"),
            Self::Skipped => f.write_str("skipped"),
        }
    }
}

// ── TraceEventKind ────────────────────────────────────────────────────────────

/// The kind of event recorded in a replay trace.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TraceEventKind {
    /// A phase started.
    PhaseStarted,
    /// A probe ran and produced an outcome.
    ProbeRan,
    /// A phase completed (all probes done).
    PhaseCompleted,
    /// Human confirmation was requested (M035 hook).
    HumanConfirmRequested,
    /// Human confirmation was granted.
    HumanConfirmGranted,
    /// Human confirmation was denied.
    HumanConfirmDenied,
    /// A safety gate triggered and blocked execution.
    SafetyGateTriggered,
    /// An error was emitted.
    ErrorEmitted,
    /// The replay reached the terminal state.
    Terminal,
}

impl TraceEventKind {
    /// Return the canonical string tag.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::PhaseStarted => "phase_started",
            Self::ProbeRan => "probe_ran",
            Self::PhaseCompleted => "phase_completed",
            Self::HumanConfirmRequested => "human_confirm_requested",
            Self::HumanConfirmGranted => "human_confirm_granted",
            Self::HumanConfirmDenied => "human_confirm_denied",
            Self::SafetyGateTriggered => "safety_gate_triggered",
            Self::ErrorEmitted => "error_emitted",
            Self::Terminal => "terminal",
        }
    }
}

impl fmt::Display for TraceEventKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── TraceEvent ────────────────────────────────────────────────────────────────

/// A single event recorded during a replay run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceEvent {
    /// Monotonic step index within this replay.
    pub step: u32,
    /// Kind of event.
    pub kind: TraceEventKind,
    /// Phase during which the event occurred.
    pub phase: Option<PhaseKind>,
    /// Optional probe outcome (present when `kind == ProbeRan`).
    pub probe_outcome: Option<ProbeOutcome>,
    /// Optional human-readable annotation.
    pub annotation: Option<String>,
}

impl TraceEvent {
    /// Construct a simple phase-started event.
    #[must_use]
    pub fn phase_started(step: u32, phase: PhaseKind) -> Self {
        Self {
            step,
            kind: TraceEventKind::PhaseStarted,
            phase: Some(phase),
            probe_outcome: None,
            annotation: None,
        }
    }

    /// Construct a probe-ran event.
    #[must_use]
    pub fn probe_ran(step: u32, phase: PhaseKind, outcome: ProbeOutcome) -> Self {
        Self {
            step,
            kind: TraceEventKind::ProbeRan,
            phase: Some(phase),
            probe_outcome: Some(outcome),
            annotation: None,
        }
    }

    /// Construct a phase-completed event.
    #[must_use]
    pub fn phase_completed(step: u32, phase: PhaseKind) -> Self {
        Self {
            step,
            kind: TraceEventKind::PhaseCompleted,
            phase: Some(phase),
            probe_outcome: None,
            annotation: None,
        }
    }

    /// Construct a terminal event.
    #[must_use]
    pub fn terminal(step: u32) -> Self {
        Self {
            step,
            kind: TraceEventKind::Terminal,
            phase: None,
            probe_outcome: None,
            annotation: None,
        }
    }

    /// Construct a human-confirm-requested event.
    #[must_use]
    pub fn human_confirm_requested(step: u32, phase: PhaseKind) -> Self {
        Self {
            step,
            kind: TraceEventKind::HumanConfirmRequested,
            phase: Some(phase),
            probe_outcome: None,
            annotation: None,
        }
    }

    /// Add an annotation to this event.
    #[must_use]
    pub fn annotated(mut self, note: impl Into<String>) -> Self {
        self.annotation = Some(note.into());
        self
    }
}

// ── VerifyTrace ────────────────────────────────────────────────────────────────

/// The complete trace produced by a replay run, used for assertion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyTrace {
    /// Ordered sequence of events recorded during replay.
    pub events: Vec<TraceEvent>,
    /// Whether the replay ended in a terminal pass state.
    pub passed: bool,
}

impl VerifyTrace {
    /// Return the number of events in this trace.
    #[must_use]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Return `true` when the trace contains no events.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Count events of a specific kind.
    #[must_use]
    pub fn count_kind(&self, kind: &TraceEventKind) -> usize {
        self.events.iter().filter(|e| &e.kind == kind).count()
    }

    /// Return `true` when the trace contains at least one event with `kind`.
    #[must_use]
    pub fn contains_kind(&self, kind: &TraceEventKind) -> bool {
        self.events.iter().any(|e| &e.kind == kind)
    }

    /// Assert that two traces match (same event sequence), returning `Err` on mismatch.
    ///
    /// # Errors
    ///
    /// Returns [`ReplayError::TraceMismatch`] when lengths differ or any event
    /// kind/phase pair does not match.
    pub fn assert_matches(&self, expected: &Self) -> Result<(), ReplayError> {
        if self.passed != expected.passed {
            return Err(ReplayError::TraceMismatch {
                reason: format!(
                    "passed mismatch: got {}, expected {}",
                    self.passed, expected.passed
                ),
            });
        }
        if self.events.len() != expected.events.len() {
            return Err(ReplayError::TraceMismatch {
                reason: format!(
                    "event count mismatch: got {}, expected {}",
                    self.events.len(),
                    expected.events.len()
                ),
            });
        }
        for (i, (got, exp)) in self.events.iter().zip(expected.events.iter()).enumerate() {
            if got.kind != exp.kind || got.phase != exp.phase {
                return Err(ReplayError::TraceMismatch {
                    reason: format!(
                        "event[{i}] mismatch: got ({}, {:?}), expected ({}, {:?})",
                        got.kind, got.phase, exp.kind, exp.phase
                    ),
                });
            }
        }
        Ok(())
    }
}

// ── ReplayInput ───────────────────────────────────────────────────────────────

/// Input to a replay run.
#[derive(Debug, Clone)]
pub struct ReplayInput {
    /// ID of the fixture to replay.
    pub fixture_id: FixtureId,
    /// Whether human confirmations should be auto-granted (`NoOp` mode).
    pub auto_confirm: bool,
    /// Simulated probe override map: probe index → outcome.
    /// If absent, the fixture's default outcomes are used.
    pub probe_overrides: HashMap<u32, ProbeOutcome>,
}

impl ReplayInput {
    /// Create a default replay input that auto-grants human confirmations and
    /// uses all fixture defaults.
    #[must_use]
    pub fn default_for(fixture_id: FixtureId) -> Self {
        Self {
            fixture_id,
            auto_confirm: true,
            probe_overrides: HashMap::new(),
        }
    }
}

// ── IncidentFixture ────────────────────────────────────────────────────────────

/// A fully deterministic replay fixture.
///
/// `replay()` is a pure function: no I/O, no state mutation.
#[derive(Debug, Clone)]
pub struct IncidentFixture {
    /// Stable identifier for this fixture.
    pub id: FixtureId,
    /// Human-readable name.
    pub name: &'static str,
    /// Brief description of what the fixture models.
    pub description: &'static str,
    /// The expected trace for a default (all-pass) run.
    pub expected_trace: VerifyTrace,
    /// Function that runs the fixture deterministically.
    #[allow(clippy::type_complexity)]
    replay_fn: fn(&ReplayInput) -> VerifyTrace,
}

impl IncidentFixture {
    /// Execute the fixture with `input` and return the resulting trace.
    #[must_use]
    pub fn replay(&self, input: &ReplayInput) -> VerifyTrace {
        (self.replay_fn)(input)
    }

    /// Return the fixture ID.
    #[must_use]
    pub fn id(&self) -> &FixtureId {
        &self.id
    }
}

// ── Fixture replay functions ───────────────────────────────────────────────────

/// Build the standard detect-only happy path trace.
fn replay_service_down_detect(input: &ReplayInput) -> VerifyTrace {
    let outcome = input
        .probe_overrides
        .get(&0)
        .cloned()
        .unwrap_or(ProbeOutcome::Pass);
    let passed = outcome.is_pass();
    VerifyTrace {
        events: vec![
            TraceEvent::phase_started(0, PhaseKind::Detect),
            TraceEvent::probe_ran(1, PhaseKind::Detect, outcome),
            TraceEvent::phase_completed(2, PhaseKind::Detect),
            TraceEvent::terminal(3),
        ],
        passed,
    }
}

/// Five-phase happy path: Detect→Block→Fix(confirm)→Verify→MetaTest.
fn replay_full_five_phase(input: &ReplayInput) -> VerifyTrace {
    let detect_out = input
        .probe_overrides
        .get(&0)
        .cloned()
        .unwrap_or(ProbeOutcome::Pass);
    let block_out = input
        .probe_overrides
        .get(&1)
        .cloned()
        .unwrap_or(ProbeOutcome::Pass);
    let fix_out = input
        .probe_overrides
        .get(&2)
        .cloned()
        .unwrap_or(ProbeOutcome::Pass);
    let verify_out = input
        .probe_overrides
        .get(&3)
        .cloned()
        .unwrap_or(ProbeOutcome::Pass);
    let meta_out = input
        .probe_overrides
        .get(&4)
        .cloned()
        .unwrap_or(ProbeOutcome::Pass);

    let passed = detect_out.is_pass()
        && block_out.is_pass()
        && fix_out.is_pass()
        && verify_out.is_pass()
        && meta_out.is_pass();

    let mut events = vec![
        TraceEvent::phase_started(0, PhaseKind::Detect),
        TraceEvent::probe_ran(1, PhaseKind::Detect, detect_out),
        TraceEvent::phase_completed(2, PhaseKind::Detect),
        TraceEvent::phase_started(3, PhaseKind::Block),
        TraceEvent::probe_ran(4, PhaseKind::Block, block_out),
        TraceEvent::phase_completed(5, PhaseKind::Block),
        TraceEvent::phase_started(6, PhaseKind::Fix),
        TraceEvent::human_confirm_requested(7, PhaseKind::Fix),
    ];
    if input.auto_confirm {
        events.push(TraceEvent {
            step: 8,
            kind: TraceEventKind::HumanConfirmGranted,
            phase: Some(PhaseKind::Fix),
            probe_outcome: None,
            annotation: None,
        });
    }
    events.extend([
        TraceEvent::probe_ran(9, PhaseKind::Fix, fix_out),
        TraceEvent::phase_completed(10, PhaseKind::Fix),
        TraceEvent::phase_started(11, PhaseKind::Verify),
        TraceEvent::probe_ran(12, PhaseKind::Verify, verify_out),
        TraceEvent::phase_completed(13, PhaseKind::Verify),
        TraceEvent::phase_started(14, PhaseKind::MetaTest),
        TraceEvent::probe_ran(15, PhaseKind::MetaTest, meta_out),
        TraceEvent::phase_completed(16, PhaseKind::MetaTest),
        TraceEvent::terminal(17),
    ]);
    VerifyTrace { events, passed }
}

/// High-error-rate: Detect fails, runbook aborts.
fn replay_high_error_rate_detect_fail(input: &ReplayInput) -> VerifyTrace {
    let outcome = input
        .probe_overrides
        .get(&0)
        .cloned()
        .unwrap_or_else(|| ProbeOutcome::Fail {
            reason: "error_rate=0.23 exceeds threshold=0.05".into(),
        });
    let passed = outcome.is_pass();
    VerifyTrace {
        events: vec![
            TraceEvent::phase_started(0, PhaseKind::Detect),
            TraceEvent::probe_ran(1, PhaseKind::Detect, outcome),
            TraceEvent::terminal(2),
        ],
        passed,
    }
}

/// Resource exhaustion: Detect→Block (disk gate passes)→Fix (OOM kill + confirm)→Verify.
fn replay_resource_exhaustion(input: &ReplayInput) -> VerifyTrace {
    let detect_out = input
        .probe_overrides
        .get(&0)
        .cloned()
        .unwrap_or(ProbeOutcome::Pass);
    let block_out = input
        .probe_overrides
        .get(&1)
        .cloned()
        .unwrap_or(ProbeOutcome::Pass);
    let fix_out = input
        .probe_overrides
        .get(&2)
        .cloned()
        .unwrap_or(ProbeOutcome::Pass);
    let verify_out = input
        .probe_overrides
        .get(&3)
        .cloned()
        .unwrap_or(ProbeOutcome::Pass);
    let passed =
        detect_out.is_pass() && block_out.is_pass() && fix_out.is_pass() && verify_out.is_pass();

    let mut events = vec![
        TraceEvent::phase_started(0, PhaseKind::Detect),
        TraceEvent::probe_ran(1, PhaseKind::Detect, detect_out).annotated("disk_used_pct=97"),
        TraceEvent::phase_completed(2, PhaseKind::Detect),
        TraceEvent::phase_started(3, PhaseKind::Block),
        TraceEvent::probe_ran(4, PhaseKind::Block, block_out).annotated("spread_path_closed=true"),
        TraceEvent::phase_completed(5, PhaseKind::Block),
        TraceEvent::phase_started(6, PhaseKind::Fix),
        TraceEvent::human_confirm_requested(7, PhaseKind::Fix),
    ];
    if input.auto_confirm {
        events.push(TraceEvent {
            step: 8,
            kind: TraceEventKind::HumanConfirmGranted,
            phase: Some(PhaseKind::Fix),
            probe_outcome: None,
            annotation: Some("auto-confirmed in test".into()),
        });
    }
    events.extend([
        TraceEvent::probe_ran(9, PhaseKind::Fix, fix_out).annotated("truncate_logs=ok"),
        TraceEvent::phase_completed(10, PhaseKind::Fix),
        TraceEvent::phase_started(11, PhaseKind::Verify),
        TraceEvent::probe_ran(12, PhaseKind::Verify, verify_out).annotated("disk_used_pct=42"),
        TraceEvent::phase_completed(13, PhaseKind::Verify),
        TraceEvent::terminal(14),
    ]);
    VerifyTrace { events, passed }
}

/// Certificate expiry: Detect→Fix (renew cert, no block phase)→Verify.
fn replay_certificate_expiry(input: &ReplayInput) -> VerifyTrace {
    let detect_out = input
        .probe_overrides
        .get(&0)
        .cloned()
        .unwrap_or(ProbeOutcome::Pass);
    let fix_out = input
        .probe_overrides
        .get(&1)
        .cloned()
        .unwrap_or(ProbeOutcome::Pass);
    let verify_out = input
        .probe_overrides
        .get(&2)
        .cloned()
        .unwrap_or(ProbeOutcome::Pass);
    let passed = detect_out.is_pass() && fix_out.is_pass() && verify_out.is_pass();

    let mut events = vec![
        TraceEvent::phase_started(0, PhaseKind::Detect),
        TraceEvent::probe_ran(1, PhaseKind::Detect, detect_out).annotated("cert_expires_in_days=2"),
        TraceEvent::phase_completed(2, PhaseKind::Detect),
        TraceEvent::phase_started(3, PhaseKind::Fix),
        TraceEvent::human_confirm_requested(4, PhaseKind::Fix),
    ];
    if input.auto_confirm {
        events.push(TraceEvent {
            step: 5,
            kind: TraceEventKind::HumanConfirmGranted,
            phase: Some(PhaseKind::Fix),
            probe_outcome: None,
            annotation: None,
        });
    }
    events.extend([
        TraceEvent::probe_ran(6, PhaseKind::Fix, fix_out).annotated("certbot_renew=ok"),
        TraceEvent::phase_completed(7, PhaseKind::Fix),
        TraceEvent::phase_started(8, PhaseKind::Verify),
        TraceEvent::probe_ran(9, PhaseKind::Verify, verify_out)
            .annotated("cert_expires_in_days=89"),
        TraceEvent::phase_completed(10, PhaseKind::Verify),
        TraceEvent::terminal(11),
    ]);
    VerifyTrace { events, passed }
}

/// Configuration drift: Detect→Block (drift gate)→Fix (rollback)→Verify.
fn replay_configuration_drift(input: &ReplayInput) -> VerifyTrace {
    let detect_out = input
        .probe_overrides
        .get(&0)
        .cloned()
        .unwrap_or(ProbeOutcome::Pass);
    let block_out = input
        .probe_overrides
        .get(&1)
        .cloned()
        .unwrap_or(ProbeOutcome::Pass);
    let fix_out = input
        .probe_overrides
        .get(&2)
        .cloned()
        .unwrap_or(ProbeOutcome::Pass);
    let verify_out = input
        .probe_overrides
        .get(&3)
        .cloned()
        .unwrap_or(ProbeOutcome::Pass);
    let passed =
        detect_out.is_pass() && block_out.is_pass() && fix_out.is_pass() && verify_out.is_pass();

    let mut events = vec![
        TraceEvent::phase_started(0, PhaseKind::Detect),
        TraceEvent::probe_ran(1, PhaseKind::Detect, detect_out)
            .annotated("config_hash_mismatch=true"),
        TraceEvent::phase_completed(2, PhaseKind::Detect),
        TraceEvent::phase_started(3, PhaseKind::Block),
        TraceEvent::probe_ran(4, PhaseKind::Block, block_out)
            .annotated("no_downstream_writes=true"),
        TraceEvent::phase_completed(5, PhaseKind::Block),
        TraceEvent::phase_started(6, PhaseKind::Fix),
        TraceEvent::human_confirm_requested(7, PhaseKind::Fix),
    ];
    if input.auto_confirm {
        events.push(TraceEvent {
            step: 8,
            kind: TraceEventKind::HumanConfirmGranted,
            phase: Some(PhaseKind::Fix),
            probe_outcome: None,
            annotation: None,
        });
    }
    events.extend([
        TraceEvent::probe_ran(9, PhaseKind::Fix, fix_out).annotated("git_rollback=ok"),
        TraceEvent::phase_completed(10, PhaseKind::Fix),
        TraceEvent::phase_started(11, PhaseKind::Verify),
        TraceEvent::probe_ran(12, PhaseKind::Verify, verify_out)
            .annotated("config_hash_mismatch=false"),
        TraceEvent::phase_completed(13, PhaseKind::Verify),
        TraceEvent::terminal(14),
    ]);
    VerifyTrace { events, passed }
}

/// Security anomaly: Detect→safety gate triggers (quarantine).
fn replay_security_anomaly_gate(input: &ReplayInput) -> VerifyTrace {
    let detect_out = input
        .probe_overrides
        .get(&0)
        .cloned()
        .unwrap_or(ProbeOutcome::Pass);
    // Safety gate always triggers in this fixture (no override).
    let passed = detect_out.is_pass();
    VerifyTrace {
        events: vec![
            TraceEvent::phase_started(0, PhaseKind::Detect),
            TraceEvent::probe_ran(1, PhaseKind::Detect, detect_out).annotated("anomaly_score=0.92"),
            TraceEvent::phase_completed(2, PhaseKind::Detect),
            TraceEvent {
                step: 3,
                kind: TraceEventKind::SafetyGateTriggered,
                phase: Some(PhaseKind::Block),
                probe_outcome: None,
                annotation: Some("quarantine_initiated=true".into()),
            },
            TraceEvent::terminal(4),
        ],
        passed,
    }
}

/// Database unavailable: Detect→Block→Fix (failover)→Verify.
fn replay_database_unavailable(input: &ReplayInput) -> VerifyTrace {
    let detect_out = input
        .probe_overrides
        .get(&0)
        .cloned()
        .unwrap_or(ProbeOutcome::Pass);
    let block_out = input
        .probe_overrides
        .get(&1)
        .cloned()
        .unwrap_or(ProbeOutcome::Pass);
    let fix_out = input
        .probe_overrides
        .get(&2)
        .cloned()
        .unwrap_or(ProbeOutcome::Pass);
    let verify_out = input
        .probe_overrides
        .get(&3)
        .cloned()
        .unwrap_or(ProbeOutcome::Pass);
    let passed =
        detect_out.is_pass() && block_out.is_pass() && fix_out.is_pass() && verify_out.is_pass();

    let mut events = vec![
        TraceEvent::phase_started(0, PhaseKind::Detect),
        TraceEvent::probe_ran(1, PhaseKind::Detect, detect_out)
            .annotated("primary_replica_reachable=false"),
        TraceEvent::phase_completed(2, PhaseKind::Detect),
        TraceEvent::phase_started(3, PhaseKind::Block),
        TraceEvent::probe_ran(4, PhaseKind::Block, block_out).annotated("writes_paused=true"),
        TraceEvent::phase_completed(5, PhaseKind::Block),
        TraceEvent::phase_started(6, PhaseKind::Fix),
        TraceEvent::human_confirm_requested(7, PhaseKind::Fix),
    ];
    if input.auto_confirm {
        events.push(TraceEvent {
            step: 8,
            kind: TraceEventKind::HumanConfirmGranted,
            phase: Some(PhaseKind::Fix),
            probe_outcome: None,
            annotation: None,
        });
    }
    events.extend([
        TraceEvent::probe_ran(9, PhaseKind::Fix, fix_out).annotated("promote_replica=ok"),
        TraceEvent::phase_completed(10, PhaseKind::Fix),
        TraceEvent::phase_started(11, PhaseKind::Verify),
        TraceEvent::probe_ran(12, PhaseKind::Verify, verify_out)
            .annotated("primary_replica_reachable=true"),
        TraceEvent::phase_completed(13, PhaseKind::Verify),
        TraceEvent::terminal(14),
    ]);
    VerifyTrace { events, passed }
}

// ── IncidentReplayRegistry ────────────────────────────────────────────────────

/// Registry of deterministic incident replay fixtures.
///
/// Use [`IncidentReplayRegistry::standard()`] for the canonical 8-fixture set.
#[derive(Debug)]
pub struct IncidentReplayRegistry {
    fixtures: HashMap<String, IncidentFixture>,
}

impl IncidentReplayRegistry {
    /// Create an empty registry (no fixtures).
    #[must_use]
    pub fn empty() -> Self {
        Self {
            fixtures: HashMap::new(),
        }
    }

    /// The canonical 8-fixture registry satisfying INV-C06-06.
    ///
    /// All fixtures have non-empty `expected_trace`.
    ///
    /// # Errors
    ///
    /// Returns [`RunbookError`] if any fixture ID is malformed (should never
    /// happen for the hardcoded standard IDs).
    pub fn standard() -> Result<Self, RunbookError> {
        let make = |raw: &str,
                    name: &'static str,
                    description: &'static str,
                    replay_fn: fn(&ReplayInput) -> VerifyTrace|
         -> Result<IncidentFixture, RunbookError> {
            let id = FixtureId::new(raw)?;
            let default_input = ReplayInput::default_for(FixtureId::new(raw)?);
            Ok(IncidentFixture {
                id,
                name,
                description,
                expected_trace: replay_fn(&default_input),
                replay_fn,
            })
        };

        let all = [
            make(
                "s112-bridge-breaker-port-drift",
                "s112-bridge-breaker-port-drift",
                "S112: circuit-breaker port drifts; Detect→Block→Fix(confirm)→Verify.",
                replay_service_down_detect,
            )?,
            make(
                "me-eventbus-dark-traffic",
                "me-eventbus-dark-traffic",
                "ME EventBus dark traffic: all 5 canonical phases execute with all probes passing.",
                replay_full_five_phase,
            )?,
            make(
                "povm-write-only-readback-zero",
                "povm-write-only-readback-zero",
                "POVM write-only readback zero: Detect probe fails, runbook aborts.",
                replay_high_error_rate_detect_fail,
            )?,
            make(
                "s117-ttl-sweep-deletes-legitimate",
                "s117-ttl-sweep-deletes-legitimate",
                "S117 TTL sweep deletes legitimate entries: Detect→Block→Fix(confirm)→Verify.",
                replay_resource_exhaustion,
            )?,
            make(
                "port-retirement-tombstone-collision",
                "port-retirement-tombstone-collision",
                "Port retirement tombstone collision: Detect→Fix(renew cert)→Verify.",
                replay_certificate_expiry,
            )?,
            make(
                "devenv-batch-dependency-failure",
                "devenv-batch-dependency-failure",
                "devenv batch dependency failure: Detect→Block(drift gate)→Fix(rollback)→Verify.",
                replay_configuration_drift,
            )?,
            make(
                "synthex-thermal-saturation-runaway",
                "synthex-thermal-saturation-runaway",
                "SYNTHEX thermal saturation runaway: Detect→SafetyGate triggers quarantine.",
                replay_security_anomaly_gate,
            )?,
            make(
                "concurrent-markdown-write-conflict",
                "concurrent-markdown-write-conflict",
                "Concurrent markdown write conflict: Detect→Block(pause writes)→Fix(failover)→Verify.",
                replay_database_unavailable,
            )?,
        ];

        let mut fixtures = HashMap::new();
        for fixture in all {
            fixtures.insert(fixture.id.as_str().to_owned(), fixture);
        }
        Ok(Self { fixtures })
    }

    /// Look up a fixture by ID.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&IncidentFixture> {
        self.fixtures.get(id)
    }

    /// Run a fixture by ID.
    ///
    /// # Errors
    ///
    /// Returns [`ReplayError::NotFound`] when `input.fixture_id` is not in
    /// this registry.
    pub fn run(&self, input: &ReplayInput) -> Result<VerifyTrace, ReplayError> {
        let fixture = self
            .fixtures
            .get(input.fixture_id.as_str())
            .ok_or_else(|| ReplayError::NotFound {
                id: input.fixture_id.as_str().to_owned(),
            })?;
        Ok(fixture.replay(input))
    }

    /// Return the number of fixtures in the registry.
    #[must_use]
    pub fn len(&self) -> usize {
        self.fixtures.len()
    }

    /// Return `true` when the registry contains no fixtures.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fixtures.is_empty()
    }

    /// Return all fixture IDs (sorted for determinism).
    #[must_use]
    pub fn all_ids(&self) -> Vec<&str> {
        let mut ids: Vec<&str> = self.fixtures.keys().map(String::as_str).collect();
        ids.sort_unstable();
        ids
    }

    /// INV-C06-06: verify that every fixture has a non-empty `expected_trace`.
    #[must_use]
    pub fn inv_c06_06_all_fixtures_have_expected_trace(&self) -> bool {
        self.fixtures.values().all(|f| !f.expected_trace.is_empty())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{
        IncidentReplayRegistry, ProbeOutcome, ReplayError, ReplayInput, TraceEvent, TraceEventKind,
        VerifyTrace,
    };
    use crate::schema::{FixtureId, PhaseKind};

    // Framework §17.8 canonical fixture IDs.
    const F_S112: &str = "s112-bridge-breaker-port-drift";
    const F_ME: &str = "me-eventbus-dark-traffic";
    const F_POVM: &str = "povm-write-only-readback-zero";
    const F_S117: &str = "s117-ttl-sweep-deletes-legitimate";
    const F_PORT: &str = "port-retirement-tombstone-collision";
    const F_DEVENV: &str = "devenv-batch-dependency-failure";
    const F_THERMAL: &str = "synthex-thermal-saturation-runaway";
    const F_MD: &str = "concurrent-markdown-write-conflict";

    const ALL_CANONICAL_IDS: [&str; 8] = [
        F_S112, F_ME, F_POVM, F_S117, F_PORT, F_DEVENV, F_THERMAL, F_MD,
    ];

    fn make_id(s: &str) -> FixtureId {
        FixtureId::new(s).expect("valid fixture id")
    }

    fn registry() -> IncidentReplayRegistry {
        IncidentReplayRegistry::standard().unwrap_or_else(|e| {
            eprintln!("FATAL: IncidentReplayRegistry::standard() failed: {e}");
            IncidentReplayRegistry::empty()
        })
    }

    // ── INV-C06-06 ────────────────────────────────────────────────────────────

    #[test]
    fn standard_registry_has_eight_fixtures() {
        assert_eq!(registry().len(), 8);
    }

    #[test]
    fn inv_c06_06_all_fixtures_have_expected_trace() {
        assert!(
            registry().inv_c06_06_all_fixtures_have_expected_trace(),
            "INV-C06-06: all fixtures must have non-empty expected_trace"
        );
    }

    #[test]
    fn all_canonical_fixture_ids_present() {
        let r = registry();
        for id in &ALL_CANONICAL_IDS {
            assert!(
                r.get(id).is_some(),
                "canonical fixture '{id}' not found in standard registry"
            );
        }
    }

    #[test]
    fn framework_s112_bridge_breaker_present() {
        assert!(registry().get(F_S112).is_some());
    }

    #[test]
    fn framework_me_eventbus_present() {
        assert!(registry().get(F_ME).is_some());
    }

    #[test]
    fn framework_povm_write_only_present() {
        assert!(registry().get(F_POVM).is_some());
    }

    #[test]
    fn framework_s117_ttl_sweep_present() {
        assert!(registry().get(F_S117).is_some());
    }

    #[test]
    fn framework_port_retirement_present() {
        assert!(registry().get(F_PORT).is_some());
    }

    #[test]
    fn framework_devenv_batch_present() {
        assert!(registry().get(F_DEVENV).is_some());
    }

    #[test]
    fn framework_synthex_thermal_present() {
        assert!(registry().get(F_THERMAL).is_some());
    }

    #[test]
    fn framework_concurrent_markdown_present() {
        assert!(registry().get(F_MD).is_some());
    }

    // ── Registry operations ───────────────────────────────────────────────────

    #[test]
    fn not_found_returns_error_2580() {
        let r = registry();
        let input = ReplayInput::default_for(make_id("no-such-fixture"));
        let err = r.run(&input).unwrap_err();
        assert_eq!(err.error_code(), 2580);
        assert!(matches!(err, ReplayError::NotFound { .. }));
    }

    #[test]
    fn registry_is_not_empty() {
        assert!(!registry().is_empty());
    }

    #[test]
    fn all_ids_sorted() {
        let reg = registry();
        let ids = reg.all_ids();
        for pair in ids.windows(2) {
            assert!(
                pair[0] < pair[1],
                "ids not sorted: {} >= {}",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn empty_registry_has_zero_len() {
        let r = IncidentReplayRegistry::empty();
        assert_eq!(r.len(), 0);
        assert!(r.is_empty());
    }

    #[test]
    fn empty_registry_get_returns_none() {
        let r = IncidentReplayRegistry::empty();
        assert!(r.get(F_S112).is_none());
    }

    #[test]
    fn empty_registry_all_ids_empty() {
        let r = IncidentReplayRegistry::empty();
        assert!(r.all_ids().is_empty());
    }

    // ── Fixture s112: s112-bridge-breaker-port-drift ─────────────────────────

    #[test]
    fn s112_default_run_passes() {
        let trace = registry()
            .run(&ReplayInput::default_for(make_id(F_S112)))
            .expect("ok");
        assert!(trace.passed);
    }

    #[test]
    fn s112_default_run_has_terminal_event() {
        let trace = registry()
            .run(&ReplayInput::default_for(make_id(F_S112)))
            .expect("ok");
        assert!(trace.contains_kind(&TraceEventKind::Terminal));
    }

    #[test]
    fn s112_probe_fail_override_causes_failure() {
        let mut input = ReplayInput::default_for(make_id(F_S112));
        input.probe_overrides.insert(
            0,
            ProbeOutcome::Fail {
                reason: "port closed".into(),
            },
        );
        let trace = registry().run(&input).expect("ok");
        assert!(!trace.passed);
    }

    #[test]
    fn s112_matches_expected_trace() {
        let r = registry();
        let fixture = r.get(F_S112).expect("found");
        let input = ReplayInput::default_for(make_id(F_S112));
        let trace = fixture.replay(&input);
        trace
            .assert_matches(&fixture.expected_trace)
            .expect("should match");
    }

    #[test]
    fn s112_default_has_detect_phase_started() {
        let trace = registry()
            .run(&ReplayInput::default_for(make_id(F_S112)))
            .expect("ok");
        assert!(trace.events.iter().any(|e| {
            e.kind == TraceEventKind::PhaseStarted && e.phase == Some(PhaseKind::Detect)
        }));
    }

    // ── Fixture me: me-eventbus-dark-traffic ─────────────────────────────────

    #[test]
    fn me_default_run_passes() {
        let trace = registry()
            .run(&ReplayInput::default_for(make_id(F_ME)))
            .expect("ok");
        assert!(trace.passed);
    }

    #[test]
    fn me_contains_human_confirm_requested() {
        let trace = registry()
            .run(&ReplayInput::default_for(make_id(F_ME)))
            .expect("ok");
        assert!(trace.contains_kind(&TraceEventKind::HumanConfirmRequested));
    }

    #[test]
    fn me_auto_confirm_grants_confirmation() {
        let trace = registry()
            .run(&ReplayInput::default_for(make_id(F_ME)))
            .expect("ok");
        assert!(trace.contains_kind(&TraceEventKind::HumanConfirmGranted));
    }

    #[test]
    fn me_contains_five_phase_started_events() {
        let trace = registry()
            .run(&ReplayInput::default_for(make_id(F_ME)))
            .expect("ok");
        assert_eq!(trace.count_kind(&TraceEventKind::PhaseStarted), 5);
    }

    #[test]
    fn me_no_autoconfirm_missing_granted_event() {
        let mut input = ReplayInput::default_for(make_id(F_ME));
        input.auto_confirm = false;
        let trace = registry().run(&input).expect("ok");
        assert!(!trace.contains_kind(&TraceEventKind::HumanConfirmGranted));
    }

    // ── Fixture povm: povm-write-only-readback-zero ───────────────────────────

    #[test]
    fn povm_default_run_fails() {
        let trace = registry()
            .run(&ReplayInput::default_for(make_id(F_POVM)))
            .expect("ok");
        assert!(!trace.passed, "povm fixture should fail by default");
    }

    #[test]
    fn povm_probe_pass_override_causes_pass() {
        let mut input = ReplayInput::default_for(make_id(F_POVM));
        input.probe_overrides.insert(0, ProbeOutcome::Pass);
        let trace = registry().run(&input).expect("ok");
        assert!(trace.passed);
    }

    #[test]
    fn povm_default_has_terminal_event() {
        let trace = registry()
            .run(&ReplayInput::default_for(make_id(F_POVM)))
            .expect("ok");
        assert!(trace.contains_kind(&TraceEventKind::Terminal));
    }

    // ── Fixture s117: s117-ttl-sweep-deletes-legitimate ───────────────────────

    #[test]
    fn s117_default_run_passes() {
        let trace = registry()
            .run(&ReplayInput::default_for(make_id(F_S117)))
            .expect("ok");
        assert!(trace.passed);
    }

    #[test]
    fn s117_has_human_confirm_granted() {
        let trace = registry()
            .run(&ReplayInput::default_for(make_id(F_S117)))
            .expect("ok");
        assert!(trace.contains_kind(&TraceEventKind::HumanConfirmGranted));
    }

    #[test]
    fn s117_fix_probe_fail_causes_overall_fail() {
        let mut input = ReplayInput::default_for(make_id(F_S117));
        input.probe_overrides.insert(
            2,
            ProbeOutcome::Fail {
                reason: "truncate failed".into(),
            },
        );
        let trace = registry().run(&input).expect("ok");
        assert!(!trace.passed);
    }

    // ── Fixture port: port-retirement-tombstone-collision ────────────────────

    #[test]
    fn port_default_run_passes() {
        let trace = registry()
            .run(&ReplayInput::default_for(make_id(F_PORT)))
            .expect("ok");
        assert!(trace.passed);
    }

    #[test]
    fn port_has_human_confirm() {
        let trace = registry()
            .run(&ReplayInput::default_for(make_id(F_PORT)))
            .expect("ok");
        assert!(trace.contains_kind(&TraceEventKind::HumanConfirmRequested));
    }

    #[test]
    fn port_matches_expected_trace() {
        let r = registry();
        let fixture = r.get(F_PORT).expect("found");
        let input = ReplayInput::default_for(make_id(F_PORT));
        fixture
            .replay(&input)
            .assert_matches(&fixture.expected_trace)
            .expect("match");
    }

    // ── Fixture devenv: devenv-batch-dependency-failure ───────────────────────

    #[test]
    fn devenv_default_run_passes() {
        let trace = registry()
            .run(&ReplayInput::default_for(make_id(F_DEVENV)))
            .expect("ok");
        assert!(trace.passed);
    }

    #[test]
    fn devenv_block_probe_fail_causes_fail() {
        let mut input = ReplayInput::default_for(make_id(F_DEVENV));
        input.probe_overrides.insert(
            1,
            ProbeOutcome::Fail {
                reason: "gate not closed".into(),
            },
        );
        let trace = registry().run(&input).expect("ok");
        assert!(!trace.passed);
    }

    // ── Fixture thermal: synthex-thermal-saturation-runaway ───────────────────

    #[test]
    fn thermal_contains_safety_gate_triggered() {
        let trace = registry()
            .run(&ReplayInput::default_for(make_id(F_THERMAL)))
            .expect("ok");
        assert!(trace.contains_kind(&TraceEventKind::SafetyGateTriggered));
    }

    #[test]
    fn thermal_default_run_passes_detect() {
        let trace = registry()
            .run(&ReplayInput::default_for(make_id(F_THERMAL)))
            .expect("ok");
        assert!(trace.passed);
    }

    #[test]
    fn thermal_detect_fail_causes_overall_fail() {
        let mut input = ReplayInput::default_for(make_id(F_THERMAL));
        input.probe_overrides.insert(
            0,
            ProbeOutcome::Fail {
                reason: "anomaly score below threshold".into(),
            },
        );
        let trace = registry().run(&input).expect("ok");
        assert!(!trace.passed);
    }

    #[test]
    fn thermal_matches_expected_trace() {
        let r = registry();
        let fixture = r.get(F_THERMAL).expect("found");
        let input = ReplayInput::default_for(make_id(F_THERMAL));
        fixture
            .replay(&input)
            .assert_matches(&fixture.expected_trace)
            .expect("match");
    }

    // ── Fixture md: concurrent-markdown-write-conflict ───────────────────────

    #[test]
    fn md_default_run_passes() {
        let trace = registry()
            .run(&ReplayInput::default_for(make_id(F_MD)))
            .expect("ok");
        assert!(trace.passed);
    }

    #[test]
    fn md_verify_probe_fail_causes_fail() {
        let mut input = ReplayInput::default_for(make_id(F_MD));
        input.probe_overrides.insert(
            3,
            ProbeOutcome::Fail {
                reason: "replica still unreachable".into(),
            },
        );
        let trace = registry().run(&input).expect("ok");
        assert!(!trace.passed);
    }

    // ── ProbeOutcome ─────────────────────────────────────────────────────────

    #[test]
    fn probe_outcome_pass_is_pass() {
        assert!(ProbeOutcome::Pass.is_pass());
        assert!(!ProbeOutcome::Pass.is_fail());
    }

    #[test]
    fn probe_outcome_fail_is_fail() {
        let f = ProbeOutcome::Fail { reason: "x".into() };
        assert!(f.is_fail());
        assert!(!f.is_pass());
    }

    #[test]
    fn probe_outcome_timeout_is_neither_pass_nor_fail() {
        assert!(!ProbeOutcome::Timeout.is_pass());
        assert!(!ProbeOutcome::Timeout.is_fail());
    }

    #[test]
    fn probe_outcome_skipped_is_neither_pass_nor_fail() {
        assert!(!ProbeOutcome::Skipped.is_pass());
        assert!(!ProbeOutcome::Skipped.is_fail());
    }

    #[test]
    fn probe_outcome_display_pass() {
        assert_eq!(ProbeOutcome::Pass.to_string(), "pass");
    }

    #[test]
    fn probe_outcome_display_timeout() {
        assert_eq!(ProbeOutcome::Timeout.to_string(), "timeout");
    }

    #[test]
    fn probe_outcome_display_skipped() {
        assert_eq!(ProbeOutcome::Skipped.to_string(), "skipped");
    }

    #[test]
    fn probe_outcome_display_fail() {
        assert_eq!(
            ProbeOutcome::Fail {
                reason: "oops".into()
            }
            .to_string(),
            "fail(oops)"
        );
    }

    // ── TraceEvent constructors ───────────────────────────────────────────────

    #[test]
    fn trace_event_phase_started_constructor() {
        let e = TraceEvent::phase_started(0, PhaseKind::Detect);
        assert_eq!(e.step, 0);
        assert_eq!(e.kind, TraceEventKind::PhaseStarted);
        assert_eq!(e.phase, Some(PhaseKind::Detect));
        assert!(e.probe_outcome.is_none());
    }

    #[test]
    fn trace_event_probe_ran_constructor() {
        let e = TraceEvent::probe_ran(1, PhaseKind::Fix, ProbeOutcome::Pass);
        assert_eq!(e.kind, TraceEventKind::ProbeRan);
        assert_eq!(e.probe_outcome, Some(ProbeOutcome::Pass));
    }

    #[test]
    fn trace_event_phase_completed_constructor() {
        let e = TraceEvent::phase_completed(2, PhaseKind::Block);
        assert_eq!(e.kind, TraceEventKind::PhaseCompleted);
        assert_eq!(e.phase, Some(PhaseKind::Block));
    }

    #[test]
    fn trace_event_terminal_constructor() {
        let e = TraceEvent::terminal(99);
        assert_eq!(e.step, 99);
        assert_eq!(e.kind, TraceEventKind::Terminal);
        assert!(e.phase.is_none());
    }

    #[test]
    fn trace_event_annotated_sets_annotation() {
        let e = TraceEvent::terminal(0).annotated("test note");
        assert_eq!(e.annotation.as_deref(), Some("test note"));
    }

    #[test]
    fn trace_event_human_confirm_requested_constructor() {
        let e = TraceEvent::human_confirm_requested(3, PhaseKind::Fix);
        assert_eq!(e.kind, TraceEventKind::HumanConfirmRequested);
        assert_eq!(e.phase, Some(PhaseKind::Fix));
    }

    // ── VerifyTrace ──────────────────────────────────────────────────────────

    #[test]
    fn verify_trace_len_and_empty() {
        let t = VerifyTrace {
            events: vec![],
            passed: true,
        };
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
    }

    #[test]
    fn verify_trace_len_non_empty() {
        let t = VerifyTrace {
            events: vec![TraceEvent::terminal(0)],
            passed: true,
        };
        assert!(!t.is_empty());
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn verify_trace_count_kind() {
        let t = VerifyTrace {
            events: vec![
                TraceEvent::phase_started(0, PhaseKind::Detect),
                TraceEvent::phase_started(1, PhaseKind::Fix),
                TraceEvent::terminal(2),
            ],
            passed: true,
        };
        assert_eq!(t.count_kind(&TraceEventKind::PhaseStarted), 2);
        assert_eq!(t.count_kind(&TraceEventKind::Terminal), 1);
    }

    #[test]
    fn verify_trace_contains_kind_true() {
        let t = VerifyTrace {
            events: vec![TraceEvent::terminal(0)],
            passed: true,
        };
        assert!(t.contains_kind(&TraceEventKind::Terminal));
        assert!(!t.contains_kind(&TraceEventKind::ProbeRan));
    }

    #[test]
    fn verify_trace_assert_matches_length_mismatch() {
        let t1 = VerifyTrace {
            events: vec![],
            passed: true,
        };
        let t2 = VerifyTrace {
            events: vec![TraceEvent::terminal(0)],
            passed: true,
        };
        let err = t1.assert_matches(&t2).unwrap_err();
        assert_eq!(err.error_code(), 2581);
    }

    #[test]
    fn verify_trace_assert_matches_passed_mismatch() {
        let t1 = VerifyTrace {
            events: vec![],
            passed: true,
        };
        let t2 = VerifyTrace {
            events: vec![],
            passed: false,
        };
        let err = t1.assert_matches(&t2).unwrap_err();
        assert!(err.to_string().contains("2581"));
    }

    #[test]
    fn verify_trace_assert_matches_event_kind_mismatch() {
        let t1 = VerifyTrace {
            events: vec![TraceEvent::phase_started(0, PhaseKind::Detect)],
            passed: true,
        };
        let t2 = VerifyTrace {
            events: vec![TraceEvent::terminal(0)],
            passed: true,
        };
        let err = t1.assert_matches(&t2).unwrap_err();
        assert_eq!(err.error_code(), 2581);
    }

    #[test]
    fn verify_trace_assert_matches_same_trace_ok() {
        let t1 = VerifyTrace {
            events: vec![TraceEvent::terminal(0)],
            passed: true,
        };
        let t2 = VerifyTrace {
            events: vec![TraceEvent::terminal(0)],
            passed: true,
        };
        assert!(t1.assert_matches(&t2).is_ok());
    }

    // ── TraceEventKind ───────────────────────────────────────────────────────

    #[test]
    fn trace_event_kind_as_str_non_empty() {
        let kinds = [
            TraceEventKind::PhaseStarted,
            TraceEventKind::ProbeRan,
            TraceEventKind::PhaseCompleted,
            TraceEventKind::HumanConfirmRequested,
            TraceEventKind::HumanConfirmGranted,
            TraceEventKind::HumanConfirmDenied,
            TraceEventKind::SafetyGateTriggered,
            TraceEventKind::ErrorEmitted,
            TraceEventKind::Terminal,
        ];
        for kind in &kinds {
            assert!(!kind.as_str().is_empty(), "{kind:?} has empty as_str");
        }
    }

    #[test]
    fn trace_event_kind_display_equals_as_str() {
        let kind = TraceEventKind::PhaseStarted;
        assert_eq!(kind.to_string(), kind.as_str());
    }

    // ── ReplayError display ──────────────────────────────────────────────────

    #[test]
    fn replay_error_not_found_display() {
        let err = ReplayError::NotFound {
            id: "no-such".into(),
        };
        assert!(err.to_string().contains("2580"));
        assert!(err.to_string().contains("no-such"));
    }

    #[test]
    fn replay_error_trace_mismatch_display() {
        let err = ReplayError::TraceMismatch {
            reason: "bad".into(),
        };
        assert!(err.to_string().contains("2581"));
    }

    #[test]
    fn replay_error_invalid_input_display() {
        let err = ReplayError::InvalidInput {
            field: "fixture_id",
            reason: "empty".into(),
        };
        assert!(err.to_string().contains("2582"));
    }

    #[test]
    fn replay_error_not_found_error_code() {
        assert_eq!(ReplayError::NotFound { id: "x".into() }.error_code(), 2580);
    }

    #[test]
    fn replay_error_trace_mismatch_error_code() {
        assert_eq!(
            ReplayError::TraceMismatch { reason: "x".into() }.error_code(),
            2581
        );
    }

    #[test]
    fn replay_error_invalid_input_error_code() {
        assert_eq!(
            ReplayError::InvalidInput {
                field: "f",
                reason: "r".into()
            }
            .error_code(),
            2582
        );
    }
}
