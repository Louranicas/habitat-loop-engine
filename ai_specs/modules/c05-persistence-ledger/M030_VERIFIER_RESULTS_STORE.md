# M030 Verifier Results Store — Append-Only Verifier Verdict Ledger

> **Module ID:** M030 | **Cluster:** C05_PERSISTENCE_LEDGER | **Layer:** L02
> **Source:** `crates/hle-storage/src/verifier_results_store.rs`
> **Error Codes:** 2450–2451
> **Role:** Append-only ledger for verifier verdicts. Only the designated verifier authority
> may call `insert`. Each row records a step verdict, the verifier binary version that
> produced it, and a foreign-key SHA linking it to an artifact row in M029 (`evidence_store`).
> No UPDATE or DELETE is exposed by the public API — the ledger is immutable after insertion.

---

## Types at a Glance

| Type | Kind | Purpose |
|------|------|---------|
| `VerifierVerdict` | enum | PASS, FAIL, or AWAITING_HUMAN outcome |
| `VerifierResult` | struct | Deserialized row from `verifier_results_store` |
| `NewVerifierResult` | struct | Insert descriptor — assembled by the verifier authority |
| `VerifierResultsStore` | struct | Table abstraction over `Arc<dyn Pool>` |

---

## `VerifierVerdict` Enum

```rust
/// Verdict emitted by the independent verifier for a single step.
/// Stored as TEXT; the SQL CHECK constraint mirrors these string values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VerifierVerdict {
    /// Step evidence independently verified; all integrity checks passed.
    Pass,
    /// Verifier rejected the step; integrity check failed or evidence missing.
    Fail,
    /// Verifier deferred: step requires human review before a verdict can be issued.
    AwaitingHuman,
}

impl VerifierVerdict {
    /// Canonical database string.
    #[must_use]
    pub const fn as_str(self) -> &'static str;

    /// Parse from database string.
    ///
    /// # Errors
    /// Returns [`StorageError::VerifierVerdictInvalid`] (2451) for unknown strings.
    pub fn from_str(s: &str) -> Result<Self, StorageError>;

    /// Returns `true` for [`VerifierVerdict::Pass`].
    #[must_use]
    pub const fn is_pass(self) -> bool;

    /// Returns `true` for [`VerifierVerdict::Fail`].
    #[must_use]
    pub const fn is_fail(self) -> bool;
}

impl std::fmt::Display for VerifierVerdict { /* delegates to as_str */ }
```

---

## `VerifierResult` Struct

```rust
/// Deserialized row from `verifier_results_store`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifierResult {
    pub id: i64,
    pub run_id: i64,
    /// Step identifier this verdict applies to.
    pub step_id: String,
    pub verdict: VerifierVerdict,
    /// SHA-256 hex of the evidence artifact certified by this verdict.
    /// Foreign key to `evidence_store.sha256_hex`.
    pub receipt_sha: String,
    /// Semver string of the verifier binary that produced this verdict.
    /// Used for reproducibility audits.
    pub verifier_version: String,
    /// Optional human-readable notes from the verifier.
    pub notes: Option<String>,
    /// Unix epoch seconds — supplied by the verifier caller.
    pub created_unix: i64,
}
```

---

## `NewVerifierResult` Struct

```rust
/// Data required to insert a verifier verdict row.
/// Only a verifier authority (not the executor) constructs this.
#[derive(Debug, Clone)]
pub struct NewVerifierResult {
    pub run_id: i64,
    pub step_id: String,
    pub verdict: VerifierVerdict,
    /// Must match a `sha256_hex` that already exists in `evidence_store`.
    pub receipt_sha: String,
    /// Semver string identifying the verifier binary or module version.
    pub verifier_version: String,
    /// Optional explanation, counterevidence locator, or human-review prompt.
    pub notes: Option<String>,
    /// Unix epoch seconds — supplied by caller.
    pub created_unix: i64,
}
```

---

## `VerifierResultsStore` Struct and Methods

```rust
#[derive(Debug, Clone)]
pub struct VerifierResultsStore {
    pool: std::sync::Arc<dyn Pool>,
    /// Reference to the evidence store, used to validate `receipt_sha` FK.
    evidence: std::sync::Arc<EvidenceStore>,
}

impl VerifierResultsStore {
    /// Construct the store. Pool must have migrations applied.
    /// `evidence` is used for pre-insert SHA validation.
    #[must_use]
    pub fn new(
        pool: std::sync::Arc<dyn Pool>,
        evidence: std::sync::Arc<EvidenceStore>,
    ) -> Self;

    /// Insert a verifier verdict row. Returns the assigned `id`.
    ///
    /// Validation sequence before INSERT:
    /// 1. Verify `receipt_sha` exists in `evidence_store` via `EvidenceStore::sha_exists`.
    ///    If absent: returns [`StorageError::VerifierInsert`] (2450).
    /// 2. Validate `verdict` can be serialised (all enum variants are valid; no error path
    ///    under normal usage).
    /// 3. Execute INSERT.
    ///
    /// # Errors
    /// - [`StorageError::VerifierInsert`] (2450) if `receipt_sha` not found in evidence_store
    ///   or if the SQL INSERT fails.
    /// - [`StorageError::VerifierVerdictInvalid`] (2451) if verdict string is not recognised
    ///   (only possible if a raw string is passed via a future migration path).
    pub fn insert(&self, result: &NewVerifierResult) -> Result<i64, StorageError>;

    /// Fetch a single verifier result by its database `id`.
    ///
    /// # Errors
    /// Returns `Ok(None)` if not found. [`StorageError::Storage`] (2499) on failure.
    pub fn get_by_id(&self, id: i64) -> Result<Option<VerifierResult>, StorageError>;

    /// Fetch all verifier results for a run, ordered by `created_unix` ascending.
    ///
    /// # Errors
    /// Returns [`StorageError::Storage`] (2499) on query failure.
    pub fn list_for_run(&self, run_id: i64) -> Result<Vec<VerifierResult>, StorageError>;

    /// Fetch the most recent verdict for a specific `(run_id, step_id)` pair.
    ///
    /// # Errors
    /// Returns `Ok(None)` if no verdict has been recorded for this step.
    pub fn latest_for_step(
        &self,
        run_id: i64,
        step_id: &str,
    ) -> Result<Option<VerifierResult>, StorageError>;

    /// Fetch all results for a run filtered by verdict.
    ///
    /// # Errors
    /// Returns [`StorageError::Storage`] (2499) on query failure.
    pub fn list_by_verdict(
        &self,
        run_id: i64,
        verdict: VerifierVerdict,
    ) -> Result<Vec<VerifierResult>, StorageError>;

    /// Count verdicts of a given type for a run.
    ///
    /// # Errors
    /// Returns [`StorageError::Storage`] (2499) on query failure.
    pub fn count_by_verdict(
        &self,
        run_id: i64,
        verdict: VerifierVerdict,
    ) -> Result<i64, StorageError>;

    /// Check whether a PASS verdict exists for `(run_id, step_id)`.
    /// Used by C04 `false_pass_auditor` (M020) to confirm authority evidence exists.
    ///
    /// # Errors
    /// Returns [`StorageError::Storage`] (2499) on query failure.
    pub fn has_pass_for_step(&self, run_id: i64, step_id: &str) -> Result<bool, StorageError>;

    /// Delete verifier result rows older than the retention window.
    /// Predicate: `created_unix < (strftime('%s','now') - retention_secs)`.
    ///
    /// **Never uses a literal integer** (framework §17.5).
    ///
    /// # Errors
    /// Returns [`StorageError::Storage`] (2499) on query failure.
    pub fn delete_older_than(&self, retention_secs: i64) -> Result<usize, StorageError>;
}
```

---

## Schema DDL (planned migration `0004_verifier_results_store.sql`)

```sql
CREATE TABLE IF NOT EXISTS verifier_results_store (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id           INTEGER NOT NULL REFERENCES workflow_runs(id),
    step_id          TEXT    NOT NULL,
    verdict          TEXT    NOT NULL CHECK (verdict IN ('pass','fail','awaiting_human')),
    receipt_sha      TEXT    NOT NULL REFERENCES evidence_store(sha256_hex),
    verifier_version TEXT    NOT NULL,
    notes            TEXT,
    created_unix     INTEGER NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_verifier_results_run_id  ON verifier_results_store(run_id);
CREATE INDEX IF NOT EXISTS idx_verifier_results_step_id ON verifier_results_store(run_id, step_id);
CREATE INDEX IF NOT EXISTS idx_verifier_results_sha     ON verifier_results_store(receipt_sha);
```

---

## Executor / Verifier Separation (HLE-UP-001)

This module enforces the use-pattern defined in `UP_EXECUTOR_VERIFIER_SPLIT.md`. The store
does not know which binary is calling it, but its design obligates the caller architecture:

- The executor (C03) writes to M027/M028/M029 — it never calls `VerifierResultsStore::insert`.
- The verifier (C04) calls `VerifierResultsStore::insert` after independently verifying the
  evidence SHA.
- A single binary that performs both the step action and inserts a PASS verdict violates
  HLE-UP-001. This cannot be prevented at the type level in the store itself, but is
  enforced by the layer DAG (C03 does not depend on M030).

---

## SQL Patterns

```sql
-- insert
INSERT INTO verifier_results_store
    (run_id, step_id, verdict, receipt_sha, verifier_version, notes, created_unix)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7);

-- latest_for_step
SELECT * FROM verifier_results_store
WHERE run_id = ?1 AND step_id = ?2
ORDER BY created_unix DESC
LIMIT 1;

-- has_pass_for_step
SELECT COUNT(*) FROM verifier_results_store
WHERE run_id = ?1 AND step_id = ?2 AND verdict = 'pass';

-- TTL sweep — retention_secs is a bound parameter
DELETE FROM verifier_results_store WHERE created_unix < (strftime('%s','now') - ?1);
```

---

## Method/Trait Table

| Method | Returns | Error Codes |
|--------|---------|-------------|
| `insert` | `Result<i64, StorageError>` | 2450, 2451 |
| `get_by_id` | `Result<Option<VerifierResult>, StorageError>` | 2499 |
| `list_for_run` | `Result<Vec<VerifierResult>, StorageError>` | 2499 |
| `latest_for_step` | `Result<Option<VerifierResult>, StorageError>` | 2499 |
| `list_by_verdict` | `Result<Vec<VerifierResult>, StorageError>` | 2499 |
| `count_by_verdict` | `Result<i64, StorageError>` | 2499 |
| `has_pass_for_step` | `Result<bool, StorageError>` | 2499 |
| `delete_older_than` | `Result<usize, StorageError>` | 2499 |

---

## Design Notes

1. **`receipt_sha` as a soft FK.** SQLite REFERENCES on a non-INTEGER-PRIMARY-KEY column
   (`sha256_hex TEXT UNIQUE`) is validated by the application via `EvidenceStore::sha_exists`
   rather than by the database engine (SQLite FK support requires `PRAGMA foreign_keys=ON`
   and the referenced column must be a primary key or have a UNIQUE constraint). The store
   therefore performs a pre-insert existence check on the evidence table.

2. **`verifier_version` enables reproducibility audits.** A future auditor can determine
   whether a verdict was produced by a version of the verifier that was later found to have
   a bug. The field is free-text semver; no parsing is performed in the store.

3. **`has_pass_for_step` is the C04 audit hook.** C04's `false_pass_auditor` (M020) calls
   this method to determine whether a PASS claim for a step has independent verifier
   authority. The query is read-only; M030 does not depend on M020.

4. **Append-only at the API level.** The public type exposes no `update` or `delete` beyond
   the TTL sweep. Correction of a wrong verdict requires a new row with the correct verdict;
   the old row remains for audit.

5. **No time calls inside the store.** `created_unix` is always caller-supplied, enabling
   deterministic test assertions without time mocking.

---

## Test Targets (minimum 50)

- `insert_pass_verdict_with_valid_sha`: inserts successfully, returns positive id
- `insert_fail_verdict`: inserts with verdict Fail
- `insert_awaiting_human_verdict`: inserts with verdict AwaitingHuman
- `insert_unknown_receipt_sha_rejected`: VerifierInsert (2450) when SHA absent in evidence
- `get_by_id_found`: insert then get_by_id returns matching record
- `get_by_id_absent`: Ok(None) for missing id
- `list_for_run_ordered`: results ordered by created_unix ascending
- `list_for_run_empty`: empty vec for run with no results
- `latest_for_step_returns_most_recent`: multiple inserts; latest_for_step returns last
- `latest_for_step_absent`: Ok(None) for step with no verdicts
- `list_by_verdict_pass_only`: only PASS rows returned
- `list_by_verdict_fail_only`: only FAIL rows returned
- `count_by_verdict_pass`: count reflects inserts
- `count_by_verdict_zero`: zero for verdict with no rows
- `has_pass_for_step_true`: returns true after PASS insert
- `has_pass_for_step_false_no_verdict`: false when no verdict recorded
- `has_pass_for_step_false_only_fail`: false when only FAIL recorded
- `delete_older_than_removes_old`: rows older than retention swept
- `delete_older_than_spares_recent`: rows within window not swept
- `verdict_as_str_round_trip`: all variants round-trip through from_str/as_str
- `verdict_unknown_string_rejected`: VerifierVerdictInvalid (2451) for unknown string
- `verdict_is_pass_true`: is_pass() true only for Pass
- `verdict_is_fail_true`: is_fail() true only for Fail
- `verifier_version_stored_verbatim`: string round-trips without modification
- `notes_optional_none`: None notes stored as NULL
- `notes_optional_some`: Some notes stored and retrieved correctly

---

*M030 Verifier Results Store Spec v1.0 | C05_PERSISTENCE_LEDGER | habitat-loop-engine*
