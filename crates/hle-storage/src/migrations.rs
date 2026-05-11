#![forbid(unsafe_code)]

//! M026 — Schema-first migration runner.
//!
//! [`applied_migrations`] returns the ordered list of migrations known to this
//! build.  [`run_migrations`] applies each migration idempotently against a
//! [`Pool`]-backed connection in ID order.
//!
//! The sole migration currently tracked is `0001_scaffold_schema`, whose SQL
//! text matches `migrations/0001_scaffold_schema.sql` in the workspace root.
//! Additional migrations are appended to `MIGRATIONS` and are automatically
//! picked up by [`applied_migrations`].
//!
//! Error codes: 2410–2412 (`MigrationNotFound`, `MigrationChecksum`, `MigrationOrder`).

use substrate_types::HleError;

use crate::pool::{with_conn_val, Pool};

// ── error helpers ──────────────────────────────────────────────────────────────

/// Error code 2410: migration SQL is empty or cannot be located.
pub(crate) fn err_migration_not_found(id: u32) -> HleError {
    HleError::new(format!(
        "[2410 MigrationNotFound] migration {id} has empty or missing SQL"
    ))
}

/// Error code 2411: migration SQL changed after it was applied.
#[allow(dead_code)]
pub(crate) fn err_migration_checksum(id: u32) -> HleError {
    HleError::new(format!(
        "[2411 MigrationChecksum] migration {id} SQL changed after apply"
    ))
}

fn err_migration_order(id: u32) -> HleError {
    HleError::new(format!(
        "[2412 MigrationOrder] migration {id} is not monotonically ordered"
    ))
}

// ── Migration descriptor ───────────────────────────────────────────────────────

/// A single, immutable migration descriptor.
///
/// `sql` is the full DDL to execute.  Applying the same migration twice is
/// safe: every statement uses `CREATE TABLE IF NOT EXISTS` / `CREATE INDEX IF
/// NOT EXISTS` so re-execution is a no-op.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Migration {
    /// Monotonically increasing identifier (starting at 1).
    pub id: u32,
    /// Human-readable name matching the SQL file basename.
    pub name: &'static str,
    /// Full DDL; must use `IF NOT EXISTS` to remain idempotent.
    pub sql: &'static str,
}

impl Migration {
    /// Construct a migration descriptor (const-friendly).
    #[must_use]
    pub const fn new(id: u32, name: &'static str, sql: &'static str) -> Self {
        Self { id, name, sql }
    }

    /// Returns `true` when the SQL string is non-empty.
    #[must_use]
    pub fn has_sql(&self) -> bool {
        !self.sql.trim().is_empty()
    }

    /// Simple byte-length of the embedded SQL.
    #[must_use]
    pub fn sql_len(&self) -> usize {
        self.sql.len()
    }
}

// ── Migration catalogue ────────────────────────────────────────────────────────

/// SQL content mirroring `migrations/0001_scaffold_schema.sql`.
///
/// This is embedded at compile-time so that tests and the CLI can apply the
/// schema without needing a filesystem path.  The canonical source of truth
/// for the DDL remains the file on disk; keep both in sync.
const SQL_0001: &str = "\
CREATE TABLE IF NOT EXISTS workflow_runs (\n\
  id INTEGER PRIMARY KEY AUTOINCREMENT,\n\
  workflow_name TEXT NOT NULL,\n\
  status TEXT NOT NULL CHECK (status IN ('running','pass','fail','awaiting-human','rolled-back')),\n\
  created_unix INTEGER NOT NULL,\n\
  completed_unix INTEGER\n\
);\n\
\n\
CREATE TABLE IF NOT EXISTS step_receipts (\n\
  id INTEGER PRIMARY KEY AUTOINCREMENT,\n\
  run_id INTEGER NOT NULL REFERENCES workflow_runs(id),\n\
  step_id TEXT NOT NULL,\n\
  state TEXT NOT NULL CHECK (state IN ('pending','running','awaiting-human','passed','failed','rolled-back')),\n\
  verifier_verdict TEXT NOT NULL,\n\
  message TEXT NOT NULL,\n\
  created_unix INTEGER NOT NULL\n\
);\n\
\n\
CREATE INDEX IF NOT EXISTS idx_step_receipts_run_id ON step_receipts(run_id);\n\
CREATE INDEX IF NOT EXISTS idx_step_receipts_step_id ON step_receipts(step_id);\n\
";

/// SQL content mirroring `migrations/0002_m028_m029_m030_m031.sql`.
///
/// Creates the supplementary tables required by M028 (`workflow_ticks`),
/// M029 (`evidence_store`), M030 (`verifier_results_store`), and M031
/// (`blockers_store`).  All statements use `IF NOT EXISTS` for idempotency.
const SQL_0002: &str = "\
CREATE TABLE IF NOT EXISTS workflow_ticks (\n\
  id INTEGER PRIMARY KEY AUTOINCREMENT,\n\
  run_id INTEGER NOT NULL REFERENCES workflow_runs(id),\n\
  tick_id INTEGER NOT NULL,\n\
  created_unix INTEGER NOT NULL,\n\
  parent_tick_id INTEGER\n\
);\n\
\n\
CREATE INDEX IF NOT EXISTS idx_workflow_ticks_run_id ON workflow_ticks(run_id);\n\
CREATE INDEX IF NOT EXISTS idx_workflow_ticks_tick_id ON workflow_ticks(tick_id);\n\
\n\
CREATE TABLE IF NOT EXISTS evidence_store (\n\
  id INTEGER PRIMARY KEY AUTOINCREMENT,\n\
  run_id INTEGER NOT NULL REFERENCES workflow_runs(id),\n\
  evidence_kind TEXT NOT NULL CHECK (evidence_kind IN ('stdout','stderr','artifact')),\n\
  path_or_inline TEXT NOT NULL,\n\
  sha256 TEXT NOT NULL,\n\
  size_bytes INTEGER NOT NULL,\n\
  created_unix INTEGER NOT NULL DEFAULT 0\n\
);\n\
\n\
CREATE INDEX IF NOT EXISTS idx_evidence_store_run_id ON evidence_store(run_id);\n\
CREATE INDEX IF NOT EXISTS idx_evidence_store_sha256 ON evidence_store(sha256);\n\
\n\
CREATE TABLE IF NOT EXISTS verifier_results_store (\n\
  id INTEGER PRIMARY KEY AUTOINCREMENT,\n\
  run_id INTEGER NOT NULL REFERENCES workflow_runs(id),\n\
  step_id TEXT NOT NULL,\n\
  verdict TEXT NOT NULL CHECK (verdict IN ('PASS','FAIL','AWAITING_HUMAN')),\n\
  receipt_sha TEXT NOT NULL,\n\
  verifier_version TEXT NOT NULL,\n\
  created_unix INTEGER NOT NULL DEFAULT 0\n\
);\n\
\n\
CREATE INDEX IF NOT EXISTS idx_verifier_results_run_id ON verifier_results_store(run_id);\n\
CREATE INDEX IF NOT EXISTS idx_verifier_results_step_id ON verifier_results_store(step_id);\n\
\n\
CREATE TABLE IF NOT EXISTS blockers_store (\n\
  id INTEGER PRIMARY KEY AUTOINCREMENT,\n\
  run_id INTEGER NOT NULL REFERENCES workflow_runs(id),\n\
  step_id TEXT NOT NULL,\n\
  blocker_kind TEXT NOT NULL,\n\
  since_unix INTEGER NOT NULL,\n\
  expected_resolver_role TEXT NOT NULL,\n\
  resolved_unix INTEGER\n\
);\n\
\n\
CREATE INDEX IF NOT EXISTS idx_blockers_store_run_id ON blockers_store(run_id);\n\
CREATE INDEX IF NOT EXISTS idx_blockers_store_resolved_unix ON blockers_store(resolved_unix);\n\
";

/// Complete, ordered list of all migrations known to this build.
static MIGRATIONS: &[Migration] = &[
    Migration {
        id: 1,
        name: "0001_scaffold_schema",
        sql: SQL_0001,
    },
    Migration {
        id: 2,
        name: "0002_m028_m029_m030_m031",
        sql: SQL_0002,
    },
];

// ── Public API ─────────────────────────────────────────────────────────────────

/// Return the ordered list of all migrations embedded in this build.
///
/// Callers that need to inspect which migrations are pending can compare this
/// list against a `schema_migrations` tracking table (not yet created in
/// scaffold; wired at M0 runtime).
#[must_use]
pub fn applied_migrations() -> Vec<Migration> {
    MIGRATIONS.to_vec()
}

/// Apply all migrations in ID order against the given pool, idempotently.
///
/// Each migration's SQL is executed as-is; idempotency is guaranteed by the
/// `IF NOT EXISTS` qualifiers in every DDL statement.
///
/// The function validates that the catalogue is monotonically ordered before
/// executing any SQL, so a mis-ordered catalogue is caught at startup rather
/// than at first insert.
///
/// # Errors
///
/// Returns `HleError` when:
/// - The migration catalogue is not monotonically ordered (2412).
/// - Any migration SQL fails to execute against the pool connection (2499).
pub fn run_migrations(pool: &dyn Pool) -> Result<(), HleError> {
    validate_order()?;
    for migration in MIGRATIONS {
        if !migration.has_sql() {
            return Err(err_migration_not_found(migration.id));
        }
        with_conn_val(pool, |conn| {
            conn.execute_sql(migration.sql)
                .map(|_rows| ())
                .map_err(|err| {
                    HleError::new(format!(
                        "[2499 Storage] migration {} '{}' failed: {err}",
                        migration.id, migration.name
                    ))
                })
        })?;
    }
    Ok(())
}

/// Validate that `MIGRATIONS` is strictly monotonically increasing by `id`.
pub(crate) fn validate_order() -> Result<(), HleError> {
    let mut prev: Option<u32> = None;
    for m in MIGRATIONS {
        if let Some(p) = prev {
            if m.id <= p {
                return Err(err_migration_order(m.id));
            }
        }
        prev = Some(m.id);
    }
    Ok(())
}

/// Validate monotonicity on an arbitrary slice (used by tests for out-of-order probes).
#[allow(dead_code)]
pub(crate) fn validate_order_slice(migrations: &[Migration]) -> Result<(), HleError> {
    let mut prev: Option<u32> = None;
    for m in migrations {
        if let Some(p) = prev {
            if m.id <= p {
                return Err(err_migration_order(m.id));
            }
        }
        prev = Some(m.id);
    }
    Ok(())
}

// ── tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{
        applied_migrations, err_migration_checksum, err_migration_not_found, err_migration_order,
        run_migrations, validate_order, validate_order_slice, Migration, MIGRATIONS,
    };
    use crate::pool::MemPool;

    // ── static catalogue ────────────────────────────────────────────────────────

    #[test]
    fn applied_migrations_returns_non_empty_list() {
        assert!(!applied_migrations().is_empty());
    }

    #[test]
    fn first_migration_id_is_one() {
        assert_eq!(applied_migrations()[0].id, 1);
    }

    #[test]
    fn first_migration_name_matches_sql_file() {
        assert_eq!(applied_migrations()[0].name, "0001_scaffold_schema");
    }

    #[test]
    fn catalogue_is_monotonically_ordered() {
        let migrations = applied_migrations();
        for window in migrations.windows(2) {
            assert!(window[1].id > window[0].id);
        }
    }

    #[test]
    fn migrations_contain_workflow_runs_ddl() {
        let sql = applied_migrations()[0].sql;
        assert!(sql.contains("workflow_runs"));
    }

    #[test]
    fn migrations_contain_step_receipts_ddl() {
        let sql = applied_migrations()[0].sql;
        assert!(sql.contains("step_receipts"));
    }

    #[test]
    fn static_migrations_list_length_matches_applied() {
        assert_eq!(MIGRATIONS.len(), applied_migrations().len());
    }

    #[test]
    fn applied_migrations_has_two_entries() {
        assert_eq!(applied_migrations().len(), 2);
    }

    #[test]
    fn second_migration_id_is_two() {
        assert_eq!(applied_migrations()[1].id, 2);
    }

    #[test]
    fn second_migration_name_matches_sql_file() {
        assert_eq!(applied_migrations()[1].name, "0002_m028_m029_m030_m031");
    }

    #[test]
    fn second_migration_contains_workflow_ticks_ddl() {
        let sql = applied_migrations()[1].sql;
        assert!(sql.contains("workflow_ticks"));
    }

    #[test]
    fn second_migration_contains_evidence_store_ddl() {
        let sql = applied_migrations()[1].sql;
        assert!(sql.contains("evidence_store"));
    }

    #[test]
    fn second_migration_contains_verifier_results_store_ddl() {
        let sql = applied_migrations()[1].sql;
        assert!(sql.contains("verifier_results_store"));
    }

    #[test]
    fn second_migration_contains_blockers_store_ddl() {
        let sql = applied_migrations()[1].sql;
        assert!(sql.contains("blockers_store"));
    }

    #[test]
    fn migration_copy_trait_works() {
        let m = applied_migrations()[0];
        let _copy = m;
        assert_eq!(m.id, 1);
    }

    #[test]
    fn migration_has_sql_returns_true_for_catalogue() {
        for m in applied_migrations() {
            assert!(m.has_sql(), "migration {} has empty SQL", m.id);
        }
    }

    #[test]
    fn migration_sql_len_nonzero() {
        for m in applied_migrations() {
            assert!(m.sql_len() > 0, "migration {} has zero-len SQL", m.id);
        }
    }

    #[test]
    fn migration_new_const_constructor() {
        const M: Migration = Migration::new(99, "test", "CREATE TABLE IF NOT EXISTS t(id INT);");
        assert_eq!(M.id, 99);
        assert_eq!(M.name, "test");
    }

    #[test]
    fn migration_eq_same_id_name_sql() {
        let a = Migration::new(1, "n", "sql");
        let b = Migration::new(1, "n", "sql");
        assert_eq!(a, b);
    }

    #[test]
    fn migration_ne_different_id() {
        let a = Migration::new(1, "n", "sql");
        let b = Migration::new(2, "n", "sql");
        assert_ne!(a, b);
    }

    #[test]
    fn migration_ne_different_sql() {
        let a = Migration::new(1, "n", "sql A");
        let b = Migration::new(1, "n", "sql B");
        assert_ne!(a, b);
    }

    #[test]
    fn migration_debug_impl_non_empty() {
        let m = applied_migrations()[0];
        assert!(!format!("{m:?}").is_empty());
    }

    #[test]
    fn migration_has_sql_false_for_empty() {
        let m = Migration::new(2, "empty", "");
        assert!(!m.has_sql());
    }

    #[test]
    fn migration_has_sql_false_for_whitespace_only() {
        let m = Migration::new(3, "ws", "   \n\t  ");
        assert!(!m.has_sql());
    }

    #[test]
    fn migration_first_sql_contains_autoincrement() {
        assert!(applied_migrations()[0].sql.contains("AUTOINCREMENT"));
    }

    #[test]
    fn migration_first_sql_contains_if_not_exists() {
        // Ensures idempotency qualifier is present.
        assert!(applied_migrations()[0].sql.contains("IF NOT EXISTS"));
    }

    #[test]
    fn migration_first_sql_contains_created_unix_column() {
        assert!(applied_migrations()[0].sql.contains("created_unix"));
    }

    // ── run_migrations ──────────────────────────────────────────────────────────

    #[test]
    fn run_migrations_succeeds_against_mem_pool() {
        let pool = MemPool::new();
        assert!(run_migrations(&pool).is_ok());
    }

    #[test]
    fn run_migrations_is_idempotent_on_mem_pool() {
        let pool = MemPool::new();
        assert!(run_migrations(&pool).is_ok());
        assert!(run_migrations(&pool).is_ok());
    }

    #[test]
    fn run_migrations_three_times_succeeds() {
        let pool = MemPool::new();
        for _ in 0..3 {
            assert!(run_migrations(&pool).is_ok());
        }
    }

    // ── validate_order ──────────────────────────────────────────────────────────

    #[test]
    fn validate_order_passes_for_catalogue() {
        assert!(validate_order().is_ok());
    }

    #[test]
    fn validate_order_slice_passes_empty() {
        assert!(validate_order_slice(&[]).is_ok());
    }

    #[test]
    fn validate_order_slice_passes_single() {
        let ms = [Migration::new(1, "a", "s")];
        assert!(validate_order_slice(&ms).is_ok());
    }

    #[test]
    fn validate_order_slice_passes_ascending() {
        let ms = [
            Migration::new(1, "a", "s"),
            Migration::new(2, "b", "s"),
            Migration::new(5, "c", "s"),
        ];
        assert!(validate_order_slice(&ms).is_ok());
    }

    #[test]
    fn validate_order_slice_rejects_duplicate_id() {
        let ms = [Migration::new(1, "a", "s"), Migration::new(1, "b", "s")];
        let err = validate_order_slice(&ms).unwrap_err();
        assert!(err.to_string().contains("2412"));
    }

    #[test]
    fn validate_order_slice_rejects_descending() {
        let ms = [Migration::new(2, "a", "s"), Migration::new(1, "b", "s")];
        let err = validate_order_slice(&ms).unwrap_err();
        assert!(err.to_string().contains("2412"));
    }

    #[test]
    fn validate_order_slice_rejects_gap_then_descend() {
        let ms = [
            Migration::new(1, "a", "s"),
            Migration::new(3, "b", "s"),
            Migration::new(2, "c", "s"),
        ];
        let err = validate_order_slice(&ms).unwrap_err();
        assert!(err.to_string().contains("2412"));
    }

    // ── error codes ─────────────────────────────────────────────────────────────

    #[test]
    fn err_migration_not_found_contains_2410() {
        assert!(err_migration_not_found(5).to_string().contains("2410"));
    }

    #[test]
    fn err_migration_not_found_contains_id() {
        assert!(err_migration_not_found(42).to_string().contains("42"));
    }

    #[test]
    fn err_migration_checksum_contains_2411() {
        assert!(err_migration_checksum(7).to_string().contains("2411"));
    }

    #[test]
    fn err_migration_checksum_contains_id() {
        assert!(err_migration_checksum(99).to_string().contains("99"));
    }

    #[test]
    fn err_migration_order_contains_2412() {
        assert!(err_migration_order(3).to_string().contains("2412"));
    }

    #[test]
    fn err_migration_order_contains_id() {
        assert!(err_migration_order(17).to_string().contains("17"));
    }

    #[test]
    fn all_three_error_codes_are_distinct() {
        let e1 = err_migration_not_found(1).to_string();
        let e2 = err_migration_checksum(1).to_string();
        let e3 = err_migration_order(1).to_string();
        assert_ne!(e1, e2);
        assert_ne!(e2, e3);
        assert_ne!(e1, e3);
    }

    // ── migration SQL content ────────────────────────────────────────────────────

    #[test]
    fn migration_0001_contains_status_check_constraint() {
        let sql = applied_migrations()[0].sql;
        assert!(sql.contains("CHECK"));
        assert!(sql.contains("running"));
        assert!(sql.contains("pass"));
        assert!(sql.contains("fail"));
    }

    #[test]
    fn migration_0001_contains_index_on_run_id() {
        assert!(applied_migrations()[0]
            .sql
            .contains("idx_step_receipts_run_id"));
    }

    #[test]
    fn migration_0001_contains_index_on_step_id() {
        assert!(applied_migrations()[0]
            .sql
            .contains("idx_step_receipts_step_id"));
    }

    // ── additional coverage ─────────────────────────────────────────────────────

    #[test]
    fn validate_order_passes_after_multiple_calls() {
        for _ in 0..3 {
            assert!(validate_order().is_ok());
        }
    }

    #[test]
    fn migration_0001_name_is_stable() {
        assert_eq!(MIGRATIONS[0].name, "0001_scaffold_schema");
    }

    #[test]
    fn migration_const_constructor_accepts_empty_sql() {
        let m = Migration::new(5, "empty", "");
        assert!(!m.has_sql());
    }

    #[test]
    fn migration_sql_len_gt_100_for_0001() {
        assert!(applied_migrations()[0].sql_len() > 100);
    }

    #[test]
    fn run_migrations_mem_pool_returns_ok_five_times() {
        let pool = MemPool::new();
        for _ in 0..5 {
            assert!(run_migrations(&pool).is_ok());
        }
    }

    #[test]
    fn validate_order_slice_two_ascending_ids() {
        let ms = [Migration::new(10, "a", "s"), Migration::new(20, "b", "s")];
        assert!(validate_order_slice(&ms).is_ok());
    }

    #[test]
    fn migration_debug_contains_id_number() {
        let m = Migration::new(7, "test", "sql");
        assert!(format!("{m:?}").contains("7"));
    }

    #[test]
    fn err_migration_not_found_message_contains_empty_keyword() {
        let e = err_migration_not_found(1);
        assert!(
            e.to_string().to_lowercase().contains("missing")
                || e.to_string().to_lowercase().contains("empty")
        );
    }

    #[test]
    fn validate_order_slice_single_element_id_100() {
        let ms = [Migration::new(100, "late", "sql")];
        assert!(validate_order_slice(&ms).is_ok());
    }

    #[test]
    fn err_migration_order_id_in_message() {
        let e = err_migration_order(55);
        assert!(e.to_string().contains("55"));
    }

    // ── SqlitePool-backed migration integration tests ────────────────────────────

    #[test]
    fn run_migrations_succeeds_against_sqlite_memory_pool() {
        use crate::pool::SqlitePool;
        let pool = SqlitePool::open_memory().unwrap();
        assert!(run_migrations(&pool).is_ok());
    }

    #[test]
    fn run_migrations_is_idempotent_on_sqlite_pool() {
        use crate::pool::SqlitePool;
        let pool = SqlitePool::open_memory().unwrap();
        assert!(run_migrations(&pool).is_ok());
        assert!(run_migrations(&pool).is_ok());
    }

    #[test]
    fn run_migrations_creates_workflow_runs_table() {
        use crate::pool::{with_conn_val, SqlitePool};
        let pool = SqlitePool::open_memory().unwrap();
        run_migrations(&pool).unwrap();
        let count: Option<i64> = with_conn_val(&pool, |conn| {
            conn.query_one_i64("SELECT COUNT(*) FROM workflow_runs")
        })
        .unwrap();
        assert_eq!(count, Some(0));
    }

    #[test]
    fn run_migrations_creates_step_receipts_table() {
        use crate::pool::{with_conn_val, SqlitePool};
        let pool = SqlitePool::open_memory().unwrap();
        run_migrations(&pool).unwrap();
        let count: Option<i64> = with_conn_val(&pool, |conn| {
            conn.query_one_i64("SELECT COUNT(*) FROM step_receipts")
        })
        .unwrap();
        assert_eq!(count, Some(0));
    }

    #[test]
    fn run_migrations_creates_workflow_ticks_table() {
        use crate::pool::{with_conn_val, SqlitePool};
        let pool = SqlitePool::open_memory().unwrap();
        run_migrations(&pool).unwrap();
        let count: Option<i64> = with_conn_val(&pool, |conn| {
            conn.query_one_i64("SELECT COUNT(*) FROM workflow_ticks")
        })
        .unwrap();
        assert_eq!(count, Some(0));
    }

    #[test]
    fn run_migrations_creates_evidence_store_table() {
        use crate::pool::{with_conn_val, SqlitePool};
        let pool = SqlitePool::open_memory().unwrap();
        run_migrations(&pool).unwrap();
        let count: Option<i64> = with_conn_val(&pool, |conn| {
            conn.query_one_i64("SELECT COUNT(*) FROM evidence_store")
        })
        .unwrap();
        assert_eq!(count, Some(0));
    }

    #[test]
    fn run_migrations_creates_verifier_results_store_table() {
        use crate::pool::{with_conn_val, SqlitePool};
        let pool = SqlitePool::open_memory().unwrap();
        run_migrations(&pool).unwrap();
        let count: Option<i64> = with_conn_val(&pool, |conn| {
            conn.query_one_i64("SELECT COUNT(*) FROM verifier_results_store")
        })
        .unwrap();
        assert_eq!(count, Some(0));
    }

    #[test]
    fn run_migrations_creates_blockers_store_table() {
        use crate::pool::{with_conn_val, SqlitePool};
        let pool = SqlitePool::open_memory().unwrap();
        run_migrations(&pool).unwrap();
        let count: Option<i64> = with_conn_val(&pool, |conn| {
            conn.query_one_i64("SELECT COUNT(*) FROM blockers_store")
        })
        .unwrap();
        assert_eq!(count, Some(0));
    }

    #[test]
    fn run_migrations_file_backed_succeeds() {
        use crate::pool::SqlitePool;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let pool = SqlitePool::open(&path).unwrap();
        assert!(run_migrations(&pool).is_ok());
    }

    #[test]
    fn migration_0002_sql_len_gt_100() {
        assert!(applied_migrations()[1].sql_len() > 100);
    }

    #[test]
    fn migration_0002_contains_if_not_exists() {
        assert!(applied_migrations()[1].sql.contains("IF NOT EXISTS"));
    }

    #[test]
    fn migration_0002_has_sql_returns_true() {
        assert!(applied_migrations()[1].has_sql());
    }
}
