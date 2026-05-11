# M048 — cli_verify

> **File:** `crates/hle-cli/src/verify.rs` | **Layer:** L06 | **Cluster:** C08_CLI_SURFACE
> **Error codes:** 2720-2723 | **Role:** `hle verify` command adapter

---

## Purpose

M048 is the thin adapter for the `hle verify` operator command. It reads a JSONL ledger file, parses receipts from it, invokes the C01 `receipt_sha_verifier` (M004) for adversarial SHA recomputation, then invokes the C04 `false_pass_auditor` (M020) for anchored-field integrity checks. It returns a bounded verdict summary to `main.rs`.

M048 is entirely read-only with respect to the ledger: it never writes to the ledger or to any store. The verifier is the sole authority for PASS/FAIL — M048 formats what the verifier surfaces, nothing more.

---

## Types at a Glance

| Type | Kind | Role |
|---|---|---|
| `CliVerify` | struct | Stateless adapter; constructed with injected verifier surfaces |
| `VerifyArgs` | struct (re-export from M046) | Validated verify command arguments |
| `VerifyReport` | struct | Aggregated result from both verifier passes |
| `ReceiptVerdict` | enum | Per-receipt outcome: `Verified`, `HashMismatch`, `FalsePassDetected`, `Inconclusive` |
| `VerifyVerdict` | enum | Top-level verdict: `AllVerified`, `Failed`, `PartiallyVerified` |

---

## Rust Signatures

```rust
use crate::args::VerifyArgs;
use substrate_types::HleError;

/// Stateless adapter for the `hle verify` command.
/// Injected with references to C01 and C04 verifier surfaces.
pub struct CliVerify<'a, S, F>
where
    S: ReceiptShaVerifier,
    F: FalsePassAuditor,
{
    sha_verifier: &'a S,
    false_pass_auditor: &'a F,
}

impl<'a, S, F> CliVerify<'a, S, F>
where
    S: ReceiptShaVerifier,
    F: FalsePassAuditor,
{
    /// Construct the adapter with injected verifier references.
    #[must_use]
    pub fn new(sha_verifier: &'a S, false_pass_auditor: &'a F) -> Self;

    /// Execute the verify command.
    ///
    /// 1. Reads and parses the ledger file at `args.ledger_path`.
    /// 2. Calls `sha_verifier.recompute()` for each receipt (C01/M004).
    /// 3. Calls `false_pass_auditor.audit()` for each receipt (C04/M020).
    /// 4. Aggregates results into a `VerifyReport`.
    /// 5. Returns a bounded formatted string.
    ///
    /// Returns `Err(HleError)` with code 2720-2723 on any failure.
    pub fn execute(&self, args: &VerifyArgs) -> Result<String, HleError>;
}

/// Per-receipt outcome from the two-pass verification.
#[derive(Debug, PartialEq)]
pub enum ReceiptVerdict {
    /// SHA matches and no false-pass indicators detected.
    Verified,
    /// SHA recomputation produced a different hash. Code 2722.
    HashMismatch { step_id: String, expected: String, actual: String },
    /// SHA matched but false-pass auditor detected an unanchored PASS. Code 2723.
    FalsePassDetected { step_id: String, reason: String },
    /// Verifier could not produce a conclusive result (e.g., missing fields).
    Inconclusive { step_id: String, reason: String },
}

/// Aggregate verdict over all receipts in a ledger.
#[derive(Debug, PartialEq)]
pub enum VerifyVerdict {
    /// All receipts verified cleanly.
    AllVerified,
    /// One or more receipts failed SHA or false-pass checks.
    Failed,
    /// Some receipts verified; some were inconclusive (no hard failure).
    PartiallyVerified,
}

/// Full verification report.
#[derive(Debug)]
pub struct VerifyReport {
    pub ledger_path: std::path::PathBuf,
    pub total_receipts: usize,
    pub verified: usize,
    pub hash_mismatches: usize,
    pub false_passes: usize,
    pub inconclusive: usize,
    pub overall: VerifyVerdict,
    pub per_receipt: Vec<ReceiptVerdict>,
}
```

---

## Method / Trait Table

| Item | Signature | Notes |
|---|---|---|
| `CliVerify::new` | `fn(sha_verifier, false_pass_auditor) -> Self` | Injection; no defaults |
| `CliVerify::execute` | `pub fn(&self, args: &VerifyArgs) -> Result<String, HleError>` | Public entry point |
| `read_ledger` | `fn(path: &Path) -> Result<Vec<Receipt>, HleError>` | Private; maps IO err -> 2720, parse err -> 2721 |
| `verify_sha` | `fn(&S, &Receipt) -> ReceiptVerdict` | Private; calls `sha_verifier.recompute()`; maps err -> 2722 |
| `audit_false_pass` | `fn(&F, &Receipt, sha_ok: bool) -> ReceiptVerdict` | Private; calls `false_pass_auditor.audit()`; maps err -> 2723 |
| `build_report` | `fn(ledger_path, receipts, per_receipt_verdicts) -> VerifyReport` | Private; aggregates counts |
| `derive_overall_verdict` | `fn(&VerifyReport) -> VerifyVerdict` | Private; pure function |
| `format_human` | `fn(&VerifyReport) -> String` | Private; bounded 2 KB |
| `format_json` | `fn(&VerifyReport) -> String` | Private; bounded 2 KB; schema `hle.verify.report.v1` |
| `ReceiptShaVerifier::recompute` | trait method | C01/M004 surface; `fn(&self, receipt: &Receipt) -> Result<bool, HleError>` |
| `FalsePassAuditor::audit` | trait method | C04/M020 surface; `fn(&self, receipt: &Receipt) -> Result<bool, HleError>` |
| `Display for VerifyVerdict` | impl | `"ALL_VERIFIED"` / `"FAILED"` / `"PARTIALLY_VERIFIED"` |

---

## Verification Pass Sequence

```
CliVerify::execute(args)
  1. read_ledger(&args.ledger_path)
       -> HleError 2720 on IO failure
       -> HleError 2721 on parse failure
       -> Vec<Receipt>

  2. For each receipt in Vec<Receipt>:
     a. sha_verifier.recompute(receipt)
           -> ReceiptVerdict::HashMismatch if hash differs  [2722]
     b. if SHA ok: false_pass_auditor.audit(receipt)
           -> ReceiptVerdict::FalsePassDetected if unanchored  [2723]
     c. else: ReceiptVerdict::Verified

  3. build_report(ledger_path, receipts, per_receipt_verdicts)
  4. derive_overall_verdict(&report)
  5. format (human or json depending on args.json)
  6. return Ok(bounded_string)
```

The two passes are sequential: false-pass audit only runs on receipts where the SHA check passed. A SHA mismatch already indicates tampering; auditing for false-passes on top of a tampered receipt is unnecessary and potentially misleading.

---

## Design Notes

### HLE-UP-001 enforcement

M048 is the implementation of the verifier-authority invariant at the CLI surface. It never reads a ledger and produces a verdict without going through both C01 SHA recomputation and C04 false-pass audit. Any shortcut (e.g., counting `verdict` fields directly from the JSONL) would be a violation. The two-pass sequence in `execute` is the enforcement point.

### Read-only invariant

M048 never writes to any store, ledger, or file. It reads the ledger once with `read_ledger` and passes receipts through the verifier surfaces. No side effects are produced.

### Empty ledger handling

An empty ledger (zero receipts) returns `HleError` 2721 (`VerifyLedgerParseFailed`) with message `"[2721] empty ledger: no receipts to verify"`. This matches the behavior in the existing `verify_ledger` in `main.rs` which errors on empty receipt slices.

### Injected trait surfaces

Both `ReceiptShaVerifier` and `FalsePassAuditor` are injected as trait references to allow mock substitution in tests. The trait signatures are defined in this module for C08 use; the authoritative implementations live in C01/M004 and C04/M020 respectively.

### JSON output schema (hle.verify.report.v1)

```json
{
  "schema": "hle.verify.report.v1",
  "ledger_path": "/path/to/ledger.jsonl",
  "total_receipts": 3,
  "verified": 3,
  "hash_mismatches": 0,
  "false_passes": 0,
  "inconclusive": 0,
  "overall": "ALL_VERIFIED"
}
```

Human-readable format (default):

```
hle verify verdict=ALL_VERIFIED receipts=3 verified=3 mismatches=0 false_passes=0 ledger=/path/to/ledger.jsonl
```

Both formats are bounded to 2 KB.

### Error code reference

| Code | Variant | Trigger |
|---|---|---|
| 2720 | `VerifyLedgerReadFailed` | `fs::read_to_string` failed |
| 2721 | `VerifyLedgerParseFailed` | Receipt JSON parse error or empty ledger |
| 2722 | `VerifyHashFailed` | `sha_verifier.recompute()` returned `Ok(false)` or `Err` |
| 2723 | `VerifyFalsePassDetected` | `false_pass_auditor.audit()` returned `Ok(false)` |

### Test surface (minimum 50 tests)

- `execute_all_verified_on_clean_ledger`
- `execute_failed_on_hash_mismatch`
- `execute_failed_on_false_pass`
- `execute_partially_verified_when_inconclusive`
- `execute_errors_2720_on_missing_ledger_file`
- `execute_errors_2721_on_unparseable_ledger`
- `execute_errors_2721_on_empty_ledger`
- `execute_sha_check_runs_for_each_receipt`
- `execute_false_pass_audit_skipped_on_hash_mismatch`
- `execute_false_pass_audit_runs_on_sha_ok`
- `execute_json_flag_emits_verify_v1_schema`
- `execute_human_output_contains_verdict`
- `execute_human_output_contains_receipt_count`
- `execute_human_output_contains_ledger_path`
- `build_report_verified_count_correct`
- `build_report_hash_mismatch_count_correct`
- `build_report_false_pass_count_correct`
- `build_report_inconclusive_count_correct`
- `derive_overall_verdict_all_verified`
- `derive_overall_verdict_failed_on_mismatch`
- `derive_overall_verdict_failed_on_false_pass`
- `derive_overall_verdict_partially_verified`
- `receipt_verdict_hash_mismatch_contains_step_id`
- `receipt_verdict_false_pass_contains_step_id`
- `format_json_is_single_line`
- `format_json_total_receipts_correct`
- `format_json_overall_verdict_string`
- `format_human_bounded_under_2kb`
- `verify_verdict_display_all_verified`
- `verify_verdict_display_failed`
- `verify_verdict_display_partially_verified`
- `read_ledger_returns_correct_count`
- `read_ledger_parses_schema_field`
- `read_ledger_parses_step_id`
- `read_ledger_parses_verdict_field`
- `read_ledger_handles_escaped_json`
- ... (additional edge cases to meet 50-test minimum)

---

*M048 cli_verify Spec v1.0 | C08_CLI_SURFACE | 2026-05-10*
