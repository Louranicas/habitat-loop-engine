# M047 — cli_run

> **File:** `crates/hle-cli/src/run.rs` | **Layer:** L06 | **Cluster:** C08_CLI_SURFACE
> **Error codes:** 2710-2713 | **Role:** `hle run` command adapter

---

## Purpose

M047 is the adapter for the `hle run` operator command. It reads the workflow file from the path supplied in `RunArgs`, invokes the C03 phase executor, stores receipts via C01 and C05 surfaces, and returns a bounded summary string with the final verdict. It carries zero business logic: workflow parsing, phase execution, and receipt storage are all delegated to their authoritative cluster modules.

---

## Types at a Glance

| Type | Kind | Role |
|---|---|---|
| `CliRun` | struct | Stateless adapter; constructed once per invocation |
| `RunSummary` | struct | Bounded run result returned by `execute` |
| `RunVerdict` | enum | `Pass`, `Fail`, `AwaitingHuman`, `Partial` |
| `CliRunError` | type alias | `HleError` with codes 2710-2713 |

---

## Rust Signatures

```rust
/// Stateless adapter for the `hle run` command.
/// Constructed with references to the C03 and C05 surfaces.
pub struct CliRun<'a, E, R, V>
where
    E: PhaseExecutor,
    R: ReceiptsStore,
    V: VerifierResultsStore,
{
    executor: &'a E,
    receipts: &'a R,
    verifier_store: &'a V,
}

impl<'a, E, R, V> CliRun<'a, E, R, V>
where
    E: PhaseExecutor,
    R: ReceiptsStore,
    V: VerifierResultsStore,
{
    /// Construct the adapter with injected executor and store references.
    #[must_use]
    pub fn new(executor: &'a E, receipts: &'a R, verifier_store: &'a V) -> Self;

    /// Execute the run command end-to-end.
    ///
    /// 1. Reads and parses the workflow file at `args.workflow_path`.
    /// 2. Invokes `executor.run_phases(&workflow)` (C03/M013).
    /// 3. Appends each receipt to `receipts` (C01/M003) and `verifier_store` (C05/M026).
    /// 4. Returns a formatted `RunSummary` as a bounded string.
    ///
    /// Returns `Err(HleError)` with code 2710-2713 on any failure.
    pub fn execute(&self, args: &RunArgs) -> Result<String, HleError>;
}

/// Bounded result of a completed run.
#[derive(Debug)]
pub struct RunSummary {
    pub verdict: RunVerdict,
    pub step_count: usize,
    pub pass_count: usize,
    pub fail_count: usize,
    pub awaiting_human_count: usize,
    pub ledger_path: std::path::PathBuf,
}

/// Execution verdict derived from phase executor output.
#[derive(Debug, PartialEq)]
pub enum RunVerdict {
    Pass,
    Fail,
    AwaitingHuman,
    /// At least one step passed and at least one step failed.
    Partial,
}
```

---

## Method / Trait Table

| Item | Signature | Notes |
|---|---|---|
| `CliRun::new` | `fn(executor, receipts, verifier_store) -> Self` | Injection pattern; no defaults |
| `CliRun::execute` | `fn(&self, args: &RunArgs) -> Result<String, HleError>` | Main adapter path |
| `read_workflow` | `fn(path: &Path) -> Result<Workflow, HleError>` | Private; maps IO error -> 2710, parse error -> 2711 |
| `store_receipts` | `fn(&R, &V, &[Receipt]) -> Result<(), HleError>` | Private; maps store error -> 2713 |
| `format_summary` | `fn(&RunSummary, json: bool) -> String` | Private; bounded 2 KB max |
| `RunSummary::verdict_str` | `fn(&self) -> &'static str` | "PASS" / "FAIL" / "AWAITING_HUMAN" / "PARTIAL" |
| `RunVerdict::from_receipts` | `fn(receipts: &[Receipt]) -> Self` | Derives verdict from receipt slice |
| `Display for RunSummary` | impl | Human-readable one-line summary |

---

## Execution Flow

```
CliRun::execute(args)
  1. read_workflow(&args.workflow_path)
       -> HleError 2710 on IO failure
       -> HleError 2711 on parse failure
  2. executor.run_phases(&workflow)
       -> HleError 2712 on executor error
       -> returns Vec<Receipt>
  3. store_receipts(self.receipts, self.verifier_store, &receipts)
       -> appends to C01 receipts_store (M003)
       -> appends to C05 verifier_results_store (M026)
       -> HleError 2713 on write failure
  4. RunVerdict::from_receipts(&receipts)
  5. format_summary(&summary, args.json)
  6. return Ok(formatted_string)
```

---

## Design Notes

### Thin adapter — no execution logic

M047 does not implement retry, timeout, or phase ordering. Those are owned by C03 `phase_executor` (M013). `execute` calls a single method on the injected `PhaseExecutor` trait and processes the returned receipts. The only logic in M047 is: IO reading, receipt aggregation into `RunSummary`, and string formatting.

### Cross-cluster dependency: C03 phase_executor

`PhaseExecutor` is a trait from C03/M013. M047 depends on it via trait bound on the `CliRun` generic parameter, not by direct crate import. This allows tests to inject a mock executor without pulling in the full C03 implementation.

```rust
pub trait PhaseExecutor {
    fn run_phases(&self, workflow: &Workflow) -> Result<Vec<Receipt>, HleError>;
}
```

### Cross-cluster dependency: C01 receipts_store and C05 verifier_results_store

Both stores are injected as trait references:

```rust
pub trait ReceiptsStore {
    fn append(&self, receipt: &Receipt) -> Result<(), HleError>;
}

pub trait VerifierResultsStore {
    fn append(&self, receipt: &Receipt, verdict: &str) -> Result<(), HleError>;
}
```

M047 writes each receipt to both stores in sequence. If `receipts_store.append` succeeds but `verifier_results_store.append` fails, M047 returns HleError 2713. It does NOT attempt to undo the `receipts_store` append (append-only ledger invariant from C05).

### Bounded output

`format_summary` produces at most 2 KB of text. If the ledger path is extremely long, it is truncated with a `[…]` marker. The `--json` flag switches to the `hle.run.summary.v1` JSON schema:

```json
{
  "schema": "hle.run.summary.v1",
  "verdict": "PASS",
  "step_count": 3,
  "pass_count": 3,
  "fail_count": 0,
  "awaiting_human_count": 0,
  "ledger_path": "/path/to/ledger.jsonl"
}
```

Human-readable format (default):

```
hle run verdict=PASS steps=3 pass=3 fail=0 human=0 ledger=/path/to/ledger.jsonl
```

### `RunVerdict::from_receipts` logic

| Condition | Verdict |
|---|---|
| All receipts `AwaitingHuman` or mix including `AwaitingHuman`, zero failures | `AwaitingHuman` |
| All receipts `Pass` | `Pass` |
| All receipts `Fail` | `Fail` |
| Mix of `Pass` and `Fail` (with or without `AwaitingHuman`) | `Partial` |
| Empty receipt slice | `Fail` (no evidence = not passed) |

### Test surface (minimum 50 tests)

- `execute_returns_pass_on_all_pass_receipts`
- `execute_returns_fail_on_all_fail_receipts`
- `execute_returns_awaiting_human_on_human_receipts`
- `execute_returns_partial_on_mixed_receipts`
- `execute_returns_2710_on_missing_workflow_file`
- `execute_returns_2711_on_invalid_workflow_toml`
- `execute_returns_2712_on_executor_error`
- `execute_returns_2713_on_receipts_store_failure`
- `execute_returns_2713_on_verifier_store_failure`
- `execute_writes_to_receipts_store`
- `execute_writes_to_verifier_results_store`
- `execute_writes_all_receipts_not_just_first`
- `format_summary_human_contains_verdict`
- `format_summary_human_contains_step_count`
- `format_summary_human_contains_ledger_path`
- `format_summary_json_valid_schema_field`
- `format_summary_json_contains_verdict`
- `format_summary_json_contains_counts`
- `format_summary_bounded_under_2kb`
- `run_verdict_pass_from_all_pass_receipts`
- `run_verdict_fail_from_empty_receipts`
- `run_verdict_awaiting_human_from_human_receipts`
- `run_verdict_partial_from_mixed`
- ... (additional edge and error cases to meet 50-test minimum)

---

*M047 cli_run Spec v1.0 | C08_CLI_SURFACE | 2026-05-10*
