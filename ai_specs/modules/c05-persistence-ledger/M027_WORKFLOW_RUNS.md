# M027 Workflow Runs — Workflow Run Table Abstraction

> **Module ID:** M027 | **Cluster:** C05_PERSISTENCE_LEDGER | **Layer:** L02
> **Source:** `crates/hle-storage/src/workflow_runs.rs`
> **Error Codes:** 2420–2422
> **Role:** One row per `hle run` invocation. Owns the lifecycle of a workflow run record —
> insert on start, update status on completion, query by id or name. Reflects status
> transitions driven by the C02 state machine (M008).

---

## Types at a Glance

| Type | Kind | Purpose |
|------|------|---------|
| `WorkflowRun` | struct | Deserialized row from `workflow_runs` |
| `RunStatus` | enum | Allowed status values matching the SQL CHECK constraint |
| `AuthorizationProfile` | enum | Profile under which the run was authorized |
| `WorkflowRunsStore` | struct | Thin table abstraction, holds `Arc<dyn Pool>` |
| `NewRun` | struct | Insert descriptor — data required to start a run |

---

## `RunStatus` Enum

```rust
/// Mirrors the SQL CHECK constraint on `workflow_runs.status`.
/// `Display` emits the lowercase string stored in the DB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunStatus {
    Running,
    Pass,
    Fail,
    AwaitingHuman,
    RolledBack,
}

impl RunStatus {
    /// Canonical string stored in the database column.
    #[must_use]
    pub const fn as_str(self) -> &'static str;

    /// Parse from the string stored in the DB.
    ///
    /// # Errors
    /// Returns [`StorageError::RunStatusInvalid`] (2422) for unknown values.
    pub fn from_str(s: &str) -> Result<Self, StorageError>;

    /// Returns true if this status is terminal (no further transitions expected).
    #[must_use]
    pub const fn is_terminal(self) -> bool;
}

impl std::fmt::Display for RunStatus { /* delegates to as_str */ }
```

Terminal statuses: `Pass`, `Fail`, `RolledBack`.
Non-terminal: `Running`, `AwaitingHuman`.

---

## `AuthorizationProfile` Enum

```rust
/// Captures which authorization profile was active at run start.
/// Stored as a text column; future migrations may add profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthorizationProfile {
    /// Bounded local M0 runtime (current authorization level).
    M0Local,
    /// Live Habitat integration (not yet authorized; reserved for future use).
    LiveIntegrated,
    /// Test/CI profile with relaxed isolation.
    TestHarness,
}

impl AuthorizationProfile {
    #[must_use]
    pub const fn as_str(self) -> &'static str;

    /// # Errors
    /// Returns [`StorageError::RunStatusInvalid`] (2422) for unknown profile strings.
    pub fn from_str(s: &str) -> Result<Self, StorageError>;
}
```

---

## `WorkflowRun` Struct

```rust
/// Deserialized row from the `workflow_runs` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowRun {
    pub id: i64,
    pub workflow_name: String,
    pub status: RunStatus,
    /// Unix epoch seconds — populated by the caller, not by `strftime`.
    pub created_unix: i64,
    /// `None` until the run reaches a terminal status.
    pub completed_unix: Option<i64>,
    /// Authorization profile active at run start.
    pub authorization_profile: AuthorizationProfile,
}
```

---

## `NewRun` Struct

```rust
/// Data required to insert a new workflow run row.
#[derive(Debug, Clone)]
pub struct NewRun {
    pub workflow_name: String,
    /// Callers supply the current Unix timestamp; the store does not call `time::now`.
    pub created_unix: i64,
    pub authorization_profile: AuthorizationProfile,
}
```

---

## `WorkflowRunsStore` Struct and Methods

```rust
#[derive(Debug, Clone)]
pub struct WorkflowRunsStore {
    pool: std::sync::Arc<dyn Pool>,
}

impl WorkflowRunsStore {
    /// Construct the store. The pool must already have migrations applied.
    #[must_use]
    pub fn new(pool: std::sync::Arc<dyn Pool>) -> Self;

    /// Insert a new run row with status `Running`.
    /// Returns the assigned `run_id`.
    ///
    /// # Errors
    /// Returns [`StorageError::RunInsert`] (2420) on SQL failure.
    pub fn insert(&self, run: &NewRun) -> Result<i64, StorageError>;

    /// Transition the run's status. Callers must respect the state machine defined
    /// in M008 (`state_machine.rs`) — this store does not re-validate transitions.
    ///
    /// # Errors
    /// - [`StorageError::RunNotFound`] (2421) if `run_id` does not exist.
    /// - [`StorageError::RunStatusInvalid`] (2422) if `new_status.as_str()` is rejected
    ///   by the SQL CHECK constraint (should not occur for valid `RunStatus` values).
    /// - [`StorageError::RunInsert`] (2420) on other SQL failure.
    pub fn set_status(
        &self,
        run_id: i64,
        new_status: RunStatus,
        completed_unix: Option<i64>,
    ) -> Result<(), StorageError>;

    /// Fetch a single run by id.
    ///
    /// # Errors
    /// Returns [`StorageError::RunNotFound`] (2421) if absent.
    pub fn get(&self, run_id: i64) -> Result<WorkflowRun, StorageError>;

    /// Fetch all runs for a workflow name, newest first.
    ///
    /// # Errors
    /// Returns [`StorageError::Storage`] (2499) on query failure.
    pub fn list_by_name(&self, workflow_name: &str) -> Result<Vec<WorkflowRun>, StorageError>;

    /// Fetch the most recent run regardless of workflow name.
    ///
    /// # Errors
    /// Returns `Ok(None)` if no runs exist. [`StorageError::Storage`] (2499) on failure.
    pub fn latest(&self) -> Result<Option<WorkflowRun>, StorageError>;

    /// Count runs with the given status.
    ///
    /// # Errors
    /// Returns [`StorageError::Storage`] (2499) on query failure.
    pub fn count_by_status(&self, status: RunStatus) -> Result<i64, StorageError>;

    /// Delete runs older than the retention window.
    /// Uses the predicate `created_unix < (strftime('%s','now') - retention_secs)`.
    /// **Never uses a literal integer** (framework §17.5).
    ///
    /// # Errors
    /// Returns [`StorageError::Storage`] (2499) on query failure.
    pub fn delete_older_than(&self, retention_secs: i64) -> Result<usize, StorageError>;
}
```

---

## SQL Patterns

All SQL executed by M027 uses bound parameters and the TTL pattern from framework §17.5:

```sql
-- insert
INSERT INTO workflow_runs (workflow_name, status, created_unix, authorization_profile)
VALUES (?1, ?2, ?3, ?4);

-- set_status
UPDATE workflow_runs SET status = ?1, completed_unix = ?2 WHERE id = ?3;

-- TTL delete — retention_secs is a bound parameter, NEVER a literal integer
DELETE FROM workflow_runs
WHERE created_unix < (strftime('%s','now') - ?1)
  AND status IN ('pass','fail','rolled-back');
```

---

## Method/Trait Table

| Method | Returns | Error Codes |
|--------|---------|-------------|
| `insert` | `Result<i64, StorageError>` | 2420 |
| `set_status` | `Result<(), StorageError>` | 2420, 2421, 2422 |
| `get` | `Result<WorkflowRun, StorageError>` | 2421, 2499 |
| `list_by_name` | `Result<Vec<WorkflowRun>, StorageError>` | 2499 |
| `latest` | `Result<Option<WorkflowRun>, StorageError>` | 2499 |
| `count_by_status` | `Result<i64, StorageError>` | 2499 |
| `delete_older_than` | `Result<usize, StorageError>` | 2499 |

---

## Design Notes

1. **No time calls inside the store.** `created_unix` and `completed_unix` are supplied
   by the caller. This keeps the store deterministically testable without mocking time.

2. **Status-checking is the caller's responsibility.** The store enforces the SQL CHECK
   constraint but does not replicate the state-machine logic from M008. Callers that need
   to validate a transition before persisting it should consult M008 directly.

3. **`authorization_profile` column.** This field is not in the scaffold migration 0001
   — it will be added in migration 0002. The spec documents the intended column shape;
   the Rust struct uses it once the migration exists.

4. **TTL deletes only terminal rows.** The `delete_older_than` method constrains deletion
   to rows with terminal status (`pass`, `fail`, `rolled-back`). Running or awaiting-human
   rows are never swept by TTL — they require explicit operator resolution.

5. **Clone-friendly.** `WorkflowRunsStore` derives `Clone` because `Arc<dyn Pool>` is
   cheap to clone. Multiple C05 modules can hold their own `WorkflowRunsStore` handle
   without coordination overhead.

---

## Test Targets (minimum 50)

- `insert_returns_id`: insert returns positive i64
- `insert_sets_running_status`: inserted row has status Running
- `set_status_pass`: transitions Running -> Pass, sets completed_unix
- `set_status_fail`: transitions Running -> Fail
- `set_status_awaiting_human`: transitions Running -> AwaitingHuman
- `set_status_not_found`: RunNotFound (2421) for missing run_id
- `get_round_trips`: insert then get returns identical struct
- `list_by_name_empty`: returns empty vec for unknown name
- `list_by_name_multiple`: returns all runs for a name, newest first
- `latest_returns_most_recent`: latest() returns last inserted
- `latest_on_empty_db`: latest() returns Ok(None)
- `count_by_status_running`: count_by_status(Running) reflects inserts
- `delete_older_than_removes_terminal`: old pass/fail rows removed
- `delete_older_than_spares_running`: running rows not swept
- `run_status_as_str_round_trip`: from_str(as_str(s)) == s for all variants
- `run_status_invalid_string_rejected`: RunStatusInvalid (2422) for unknown status
- `authorization_profile_round_trip`: M0Local, LiveIntegrated, TestHarness
- `is_terminal_pass_fail_rolledback`: is_terminal() true for terminal variants
- `is_terminal_running_awaiting`: is_terminal() false for non-terminal variants

---

*M027 Workflow Runs Spec v1.0 | C05_PERSISTENCE_LEDGER | habitat-loop-engine*
