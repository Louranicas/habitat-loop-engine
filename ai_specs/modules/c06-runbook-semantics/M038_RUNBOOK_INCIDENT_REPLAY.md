# M038 Runbook Incident Replay — `crates/hle-runbook/src/incident_replay.rs`

> **Layer:** L07 | **Cluster:** C06 Runbook Semantics | **Error Codes:** 2580
> **Role:** Deterministic replay fixtures for the 8 canonical incident signatures — specification-as-code.
> **LOC target:** ~400 | **Test target:** ≥50

---

## Purpose

M038 provides a deterministic replay harness for the 8 incident types named in Framework §17.8. Each fixture specifies a structured `ReplayInput` (the conditions that trigger the incident), an `expected_trace` (the sequence of `TraceEvent` values the executor must emit as it works through the runbook phases), and a post-run `VerifyTrace` assertion. When a test or meta-test phase runs M038 against a real runbook execution, the actual trace is compared to the expected trace. Divergence is a test failure — error code 2580.

The fixtures serve double duty:
1. **Regression tests.** They document exactly how the system should respond to each incident type. Changing the expected behavior requires a deliberate fixture update.
2. **Living specification.** The fixture map is the authoritative record of what "correct" incident handling looks like for each of the 8 scenarios.

The `MetaTest` phase (from M034) maps to `WorkflowStepKind::Replay`, which invokes M038 as its execution body.

---

## Types at a Glance

| Type | Kind | Notes |
|------|------|-------|
| `IncidentFixture` | struct | One complete fixture (input + expected trace) |
| `FixtureId` | newtype(`String`) | Validated fixture identifier matching §17.8 IDs |
| `ReplayInput` | struct | Simulated conditions that trigger the incident |
| `TraceEvent` | enum | A single observable event in the runbook execution trace |
| `TraceEventKind` | enum | Classifies the type of trace event |
| `VerifyTrace` | struct | Assertion engine for expected-vs-actual comparison |
| `ReplayError` | struct | Error code 2580 when actual trace diverges |
| `IncidentReplayRegistry` | struct | All 8 fixtures keyed by `FixtureId` |

---

## Newtype: `FixtureId`

```rust
/// Validated identifier for a replay fixture.
///
/// Must match one of the 8 canonical IDs from Framework §17.8.
/// Pattern: `[a-z0-9_]+`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FixtureId(String);

impl FixtureId {
    pub fn new(raw: impl Into<String>) -> Result<Self, ReplayError>;
    #[must_use]
    pub fn as_str(&self) -> &str;
}
```

---

## Struct: `ReplayInput`

```rust
/// Simulated conditions that cause the incident to manifest.
///
/// Each fixture pre-populates a `ReplayInput` that represents the worst-case
/// incident manifestation for that signature type.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplayInput {
    /// Pre-configured state flags that simulate the incident conditions.
    pub state_flags: std::collections::HashMap<String, String>,
    /// Simulated service health values at replay start (service_id → health_score 0..1).
    pub service_health: std::collections::HashMap<String, f64>,
    /// Simulated evidence already present before the runbook runs.
    pub pre_existing_evidence: Vec<ManualEvidence>,
    /// Maximum ticks allowed for the replay to complete.
    pub tick_budget: u64,
}
```

| Method | Signature | Notes |
|--------|-----------|-------|
| `flag` | `fn(&self, key: &str) -> Option<&str>` | Retrieve a state flag value |
| `has_degraded_service` | `fn(&self) -> bool` | Any service_health entry < 0.5 |

---

## Enum: `TraceEvent`

```rust
/// A single observable event in the runbook execution trace.
///
/// The executor emits `TraceEvent` values as it progresses through phases.
/// M038 collects these and compares them to the expected trace.
#[derive(Debug, Clone, PartialEq)]
pub enum TraceEvent {
    /// A runbook phase began execution.
    PhaseStarted { phase: PhaseKind },
    /// A probe within a phase executed and returned an outcome.
    ProbeExecuted {
        phase: PhaseKind,
        probe_id: String,
        outcome: ProbeOutcome,
    },
    /// Human confirmation was requested.
    ConfirmRequested {
        phase: PhaseKind,
        safety_class: SafetyClass,
    },
    /// Human confirmation was received.
    ConfirmReceived {
        phase: PhaseKind,
        outcome: ConfirmOutcome,
    },
    /// Evidence was attached to a phase.
    EvidenceAttached {
        phase: PhaseKind,
        evidence_kind: EvidenceKind,
    },
    /// A phase completed with a pass/fail outcome.
    PhaseCompleted {
        phase: PhaseKind,
        passed: bool,
    },
    /// The runbook reached `AwaitingHuman` state.
    AwaitingHuman { state: AwaitingHumanState },
    /// The runbook completed (all phases done).
    RunbookCompleted { success: bool },
    /// Safety policy blocked a phase.
    SafetyBlocked { phase: PhaseKind, violation: String },
}
```

---

## Enum: `ProbeOutcome`

```rust
/// Outcome of a single probe execution in a replay context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeOutcome {
    /// Probe ran and matched its expected exit code / predicate.
    Pass,
    /// Probe ran but did not match expected outcome.
    Fail,
    /// Probe was skipped (command was None; manual observation required).
    Skipped,
}
```

---

## Struct: `IncidentFixture`

```rust
/// A complete incident replay fixture.
///
/// # Invariants
/// - `expected_trace` is non-empty (INV-C06-06).
/// - `id` matches one of the 8 canonical §17.8 fixture IDs.
/// - `input` and `expected_trace` are deterministic — no randomness.
#[derive(Debug, Clone)]
pub struct IncidentFixture {
    pub id: FixtureId,
    /// Human-readable description of the incident scenario.
    pub description: String,
    /// The `IncidentSignature` this fixture implements.
    pub signature: IncidentSignature,
    /// Pre-configured replay conditions.
    pub input: ReplayInput,
    /// Expected sequence of trace events. Order matters.
    pub expected_trace: Vec<TraceEvent>,
    /// Path to the associated runbook TOML (relative to runbook dir).
    pub runbook_path: &'static str,
}
```

| Method | Signature | Notes |
|--------|-----------|-------|
| `expected_phase_sequence` | `fn(&self) -> Vec<PhaseKind>` | Phases present in the expected trace, in order |
| `expects_human_confirm` | `fn(&self) -> bool` | Trace contains at least one `ConfirmRequested` event |
| `expects_safety_blocked` | `fn(&self) -> bool` | Trace contains at least one `SafetyBlocked` event |

---

## Struct: `VerifyTrace`

```rust
/// Assertion engine that compares an actual execution trace to an expected trace.
///
/// Matching is ordered-and-exact by default. Use `with_subsequence_match`
/// for fixtures where intermediate events may vary (e.g., non-deterministic probe order).
#[derive(Debug, Clone, Default)]
pub struct VerifyTrace {
    subsequence_match: bool,
    ignore_event_kinds: Vec<TraceEventKind>,
}
```

| Method | Signature | Notes |
|--------|-----------|-------|
| `with_subsequence_match` | `fn(self, v: bool) -> Self` | When true, expected is a subsequence of actual |
| `ignoring` | `fn(self, kind: TraceEventKind) -> Self` | Exclude event kind from comparison |
| `verify` | `fn(&self, fixture: &IncidentFixture, actual: &[TraceEvent]) -> Result<(), ReplayError>` | Primary assertion |

---

## Struct: `ReplayError`

```rust
/// Error code 2580 — actual trace diverged from the expected fixture trace.
#[derive(Debug)]
pub struct ReplayError {
    pub fixture_id: String,
    pub divergence_index: usize,
    pub expected: TraceEvent,
    pub actual: Option<TraceEvent>,
    pub message: String,
}
```

`ReplayError` implements `ErrorClassifier` with code 2580, severity High, retryable=false.

---

## Struct: `IncidentReplayRegistry`

```rust
/// All 8 incident replay fixtures, keyed by `FixtureId`.
///
/// Use `IncidentReplayRegistry::standard()` to get the canonical set.
/// Fixtures are constructed once and reused; the registry is cheap to clone.
#[derive(Debug, Clone)]
pub struct IncidentReplayRegistry {
    fixtures: std::collections::HashMap<FixtureId, IncidentFixture>,
}
```

| Method | Signature | Notes |
|--------|-----------|-------|
| `standard` | `fn() -> Self` | Canonical 8 fixtures from Framework §17.8 |
| `get` | `fn(&self, id: &FixtureId) -> Option<&IncidentFixture>` | |
| `all` | `fn(&self) -> Vec<&IncidentFixture>` | Deterministic iteration order by fixture ID |
| `for_signature` | `fn(&self, sig: IncidentSignature) -> Option<&IncidentFixture>` | |
| `count` | `fn(&self) -> usize` | Always 8 for `standard()` |

---

## Framework §17.8 Fixture Map

The `standard()` registry contains exactly these 8 fixtures:

| Fixture ID | Signature | Path | Default Safety |
|---|---|---|---|
| `s112_bridge_breaker_port_drift` | `S112BridgeBreakerPortDrift` | `runbooks/incident_replay/s112-bridge-breaker.toml` | Hard |
| `me_eventbus_dark_traffic` | `MeEventbusDarkTraffic` | `runbooks/incident_replay/me-eventbus-dark.toml` | Soft |
| `povm_write_only_readback_zero` | `PovmWriteOnlyReadbackZero` | `runbooks/incident_replay/povm-write-only.toml` | Hard |
| `s117_ttl_sweep_deletes_legitimate` | `S117TtlSweepDeletesLegitimate` | `runbooks/incident_replay/s117-ttl-sweep.toml` | Hard |
| `port_retirement_tombstone_collision` | `PortRetirementTombstoneCollision` | `runbooks/incident_replay/port-retirement.toml` | Hard |
| `devenv_batch_dependency_failure` | `DevenvBatchDependencyFailure` | `runbooks/incident_replay/devenv-batch-failure.toml` | Hard |
| `synthex_thermal_saturation_runaway` | `SynthexThermalSaturationRunaway` | `runbooks/incident_replay/thermal-saturation.toml` | Safety |
| `concurrent_markdown_write_conflict` | `ConcurrentMarkdownWriteConflict` | `runbooks/incident_replay/concurrent-markdown-write.toml` | Soft |

---

## Design Notes

- `TraceEvent` uses owned `String` fields rather than `&str` because trace events are collected at runtime and stored in `Vec<TraceEvent>`. The clone cost is acceptable given that traces are short (typically under 30 events per fixture).
- `VerifyTrace::subsequence_match` enables testing against fixtures where probe output ordering may vary due to concurrent phase execution. All 8 standard fixtures use exact-match (`subsequence_match: false`).
- M038 takes a dependency on M036 (`ManualEvidence`) for `pre_existing_evidence` in `ReplayInput`. This is a forward reference within C06 — acceptable because M038 is a consumer of the other C06 modules, not a foundation type.
- The `FalsePassAuditor` from C04 (Anti-Pattern Intelligence) consumes M038 fixtures as test inputs. This is a cross-cluster dependency: C04 imports `IncidentReplayRegistry` to validate that its false-pass detection catches runbooks that claim PASS without real evidence. The import direction is C04 → C06 (C04 uses C06 types), which is permitted by the layer DAG (C04 is L04, C06 is L07 — upper layers may be used by lower layers as long as there are no circular deps).
- `IncidentFixture::expected_trace` is authored as hardcoded `Vec<TraceEvent>` literals in `standard()`. This makes the fixture map readable, auditable, and diff-friendly in code review.

---

## Cluster Invariants (this module)

- `IncidentReplayRegistry::standard().count()` must equal 8. A fixture count other than 8 is a test failure.
- Every `IncidentFixture::expected_trace` in `standard()` is non-empty (INV-C06-06). The test `all_standard_fixtures_have_non_empty_trace` enforces this.
- `VerifyTrace::verify` is deterministic — given the same `fixture` and `actual`, it always returns the same `Result`.
- Adding a new `IncidentSignature` variant in M037 without adding a corresponding fixture in `standard()` violates this module's invariant and will cause `for_signature` to return `None`, which is detected by integration tests.

---

*M038 Runbook Incident Replay | C06 Runbook Semantics | Habitat Loop Engine | 2026-05-10*
