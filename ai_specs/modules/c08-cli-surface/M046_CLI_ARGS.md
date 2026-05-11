# M046 — cli_args

> **File:** `crates/hle-cli/src/args.rs` | **Layer:** L06 | **Cluster:** C08_CLI_SURFACE
> **Error codes:** 2700-2703 | **Role:** typed hand-rolled CLI argument parser

---

## Purpose

M046 is the single gateway through which raw `&[String]` argv reaches the rest of C08. It converts untyped strings into a `ParsedCommand` enum that carries all validated, typed values needed by each command module. No other C08 module ever reads `std::env::args()` or parses flag strings directly.

The parser is hand-rolled (no `clap`, no `structopt`, no external arg-parsing crate) to preserve the zero-external-deps discipline established by `main.rs`. The algorithm is a two-pass linear scan over the argv slice.

---

## Types at a Glance

| Type | Kind | Role |
|---|---|---|
| `ParsedCommand` | enum | Discriminated union of all valid commands |
| `RunArgs` | struct | Validated arguments for `hle run` |
| `VerifyArgs` | struct | Validated arguments for `hle verify` |
| `DaemonOnceArgs` | struct | Validated arguments for `hle daemon --once` |
| `StatusArgs` | struct | Validated arguments for `hle status` |
| `ArgsError` | enum | Structured parse error (codes 2700-2703) |
| `Flag` | enum | All recognized flag names |

---

## Rust Signatures

```rust
/// Top-level parse entry point. Consumes argv (skipping argv[0]).
///
/// Returns `Ok(ParsedCommand::Help)` if args is empty or first token is help/--help.
/// Returns `Ok(ParsedCommand::Version)` if first token is version/--version.
/// Returns `Err(ArgsError)` on unknown flag, missing required flag, or missing value.
#[must_use]
pub fn parse(args: &[String]) -> Result<ParsedCommand, ArgsError>;

/// The discriminated union of all valid top-level commands.
#[derive(Debug, PartialEq)]
pub enum ParsedCommand {
    Run(RunArgs),
    Verify(VerifyArgs),
    DaemonOnce(DaemonOnceArgs),
    Status(StatusArgs),
    Help,
    Version,
}

/// Validated arguments for `hle run`.
#[derive(Debug, PartialEq)]
pub struct RunArgs {
    /// Path to the workflow TOML file. Required.
    pub workflow_path: std::path::PathBuf,
    /// Path to the ledger JSONL file. Required.
    pub ledger_path: std::path::PathBuf,
    /// Emit JSON-formatted output. Optional.
    pub json: bool,
}

/// Validated arguments for `hle verify`.
#[derive(Debug, PartialEq)]
pub struct VerifyArgs {
    /// Path to the ledger JSONL file. Required.
    pub ledger_path: std::path::PathBuf,
    /// Emit JSON-formatted output. Optional.
    pub json: bool,
}

/// Validated arguments for `hle daemon --once`.
#[derive(Debug, PartialEq)]
pub struct DaemonOnceArgs {
    /// Path to the workflow TOML file. Required.
    pub workflow_path: std::path::PathBuf,
    /// Path to the ledger JSONL file. Required.
    pub ledger_path: std::path::PathBuf,
    /// Emit JSON-formatted output. Optional.
    pub json: bool,
}

/// Validated arguments for `hle status`.
#[derive(Debug, PartialEq)]
pub struct StatusArgs {
    /// Emit machine-readable JSON (hle.status.v1 schema). Optional.
    pub json: bool,
}

/// Structured parse error.
#[derive(Debug, PartialEq)]
pub enum ArgsError {
    /// An unrecognized flag was encountered. Code 2700.
    UnknownFlag { flag: String },
    /// A required flag was not present. Code 2701.
    MissingFlag { flag: &'static str },
    /// A flag was present but its value token was absent. Code 2702.
    MissingValue { flag: &'static str },
    /// Two mutually exclusive flags were both present. Code 2703.
    Conflict { flag_a: &'static str, flag_b: &'static str },
}
```

---

## Method / Trait Table

| Item | Signature | Notes |
|---|---|---|
| `parse` | `fn(args: &[String]) -> Result<ParsedCommand, ArgsError>` | Public entry point; dispatches by first token |
| `parse_run` | `fn(args: &[String]) -> Result<RunArgs, ArgsError>` | Private; called when first token is "run" |
| `parse_verify` | `fn(args: &[String]) -> Result<VerifyArgs, ArgsError>` | Private; called when first token is "verify" |
| `parse_daemon` | `fn(args: &[String]) -> Result<DaemonOnceArgs, ArgsError>` | Private; validates --once present before --workflow/--ledger |
| `parse_status` | `fn(args: &[String]) -> Result<StatusArgs, ArgsError>` | Private; only --json flag recognized |
| `flag_path` | `fn(args: &[String], name: &'static str) -> Result<PathBuf, ArgsError>` | Private; `windows(2)` scan; errors MissingFlag or MissingValue |
| `flag_bool` | `fn(args: &[String], name: &'static str) -> bool` | Private; presence-only scan; no value expected |
| `reject_unknown` | `fn(args: &[String], known: &[&str]) -> Result<(), ArgsError>` | Private; rejects any `--*` not in known list |
| `ArgsError::code` | `fn(&self) -> u16` | Returns 2700-2703 |
| `Display for ArgsError` | impl | Human-readable; includes flag name in message |
| `Display for ParsedCommand` | impl | Debug-friendly label ("Run", "Verify", etc.) |

---

## Design Notes

### Strict rejection policy

`reject_unknown` iterates argv and rejects any token that starts with `--` and is not in the `known` slice for the current subcommand. This fires before `flag_path` and `flag_bool` checks, so the error message names the exact offending flag. Positional arguments without a `--` prefix that appear after the subcommand word are also rejected (code 2700) unless the subcommand explicitly accepts them.

### Flag parsing algorithm

All flag values are parsed with `windows(2)` sliding scan (matching the existing `flag_path` pattern in `main.rs`). This means `--workflow /path` is accepted but `--workflow=/path` is rejected as `MissingValue` on `--workflow` followed by `UnknownFlag` on `--workflow=/path`. The strict form is intentional: avoid ambiguous `=`-split edge cases without a dep.

### `--once` invariant for daemon

`parse_daemon` checks for `--once` as the first step before attempting to parse `--workflow` and `--ledger`. If `--once` is absent, it returns `ArgsError::MissingFlag { flag: "--once" }`. This ensures the error code (2701) and the message ("missing required flag --once") are uniform regardless of what other flags are present, and the containing `DaemonOnce` code path in M049 can rely on `DaemonOnceArgs` always having `--once` semantically satisfied.

### No `PathBuf` validation at parse time

M046 converts the string values of `--workflow` and `--ledger` to `PathBuf` but does NOT check for file existence. Existence checks are delegated to the command modules (M047, M048, M049) so that test code can supply non-existent paths to the parser without touching the filesystem.

### Recognized flag set per subcommand

| Subcommand | Required flags | Optional flags |
|---|---|---|
| `run` | `--workflow`, `--ledger` | `--json` |
| `verify` | `--ledger` | `--json` |
| `daemon` | `--once`, `--workflow`, `--ledger` | `--json` |
| `status` | (none) | `--json` |

Any flag not in the above table for its subcommand produces `ArgsError::UnknownFlag`.

### `HleError` conversion

`ArgsError` implements `From<ArgsError> for HleError` so that command modules can use `?` propagation from `parse(args)?`. The conversion preserves the error code in the `HleError` message prefix: `"[2700] unknown flag: --bogus"`.

### Test surface (minimum 50 tests)

- `parse_empty_returns_help`
- `parse_help_flag_returns_help`
- `parse_version_flag_returns_version`
- `parse_run_valid_returns_run_args`
- `parse_run_missing_workflow_errors_2701`
- `parse_run_missing_ledger_errors_2701`
- `parse_run_unknown_flag_errors_2700`
- `parse_run_with_json_flag`
- `parse_run_workflow_missing_value_errors_2702`
- `parse_verify_valid_returns_verify_args`
- `parse_verify_missing_ledger_errors_2701`
- `parse_verify_unknown_flag_errors_2700`
- `parse_daemon_valid_with_once_returns_daemon_args`
- `parse_daemon_missing_once_errors_2701`
- `parse_daemon_missing_workflow_errors_2701`
- `parse_daemon_missing_ledger_errors_2701`
- `parse_daemon_unknown_flag_errors_2700`
- `parse_status_no_flags_returns_status_args`
- `parse_status_with_json_flag`
- `parse_status_unknown_flag_errors_2700`
- `args_error_unknown_flag_display_contains_flag_name`
- `args_error_missing_flag_display_contains_flag_name`
- `args_error_missing_value_display_contains_flag_name`
- `args_error_conflict_display_contains_both_flag_names`
- `args_error_code_unknown_flag_is_2700`
- `args_error_code_missing_flag_is_2701`
- `args_error_code_missing_value_is_2702`
- `args_error_code_conflict_is_2703`
- ... (additional edge cases to meet 50-test minimum)

---

*M046 cli_args Spec v1.0 | C08_CLI_SURFACE | 2026-05-10*
