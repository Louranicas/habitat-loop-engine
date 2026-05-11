# M031 Blockers Store — Blocked and Awaiting-Human State Persistence

> **Module ID:** M031 | **Cluster:** C05_PERSISTENCE_LEDGER | **Layer:** L02
> **Source:** `crates/hle-storage/src/blockers_store.rs`
> **Error Codes:** 2460–2461
> **Role:** Persistence for workflow steps that are blocked and awaiting human input.
> A blocker row is inserted when a step transitions to the `AwaitingHuman` status and is
> resolved (by setting `resolved_unix` and `resolved_by`) when the human takes action.
> Unlike M029/M030 the resolution is an in-place update — this is the only permitted
> post-insert mutation in the cluster. Insertion is idempotent per `(run_id, step_id)` pair.

---

## Types at a Glance

| Type | Kind | Purpose |
|------|------|---------|
| `BlockerKind` | enum | Reason the step is blocked |
| `BlockerRecord` | struct | Deserialized row from `blockers_store` |
| `NewBlocker` | struct | Insert descriptor |
| `BlockerResolution` | struct | Resolution data applied by `resolve_blocker` |
| `BlockersStore` | struct | Table abstraction over `Arc<dyn Pool>` |

---

## `BlockerKind` Enum

```rust
/// Classifies why a step is blocked.
/// Stored as a TEXT column. Add variants; never rename existing ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockerKind {
    /// A human must review and approve before the step can proceed.
    HumanApprovalRequired,
    /// Step output requires manual inspection before the workflow continues.
    ManualInspectionRequired,
    /// External dependency is unavailable; a human must resolve the dependency.
    ExternalDependencyUnavailable,
    /// Security policy gate requires human override.
    SecurityPolicyGate,
    /// Verifier issued an AWAITING_HUMAN verdict for this step.
    VerifierDeferral,
}

impl BlockerKind {
    /// Canonical database string.
    #[must_use]
    pub const fn as_str(self) -> &'static str;

    /// Parse from database string.
    ///
    /// # Errors
    /// Returns [`StorageError::BlockerInsert`] (2460) for unknown strings.
    pub fn from_str(s: &str) -> Result<Self, StorageError>;
}

impl std::fmt::Display for BlockerKind { /* delegates to as_str */ }
```

---

## `BlockerRecord` Struct

```rust
/// Deserialized row from `blockers_store`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockerRecord {
    pub id: i64,
    pub run_id: i64,
    pub step_id: String,
    pub blocker_kind: BlockerKind,
    /// Unix epoch seconds when the blocker was first recorded.
    pub since_unix: i64,
    /// Human-readable description of what is needed to resolve this blocker.
    pub description: Option<String>,
    /// The role expected to resolve this blocker, e.g. `"@0.A"` (Luke), `"ops-on-call"`.
    pub expected_resolver_role: String,
    /// Unix epoch seconds when the blocker was resolved. `None` until resolved.
    pub resolved_unix: Option<i64>,
    /// Who resolved it — free-text role or agent identifier.
    pub resolved_by: Option<String>,
    /// Optional resolution note from the resolver.
    pub resolution_note: Option<String>,
}

impl BlockerRecord {
    /// Returns `true` if `resolved_unix` is set.
    #[must_use]
    pub fn is_resolved(&self) -> bool;

    /// Duration the blocker has been open, in seconds.
    /// Uses caller-supplied `now_unix` rather than system time.
    #[must_use]
    pub fn open_duration_secs(&self, now_unix: i64) -> i64;
}
```

---

## `NewBlocker` Struct

```rust
/// Data required to insert a new blocker row.
#[derive(Debug, Clone)]
pub struct NewBlocker {
    pub run_id: i64,
    pub step_id: String,
    pub blocker_kind: BlockerKind,
    /// Unix epoch seconds — caller-supplied.
    pub since_unix: i64,
    pub description: Option<String>,
    /// Role expected to provide resolution; must not be empty.
    pub expected_resolver_role: String,
}
```

---

## `BlockerResolution` Struct

```rust
/// Data required to mark a blocker as resolved.
#[derive(Debug, Clone)]
pub struct BlockerResolution {
    pub resolved_unix: i64,
    pub resolved_by: String,
    pub resolution_note: Option<String>,
}
```

---

## `BlockersStore` Struct and Methods

```rust
#[derive(Debug, Clone)]
pub struct BlockersStore {
    pool: std::sync::Arc<dyn Pool>,
}

impl BlockersStore {
    /// Construct the store. Pool must have migrations applied.
    #[must_use]
    pub fn new(pool: std::sync::Arc<dyn Pool>) -> Self;

    /// Insert a new blocker row. Returns the assigned `id`.
    ///
    /// Idempotency: if a row for `(run_id, step_id)` already exists and is unresolved,
    /// the insert is a no-op and returns the existing row's `id`.
    /// If the existing row is resolved, a new row is inserted (re-blocking is permitted).
    ///
    /// # Errors
    /// - [`StorageError::BlockerInsert`] (2460) on SQL failure.
    pub fn insert(&self, blocker: &NewBlocker) -> Result<i64, StorageError>;

    /// Fetch a single unresolved blocker for `(run_id, step_id)`.
    ///
    /// # Errors
    /// - [`StorageError::BlockerNotFound`] (2461) if no unresolved row exists.
    /// - [`StorageError::Storage`] (2499) on query failure.
    pub fn get_unresolved(
        &self,
        run_id: i64,
        step_id: &str,
    ) -> Result<BlockerRecord, StorageError>;

    /// Fetch a single blocker by its database `id`.
    ///
    /// # Errors
    /// Returns `Ok(None)` if not found. [`StorageError::Storage`] (2499) on failure.
    pub fn get_by_id(&self, id: i64) -> Result<Option<BlockerRecord>, StorageError>;

    /// Fetch all unresolved blockers for a run, ordered by `since_unix` ascending.
    ///
    /// # Errors
    /// Returns [`StorageError::Storage`] (2499) on query failure.
    pub fn list_unresolved_for_run(&self, run_id: i64) -> Result<Vec<BlockerRecord>, StorageError>;

    /// Fetch all blockers (resolved and unresolved) for a run.
    ///
    /// # Errors
    /// Returns [`StorageError::Storage`] (2499) on query failure.
    pub fn list_all_for_run(&self, run_id: i64) -> Result<Vec<BlockerRecord>, StorageError>;

    /// Mark the unresolved blocker for `(run_id, step_id)` as resolved.
    ///
    /// Updates `resolved_unix`, `resolved_by`, and `resolution_note` in-place.
    /// This is the only UPDATE permitted in the cluster, and it is bounded: only
    /// the three resolution columns are modified; all other columns are immutable.
    ///
    /// # Errors
    /// - [`StorageError::BlockerNotFound`] (2461) if no unresolved blocker exists for
    ///   the given `(run_id, step_id)` pair.
    /// - [`StorageError::Storage`] (2499) on SQL failure.
    pub fn resolve_blocker(
        &self,
        run_id: i64,
        step_id: &str,
        resolution: &BlockerResolution,
    ) -> Result<(), StorageError>;

    /// Count unresolved blockers for a run.
    ///
    /// # Errors
    /// Returns [`StorageError::Storage`] (2499) on query failure.
    pub fn count_unresolved(&self, run_id: i64) -> Result<i64, StorageError>;

    /// Fetch all unresolved blockers whose expected resolver matches the given role.
    /// Used by the CLI status command (M046) to surface blockers relevant to the
    /// current operator.
    ///
    /// # Errors
    /// Returns [`StorageError::Storage`] (2499) on query failure.
    pub fn list_by_resolver_role(
        &self,
        run_id: i64,
        role: &str,
    ) -> Result<Vec<BlockerRecord>, StorageError>;

    /// Delete resolved blocker rows older than the retention window.
    /// Only resolved rows are eligible — unresolved blockers are never swept by TTL.
    /// Predicate: `resolved_unix IS NOT NULL AND since_unix < (strftime('%s','now') - retention_secs)`.
    ///
    /// **Never uses a literal integer** (framework §17.5).
    ///
    /// # Errors
    /// Returns [`StorageError::Storage`] (2499) on query failure.
    pub fn delete_resolved_older_than(&self, retention_secs: i64) -> Result<usize, StorageError>;
}
```

---

## Schema DDL (planned migration `0005_blockers_store.sql`)

```sql
CREATE TABLE IF NOT EXISTS blockers_store (
    id                    INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id                INTEGER NOT NULL REFERENCES workflow_runs(id),
    step_id               TEXT    NOT NULL,
    blocker_kind          TEXT    NOT NULL,
    since_unix            INTEGER NOT NULL,
    description           TEXT,
    expected_resolver_role TEXT   NOT NULL,
    resolved_unix         INTEGER,
    resolved_by           TEXT,
    resolution_note       TEXT
) STRICT;

CREATE INDEX IF NOT EXISTS idx_blockers_run_id    ON blockers_store(run_id);
CREATE INDEX IF NOT EXISTS idx_blockers_step_id   ON blockers_store(run_id, step_id);
CREATE INDEX IF NOT EXISTS idx_blockers_unresolved ON blockers_store(run_id)
    WHERE resolved_unix IS NULL;
CREATE INDEX IF NOT EXISTS idx_blockers_resolver  ON blockers_store(expected_resolver_role)
    WHERE resolved_unix IS NULL;
```

The partial indexes on `WHERE resolved_unix IS NULL` accelerate the common query path
(listing open blockers) without scanning resolved historical rows.

---

## SQL Patterns

```sql
-- insert
INSERT INTO blockers_store
    (run_id, step_id, blocker_kind, since_unix, description, expected_resolver_role)
VALUES (?1, ?2, ?3, ?4, ?5, ?6);

-- idempotency check before insert
SELECT id FROM blockers_store
WHERE run_id = ?1 AND step_id = ?2 AND resolved_unix IS NULL
LIMIT 1;

-- get_unresolved
SELECT * FROM blockers_store
WHERE run_id = ?1 AND step_id = ?2 AND resolved_unix IS NULL
LIMIT 1;

-- resolve_blocker
UPDATE blockers_store
SET resolved_unix = ?1, resolved_by = ?2, resolution_note = ?3
WHERE run_id = ?4 AND step_id = ?5 AND resolved_unix IS NULL;

-- TTL delete — retention_secs is a bound parameter; only resolved rows swept
DELETE FROM blockers_store
WHERE resolved_unix IS NOT NULL
  AND since_unix < (strftime('%s','now') - ?1);
```

---

## Permitted Mutations

| Column | Created By | Mutated By |
|--------|-----------|------------|
| All non-resolution columns | `insert` | Never |
| `resolved_unix` | NULL | `resolve_blocker` once |
| `resolved_by` | NULL | `resolve_blocker` once |
| `resolution_note` | NULL | `resolve_blocker` once |

Once `resolve_blocker` sets `resolved_unix`, subsequent calls for the same `(run_id, step_id)`
find no unresolved row and return `BlockerNotFound` (2461).

---

## Method/Trait Table

| Method | Returns | Error Codes |
|--------|---------|-------------|
| `insert` | `Result<i64, StorageError>` | 2460 |
| `get_unresolved` | `Result<BlockerRecord, StorageError>` | 2461, 2499 |
| `get_by_id` | `Result<Option<BlockerRecord>, StorageError>` | 2499 |
| `list_unresolved_for_run` | `Result<Vec<BlockerRecord>, StorageError>` | 2499 |
| `list_all_for_run` | `Result<Vec<BlockerRecord>, StorageError>` | 2499 |
| `resolve_blocker` | `Result<(), StorageError>` | 2461, 2499 |
| `count_unresolved` | `Result<i64, StorageError>` | 2499 |
| `list_by_resolver_role` | `Result<Vec<BlockerRecord>, StorageError>` | 2499 |
| `delete_resolved_older_than` | `Result<usize, StorageError>` | 2499 |

---

## Design Notes

1. **Idempotent insert.** If a step sends two `AwaitingHuman` events (e.g., due to retry
   logic in the state machine), the second insert is a no-op. The existing unresolved row's
   id is returned. This prevents duplicate blocker rows for the same step in the same run.

2. **Unresolved rows are never TTL-swept.** The `delete_resolved_older_than` method
   constrains the TTL predicate to `resolved_unix IS NOT NULL`. An unresolved blocker
   remains visible in the ledger until a human explicitly resolves it — time alone cannot
   expire it.

3. **`expected_resolver_role` is free-text.** The value `"@0.A"` conventionally refers to
   Luke (the human authority at node 0.A). Other roles such as `"ops-on-call"` or
   `"security-reviewer"` are project-specific conventions. The store does not validate
   or enumerate roles.

4. **Resolution is a bounded UPDATE.** The `resolve_blocker` method issues a single
   `UPDATE` that targets exactly three columns (`resolved_unix`, `resolved_by`,
   `resolution_note`) and is guarded by `WHERE resolved_unix IS NULL`. If no row matches
   (already resolved or never inserted), it returns `BlockerNotFound` (2461).

5. **Partial indexes for open-blocker queries.** The partial index
   `WHERE resolved_unix IS NULL` on `run_id` and `expected_resolver_role` ensures that
   the common monitoring path (list all open blockers for a role) does not scan historical
   resolved rows.

6. **No time calls inside the store.** `since_unix` and `resolved_unix` are always
   caller-supplied. `open_duration_secs` accepts `now_unix` as a parameter.

7. **Runbook integration.** C06 `runbook_human_confirm` (M031 in the runbook cluster)
   uses M031 (this store) as its backing persistence. The runbook layer does not bypass
   the store — it calls `insert` and `resolve_blocker` through the public API.

---

## Test Targets (minimum 50)

- `insert_new_blocker_returns_id`: inserts successfully, returns positive id
- `insert_idempotent_unresolved_returns_existing_id`: second insert returns first id
- `insert_after_resolution_creates_new_row`: re-blocking after resolution inserts new row
- `get_unresolved_found`: returns unresolved row for valid (run_id, step_id)
- `get_unresolved_not_found`: BlockerNotFound (2461) when no unresolved row
- `get_by_id_found`: insert then get_by_id returns matching record
- `get_by_id_absent`: Ok(None) for missing id
- `list_unresolved_for_run_ordered`: rows ordered ascending by since_unix
- `list_unresolved_for_run_excludes_resolved`: resolved rows not in list
- `list_all_for_run_includes_resolved`: both resolved and unresolved returned
- `list_all_for_run_empty`: empty vec for run with no blockers
- `resolve_blocker_sets_fields`: resolved_unix, resolved_by, resolution_note persisted
- `resolve_blocker_not_found`: BlockerNotFound (2461) when no unresolved row exists
- `resolve_blocker_idempotent_fails_second_call`: second resolve returns 2461
- `count_unresolved_zero`: zero for run with no blockers
- `count_unresolved_decrements_after_resolve`: count drops after resolve
- `list_by_resolver_role_matches`: returns only rows for given role
- `list_by_resolver_role_no_match`: empty vec for unknown role
- `delete_resolved_older_than_removes_resolved`: old resolved rows swept
- `delete_resolved_older_than_spares_unresolved`: unresolved rows never swept
- `delete_resolved_older_than_spares_recent`: recent resolved rows within window kept
- `blocker_kind_as_str_round_trip`: all variants round-trip through from_str/as_str
- `blocker_kind_unknown_rejected`: BlockerInsert (2460) for unknown string
- `is_resolved_true`: is_resolved() true after resolve_blocker
- `is_resolved_false`: is_resolved() false for unresolved row
- `open_duration_secs_calculation`: open_duration_secs = now_unix - since_unix
- `description_optional_none`: None description stored as NULL and retrieved
- `description_optional_some`: Some description stored and retrieved correctly
- `resolution_note_optional`: None and Some resolution_note both handled
- `expected_resolver_role_stored_verbatim`: free-text role round-trips without change

---

*M031 Blockers Store Spec v1.0 | C05_PERSISTENCE_LEDGER | habitat-loop-engine*
