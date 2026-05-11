#![forbid(unsafe_code)]

//! M027 — Workflow run table abstraction.
//!
//! [`WorkflowRun`] mirrors one row in `workflow_runs`.  [`WorkflowRunsStore`]
//! exposes append-only `insert` and a read-only `list_all` helper.
//!
//! Status values align with the `CHECK` constraint in
//! `migrations/0001_scaffold_schema.sql`:
//! `running | pass | fail | awaiting-human | rolled-back`.
//!
//! Error codes: 2420–2422 (`RunInsert`, `RunNotFound`, `RunStatusInvalid`).

use substrate_types::HleError;

use crate::pool::{with_conn_val, Pool};

// ── row mapping helper ─────────────────────────────────────────────────────────

fn row_to_run(row: (i64, String, String, i64, Option<i64>)) -> Result<WorkflowRun, HleError> {
    let (id, workflow_name, status_str, created_unix, completed_unix) = row;
    let status = RunStatus::parse_str(&status_str)?;
    Ok(WorkflowRun {
        id,
        workflow_name,
        status,
        created_unix,
        completed_unix,
        // authorization_profile is not persisted in the current schema; default to empty.
        authorization_profile: String::new(),
    })
}

// ── error helpers ──────────────────────────────────────────────────────────────

fn err_run_insert(detail: impl core::fmt::Display) -> HleError {
    HleError::new(format!("[2420 RunInsert] {detail}"))
}

/// Error code 2421: run row not found by id.
#[allow(dead_code)]
pub(crate) fn err_run_not_found(id: i64) -> HleError {
    HleError::new(format!("[2421 RunNotFound] run_id={id} not found"))
}

fn err_run_status_invalid(status: &str) -> HleError {
    HleError::new(format!(
        "[2422 RunStatusInvalid] unknown run status: {status}"
    ))
}

// ── RunStatus ──────────────────────────────────────────────────────────────────

/// Status of a workflow run — matches the `CHECK` constraint in `workflow_runs`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunStatus {
    Running,
    Pass,
    Fail,
    AwaitingHuman,
    RolledBack,
}

impl RunStatus {
    /// Wire string used in SQL and JSONL representations.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::AwaitingHuman => "awaiting-human",
            Self::RolledBack => "rolled-back",
        }
    }

    /// Parse from the wire string stored in the database.
    ///
    /// # Errors
    ///
    /// Returns `HleError` (2422) when `s` is not a known status string.
    pub fn parse_str(s: &str) -> Result<Self, HleError> {
        match s {
            "running" => Ok(Self::Running),
            "pass" => Ok(Self::Pass),
            "fail" => Ok(Self::Fail),
            "awaiting-human" => Ok(Self::AwaitingHuman),
            "rolled-back" => Ok(Self::RolledBack),
            other => Err(err_run_status_invalid(other)),
        }
    }

    /// Return `true` when no further state transition is expected.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Pass | Self::Fail | Self::RolledBack)
    }

    /// Return `true` when the run can still accept new ticks.
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Running | Self::AwaitingHuman)
    }

    /// All valid status strings as a compile-time array (used in validation tests).
    #[must_use]
    pub const fn all_strs() -> [&'static str; 5] {
        ["running", "pass", "fail", "awaiting-human", "rolled-back"]
    }
}

// ── WorkflowRun ────────────────────────────────────────────────────────────────

/// One row from the `workflow_runs` table.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkflowRun {
    /// Auto-increment primary key assigned by the database at insert.
    pub id: i64,
    /// Name of the workflow definition that spawned this run.
    pub workflow_name: String,
    /// Current run status — kept in sync with the `CHECK` constraint.
    pub status: RunStatus,
    /// Unix timestamp (seconds) when the run was created.
    pub created_unix: i64,
    /// Unix timestamp (seconds) when the run reached a terminal status, if any.
    pub completed_unix: Option<i64>,
    /// Authorization profile string that authorised this run (free-form tag).
    pub authorization_profile: String,
}

impl WorkflowRun {
    /// Construct a new run descriptor (id 0 = not yet persisted).
    #[must_use]
    pub fn new(
        workflow_name: impl Into<String>,
        authorization_profile: impl Into<String>,
        created_unix: i64,
    ) -> Self {
        Self {
            id: 0,
            workflow_name: workflow_name.into(),
            status: RunStatus::Running,
            created_unix,
            completed_unix: None,
            authorization_profile: authorization_profile.into(),
        }
    }

    /// Return `true` when `completed_unix > created_unix`.
    ///
    /// Always `false` for unpersisted runs (no `completed_unix`).
    #[must_use]
    pub fn completion_is_after_creation(&self) -> bool {
        match self.completed_unix {
            Some(c) => c > self.created_unix,
            None => false,
        }
    }

    /// Return `true` when the run is eligible for TTL deletion:
    /// status is terminal and age ≥ `retention_secs`.
    #[must_use]
    pub fn ttl_eligible(&self, now_unix: i64, retention_secs: i64) -> bool {
        self.status.is_terminal() && (now_unix.saturating_sub(self.created_unix)) >= retention_secs
    }
}

// ── WorkflowRunsStore ──────────────────────────────────────────────────────────

/// Append-only access layer for the `workflow_runs` table.
pub struct WorkflowRunsStore<'pool> {
    pool: &'pool dyn Pool,
}

impl<'pool> WorkflowRunsStore<'pool> {
    /// Bind the store to a pool.
    #[must_use]
    pub fn new(pool: &'pool dyn Pool) -> Self {
        Self { pool }
    }

    /// Insert a new run row and return the assigned id.
    ///
    /// # Errors
    ///
    /// Returns `HleError` (2420) when the pool connection fails or the INSERT
    /// is rejected.
    pub fn insert(&self, run: &WorkflowRun) -> Result<i64, HleError> {
        let sql = format!(
            "INSERT INTO workflow_runs (workflow_name, status, created_unix, completed_unix) \
             VALUES ('{}', '{}', {}, {})",
            sql_escape(&run.workflow_name),
            run.status.as_str(),
            run.created_unix,
            run.completed_unix
                .map_or_else(|| "NULL".to_owned(), |v| v.to_string()),
        );
        with_conn_val(self.pool, |conn| {
            conn.execute_sql(&sql).map_err(err_run_insert)?;
            // In the real backend this returns the last_insert_rowid; the
            // MemPool stub returns None, so we fall back to a sentinel 0.
            conn.query_one_i64("SELECT last_insert_rowid()")
                .map(|opt| opt.unwrap_or(0))
                .map_err(err_run_insert)
        })
    }

    /// Return all run rows ordered by id ascending (full table scan — M0 scope).
    ///
    /// # Errors
    ///
    /// Returns `HleError` when the pool fails to acquire a connection.
    pub fn list_all(&self) -> Result<Vec<WorkflowRun>, HleError> {
        let sql = "SELECT id, workflow_name, status, created_unix, completed_unix \
                   FROM workflow_runs ORDER BY id ASC";
        with_conn_val(self.pool, |conn| {
            let rows = conn.query_rows_5col(sql)?;
            rows.into_iter().map(row_to_run).collect()
        })
    }

    /// Return all runs filtered by workflow name.
    ///
    /// # Errors
    ///
    /// Returns `HleError` when the pool fails to acquire a connection.
    pub fn list_by_name(&self, workflow_name: &str) -> Result<Vec<WorkflowRun>, HleError> {
        let sql = format!(
            "SELECT id, workflow_name, status, created_unix, completed_unix \
             FROM workflow_runs WHERE workflow_name = '{}' ORDER BY id ASC",
            sql_escape(workflow_name),
        );
        with_conn_val(self.pool, |conn| {
            let rows = conn.query_rows_5col(&sql)?;
            rows.into_iter().map(row_to_run).collect()
        })
    }

    /// Return all runs with a terminal status older than `retention_secs`.
    ///
    /// Uses `now_unix - retention_secs` arithmetic; never literal timestamps.
    ///
    /// # Errors
    ///
    /// Returns `HleError` when the pool fails to acquire a connection.
    pub fn list_ttl_eligible(
        &self,
        now_unix: i64,
        retention_secs: i64,
    ) -> Result<Vec<WorkflowRun>, HleError> {
        let threshold = now_unix.saturating_sub(retention_secs);
        let sql = format!(
            "SELECT id, workflow_name, status, created_unix, completed_unix \
             FROM workflow_runs \
             WHERE status IN ('pass','fail','rolled-back') AND created_unix < {threshold} \
             ORDER BY id ASC",
        );
        with_conn_val(self.pool, |conn| {
            let rows = conn.query_rows_5col(&sql)?;
            rows.into_iter().map(row_to_run).collect()
        })
    }
}

// ── TTL helpers ────────────────────────────────────────────────────────────────

/// SQL predicate for workflow run TTL deletion (terminal rows only).
///
/// Uses `now_unix - retention_secs` arithmetic (framework §17.5).
#[must_use]
pub fn run_ttl_predicate(now_unix: i64, retention_secs: i64) -> String {
    format!(
        "status IN ('pass','fail','rolled-back') AND \
         created_unix < ({now_unix} - {retention_secs})"
    )
}

// ── helpers ────────────────────────────────────────────────────────────────────

/// Minimal SQL string escaping: replace `'` with `''`.
pub(crate) fn sql_escape(s: &str) -> String {
    s.replace('\'', "''")
}

// ── tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{err_run_not_found, run_ttl_predicate, RunStatus, WorkflowRun, WorkflowRunsStore};
    use crate::pool::MemPool;

    fn now_unix() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    // ── RunStatus wire strings ──────────────────────────────────────────────────

    #[test]
    fn run_status_running_str_is_stable() {
        assert_eq!(RunStatus::Running.as_str(), "running");
    }

    #[test]
    fn run_status_pass_str_is_stable() {
        assert_eq!(RunStatus::Pass.as_str(), "pass");
    }

    #[test]
    fn run_status_fail_str_is_stable() {
        assert_eq!(RunStatus::Fail.as_str(), "fail");
    }

    #[test]
    fn run_status_awaiting_human_str_is_stable() {
        assert_eq!(RunStatus::AwaitingHuman.as_str(), "awaiting-human");
    }

    #[test]
    fn run_status_rolled_back_str_is_stable() {
        assert_eq!(RunStatus::RolledBack.as_str(), "rolled-back");
    }

    // ── RunStatus is_terminal ───────────────────────────────────────────────────

    #[test]
    fn run_status_running_is_not_terminal() {
        assert!(!RunStatus::Running.is_terminal());
    }

    #[test]
    fn run_status_pass_is_terminal() {
        assert!(RunStatus::Pass.is_terminal());
    }

    #[test]
    fn run_status_fail_is_terminal() {
        assert!(RunStatus::Fail.is_terminal());
    }

    #[test]
    fn run_status_rolled_back_is_terminal() {
        assert!(RunStatus::RolledBack.is_terminal());
    }

    #[test]
    fn run_status_awaiting_human_is_not_terminal() {
        assert!(!RunStatus::AwaitingHuman.is_terminal());
    }

    // ── RunStatus is_active ─────────────────────────────────────────────────────

    #[test]
    fn run_status_running_is_active() {
        assert!(RunStatus::Running.is_active());
    }

    #[test]
    fn run_status_awaiting_human_is_active() {
        assert!(RunStatus::AwaitingHuman.is_active());
    }

    #[test]
    fn run_status_pass_is_not_active() {
        assert!(!RunStatus::Pass.is_active());
    }

    #[test]
    fn run_status_fail_is_not_active() {
        assert!(!RunStatus::Fail.is_active());
    }

    #[test]
    fn run_status_rolled_back_is_not_active() {
        assert!(!RunStatus::RolledBack.is_active());
    }

    // ── RunStatus parse_str ─────────────────────────────────────────────────────

    #[test]
    fn run_status_parses_running() {
        assert_eq!(RunStatus::parse_str("running"), Ok(RunStatus::Running));
    }

    #[test]
    fn run_status_parses_pass() {
        assert_eq!(RunStatus::parse_str("pass"), Ok(RunStatus::Pass));
    }

    #[test]
    fn run_status_parses_fail() {
        assert_eq!(RunStatus::parse_str("fail"), Ok(RunStatus::Fail));
    }

    #[test]
    fn run_status_parses_awaiting_human() {
        assert_eq!(
            RunStatus::parse_str("awaiting-human"),
            Ok(RunStatus::AwaitingHuman)
        );
    }

    #[test]
    fn run_status_parses_rolled_back() {
        assert_eq!(
            RunStatus::parse_str("rolled-back"),
            Ok(RunStatus::RolledBack)
        );
    }

    #[test]
    fn run_status_rejects_unknown() {
        assert!(RunStatus::parse_str("unknown").is_err());
    }

    #[test]
    fn run_status_error_contains_code() {
        let err = RunStatus::parse_str("bad");
        assert!(err.unwrap_err().to_string().contains("2422"));
    }

    #[test]
    fn run_status_error_contains_bad_value() {
        let err = RunStatus::parse_str("bogus-status").unwrap_err();
        assert!(err.to_string().contains("bogus-status"));
    }

    #[test]
    fn run_status_all_strs_roundtrip() {
        for s in RunStatus::all_strs() {
            assert!(RunStatus::parse_str(s).is_ok(), "failed to parse: {s}");
        }
    }

    #[test]
    fn run_status_all_strs_are_unique() {
        let strs = RunStatus::all_strs();
        let mut seen = std::collections::HashSet::new();
        for s in strs {
            assert!(seen.insert(s), "duplicate status string: {s}");
        }
    }

    #[test]
    fn run_status_rejects_uppercase_running() {
        assert!(RunStatus::parse_str("Running").is_err());
    }

    #[test]
    fn run_status_rejects_empty_string() {
        assert!(RunStatus::parse_str("").is_err());
    }

    // ── WorkflowRun construction ────────────────────────────────────────────────

    #[test]
    fn workflow_run_new_defaults_to_running() {
        let run = WorkflowRun::new("demo", "m0-local", now_unix());
        assert_eq!(run.status, RunStatus::Running);
    }

    #[test]
    fn workflow_run_new_has_no_completed_unix() {
        let run = WorkflowRun::new("demo", "m0-local", now_unix());
        assert!(run.completed_unix.is_none());
    }

    #[test]
    fn workflow_run_new_sets_workflow_name() {
        let run = WorkflowRun::new("my-flow", "m0-local", now_unix());
        assert_eq!(run.workflow_name, "my-flow");
    }

    #[test]
    fn workflow_run_new_sets_auth_profile() {
        let run = WorkflowRun::new("demo", "auth-abc", now_unix());
        assert_eq!(run.authorization_profile, "auth-abc");
    }

    #[test]
    fn workflow_run_new_id_is_zero() {
        let run = WorkflowRun::new("demo", "m0-local", now_unix());
        assert_eq!(run.id, 0);
    }

    #[test]
    fn workflow_run_new_sets_created_unix() {
        let ts = 1_700_000_000_i64;
        let run = WorkflowRun::new("demo", "m0-local", ts);
        assert_eq!(run.created_unix, ts);
    }

    // ── WorkflowRun helpers ─────────────────────────────────────────────────────

    #[test]
    fn completion_is_after_creation_false_when_none() {
        let run = WorkflowRun::new("demo", "m0-local", 1000);
        assert!(!run.completion_is_after_creation());
    }

    #[test]
    fn completion_is_after_creation_true_when_later() {
        let mut run = WorkflowRun::new("demo", "m0-local", 1000);
        run.completed_unix = Some(2000);
        assert!(run.completion_is_after_creation());
    }

    #[test]
    fn completion_is_after_creation_false_when_same() {
        let mut run = WorkflowRun::new("demo", "m0-local", 1000);
        run.completed_unix = Some(1000);
        assert!(!run.completion_is_after_creation());
    }

    #[test]
    fn completion_is_after_creation_false_when_earlier() {
        let mut run = WorkflowRun::new("demo", "m0-local", 2000);
        run.completed_unix = Some(1000);
        assert!(!run.completion_is_after_creation());
    }

    #[test]
    fn ttl_eligible_false_for_non_terminal() {
        let run = WorkflowRun::new("demo", "m0-local", 1000);
        assert!(!run.ttl_eligible(1_000_000, 86_400));
    }

    #[test]
    fn ttl_eligible_true_for_terminal_and_old() {
        let mut run = WorkflowRun::new("demo", "m0-local", 1000);
        run.status = RunStatus::Pass;
        // now=2000, retention=100 → age=1000 ≥ 100
        assert!(run.ttl_eligible(2000, 100));
    }

    #[test]
    fn ttl_eligible_false_for_terminal_but_too_recent() {
        let now = now_unix();
        let mut run = WorkflowRun::new("demo", "m0-local", now);
        run.status = RunStatus::Fail;
        // retention 1 year: too young
        assert!(!run.ttl_eligible(now, 365 * 86_400));
    }

    // ── WorkflowRunsStore ───────────────────────────────────────────────────────

    #[test]
    fn store_insert_succeeds_against_mem_pool() {
        let pool = MemPool::new();
        let store = WorkflowRunsStore::new(&pool);
        let run = WorkflowRun::new("demo", "m0-local", now_unix());
        assert!(store.insert(&run).is_ok());
    }

    #[test]
    fn store_list_all_returns_empty_on_mem_pool() {
        let pool = MemPool::new();
        let store = WorkflowRunsStore::new(&pool);
        let rows = store.list_all();
        assert!(rows.is_ok());
        assert!(rows.unwrap().is_empty());
    }

    #[test]
    fn store_list_by_name_returns_empty_on_mem_pool() {
        let pool = MemPool::new();
        let store = WorkflowRunsStore::new(&pool);
        let rows = store.list_by_name("any-flow");
        assert!(rows.is_ok());
        assert!(rows.unwrap().is_empty());
    }

    #[test]
    fn store_list_ttl_eligible_returns_empty_on_mem_pool() {
        let pool = MemPool::new();
        let store = WorkflowRunsStore::new(&pool);
        let rows = store.list_ttl_eligible(now_unix(), 86_400);
        assert!(rows.is_ok());
        assert!(rows.unwrap().is_empty());
    }

    #[test]
    fn store_multiple_inserts_all_succeed() {
        let pool = MemPool::new();
        let store = WorkflowRunsStore::new(&pool);
        for i in 0..5_i64 {
            let run = WorkflowRun::new("flow", "auth", 1_000_000 + i);
            assert!(store.insert(&run).is_ok());
        }
    }

    // ── sql_escape ──────────────────────────────────────────────────────────────

    #[test]
    fn sql_escape_replaces_single_quote() {
        assert_eq!(super::sql_escape("it's"), "it''s");
    }

    #[test]
    fn sql_escape_no_op_for_clean_string() {
        assert_eq!(super::sql_escape("clean"), "clean");
    }

    #[test]
    fn sql_escape_multiple_quotes() {
        assert_eq!(super::sql_escape("a'b'c"), "a''b''c");
    }

    // ── error helpers ───────────────────────────────────────────────────────────

    #[test]
    fn err_run_not_found_contains_2421() {
        assert!(err_run_not_found(5).to_string().contains("2421"));
    }

    #[test]
    fn err_run_not_found_contains_id() {
        assert!(err_run_not_found(42).to_string().contains("42"));
    }

    // ── TTL predicate ───────────────────────────────────────────────────────────

    #[test]
    fn run_ttl_predicate_contains_terminal_statuses() {
        let pred = run_ttl_predicate(now_unix(), 86_400);
        assert!(pred.contains("pass"));
        assert!(pred.contains("fail"));
        assert!(pred.contains("rolled-back"));
    }

    #[test]
    fn run_ttl_predicate_contains_now_unix() {
        let now = 1_700_000_000_i64;
        let pred = run_ttl_predicate(now, 86_400);
        assert!(pred.contains(&now.to_string()));
    }

    #[test]
    fn run_ttl_predicate_contains_retention_term() {
        let pred = run_ttl_predicate(1_700_000_000, 7_776_000);
        assert!(pred.contains("7776000"));
    }

    // ── SqlitePool integration tests ────────────────────────────────────────────

    fn make_sqlite_pool() -> crate::pool::SqlitePool {
        let pool = crate::pool::SqlitePool::open_memory().unwrap();
        crate::migrations::run_migrations(&pool).unwrap();
        pool
    }

    #[test]
    fn sqlite_insert_returns_nonzero_id() {
        let pool = make_sqlite_pool();
        let store = WorkflowRunsStore::new(&pool);
        let run = WorkflowRun::new("demo", "m0-local", 1_700_000_000);
        let id = store.insert(&run).unwrap();
        assert!(id > 0);
    }

    #[test]
    fn sqlite_insert_then_list_all_returns_one_row() {
        let pool = make_sqlite_pool();
        let store = WorkflowRunsStore::new(&pool);
        let run = WorkflowRun::new("my-flow", "m0-local", 1_700_000_000);
        store.insert(&run).unwrap();
        let rows = store.list_all().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].workflow_name, "my-flow");
    }

    #[test]
    fn sqlite_insert_multiple_then_list_all_returns_all() {
        let pool = make_sqlite_pool();
        let store = WorkflowRunsStore::new(&pool);
        for i in 0..5_i64 {
            let run = WorkflowRun::new("flow", "auth", 1_700_000_000 + i);
            store.insert(&run).unwrap();
        }
        assert_eq!(store.list_all().unwrap().len(), 5);
    }

    #[test]
    fn sqlite_list_by_name_filters_correctly() {
        let pool = make_sqlite_pool();
        let store = WorkflowRunsStore::new(&pool);
        store
            .insert(&WorkflowRun::new("flow-a", "m0", 1_700_000_001))
            .unwrap();
        store
            .insert(&WorkflowRun::new("flow-b", "m0", 1_700_000_002))
            .unwrap();
        store
            .insert(&WorkflowRun::new("flow-a", "m0", 1_700_000_003))
            .unwrap();
        let rows = store.list_by_name("flow-a").unwrap();
        assert_eq!(rows.len(), 2);
        for r in &rows {
            assert_eq!(r.workflow_name, "flow-a");
        }
    }

    #[test]
    fn sqlite_list_ttl_eligible_returns_terminal_old_rows() {
        let pool = make_sqlite_pool();
        let store = WorkflowRunsStore::new(&pool);
        // Insert a terminal run with old created_unix.
        let mut run = WorkflowRun::new("old-flow", "m0", 1_000);
        run.status = RunStatus::Pass;
        store.insert(&run).unwrap();
        // Insert a running run.
        store
            .insert(&WorkflowRun::new("new-flow", "m0", 1_000_000_000))
            .unwrap();
        // TTL: now=2_000_000, retention=1_000 → threshold=1_999_000. Only the old row qualifies.
        let eligible = store.list_ttl_eligible(2_000_000, 1_000).unwrap();
        assert_eq!(eligible.len(), 1);
        assert_eq!(eligible[0].workflow_name, "old-flow");
    }

    #[test]
    fn sqlite_insert_ids_are_sequential() {
        let pool = make_sqlite_pool();
        let store = WorkflowRunsStore::new(&pool);
        let id1 = store.insert(&WorkflowRun::new("f", "m0", 1)).unwrap();
        let id2 = store.insert(&WorkflowRun::new("f", "m0", 2)).unwrap();
        assert!(id2 > id1);
    }

    #[test]
    fn sqlite_list_all_status_preserved() {
        let pool = make_sqlite_pool();
        let store = WorkflowRunsStore::new(&pool);
        let run = WorkflowRun::new("flow", "m0", 1_700_000_000);
        store.insert(&run).unwrap();
        let rows = store.list_all().unwrap();
        assert_eq!(rows[0].status, RunStatus::Running);
    }

    #[test]
    fn sqlite_list_all_created_unix_preserved() {
        let pool = make_sqlite_pool();
        let store = WorkflowRunsStore::new(&pool);
        let ts = 1_700_000_001_i64;
        store.insert(&WorkflowRun::new("f", "m0", ts)).unwrap();
        let rows = store.list_all().unwrap();
        assert_eq!(rows[0].created_unix, ts);
    }
}
