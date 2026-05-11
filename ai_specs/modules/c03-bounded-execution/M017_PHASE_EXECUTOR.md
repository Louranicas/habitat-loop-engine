# M017 PhaseExecutor — phase_executor.rs

> **File:** `crates/hle-executor/src/phase_executor.rs` | **Target LOC:** ~320 | **Target Tests:** 55
> **Layer:** L03 | **Cluster:** C03_BOUNDED_EXECUTION | **Error Codes:** 2220-2223
> **Role:** Phase-aware step sequencer. Runs a sequence of `ExecutionPhase` steps using `LocalRunner` (M016), emits a `Receipt` per step, and halts on the first `Failed` or `AwaitingHuman` outcome. The sole entry point for phase-aware execution in L03.

---

## Types at a Glance

| Type | Kind | Copy | Purpose |
|---|---|---|---|
| `PhaseExecutor` | struct | No | Configured executor with baked-in runner and policies |
| `ExecutionPhase` | enum | Yes | 8 framework phases from §17.7 |
| `PhaseStep` | struct | No | One step: phase label, command, expected state, human-confirm flag |
| `PhaseSequence` | struct | No | Ordered list of `PhaseStep`s for one execution run |
| `ExecutionResult` | struct | No | Full result: receipts per step, final verdict, halt reason |
| `PhaseExecutorError` | enum | No | Errors 2220-2223 |

---

## ExecutionPhase

```rust
/// The 8 phases defined in the HLE deployment framework §17.7.
///
/// Phase ordering is significant: Detect must precede Block, Fix precedes Verify,
/// MetaTest precedes Receipt, Receipt precedes Persist, Persist precedes Notify.
/// `PhaseExecutor` enforces this ordering at construction time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ExecutionPhase {
    Detect    = 0,
    Block     = 1,
    Fix       = 2,
    Verify    = 3,
    MetaTest  = 4,
    Receipt   = 5,
    Persist   = 6,
    Notify    = 7,
}

impl ExecutionPhase {
    #[must_use] pub const fn as_str(self) -> &'static str;
    #[must_use] pub const fn index(self) -> usize;          // 0..=7
    #[must_use] pub const fn from_index(i: usize) -> Option<Self>;
    #[must_use] pub const fn is_verification_phase(self) -> bool; // Verify | MetaTest | Receipt
}
```

**Traits:** `Display` ("Detect"), `PartialOrd`/`Ord` by repr value

---

## PhaseStep

```rust
/// One step in a phase-aware execution sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhaseStep {
    pub phase: ExecutionPhase,
    pub step_id: String,
    pub label: BoundedString,       // M015 — max MAX_STEP_LABEL_BYTES
    pub command: String,            // passed verbatim to LocalRunner (M016)
    pub expected_state: StepState,  // verifier expected outcome
    pub requires_human: bool,       // if true, skip command and emit AwaitingHuman
}

impl PhaseStep {
    pub fn new(
        phase: ExecutionPhase,
        step_id: impl Into<String>,
        label: impl Into<String>,
        command: impl Into<String>,
        expected_state: StepState,
    ) -> Result<Self, PhaseExecutorError>;

    pub fn awaiting_human(
        phase: ExecutionPhase,
        step_id: impl Into<String>,
        label: impl Into<String>,
    ) -> Result<Self, PhaseExecutorError>;

    #[must_use] pub fn is_human_step(&self) -> bool;
}
```

---

## PhaseSequence

```rust
/// An ordered collection of `PhaseStep`s. Validated at construction:
/// - At least one step required.
/// - Phase ordering must be non-decreasing (Detect steps may not follow Fix steps).
/// - No duplicate step_ids within the sequence.
#[derive(Debug, Clone)]
pub struct PhaseSequence {
    steps: Vec<PhaseStep>,
}

impl PhaseSequence {
    pub fn new(steps: Vec<PhaseStep>) -> Result<Self, PhaseExecutorError>;
    #[must_use] pub fn steps(&self) -> &[PhaseStep];
    #[must_use] pub fn len(&self) -> usize;
    #[must_use] pub fn is_empty(&self) -> bool;
    #[must_use] pub fn phases_present(&self) -> Vec<ExecutionPhase>;
}
```

Phase ordering validation: the `phase` field of each step must be `>=` the phase of the preceding step (non-decreasing). Mixed phases in the same "tier" are allowed (e.g., two `Fix` steps in a row). A `Notify` step appearing before a `Detect` step is rejected.

---

## PhaseExecutor

```rust
/// Executes a `PhaseSequence` using a `LocalRunner`.
///
/// Stops at the first Failed or AwaitingHuman step. Emits one `Receipt`
/// per executed step to the ledger path. The verifier (C01/C04) is the
/// sole PASS authority; `PhaseExecutor` only submits draft states.
#[derive(Debug)]
pub struct PhaseExecutor {
    runner: LocalRunner,             // M016
    retry_policy: RetryPolicy,       // M019 — applied per step on CommandRejected/SpawnFailed
    ledger_path: std::path::PathBuf, // JSONL receipt ledger (mirrors substrate_emit pattern)
}

impl PhaseExecutor {
    pub fn new(
        runner: LocalRunner,
        retry_policy: RetryPolicy,
        ledger_path: impl Into<std::path::PathBuf>,
    ) -> Self;

    /// Run all steps in `sequence` in order. Returns `ExecutionResult` regardless
    /// of individual step outcomes; returns `Err` only for infrastructure failures
    /// (ledger write failure, unexpected OS error).
    pub fn run_phases(
        &self,
        sequence: &PhaseSequence,
    ) -> Result<ExecutionResult, PhaseExecutorError>;

    #[must_use] pub fn ledger_path(&self) -> &std::path::Path;
}
```

### run_phases algorithm

```rust
pub fn run_phases(&self, sequence: &PhaseSequence) -> Result<ExecutionResult, PhaseExecutorError> {
    // sequence validated at PhaseSequence::new — never empty
    let mut receipts: Vec<Receipt> = Vec::with_capacity(sequence.len());

    for step in sequence.steps() {
        let (draft_state, message) = if step.requires_human {
            (StepState::AwaitingHuman, String::new())
        } else {
            self.run_step_with_retry(step)?
        };

        let mut receipt = build_receipt(step, draft_state, &message);
        append_jsonl_receipt(&self.ledger_path, &receipt)
            .map_err(|e| PhaseExecutorError::LedgerWriteFailed { reason: e.to_string() })?;

        let should_halt = matches!(draft_state, StepState::Failed | StepState::AwaitingHuman);
        receipts.push(receipt);

        if should_halt {
            return Ok(ExecutionResult::halted(receipts, draft_state));
        }
    }

    Ok(ExecutionResult::completed(receipts))
}

fn run_step_with_retry(
    &self,
    step: &PhaseStep,
) -> Result<(StepState, String), PhaseExecutorError> {
    // M019 RetryPolicy drives the attempt loop
    self.retry_policy.execute(|| {
        let output = self.runner.run(&step.command)?;
        Ok((output.to_step_state(), output.combined_message))
    })
    .map_err(|e| PhaseExecutorError::StepFailed {
        phase: step.phase,
        step_id: step.step_id.clone(),
        message: e.to_string(),
    })
}
```

Retry applies only to `RunnerError::SpawnFailed` (retryable) and `RunnerError::CommandRejected` (never retryable — rejected immediately). `StepState::Failed` from a successful-but-failing command is not retried; retry is an infrastructure concern, not a correctness concern.

---

## ExecutionResult

```rust
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub receipts: Vec<Receipt>,
    pub verdict: ExecutionVerdict,
    pub halt_reason: Option<HaltReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionVerdict {
    Completed,      // all steps ran and none failed
    Halted,         // stopped early due to Failed or AwaitingHuman
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HaltReason {
    StepFailed,
    AwaitingHuman,
}

impl ExecutionResult {
    #[must_use] pub fn is_complete(&self) -> bool;
    #[must_use] pub fn is_halted(&self) -> bool;
    #[must_use] pub fn last_receipt(&self) -> Option<&Receipt>;
    #[must_use] pub fn receipts_for_phase(&self, phase: ExecutionPhase) -> Vec<&Receipt>;
    #[must_use] pub fn step_count(&self) -> usize;
    #[must_use] pub fn passed_count(&self) -> usize;
    #[must_use] pub fn failed_count(&self) -> usize;
}
```

---

## PhaseExecutorError

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhaseExecutorError {
    /// Code 2220. PhaseSequence was empty.
    PhaseSequenceEmpty,
    /// Code 2221. A step returned Failed; sequence halted.
    StepFailed { phase: ExecutionPhase, step_id: String, message: String },
    /// Code 2222. A step returned AwaitingHuman; sequence halted.
    AwaitingHuman { phase: ExecutionPhase, step_id: String },
    /// Code 2223. JSONL ledger append failed.
    LedgerWriteFailed { reason: String },
}
```

`StepFailed` and `AwaitingHuman` are **not** returned from `run_phases` — those outcomes are encoded in `ExecutionResult`. These error variants exist for `PhaseSequence::new` validation failures and infrastructure errors only.

---

## Design Notes

- M017 is the topology successor to `substrate_emit::execute_local_workflow`. It adds phase awareness (8 `ExecutionPhase` variants), retry delegation (M019), and a `PhaseSequence` validator that catches misordered phases at construction time rather than at runtime.
- The ledger append pattern (`append_jsonl_receipt`) is preserved from `substrate_emit` to ensure the JSONL evidence format is consistent across M0 crates. M017 does not re-implement ledger logic; it imports the free function.
- `run_phases` returns `Ok(ExecutionResult)` even when steps fail or await human input. `Err` from `run_phases` means the infrastructure itself failed (ledger write, unexpected OS error). This distinction matters for callers in C08 CLI: a `Failed` step is not a Rust error.
- `PhaseSequence` validates phase ordering and duplicate `step_id`s at construction. This ensures verifier-visible evidence is well-formed before any command is run.
- C06 (`runbook_phase_map`, M030) maps `RunbookPhase` values onto `ExecutionPhase` variants. The 8 `ExecutionPhase` values are the canonical enum; runbooks use them by mapping, not by extending.

---

## Cross-Cluster Events Emitted

- Each step receipt goes to C01 (`receipts_store`, M003) via `append_jsonl_receipt` to the ledger.
- Completed `ExecutionResult` is consumed by C05 (`verifier_results_store`, M026) and C04 (`receipt_sha_verifier`, M004) after `run_phases` returns.
- `AwaitingHuman` events are consumed by C06 (`runbook_human_confirm`, M031) to surface the blocker in the runbook UI.

---

*M017 PhaseExecutor Spec v1.0 | C03 Bounded Execution | 2026-05-10*
