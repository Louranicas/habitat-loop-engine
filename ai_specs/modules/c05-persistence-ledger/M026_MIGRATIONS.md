# M026 Migrations — Schema-First Migration Runner

> **Module ID:** M026 | **Cluster:** C05_PERSISTENCE_LEDGER | **Layer:** L02
> **Source:** `crates/hle-storage/src/migrations.rs`
> **Error Codes:** 2410–2412
> **Role:** Idempotent, ordered schema migration runner. Reads SQL files from `migrations/`,
> applies them in order, and tracks applied migrations in a `_schema_migrations` meta-table.
> Must run to completion before any other C05 module may use the database.

---

## Types at a Glance

| Type | Kind | Purpose |
|------|------|---------|
| `Migration` | struct | Single migration descriptor: id, name, SQL content |
| `MigrationRunner` | struct | Applies a set of `Migration` values via a `Pool` |
| `AppliedRecord` | struct | Row from `_schema_migrations`; used for idempotency checks |

---

## `Migration` Struct

```rust
/// Descriptor for one versioned schema migration.
/// SQL content is embedded at compile time via `include_str!` macros in the
/// `migrations()` free function — never read from disk at runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Migration {
    /// Monotonically increasing migration identifier, starting at 1.
    pub id: u32,
    /// Human-readable migration name, used in error messages and the meta-table.
    pub name: &'static str,
    /// Full SQL content of the migration. May contain multiple statements
    /// separated by semicolons; the runner executes them inside a single transaction.
    pub sql: &'static str,
}

impl Migration {
    /// Construct a migration descriptor.
    #[must_use]
    pub const fn new(id: u32, name: &'static str, sql: &'static str) -> Self;

    /// SHA-256 of `self.sql`, used for checksum verification.
    #[must_use]
    pub fn checksum(&self) -> [u8; 32];
}
```

---

## Canonical Migration Set

The canonical set is defined by the `migrations()` free function. SQL content is
embedded at compile time using `include_str!`:

```rust
/// Returns all known migrations in ID order.
/// Callers must not assume the slice is sorted; call [`MigrationRunner::apply_all`]
/// which enforces ordering.
#[must_use]
pub fn migrations() -> Vec<Migration> {
    vec![
        Migration::new(
            1,
            "scaffold_schema",
            include_str!("../../../migrations/0001_scaffold_schema.sql"),
        ),
        // Future migrations added here, never removed.
    ]
}
```

This approach makes the migration set part of the compiled binary — no filesystem
access is needed at runtime beyond the database file itself.

---

## `AppliedRecord` Struct

```rust
/// Row from the `_schema_migrations` meta-table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedRecord {
    pub migration_id: u32,
    pub name: String,
    /// Hex-encoded SHA-256 of the SQL content at apply time.
    pub checksum_hex: String,
    /// Unix timestamp (seconds) when the migration was applied.
    pub applied_unix: i64,
}
```

---

## `MigrationRunner` Struct

```rust
#[derive(Debug)]
pub struct MigrationRunner {
    migrations: Vec<Migration>,
}

impl MigrationRunner {
    /// Construct a runner from the canonical migration set.
    #[must_use]
    pub fn new() -> Self;

    /// Construct a runner from a custom set (for testing).
    #[must_use]
    pub fn with_migrations(migrations: Vec<Migration>) -> Self;

    /// Apply all pending migrations against the pool.
    ///
    /// Algorithm:
    /// 1. Create `_schema_migrations` meta-table if absent (DDL is idempotent).
    /// 2. Load all `AppliedRecord` rows.
    /// 3. Verify IDs are monotonically increasing (error 2412 if not).
    /// 4. For each migration (sorted by id):
    ///    a. If already applied: verify checksum matches (error 2411 if mismatch).
    ///    b. If not applied: execute inside a transaction, record in meta-table.
    /// 5. Return `Ok(applied_count)`.
    ///
    /// # Errors
    /// - [`StorageError::MigrationNotFound`] (2410) if a migration SQL file is empty.
    /// - [`StorageError::MigrationChecksum`] (2411) if an applied migration's SQL changed.
    /// - [`StorageError::MigrationOrder`] (2412) if IDs are non-monotonic.
    /// - [`StorageError::Storage`] (2499) for underlying SQLite errors.
    pub fn apply_all(&self, pool: &dyn Pool) -> Result<usize, StorageError>;

    /// Return all applied records from the meta-table.
    ///
    /// # Errors
    /// Returns [`StorageError::Storage`] on query failure.
    pub fn applied_records(&self, pool: &dyn Pool) -> Result<Vec<AppliedRecord>, StorageError>;

    /// Check whether migration `id` has been applied.
    ///
    /// # Errors
    /// Returns [`StorageError::Storage`] on query failure.
    pub fn is_applied(&self, pool: &dyn Pool, id: u32) -> Result<bool, StorageError>;
}

impl Default for MigrationRunner {
    fn default() -> Self { Self::new() }
}
```

---

## Meta-Table DDL (embedded, not in `migrations/`)

The `_schema_migrations` table is created by the runner itself, not by a numbered
migration. This avoids bootstrapping circularity:

```sql
CREATE TABLE IF NOT EXISTS _schema_migrations (
    migration_id   INTEGER PRIMARY KEY,
    name           TEXT    NOT NULL,
    checksum_hex   TEXT    NOT NULL,
    applied_unix   INTEGER NOT NULL
);
```

This DDL is embedded as a constant in `migrations.rs` and is not subject to the
checksum-verification pass applied to numbered migrations.

---

## Method/Trait Table

| Item | Signature | Notes |
|------|-----------|-------|
| `Migration::new` | `const fn(u32, &'static str, &'static str) -> Self` | Compile-time construction |
| `Migration::checksum` | `fn(&self) -> [u8; 32]` | SHA-256 of `self.sql` |
| `migrations` | `fn() -> Vec<Migration>` | Canonical set, compile-embedded SQL |
| `MigrationRunner::new` | `fn() -> Self` | Uses `migrations()` |
| `MigrationRunner::with_migrations` | `fn(Vec<Migration>) -> Self` | Test injection |
| `MigrationRunner::apply_all` | `fn(&self, &dyn Pool) -> Result<usize, StorageError>` | Primary entry point |
| `MigrationRunner::applied_records` | `fn(&self, &dyn Pool) -> Result<Vec<AppliedRecord>, StorageError>` | Audit query |
| `MigrationRunner::is_applied` | `fn(&self, &dyn Pool, u32) -> Result<bool, StorageError>` | Point check |

---

## Reference: Migration 0001

The existing `migrations/0001_scaffold_schema.sql` defines:

```sql
-- workflow_runs: one row per `hle run` invocation
CREATE TABLE IF NOT EXISTS workflow_runs (
  id             INTEGER PRIMARY KEY AUTOINCREMENT,
  workflow_name  TEXT    NOT NULL,
  status         TEXT    NOT NULL CHECK (status IN
                   ('running','pass','fail','awaiting-human','rolled-back')),
  created_unix   INTEGER NOT NULL,
  completed_unix INTEGER
);

-- step_receipts: per-step state ledger
CREATE TABLE IF NOT EXISTS step_receipts (
  id               INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id           INTEGER NOT NULL REFERENCES workflow_runs(id),
  step_id          TEXT    NOT NULL,
  state            TEXT    NOT NULL CHECK (state IN
                     ('pending','running','awaiting-human','passed','failed','rolled-back')),
  verifier_verdict TEXT    NOT NULL,
  message          TEXT    NOT NULL,
  created_unix     INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_step_receipts_run_id ON step_receipts(run_id);
CREATE INDEX IF NOT EXISTS idx_step_receipts_step_id ON step_receipts(step_id);
```

Future migrations (0002, 0003, …) add the C05-specific tables for M028–M031 and
extend `workflow_runs` with the `authorization_profile` column.

---

## Design Notes

1. **Schema in `migrations/`, never in Rust source.** All `CREATE TABLE` DDL must live in
   numbered `.sql` files under `migrations/`. Rust source embeds the SQL via `include_str!`
   but does not construct SQL strings inline.

2. **Idempotent apply.** Running `apply_all` twice on the same database is safe. Applied
   migrations are skipped; their checksum is verified to detect accidental edits.

3. **Checksum protection.** If a migration's SQL changes after it has been applied, the
   runner returns `MigrationChecksum` (2411). This prevents silent schema drift in
   production databases.

4. **Single transaction per migration.** Each unapplied migration is wrapped in
   `BEGIN ... COMMIT`. If the SQL or the meta-table insert fails, the transaction rolls
   back and the error propagates. Partial migration application is not possible.

5. **No down-migrations.** The HLE append-only philosophy extends to schema: migrations
   never roll back. Corrections are new, additive migrations.

---

## Test Targets (minimum 50)

- `migration_apply_fresh_db`: apply_all on empty pool returns count > 0
- `migration_idempotent_second_apply`: apply_all twice returns Ok(0) on second call
- `migration_records_after_apply`: applied_records returns non-empty after apply_all
- `migration_is_applied_true`: is_applied returns true for migration 1 after apply_all
- `migration_is_applied_false`: is_applied returns false before apply_all
- `migration_checksum_mismatch_detected`: mutated SQL triggers MigrationChecksum (2411)
- `migration_order_non_monotonic`: out-of-order IDs trigger MigrationOrder (2412)
- `migration_empty_sql_rejected`: empty SQL string triggers MigrationNotFound (2410)
- `migration_transaction_rollback_on_bad_sql`: bad SQL leaves meta-table unchanged
- `migration_schema_tables_present`: after apply, workflow_runs and step_receipts exist

---

*M026 Migrations Spec v1.0 | C05_PERSISTENCE_LEDGER | habitat-loop-engine*
