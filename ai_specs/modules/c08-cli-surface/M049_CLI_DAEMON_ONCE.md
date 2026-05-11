# M049 — cli_daemon_once

> **File:** `crates/hle-cli/src/daemon_once.rs` | **Layer:** L06 | **Cluster:** C08_CLI_SURFACE
> **Error codes:** 2730-2731 | **Role:** bounded `hle daemon --once` adapter

---

## Purpose

M049 is the adapter for the `hle daemon --once` operator command. Its primary responsibility is to preserve the one-shot boundary: it refuses to execute without the `--once` flag, delegates the actual run to M047 `CliRun`, and wraps the output with a bounded-once completion marker.

The rejection message when `--once` is absent MUST contain the exact framework string `"this codebase needs to be 'one shotted'"` verbatim. This string is detected by downstream tooling (quality gate scripts, verifier tests) to confirm the boundary is enforced.

---

## Types at a Glance

| Type | Kind | Role |
|---|---|---|
| `CliDaemonOnce` | struct | Adapter; wraps an injected `CliRun` reference |
| `DaemonOnceArgs` | struct (re-export from M046) | Validated daemon command arguments |
| `DaemonOnceResult` | struct | Wraps `RunSummary` with a bounded-once completion prefix |

---

## Rust Signatures

```rust
use crate::args::DaemonOnceArgs;
use crate::run::CliRun;
use substrate_types::HleError;

/// Bounded one-shot adapter for the `hle daemon --once` command.
///
/// Wraps a `CliRun` reference. The `--once` flag is required at the M046
/// parser level (code 2701); M049 enforces it again at the adapter level
/// with code 2730 to provide defense-in-depth.
pub struct CliDaemonOnce<'a, R: BoundedRunExecutor> {
    run_executor: &'a R,
}

impl<'a, R: BoundedRunExecutor> CliDaemonOnce<'a, R> {
    /// Construct the adapter with an injected run executor.
    #[must_use]
    pub fn new(run_executor: &'a R) -> Self;

    /// Execute the bounded daemon command.
    ///
    /// Guard: if `args.once` is false, returns `HleError` 2730 with the
    /// exact message containing the framework string
    /// `"this codebase needs to be 'one shotted'"`.
    ///
    /// If the guard passes, delegates to `run_executor.execute_run()` and
    /// wraps the result string with the bounded-once completion prefix.
    ///
    /// Returns `Err(HleError)` with code 2730 (missing --once) or
    /// 2731 (inner run error).
    pub fn execute(&self, args: &DaemonOnceArgs) -> Result<String, HleError>;
}

/// Minimal trait abstraction over `CliRun` for test injection.
pub trait BoundedRunExecutor {
    fn execute_run(&self, workflow_path: &std::path::Path, ledger_path: &std::path::Path, json: bool)
        -> Result<String, HleError>;
}

/// Result wrapper returned by `CliDaemonOnce::execute` on success.
#[derive(Debug)]
pub struct DaemonOnceResult {
    /// The bounded-once prefix.
    pub prefix: &'static str,
    /// The inner run summary string.
    pub inner: String,
}

impl DaemonOnceResult {
    /// Combine prefix and inner into the final output string.
    ///
    /// Output: `"hle daemon bounded-once complete; {inner}"`
    #[must_use]
    pub fn to_output(&self) -> String;
}
```

---

## Method / Trait Table

| Item | Signature | Notes |
|---|---|---|
| `CliDaemonOnce::new` | `fn(run_executor: &'a R) -> Self` | Injection; no defaults |
| `CliDaemonOnce::execute` | `pub fn(&self, args: &DaemonOnceArgs) -> Result<String, HleError>` | Public entry point |
| `guard_once` | `fn(once: bool) -> Result<(), HleError>` | Private; enforces `--once` requirement; emits 2730 |
| `wrap_result` | `fn(inner: String) -> String` | Private; prepends `"hle daemon bounded-once complete; "` |
| `DaemonOnceResult::to_output` | `pub fn(&self) -> String` | Combines prefix and inner |
| `BoundedRunExecutor::execute_run` | trait method | Thin bridge to `CliRun::execute`; defined here for injectability |

---

## Execution Flow

```
CliDaemonOnce::execute(args)
  1. guard_once(args.once)
       -> if false: return Err(HleError 2730)
          message must contain: "this codebase needs to be 'one shotted'"
  2. run_executor.execute_run(
         workflow_path = &args.workflow_path,
         ledger_path   = &args.ledger_path,
         json          = args.json,
       )
       -> if Err: wrap with HleError 2731, preserve inner message
       -> if Ok(inner): wrap_result(inner)
  3. return Ok(wrap_result(inner))
```

---

## Design Notes

### The `--once` double guard

M046 `parse_daemon` already rejects calls without `--once` with code 2701. M049 re-checks the parsed `args.once` field at the adapter level with code 2730. This double guard ensures that:

1. Any code path that bypasses M046 (e.g., unit tests constructing `DaemonOnceArgs` directly with `once: false`) still encounters the M049 rejection.
2. The rejection is detectable independently of argument parsing errors.

The error messages are intentionally distinct so diagnostics can identify which layer rejected the call.

### Framework string invariant

The exact string `"this codebase needs to be 'one shotted'"` must appear verbatim in the 2730 error message. Quality gate scripts (`scripts/verify-script-safety.sh`) grep for this string to confirm the one-shot boundary is active. It must not be paraphrased, abbreviated, or split across interpolation.

Canonical implementation:

```rust
fn guard_once(once: bool) -> Result<(), HleError> {
    if once {
        return Ok(());
    }
    Err(HleError::new(
        "[2730] daemon command requires --once because this codebase needs to be 'one shotted' \
         for bounded M0 operation",
    ))
}
```

### Inner error wrapping (code 2731)

When `run_executor.execute_run` returns `Err(inner_error)`, M049 wraps it:

```rust
HleError::new(format!("[2731] daemon --once inner run failed: {inner_error}"))
```

The inner error message is preserved in the wrapper so operators can diagnose the root cause without inspecting the ledger directly.

### Output format

Successful completion output:

```
hle daemon bounded-once complete; hle run verdict=PASS steps=3 pass=3 fail=0 human=0 ledger=/path/to/ledger.jsonl
```

The prefix `"hle daemon bounded-once complete; "` is a const `DAEMON_ONCE_PREFIX: &str` in M049 so tests can assert the exact prefix without string duplication.

### JSON mode

When `args.json` is true, the inner run module formats its output as `hle.run.summary.v1` JSON (see M047). M049 does not re-wrap in a separate JSON envelope. The `--json` flag is passed through to `execute_run` unchanged.

### One-shot vs daemon distinction

M049 does NOT start a loop, schedule a retry, or re-invoke the workflow. A single call to `run_executor.execute_run` constitutes the entire daemon lifecycle. The word "daemon" in the command name is a convention preserved from earlier design; the behavior is identical to `hle run` modulo the `--once` guard and the completion prefix. Future multi-iteration daemon behavior requires a separate authorization receipt — not this module.

### Test surface (minimum 50 tests)

- `execute_succeeds_with_once_flag`
- `execute_fails_2730_without_once_flag`
- `execute_error_message_contains_one_shotted_string`
- `execute_error_message_contains_2730_code`
- `execute_passes_workflow_path_to_inner`
- `execute_passes_ledger_path_to_inner`
- `execute_passes_json_flag_to_inner`
- `execute_wraps_inner_error_with_2731`
- `execute_inner_error_message_preserved_in_2731`
- `execute_ok_contains_bounded_once_complete_prefix`
- `execute_ok_contains_inner_run_result`
- `guard_once_true_returns_ok`
- `guard_once_false_returns_err`
- `guard_once_err_message_contains_one_shotted`
- `guard_once_err_code_is_2730`
- `wrap_result_prepends_prefix`
- `wrap_result_preserves_inner`
- `daemon_once_result_to_output_combines_prefix_and_inner`
- `daemon_once_result_prefix_is_const`
- `execute_round_trip_with_mock_run_executor`
- `execute_json_mode_passes_json_true_to_inner`
- `execute_human_mode_passes_json_false_to_inner`
- `execute_once_field_false_in_parsed_args_errors`
- `execute_once_field_true_in_parsed_args_passes`
- `args_without_once_errors_before_workflow_check`
- ... (additional integration and boundary cases to meet 50-test minimum)

---

*M049 cli_daemon_once Spec v1.0 | C08_CLI_SURFACE | 2026-05-10*
