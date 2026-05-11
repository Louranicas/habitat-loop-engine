# C08 — CLI Surface — Cluster Overview

> **Cluster:** C08_CLI_SURFACE | **Layer:** L06 | **Modules:** 5 (M046-M050)
> **Error code range:** 2700-2799 | **Synergy:** operator commands remain thin typed adapters over executor/verifier/runbook authority

---

## File Map

```
crates/hle-cli/src/
├── main.rs          # entrypoint: dispatch ParsedCommand -> module fn -> print/exit
├── args.rs          # M046 cli_args   — typed hand-rolled argument parser
├── run.rs           # M047 cli_run    — hle run adapter
├── verify.rs        # M048 cli_verify — hle verify adapter
├── daemon_once.rs   # M049 cli_daemon_once — bounded hle daemon --once adapter
└── status.rs        # M050 cli_status — topology and status report adapter
```

`main.rs` is the coordinator that calls `parse(args)` (M046) to produce a `ParsedCommand`, then dispatches to one of the four command modules (M047-M050). It owns only the print/exit surface; zero business logic lives in `main.rs`.

---

## Dependency Graph (Internal)

```
main.rs
  -> M046 args.rs          (produces ParsedCommand)
  -> M047 run.rs           (handles ParsedCommand::Run)
  -> M048 verify.rs        (handles ParsedCommand::Verify)
  -> M049 daemon_once.rs   (handles ParsedCommand::DaemonOnce)
  -> M050 status.rs        (handles ParsedCommand::Status)

M047/M048/M049 depend on M046 types (ParsedCommand fields)
M049 depends on M047 (wraps CliRun::execute)
M050 has no dependency on M047-M049 (reads plan.toml and status files directly)
```

---

## Cross-Cluster Dependencies

| C08 Module | Calls Into | Cluster |
|---|---|---|
| M047 cli_run | `phase_executor::run_phases()` | C03 Bounded Execution |
| M047 cli_run | `verifier_results_store::append()` | C05 Persistence Ledger |
| M047 cli_run | `receipts_store::append()` | C01 Evidence Integrity |
| M048 cli_verify | `receipt_sha_verifier::recompute()` | C01 Evidence Integrity |
| M048 cli_verify | `false_pass_auditor::audit()` | C04 Anti-Pattern Intelligence |
| M049 cli_daemon_once | `cli_run::execute()` via M047 | C08 internal |
| M050 cli_status | reads `plan.toml`, `scaffold-status.json`, gate JSON | filesystem only |

C08 is a pure consumer: it reads types from C01/C02 (via `substrate-types`), calls into C03/C04/C05 surfaces, and returns formatted strings to `main.rs`. It writes nothing to evidence stores directly — all writes are delegated to C03/C05 module functions.

---

## Design Principles

1. **Thin adapters only.** No business logic in any C08 module. Each command module parses its sub-arguments from `ParsedCommand` fields, calls the authoritative module, and formats the result string. Anything else is a layer violation.

2. **Uniform return type.** Every public command function returns `Result<String, HleError>`. `main.rs` prints the `Ok` value to stdout and exits 0; prints the `Err` to stderr and exits 1. No command function calls `std::process::exit` directly.

3. **No verifier bypass.** CLI commands MUST NOT read receipts and produce verdicts without going through C01 `receipt_sha_verifier` (M004) and C04 `false_pass_auditor` (M020). HLE-UP-001 is enforced at this layer.

4. **Strict argument parsing.** M046 rejects unknown flags and missing required flags with structured `ArgsError` values that include the offending flag name in the message. Silent truncation is forbidden.

5. **Bounded output.** Status and summary strings are bounded (max 8 KB for status, max 2 KB for run/verify summaries). Use `BoundedString` capacity constants from C03/M011 where available; emit a truncation marker if the limit is reached.

6. **One-shot invariant.** M049 enforces the `--once` flag at the argument level. The framework string `"this codebase needs to be 'one shotted'"` must appear verbatim in the rejection error message so downstream tooling can detect it.

7. **No silent failures.** Every error path emits a structured message via `HleError`. Partial success (e.g., some receipts verified, some failed) is reported in the Ok string as a sub-verdict, not silently dropped.

---

## Error Strategy (2700-2799)

| Code | Variant | Trigger |
|---|---|---|
| 2700 | `ArgsUnknownFlag` | Unrecognized flag in argv |
| 2701 | `ArgsMissingFlag` | Required flag absent |
| 2702 | `ArgsMissingValue` | Flag present but value missing |
| 2703 | `ArgsConflict` | Mutually exclusive flags both present |
| 2710 | `RunWorkflowReadFailed` | Cannot read workflow file |
| 2711 | `RunWorkflowParseFailed` | Workflow TOML parse error |
| 2712 | `RunExecutorFailed` | phase_executor returned error |
| 2713 | `RunLedgerWriteFailed` | verifier_results_store append failed |
| 2720 | `VerifyLedgerReadFailed` | Cannot read ledger file |
| 2721 | `VerifyLedgerParseFailed` | Ledger JSON parse error |
| 2722 | `VerifyHashFailed` | receipt_sha_verifier mismatch |
| 2723 | `VerifyFalsePassDetected` | false_pass_auditor rejected a receipt |
| 2730 | `DaemonOnceMissingFlag` | --once absent from daemon invocation |
| 2731 | `DaemonOnceInnerFailed` | Wrapped CliRun returned error |
| 2740 | `StatusReadFailed` | Cannot read plan.toml or status JSON |
| 2741 | `StatusParseFailed` | Status JSON parse error |
| 2799 | `CliOther` | Unclassified CLI error |

Errors 2700-2703 are generated by M046. Errors 2710-2713 by M047. Errors 2720-2723 by M048. Errors 2730-2731 by M049. Errors 2740-2741 by M050. Code 2799 is the catch-all for `HleError::new(msg)` conversions that have not yet been promoted to a typed variant.

---

## Quality Gate

```bash
# Zero-tolerance
cargo clippy --workspace --all-targets -- -D warnings -W clippy::pedantic
cargo test --workspace --all-targets
# Per-module minimums (50 tests each)
# M046: parser unit tests (flag presence, unknown flags, missing values, conflicts)
# M047: run adapter tests (mock executor, receipt write, verdict formatting)
# M048: verify adapter tests (SHA recompute, false-pass audit, report formatting)
# M049: daemon --once gate tests + integration round-trip
# M050: status parse + JSON output + human-readable formatting
```

*C08 CLI Surface Overview v1.0 | habitat-loop-engine | 2026-05-10*
