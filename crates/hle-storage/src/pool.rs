#![forbid(unsafe_code)]

//! M025 — Connection pool trait and implementations.
//!
//! The `Pool` trait is the only sanctioned way for C05 modules to obtain a
//! database connection.
//!
//! Two implementations are provided:
//!
//! * [`SqlitePool`] — the real WAL-backed pool backed by `SQLite`.  Open with
//!   [`SqlitePool::open_memory`] (`:memory:`) or [`SqlitePool::open`] (file).
//!   This is the production backend wired at M0 initialisation.
//!
//! * [`MemPool`] — in-memory stub that accepts all SQL as no-ops.  Used for
//!   unit tests that exercise *logic only* (not persistence).
//!
//! Error codes: 2400–2402 (`PoolInit`, `ConnectionAcquire`, `ConnectionLeak`).
//!
//! ## Row tuple type aliases
//!
//! Complex return tuples are defined as type aliases (`Row5Col`, `RowTick`,
//! `RowEvidence`, `RowVerifier`, `RowBlocker`) to satisfy the
//! `clippy::type_complexity` lint and improve readability.

use std::sync::Mutex;

use rusqlite::Connection as RusqliteConnection;
use substrate_types::HleError;

// ── row tuple type aliases ─────────────────────────────────────────────────────

/// Row type for `query_rows_5col`: `(id, workflow_name, status, created_unix, completed_unix)`.
pub type Row5Col = (i64, String, String, i64, Option<i64>);

/// Row type for `query_rows_tick`: `(id, run_id, tick_id, created_unix, parent_tick_id)`.
pub type RowTick = (i64, i64, i64, i64, Option<i64>);

/// Row type for `query_rows_evidence`: `(id, run_id, evidence_kind, path_or_inline, sha256, size_bytes)`.
pub type RowEvidence = (i64, i64, String, String, String, i64);

/// Row type for `query_rows_verifier`: `(id, run_id, step_id, verdict, receipt_sha, verifier_version)`.
pub type RowVerifier = (i64, i64, String, String, String, String);

/// Row type for `query_rows_blocker`: `(id, run_id, step_id, blocker_kind, since_unix, expected_resolver_role, resolved_unix)`.
pub type RowBlocker = (i64, i64, String, String, i64, String, Option<i64>);

// ── error helpers ──────────────────────────────────────────────────────────────

/// Error code 2400: database file cannot be opened or WAL pragma fails.
pub fn err_pool_init(detail: impl core::fmt::Display) -> HleError {
    HleError::new(format!("[2400 PoolInit] {detail}"))
}

/// Error code 2401: pool exhausted; timeout exceeded.
pub fn err_connection_acquire(detail: impl core::fmt::Display) -> HleError {
    HleError::new(format!("[2401 ConnectionAcquire] {detail}"))
}

/// Error code 2402: guard dropped without explicit release.
pub fn err_connection_leak(detail: impl core::fmt::Display) -> HleError {
    HleError::new(format!("[2402 ConnectionLeak] {detail}"))
}

// ── Connection trait ───────────────────────────────────────────────────────────

/// Minimal surface a concrete backend connection must expose to C05 modules.
///
/// A real backend wraps `rusqlite::Connection`; `MemConnection` is the stub.
pub trait Connection {
    /// Execute a SQL statement that produces no result rows.
    ///
    /// # Errors
    ///
    /// Returns `HleError` when the statement cannot be prepared or executed.
    fn execute_sql(&self, sql: &str) -> Result<usize, HleError>;

    /// Query a single optional `i64` from the first column of the first row.
    ///
    /// Used by higher modules to read auto-increment IDs.
    ///
    /// # Errors
    ///
    /// Returns `HleError` on prepare or step failure.
    fn query_one_i64(&self, sql: &str) -> Result<Option<i64>, HleError>;

    /// Query all rows from a SELECT returning [`Row5Col`].
    ///
    /// Used by `WorkflowRunsStore::list_all` and friends to read rows back.
    ///
    /// # Errors
    ///
    /// Returns `HleError` on prepare or step failure.
    fn query_rows_5col(&self, sql: &str) -> Result<Vec<Row5Col>, HleError>;

    /// Query all rows returning [`RowTick`].
    ///
    /// Used by `WorkflowTicksStore::list_for_run` etc.
    ///
    /// # Errors
    ///
    /// Returns `HleError` on prepare or step failure.
    fn query_rows_tick(&self, sql: &str) -> Result<Vec<RowTick>, HleError>;

    /// Query all rows returning [`RowEvidence`].
    ///
    /// Used by `EvidenceStore`.
    ///
    /// # Errors
    ///
    /// Returns `HleError` on prepare or step failure.
    fn query_rows_evidence(&self, sql: &str) -> Result<Vec<RowEvidence>, HleError>;

    /// Query all rows returning [`RowVerifier`].
    ///
    /// Used by `VerifierResultsStore`.
    ///
    /// # Errors
    ///
    /// Returns `HleError` on prepare or step failure.
    fn query_rows_verifier(&self, sql: &str) -> Result<Vec<RowVerifier>, HleError>;

    /// Query all rows returning [`RowBlocker`].
    ///
    /// Used by `BlockersStore`.
    ///
    /// # Errors
    ///
    /// Returns `HleError` on prepare or step failure.
    fn query_rows_blocker(&self, sql: &str) -> Result<Vec<RowBlocker>, HleError>;

    /// Execute an `UPDATE` statement and return the number of affected rows.
    ///
    /// # Errors
    ///
    /// Returns `HleError` on prepare or execute failure.
    fn execute_update(&self, sql: &str) -> Result<usize, HleError>;
}

// ── Pool trait ─────────────────────────────────────────────────────────────────

/// Boxed unit-returning closure passed to [`Pool::with_conn`].
///
/// This type alias keeps `Pool` dyn-compatible and suppresses the
/// `clippy::type_complexity` lint on the trait method signature.
pub type ConnFn<'a> = Box<dyn FnOnce(&dyn Connection) -> Result<(), HleError> + 'a>;

/// Connection pool — the sole entry-point for obtaining database connections.
///
/// All C05 modules receive a `&dyn Pool` and call [`Pool::with_conn`].  They
/// must not open raw connections or bypass the pool.
///
/// `with_conn` is dyn-compatible: it takes a boxed closure returning `HleError`.
/// Callers that need to return a typed `T` use the free [`with_conn_val`] wrapper.
pub trait Pool {
    /// Acquire a connection, run `f` within its scope, release on return.
    ///
    /// The closure is boxed to keep `Pool` dyn-compatible.
    ///
    /// # Errors
    ///
    /// Returns `HleError` when the pool fails to acquire a connection or when
    /// `f` returns an error.
    fn with_conn(&self, f: ConnFn<'_>) -> Result<(), HleError>;
}

/// Convenience wrapper — runs `f` against a pool connection and returns `T`.
///
/// # Errors
///
/// Returns `HleError` on pool acquisition failure or when `f` returns `Err`.
pub fn with_conn_val<T, F>(pool: &dyn Pool, f: F) -> Result<T, HleError>
where
    F: FnOnce(&dyn Connection) -> Result<T, HleError>,
{
    let mut result: Option<T> = None;
    pool.with_conn(Box::new(|conn| {
        result = Some(f(conn)?);
        Ok(())
    }))?;
    result.ok_or_else(|| HleError::new("[2401 ConnectionAcquire] closure did not run"))
}

// ── SqliteConnection ───────────────────────────────────────────────────────────

/// Real rusqlite-backed connection, borrowed from `SqlitePool`.
///
/// All SQL is forwarded to the underlying `rusqlite::Connection`.
pub(crate) struct SqliteConnection<'conn> {
    conn: &'conn RusqliteConnection,
}

impl<'conn> SqliteConnection<'conn> {
    fn new(conn: &'conn RusqliteConnection) -> Self {
        Self { conn }
    }
}

/// Map a `rusqlite::Error` to an `HleError` with code 2499.
fn map_rusqlite_err(err: &rusqlite::Error) -> HleError {
    HleError::new(format!("[2499 Storage] rusqlite: {err}"))
}

impl Connection for SqliteConnection<'_> {
    fn execute_sql(&self, sql: &str) -> Result<usize, HleError> {
        // rusqlite's `execute` only handles a single statement.  For DDL batches
        // (migrations) we use `execute_batch` which returns no row-count.
        // Detect multi-statement SQL by checking for a semicolon that is not
        // at the very end of the trimmed string.
        let trimmed = sql.trim().trim_end_matches(';');
        if trimmed.contains(';') {
            // Multi-statement: use execute_batch (no row count available).
            self.conn
                .execute_batch(sql)
                .map(|()| 0_usize)
                .map_err(|e| map_rusqlite_err(&e))
        } else {
            self.conn.execute(sql, []).map_err(|e| map_rusqlite_err(&e))
        }
    }

    fn query_one_i64(&self, sql: &str) -> Result<Option<i64>, HleError> {
        let mut stmt = self.conn.prepare(sql).map_err(|e| map_rusqlite_err(&e))?;
        let mut rows = stmt.query([]).map_err(|e| map_rusqlite_err(&e))?;
        match rows.next().map_err(|e| map_rusqlite_err(&e))? {
            None => Ok(None),
            Some(row) => {
                let val: Option<i64> = row.get(0).map_err(|e| map_rusqlite_err(&e))?;
                Ok(val)
            }
        }
    }

    fn query_rows_5col(&self, sql: &str) -> Result<Vec<Row5Col>, HleError> {
        let mut stmt = self.conn.prepare(sql).map_err(|e| map_rusqlite_err(&e))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                ))
            })
            .map_err(|e| map_rusqlite_err(&e))?;
        rows.map(|r| r.map_err(|e| map_rusqlite_err(&e)))
            .collect::<Result<Vec<_>, _>>()
    }

    fn query_rows_tick(&self, sql: &str) -> Result<Vec<RowTick>, HleError> {
        let mut stmt = self.conn.prepare(sql).map_err(|e| map_rusqlite_err(&e))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                ))
            })
            .map_err(|e| map_rusqlite_err(&e))?;
        rows.map(|r| r.map_err(|e| map_rusqlite_err(&e)))
            .collect::<Result<Vec<_>, _>>()
    }

    fn query_rows_evidence(&self, sql: &str) -> Result<Vec<RowEvidence>, HleError> {
        let mut stmt = self.conn.prepare(sql).map_err(|e| map_rusqlite_err(&e))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            })
            .map_err(|e| map_rusqlite_err(&e))?;
        rows.map(|r| r.map_err(|e| map_rusqlite_err(&e)))
            .collect::<Result<Vec<_>, _>>()
    }

    fn query_rows_verifier(&self, sql: &str) -> Result<Vec<RowVerifier>, HleError> {
        let mut stmt = self.conn.prepare(sql).map_err(|e| map_rusqlite_err(&e))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(|e| map_rusqlite_err(&e))?;
        rows.map(|r| r.map_err(|e| map_rusqlite_err(&e)))
            .collect::<Result<Vec<_>, _>>()
    }

    fn query_rows_blocker(&self, sql: &str) -> Result<Vec<RowBlocker>, HleError> {
        let mut stmt = self.conn.prepare(sql).map_err(|e| map_rusqlite_err(&e))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<i64>>(6)?,
                ))
            })
            .map_err(|e| map_rusqlite_err(&e))?;
        rows.map(|r| r.map_err(|e| map_rusqlite_err(&e)))
            .collect::<Result<Vec<_>, _>>()
    }

    fn execute_update(&self, sql: &str) -> Result<usize, HleError> {
        self.conn.execute(sql, []).map_err(|e| map_rusqlite_err(&e))
    }
}

// ── SqlitePool ─────────────────────────────────────────────────────────────────

/// Real WAL-backed connection pool backed by `SQLite`.
///
/// Wraps a single `rusqlite::Connection` behind a `Mutex` for single-writer
/// access.  For M0 scope (one-shot, foreground) this is sufficient.
///
/// Open with [`SqlitePool::open_memory`] for an in-process `:memory:` database
/// (ideal for tests) or [`SqlitePool::open`] for a file-backed WAL database.
pub struct SqlitePool {
    conn: Mutex<RusqliteConnection>,
}

impl SqlitePool {
    /// Open a new in-memory `SQLite` database.
    ///
    /// Each call creates an isolated database; there is no sharing between
    /// instances.  Suitable for integration tests that must run in isolation.
    ///
    /// # Errors
    ///
    /// Returns `HleError` (2400) when the connection cannot be opened.
    pub fn open_memory() -> Result<Self, HleError> {
        let conn = RusqliteConnection::open_in_memory()
            .map_err(|e| err_pool_init(format!("in-memory open failed: {e}")))?;
        Self::configure(conn)
    }

    /// Open a file-backed `SQLite` database at `path` with WAL journal mode.
    ///
    /// # Errors
    ///
    /// Returns `HleError` (2400) when the file cannot be opened or the WAL
    /// pragma fails.
    pub fn open(path: &std::path::Path) -> Result<Self, HleError> {
        let conn = RusqliteConnection::open(path)
            .map_err(|e| err_pool_init(format!("file open failed at {}: {e}", path.display())))?;
        Self::configure(conn)
    }

    /// Apply common connection pragmas (WAL, foreign keys).
    fn configure(conn: RusqliteConnection) -> Result<Self, HleError> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;\
             PRAGMA foreign_keys = ON;\
             PRAGMA synchronous = NORMAL;",
        )
        .map_err(|e| err_pool_init(format!("pragma setup failed: {e}")))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

impl Pool for SqlitePool {
    fn with_conn(&self, f: ConnFn<'_>) -> Result<(), HleError> {
        let guard = self
            .conn
            .lock()
            .map_err(|_| err_connection_acquire("mutex poisoned"))?;
        let sqlite_conn = SqliteConnection::new(&guard);
        f(&sqlite_conn)
    }
}

// ── MemConnection ──────────────────────────────────────────────────────────────

/// In-memory stub connection used by `MemPool`.
///
/// All SQL is silently accepted and returns harmless defaults.  This is
/// intentional: the stub exists only so that the trait surface compiles and
/// unit tests that exercise *logic* can run without a real database.
#[derive(Default)]
pub struct MemConnection;

impl Connection for MemConnection {
    fn execute_sql(&self, _sql: &str) -> Result<usize, HleError> {
        Ok(0)
    }

    fn query_one_i64(&self, _sql: &str) -> Result<Option<i64>, HleError> {
        Ok(None)
    }

    fn query_rows_5col(&self, _sql: &str) -> Result<Vec<Row5Col>, HleError> {
        Ok(Vec::new())
    }

    fn query_rows_tick(&self, _sql: &str) -> Result<Vec<RowTick>, HleError> {
        Ok(Vec::new())
    }

    fn query_rows_evidence(&self, _sql: &str) -> Result<Vec<RowEvidence>, HleError> {
        Ok(Vec::new())
    }

    fn query_rows_verifier(&self, _sql: &str) -> Result<Vec<RowVerifier>, HleError> {
        Ok(Vec::new())
    }

    fn query_rows_blocker(&self, _sql: &str) -> Result<Vec<RowBlocker>, HleError> {
        Ok(Vec::new())
    }

    fn execute_update(&self, _sql: &str) -> Result<usize, HleError> {
        Ok(0)
    }
}

// ── MemPool ────────────────────────────────────────────────────────────────────

/// In-memory pool stub for scaffold verification and unit tests.
///
/// Always returns a fresh [`MemConnection`]; no WAL, no foreign-keys, no
/// persistence.  A real WAL-backed pool is wired at M0 initialisation via
/// [`SqlitePool`].
#[derive(Debug, Default)]
pub struct MemPool;

impl MemPool {
    /// Create a new in-memory pool stub.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Pool for MemPool {
    fn with_conn(
        &self,
        f: Box<dyn FnOnce(&dyn Connection) -> Result<(), HleError> + '_>,
    ) -> Result<(), HleError> {
        f(&MemConnection)
    }
}

// ── FailPool (test helper) ─────────────────────────────────────────────────────

/// Test-only pool that always returns a connection-acquire error.
///
/// Used to verify that store methods correctly propagate pool failures.
#[cfg(test)]
pub(crate) struct FailPool;

#[cfg(test)]
impl Pool for FailPool {
    fn with_conn(
        &self,
        _f: Box<dyn FnOnce(&dyn Connection) -> Result<(), HleError> + '_>,
    ) -> Result<(), HleError> {
        Err(err_connection_acquire("FailPool: simulated exhaustion"))
    }
}

// ── FailConnection (test helper) ───────────────────────────────────────────────

/// Test-only connection that always returns an error on every call.
#[cfg(test)]
pub(crate) struct FailConnection;

#[cfg(test)]
impl Connection for FailConnection {
    fn execute_sql(&self, _sql: &str) -> Result<usize, HleError> {
        Err(HleError::new("[2499 Storage] FailConnection: execute_sql"))
    }

    fn query_one_i64(&self, _sql: &str) -> Result<Option<i64>, HleError> {
        Err(HleError::new(
            "[2499 Storage] FailConnection: query_one_i64",
        ))
    }

    fn query_rows_5col(&self, _sql: &str) -> Result<Vec<Row5Col>, HleError> {
        Err(HleError::new(
            "[2499 Storage] FailConnection: query_rows_5col",
        ))
    }

    fn query_rows_tick(&self, _sql: &str) -> Result<Vec<RowTick>, HleError> {
        Err(HleError::new(
            "[2499 Storage] FailConnection: query_rows_tick",
        ))
    }

    fn query_rows_evidence(&self, _sql: &str) -> Result<Vec<RowEvidence>, HleError> {
        Err(HleError::new(
            "[2499 Storage] FailConnection: query_rows_evidence",
        ))
    }

    fn query_rows_verifier(&self, _sql: &str) -> Result<Vec<RowVerifier>, HleError> {
        Err(HleError::new(
            "[2499 Storage] FailConnection: query_rows_verifier",
        ))
    }

    fn query_rows_blocker(&self, _sql: &str) -> Result<Vec<RowBlocker>, HleError> {
        Err(HleError::new(
            "[2499 Storage] FailConnection: query_rows_blocker",
        ))
    }

    fn execute_update(&self, _sql: &str) -> Result<usize, HleError> {
        Err(HleError::new(
            "[2499 Storage] FailConnection: execute_update",
        ))
    }
}

/// Test-only pool whose connection always errors.
#[cfg(test)]
pub(crate) struct FailConnPool;

#[cfg(test)]
impl Pool for FailConnPool {
    fn with_conn(
        &self,
        f: Box<dyn FnOnce(&dyn Connection) -> Result<(), HleError> + '_>,
    ) -> Result<(), HleError> {
        f(&FailConnection)
    }
}

// ── tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{
        err_connection_acquire, err_connection_leak, err_pool_init, with_conn_val, Connection,
        FailConnPool, FailPool, MemConnection, MemPool, Pool, SqlitePool,
    };
    use std::sync::Arc;
    use substrate_types::HleError;

    // ── error code tests ────────────────────────────────────────────────────────

    #[test]
    fn err_pool_init_contains_code() {
        assert!(err_pool_init("detail").to_string().contains("2400"));
    }

    #[test]
    fn err_pool_init_contains_detail() {
        assert!(err_pool_init("my detail").to_string().contains("my detail"));
    }

    #[test]
    fn err_connection_acquire_contains_code() {
        assert!(err_connection_acquire("detail")
            .to_string()
            .contains("2401"));
    }

    #[test]
    fn err_connection_acquire_contains_detail() {
        assert!(err_connection_acquire("timeout msg")
            .to_string()
            .contains("timeout msg"));
    }

    #[test]
    fn err_connection_leak_contains_code() {
        assert!(err_connection_leak("detail").to_string().contains("2402"));
    }

    #[test]
    fn err_connection_leak_contains_detail() {
        assert!(err_connection_leak("leaked guard")
            .to_string()
            .contains("leaked guard"));
    }

    #[test]
    fn err_pool_init_is_different_from_acquire() {
        let e1 = err_pool_init("x");
        let e2 = err_connection_acquire("x");
        assert_ne!(e1.to_string(), e2.to_string());
    }

    #[test]
    fn err_codes_are_distinct() {
        let codes = [
            err_pool_init("").to_string(),
            err_connection_acquire("").to_string(),
            err_connection_leak("").to_string(),
        ];
        // All three messages are distinct
        assert_ne!(codes[0], codes[1]);
        assert_ne!(codes[1], codes[2]);
        assert_ne!(codes[0], codes[2]);
    }

    // ── MemConnection tests ─────────────────────────────────────────────────────

    #[test]
    fn mem_connection_execute_sql_returns_zero() {
        let conn = MemConnection;
        assert_eq!(conn.execute_sql("SELECT 1"), Ok(0));
    }

    #[test]
    fn mem_connection_query_one_i64_returns_none() {
        let conn = MemConnection;
        assert_eq!(conn.query_one_i64("SELECT 1"), Ok(None));
    }

    #[test]
    fn mem_connection_execute_sql_accepts_any_string() {
        let conn = MemConnection;
        assert!(conn.execute_sql("NOT VALID SQL AT ALL").is_ok());
    }

    #[test]
    fn mem_connection_query_one_i64_accepts_any_string() {
        let conn = MemConnection;
        assert!(conn.query_one_i64("garbage").is_ok());
    }

    #[test]
    fn mem_connection_execute_sql_empty_string() {
        let conn = MemConnection;
        assert_eq!(conn.execute_sql(""), Ok(0));
    }

    #[test]
    fn mem_connection_query_one_i64_empty_string() {
        let conn = MemConnection;
        assert_eq!(conn.query_one_i64(""), Ok(None));
    }

    #[test]
    fn mem_connection_default_constructs() {
        let _conn = MemConnection::default();
    }

    // ── MemPool construction ────────────────────────────────────────────────────

    #[test]
    fn mem_pool_default_constructs() {
        let _pool = MemPool::default();
    }

    #[test]
    fn mem_pool_new_constructs() {
        let _pool = MemPool::new();
    }

    #[test]
    fn mem_pool_debug_impl_exists() {
        let pool = MemPool::new();
        let s = format!("{pool:?}");
        assert!(!s.is_empty());
    }

    // ── with_conn basic behaviour ───────────────────────────────────────────────

    #[test]
    fn mem_pool_with_conn_calls_closure() {
        let pool = MemPool::new();
        let result: Result<u8, substrate_types::HleError> = with_conn_val(&pool, |_conn| Ok(42_u8));
        assert_eq!(result, Ok(42));
    }

    #[test]
    fn mem_pool_propagates_closure_error() {
        let pool = MemPool::new();
        let result: Result<(), substrate_types::HleError> =
            with_conn_val(&pool, |_conn| Err(substrate_types::HleError::new("forced")));
        assert!(result.is_err());
    }

    #[test]
    fn mem_pool_closure_error_message_preserved() {
        let pool = MemPool::new();
        let result: Result<(), substrate_types::HleError> = with_conn_val(&pool, |_conn| {
            Err(substrate_types::HleError::new("unique-msg"))
        });
        assert!(result.unwrap_err().to_string().contains("unique-msg"));
    }

    #[test]
    fn mem_pool_with_conn_runs_sql_in_closure() {
        let pool = MemPool::new();
        let mut ran = false;
        pool.with_conn(Box::new(|conn| {
            conn.execute_sql("SELECT 1")?;
            ran = true;
            Ok(())
        }))
        .unwrap();
        assert!(ran);
    }

    #[test]
    fn mem_pool_with_conn_val_returns_string() {
        let pool = MemPool::new();
        let s: String = with_conn_val(&pool, |_conn| Ok("hello".to_owned())).unwrap();
        assert_eq!(s, "hello");
    }

    #[test]
    fn mem_pool_with_conn_val_called_multiple_times() {
        let pool = MemPool::new();
        for i in 0_u8..5 {
            let v: u8 = with_conn_val(&pool, |_conn| Ok(i)).unwrap();
            assert_eq!(v, i);
        }
    }

    // ── trait-object compatibility ──────────────────────────────────────────────

    #[test]
    fn pool_as_dyn_ref() {
        let pool = MemPool::new();
        let dyn_pool: &dyn Pool = &pool;
        let result: Result<u32, substrate_types::HleError> =
            with_conn_val(dyn_pool, |_conn| Ok(7_u32));
        assert_eq!(result, Ok(7));
    }

    #[test]
    fn pool_as_arc_dyn() {
        let pool: Arc<dyn Pool> = Arc::new(MemPool::new());
        let result: Result<i64, substrate_types::HleError> =
            with_conn_val(pool.as_ref(), |_conn| Ok(99_i64));
        assert_eq!(result, Ok(99));
    }

    #[test]
    fn pool_boxed_dyn() {
        let pool: Box<dyn Pool> = Box::new(MemPool::new());
        let result: Result<bool, substrate_types::HleError> =
            with_conn_val(pool.as_ref(), |_conn| Ok(true));
        assert_eq!(result, Ok(true));
    }

    // ── FailPool tests ──────────────────────────────────────────────────────────

    #[test]
    fn fail_pool_with_conn_returns_acquire_error() {
        let pool = FailPool;
        let result = pool.with_conn(Box::new(|_conn| Ok(())));
        assert!(result.is_err());
    }

    #[test]
    fn fail_pool_with_conn_error_contains_2401() {
        let pool = FailPool;
        let err = pool.with_conn(Box::new(|_conn| Ok(()))).unwrap_err();
        assert!(err.to_string().contains("2401"));
    }

    #[test]
    fn fail_pool_with_conn_val_returns_err() {
        let pool = FailPool;
        let result: Result<u8, substrate_types::HleError> = with_conn_val(&pool, |_conn| Ok(1));
        assert!(result.is_err());
    }

    // ── FailConnPool tests ──────────────────────────────────────────────────────

    #[test]
    fn fail_conn_pool_execute_sql_propagates_error() {
        let pool = FailConnPool;
        let result = pool.with_conn(Box::new(|conn| conn.execute_sql("SELECT 1").map(|_| ())));
        assert!(result.is_err());
    }

    #[test]
    fn fail_conn_pool_query_one_i64_propagates_error() {
        let pool = FailConnPool;
        let result: Result<Option<i64>, substrate_types::HleError> =
            with_conn_val(&pool, |conn| conn.query_one_i64("SELECT 1"));
        assert!(result.is_err());
    }

    // ── with_conn_val sentinel test ─────────────────────────────────────────────

    #[test]
    fn with_conn_val_error_when_closure_not_run() {
        // If with_conn returns Ok(()) but never runs f, we get a sentinel error.
        // We model this with a custom pool that never calls f.
        struct NullPool;
        impl Pool for NullPool {
            fn with_conn(
                &self,
                _f: Box<dyn FnOnce(&dyn Connection) -> Result<(), HleError> + '_>,
            ) -> Result<(), HleError> {
                // Intentionally skips calling f
                Ok(())
            }
        }
        let pool = NullPool;
        let result: Result<u8, substrate_types::HleError> = with_conn_val(&pool, |_conn| Ok(1));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("2401"));
    }

    // ── connection reuse within same call ───────────────────────────────────────

    #[test]
    fn mem_pool_closure_sees_connection_execute_and_query() {
        let pool = MemPool::new();
        let result: Result<Option<i64>, _> = with_conn_val(&pool, |conn| {
            conn.execute_sql("INSERT INTO t VALUES (1)")?;
            conn.query_one_i64("SELECT last_insert_rowid()")
        });
        assert!(result.is_ok());
        assert!(result.unwrap().is_none()); // MemPool always returns None
    }

    #[test]
    fn mem_pool_closure_chained_calls_all_succeed() {
        let pool = MemPool::new();
        let result: Result<usize, _> = with_conn_val(&pool, |conn| {
            let a = conn.execute_sql("SQL A")?;
            let b = conn.execute_sql("SQL B")?;
            Ok(a + b)
        });
        assert_eq!(result, Ok(0)); // MemPool: 0 + 0
    }

    // ── additional coverage ─────────────────────────────────────────────────────

    #[test]
    fn err_pool_init_non_empty_message() {
        assert!(!err_pool_init("").to_string().is_empty());
    }

    #[test]
    fn err_connection_acquire_non_empty_message() {
        assert!(!err_connection_acquire("").to_string().is_empty());
    }

    #[test]
    fn err_connection_leak_non_empty_message() {
        assert!(!err_connection_leak("").to_string().is_empty());
    }

    #[test]
    fn pool_with_conn_unit_closure_ok() {
        let pool = MemPool::new();
        assert!(pool.with_conn(Box::new(|_conn| Ok(()))).is_ok());
    }

    #[test]
    fn pool_with_conn_error_propagated() {
        let pool = MemPool::new();
        let res = pool.with_conn(Box::new(|_conn| Err(HleError::new("test-err"))));
        assert!(res.is_err());
    }

    #[test]
    fn mem_pool_clone_via_default() {
        let _a = MemPool::default();
        let _b = MemPool::default();
    }

    #[test]
    fn with_conn_val_ok_unit() {
        let pool = MemPool::new();
        let r: Result<(), _> = with_conn_val(&pool, |_| Ok(()));
        assert!(r.is_ok());
    }

    #[test]
    fn with_conn_val_returns_vec() {
        let pool = MemPool::new();
        let r: Result<Vec<u8>, _> = with_conn_val(&pool, |_| Ok(vec![1, 2, 3]));
        assert_eq!(r.unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn with_conn_val_error_wraps_message() {
        let pool = MemPool::new();
        let r: Result<i32, _> = with_conn_val(&pool, |_| Err(HleError::new("inner")));
        assert!(r.unwrap_err().to_string().contains("inner"));
    }

    #[test]
    fn fail_pool_error_is_2401() {
        let pool = FailPool;
        let r = with_conn_val::<(), _>(&pool, |_| Ok(()));
        assert!(r.unwrap_err().to_string().contains("2401"));
    }

    #[test]
    fn fail_conn_pool_execute_error_has_2499() {
        let pool = FailConnPool;
        let r = pool.with_conn(Box::new(|conn| conn.execute_sql("x").map(|_| ())));
        assert!(r.unwrap_err().to_string().contains("2499"));
    }

    #[test]
    fn mem_conn_execute_sql_returns_ok_zero_always() {
        let conn = MemConnection::default();
        for sql in ["", "CREATE TABLE t(id INT)", "INSERT INTO t VALUES(1)"] {
            assert_eq!(conn.execute_sql(sql), Ok(0));
        }
    }

    #[test]
    fn mem_conn_query_i64_returns_ok_none_always() {
        let conn = MemConnection::default();
        for sql in ["", "SELECT 1", "SELECT MAX(id) FROM t"] {
            assert_eq!(conn.query_one_i64(sql), Ok(None));
        }
    }

    #[test]
    fn pool_trait_object_in_fn_arg() {
        fn do_work(pool: &dyn Pool) -> Result<u8, HleError> {
            with_conn_val(pool, |_| Ok(99_u8))
        }
        let pool = MemPool::new();
        assert_eq!(do_work(&pool), Ok(99));
    }

    #[test]
    fn with_conn_val_returns_option_none() {
        let pool = MemPool::new();
        let r: Result<Option<i32>, HleError> = with_conn_val(&pool, |_| Ok(None));
        assert_eq!(r, Ok(None));
    }

    #[test]
    fn mem_pool_multiple_sequential_with_conn_calls() {
        let pool = MemPool::new();
        for _ in 0..10 {
            assert!(pool.with_conn(Box::new(|_| Ok(()))).is_ok());
        }
    }

    // ── SqlitePool tests ────────────────────────────────────────────────────────

    #[test]
    fn sqlite_pool_open_memory_succeeds() {
        assert!(SqlitePool::open_memory().is_ok());
    }

    #[test]
    fn sqlite_pool_execute_sql_creates_table() {
        let pool = SqlitePool::open_memory().unwrap();
        let result = pool.with_conn(Box::new(|conn| {
            conn.execute_sql("CREATE TABLE IF NOT EXISTS t (id INTEGER PRIMARY KEY)")
                .map(|_| ())
        }));
        assert!(result.is_ok());
    }

    #[test]
    fn sqlite_pool_query_one_i64_returns_value() {
        let pool = SqlitePool::open_memory().unwrap();
        pool.with_conn(Box::new(|conn| {
            conn.execute_sql(
                "CREATE TABLE IF NOT EXISTS t (id INTEGER PRIMARY KEY AUTOINCREMENT, v INTEGER)",
            )
            .map(|_| ())
        }))
        .unwrap();
        pool.with_conn(Box::new(|conn| {
            conn.execute_sql("INSERT INTO t (v) VALUES (42)")
                .map(|_| ())
        }))
        .unwrap();
        let v: Option<i64> = with_conn_val(&pool, |conn| {
            conn.query_one_i64("SELECT last_insert_rowid()")
        })
        .unwrap();
        assert_eq!(v, Some(1));
    }

    #[test]
    fn sqlite_pool_execute_batch_multi_statement_ok() {
        let pool = SqlitePool::open_memory().unwrap();
        let sql = "CREATE TABLE IF NOT EXISTS a (id INT); CREATE TABLE IF NOT EXISTS b (id INT);";
        let result = pool.with_conn(Box::new(|conn| conn.execute_sql(sql).map(|_| ())));
        assert!(result.is_ok());
    }

    #[test]
    fn sqlite_pool_select_returns_row() {
        let pool = SqlitePool::open_memory().unwrap();
        let v: Option<i64> = with_conn_val(&pool, |conn| conn.query_one_i64("SELECT 42")).unwrap();
        assert_eq!(v, Some(42));
    }

    #[test]
    fn sqlite_pool_as_dyn_pool() {
        let pool = SqlitePool::open_memory().unwrap();
        let dyn_pool: &dyn Pool = &pool;
        let r: Result<u32, HleError> = with_conn_val(dyn_pool, |_| Ok(7_u32));
        assert_eq!(r, Ok(7));
    }

    #[test]
    fn sqlite_pool_open_file_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pool_test.db");
        let pool = SqlitePool::open(&path).unwrap();
        let r: Result<u32, HleError> = with_conn_val(&pool, |_| Ok(99_u32));
        assert_eq!(r, Ok(99));
    }

    #[test]
    fn sqlite_pool_multiple_inserts_accumulate() {
        let pool = SqlitePool::open_memory().unwrap();
        pool.with_conn(Box::new(|conn| {
            conn.execute_sql("CREATE TABLE t (id INTEGER PRIMARY KEY AUTOINCREMENT)")
                .map(|_| ())
        }))
        .unwrap();
        for _ in 0..5 {
            pool.with_conn(Box::new(|conn| {
                conn.execute_sql("INSERT INTO t DEFAULT VALUES").map(|_| ())
            }))
            .unwrap();
        }
        let count: Option<i64> =
            with_conn_val(&pool, |conn| conn.query_one_i64("SELECT COUNT(*) FROM t")).unwrap();
        assert_eq!(count, Some(5));
    }
}
