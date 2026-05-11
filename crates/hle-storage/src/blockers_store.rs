#![forbid(unsafe_code)]

//! M031 — Blocked / awaiting-human state persistence.
//!
//! A [`Blocker`] row captures the moment a workflow step enters a wait state
//! (human approval, external dependency, etc.) and records the expected
//! resolver role so that the runbook subsystem and CLI status display can
//! surface actionable guidance.
//!
//! The store is append-only at the insert surface.  Resolution is recorded as
//! a new `resolved_unix` update in the real backend; the stub leaves resolution
//! as a no-op consistent with the `MemPool` contract.
//!
//! Error codes: 2460–2461 (`BlockerInsert`, `BlockerNotFound`).

use substrate_types::HleError;

use crate::pool::{with_conn_val, Pool};

// ── row mapping helper ─────────────────────────────────────────────────────────

fn row_to_blocker(row: (i64, i64, String, String, i64, String, Option<i64>)) -> Blocker {
    let (id, run_id, step_id, blocker_kind, since_unix, expected_resolver_role, resolved_unix) =
        row;
    Blocker {
        id,
        run_id,
        step_id,
        blocker_kind,
        since_unix,
        expected_resolver_role,
        resolved_unix,
    }
}

// ── error helpers ──────────────────────────────────────────────────────────────

pub(crate) fn err_blocker_insert(detail: impl core::fmt::Display) -> HleError {
    HleError::new(format!("[2460 BlockerInsert] {detail}"))
}

pub(crate) fn err_blocker_not_found(id: i64) -> HleError {
    HleError::new(format!("[2461 BlockerNotFound] blocker row {id} not found"))
}

// ── well-known blocker kinds ───────────────────────────────────────────────────

/// Well-known blocker kind: step requires manual human approval before resuming.
pub const BLOCKER_KIND_HUMAN_APPROVAL: &str = "human-approval";

/// Well-known blocker kind: step is awaiting an external dependency.
pub const BLOCKER_KIND_EXTERNAL_DEP: &str = "external-dependency";

/// Well-known blocker kind: step is rate-limited and must wait.
pub const BLOCKER_KIND_RATE_LIMIT: &str = "rate-limit";

// ── Blocker ────────────────────────────────────────────────────────────────────

/// One row in the `blockers_store` table.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Blocker {
    /// Auto-increment primary key.
    pub id: i64,
    /// Foreign key → `workflow_runs.id`.
    pub run_id: i64,
    /// Step identifier within the workflow that is blocked.
    pub step_id: String,
    /// Free-form tag describing the blocker category (e.g. `"human-approval"`).
    pub blocker_kind: String,
    /// Unix timestamp (seconds) when the blocker was recorded.
    pub since_unix: i64,
    /// Role string expected to resolve this blocker (e.g. `"operator"`, `"qa-lead"`).
    pub expected_resolver_role: String,
    /// Unix timestamp (seconds) when the blocker was resolved; `None` = unresolved.
    pub resolved_unix: Option<i64>,
}

impl Blocker {
    /// Construct a new unresolved blocker (id 0 = not yet persisted).
    #[must_use]
    pub fn new(
        run_id: i64,
        step_id: impl Into<String>,
        blocker_kind: impl Into<String>,
        since_unix: i64,
        expected_resolver_role: impl Into<String>,
    ) -> Self {
        Self {
            id: 0,
            run_id,
            step_id: step_id.into(),
            blocker_kind: blocker_kind.into(),
            since_unix,
            expected_resolver_role: expected_resolver_role.into(),
            resolved_unix: None,
        }
    }

    /// Return `true` when the blocker has been resolved.
    #[must_use]
    pub fn is_resolved(&self) -> bool {
        self.resolved_unix.is_some()
    }

    /// Age of the blocker in seconds at the given reference timestamp.
    ///
    /// Returns 0 when `now_unix` is before `since_unix` (clock skew guard).
    #[must_use]
    pub fn age_secs(&self, now_unix: i64) -> u64 {
        u64::try_from(now_unix.saturating_sub(self.since_unix).max(0)).unwrap_or(0)
    }

    /// Return `true` when the resolved blocker is eligible for TTL deletion.
    ///
    /// A resolved blocker is eligible when `(now_unix - resolved_unix) >= retention_secs`.
    /// Unresolved blockers are NEVER TTL-eligible.
    #[must_use]
    pub fn ttl_eligible(&self, now_unix: i64, retention_secs: i64) -> bool {
        match self.resolved_unix {
            Some(r) => (now_unix.saturating_sub(r)) >= retention_secs,
            None => false,
        }
    }

    /// Resolve this blocker in-memory (does not persist).
    ///
    /// Returns a new `Blocker` with `resolved_unix` set.
    #[must_use]
    pub fn with_resolution(mut self, resolved_unix: i64) -> Self {
        self.resolved_unix = Some(resolved_unix);
        self
    }
}

// ── BlockersStore ──────────────────────────────────────────────────────────────

/// Append-only access layer for the `blockers_store` table.
pub struct BlockersStore<'pool> {
    pool: &'pool dyn Pool,
}

impl<'pool> BlockersStore<'pool> {
    /// Bind the store to a pool.
    #[must_use]
    pub fn new(pool: &'pool dyn Pool) -> Self {
        Self { pool }
    }

    /// Append a blocker row and return the assigned row id.
    ///
    /// # Errors
    ///
    /// Returns `HleError` (2460) when the INSERT fails.
    pub fn insert(&self, blocker: &Blocker) -> Result<i64, HleError> {
        let sql = format!(
            "INSERT INTO blockers_store \
             (run_id, step_id, blocker_kind, since_unix, expected_resolver_role, resolved_unix) \
             VALUES ({}, '{}', '{}', {}, '{}', {})",
            blocker.run_id,
            sql_escape(&blocker.step_id),
            sql_escape(&blocker.blocker_kind),
            blocker.since_unix,
            sql_escape(&blocker.expected_resolver_role),
            blocker
                .resolved_unix
                .map_or_else(|| "NULL".to_owned(), |v| v.to_string()),
        );
        with_conn_val(self.pool, |conn| {
            conn.execute_sql(&sql).map_err(err_blocker_insert)?;
            conn.query_one_i64("SELECT last_insert_rowid()")
                .map(|opt| opt.unwrap_or(0))
                .map_err(err_blocker_insert)
        })
    }

    /// Return all unresolved blockers for a run, ordered by `since_unix` ascending.
    ///
    /// # Errors
    ///
    /// Returns `HleError` when the pool fails to acquire a connection.
    pub fn list_unresolved(&self, run_id: i64) -> Result<Vec<Blocker>, HleError> {
        let sql = format!(
            "SELECT id, run_id, step_id, blocker_kind, since_unix, expected_resolver_role, \
             resolved_unix FROM blockers_store \
             WHERE run_id = {run_id} AND resolved_unix IS NULL \
             ORDER BY since_unix ASC"
        );
        with_conn_val(self.pool, |conn| {
            let rows = conn.query_rows_blocker(&sql)?;
            Ok(rows.into_iter().map(row_to_blocker).collect())
        })
    }

    /// Return all blockers (resolved and unresolved) for a run.
    ///
    /// # Errors
    ///
    /// Returns `HleError` when the pool fails to acquire a connection.
    pub fn list_all_for_run(&self, run_id: i64) -> Result<Vec<Blocker>, HleError> {
        let sql = format!(
            "SELECT id, run_id, step_id, blocker_kind, since_unix, expected_resolver_role, \
             resolved_unix FROM blockers_store WHERE run_id = {run_id} \
             ORDER BY since_unix ASC"
        );
        with_conn_val(self.pool, |conn| {
            let rows = conn.query_rows_blocker(&sql)?;
            Ok(rows.into_iter().map(row_to_blocker).collect())
        })
    }

    /// Record that a blocker has been resolved.
    ///
    /// In the real WAL backend this sets `resolved_unix` for the given id.
    /// The `MemPool` stub returns `BlockerNotFound` to surface the call site.
    ///
    /// # Errors
    ///
    /// Returns `HleError` (2461) when the pool cannot acquire a connection or
    /// the blocker id is not found.
    pub fn resolve(&self, blocker_id: i64, resolved_unix: i64) -> Result<(), HleError> {
        let sql = format!(
            "UPDATE blockers_store SET resolved_unix = {resolved_unix} WHERE id = {blocker_id}"
        );
        with_conn_val(self.pool, |conn| {
            let affected = conn
                .execute_update(&sql)
                .map_err(|e| HleError::new(format!("[2460 BlockerInsert] update failed: {e}")))?;
            if affected == 0 {
                Err(err_blocker_not_found(blocker_id))
            } else {
                Ok(())
            }
        })
    }

    /// Return resolved blockers that are eligible for TTL deletion.
    ///
    /// Uses `now_unix - retention_secs` arithmetic (no literal timestamps).
    ///
    /// # Errors
    ///
    /// Returns `HleError` when the pool fails to acquire a connection.
    pub fn list_ttl_expired(
        &self,
        now_unix: i64,
        retention_secs: i64,
    ) -> Result<Vec<Blocker>, HleError> {
        let threshold = now_unix.saturating_sub(retention_secs);
        let sql = format!(
            "SELECT id, run_id, step_id, blocker_kind, since_unix, expected_resolver_role, \
             resolved_unix FROM blockers_store \
             WHERE resolved_unix IS NOT NULL AND resolved_unix < {threshold} \
             ORDER BY resolved_unix ASC"
        );
        with_conn_val(self.pool, |conn| {
            let rows = conn.query_rows_blocker(&sql)?;
            Ok(rows.into_iter().map(row_to_blocker).collect())
        })
    }
}

// ── TTL helpers ────────────────────────────────────────────────────────────────

/// SQL predicate for blocker TTL deletion (resolved rows only).
///
/// Uses `now_ms - retention_ms` (framework §17.5 — no literal integer timestamps).
#[must_use]
pub fn blocker_ttl_predicate(now_ms: i64, retention_ms: i64) -> String {
    format!(
        "resolved_unix IS NOT NULL AND \
         resolved_unix < ({now_ms} / 1000 - {retention_ms} / 1000)"
    )
}

// ── helpers ────────────────────────────────────────────────────────────────────

fn sql_escape(s: &str) -> String {
    s.replace('\'', "''")
}

// ── tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{
        blocker_ttl_predicate, err_blocker_insert, err_blocker_not_found, Blocker, BlockersStore,
        BLOCKER_KIND_EXTERNAL_DEP, BLOCKER_KIND_HUMAN_APPROVAL, BLOCKER_KIND_RATE_LIMIT,
    };
    use crate::pool::MemPool;

    fn now_unix() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    fn now_ms() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }

    // ── Blocker::new ────────────────────────────────────────────────────────────

    #[test]
    fn blocker_new_is_unresolved() {
        let b = Blocker::new(1, "s1", "human-approval", now_unix(), "operator");
        assert!(!b.is_resolved());
    }

    #[test]
    fn blocker_new_has_zero_id() {
        let b = Blocker::new(1, "s1", "human-approval", now_unix(), "operator");
        assert_eq!(b.id, 0);
    }

    #[test]
    fn blocker_new_sets_run_id() {
        let b = Blocker::new(42, "s1", "human-approval", now_unix(), "operator");
        assert_eq!(b.run_id, 42);
    }

    #[test]
    fn blocker_new_sets_step_id() {
        let b = Blocker::new(1, "step-abc", "human-approval", now_unix(), "operator");
        assert_eq!(b.step_id, "step-abc");
    }

    #[test]
    fn blocker_new_sets_expected_resolver_role() {
        let b = Blocker::new(1, "s1", "human-approval", now_unix(), "qa-lead");
        assert_eq!(b.expected_resolver_role, "qa-lead");
    }

    #[test]
    fn blocker_new_sets_blocker_kind() {
        let b = Blocker::new(1, "s1", "rate-limit", now_unix(), "operator");
        assert_eq!(b.blocker_kind, "rate-limit");
    }

    #[test]
    fn blocker_new_sets_since_unix() {
        let ts = 1_700_000_000_i64;
        let b = Blocker::new(1, "s1", "human-approval", ts, "operator");
        assert_eq!(b.since_unix, ts);
    }

    #[test]
    fn blocker_new_resolved_unix_is_none() {
        let b = Blocker::new(1, "s1", "human-approval", now_unix(), "operator");
        assert!(b.resolved_unix.is_none());
    }

    // ── Blocker::is_resolved ────────────────────────────────────────────────────

    #[test]
    fn blocker_is_resolved_when_resolved_unix_is_set() {
        let mut b = Blocker::new(1, "s1", "human-approval", now_unix(), "operator");
        b.resolved_unix = Some(now_unix() + 100);
        assert!(b.is_resolved());
    }

    #[test]
    fn blocker_is_not_resolved_without_resolved_unix() {
        let b = Blocker::new(1, "s1", "human-approval", now_unix(), "operator");
        assert!(!b.is_resolved());
    }

    // ── Blocker::age_secs ───────────────────────────────────────────────────────

    #[test]
    fn blocker_age_secs_returns_zero_for_future_since() {
        let b = Blocker::new(1, "s1", "human-approval", now_unix() + 9999, "operator");
        assert_eq!(b.age_secs(now_unix()), 0);
    }

    #[test]
    fn blocker_age_secs_returns_elapsed_seconds() {
        let since = 1_000_i64;
        let now = 1_060_i64;
        let b = Blocker::new(1, "s1", "human-approval", since, "operator");
        assert_eq!(b.age_secs(now), 60);
    }

    #[test]
    fn blocker_age_secs_exact_same_time_is_zero() {
        let ts = 1_000_i64;
        let b = Blocker::new(1, "s1", "human-approval", ts, "operator");
        assert_eq!(b.age_secs(ts), 0);
    }

    #[test]
    fn blocker_age_secs_large_elapsed() {
        let since = 0_i64;
        let now = 365 * 86_400_i64; // 1 year
        let b = Blocker::new(1, "s1", "human-approval", since, "operator");
        assert_eq!(b.age_secs(now), 365 * 86_400);
    }

    // ── Blocker::ttl_eligible ───────────────────────────────────────────────────

    #[test]
    fn ttl_eligible_false_for_unresolved() {
        let b = Blocker::new(1, "s1", "human-approval", 1_000, "operator");
        assert!(!b.ttl_eligible(1_000_000, 86_400));
    }

    #[test]
    fn ttl_eligible_true_for_resolved_and_old() {
        let mut b = Blocker::new(1, "s1", "human-approval", 1_000, "operator");
        b.resolved_unix = Some(1_100);
        // now=2000, retention=100 → age_since_resolved=900 ≥ 100
        assert!(b.ttl_eligible(2000, 100));
    }

    #[test]
    fn ttl_eligible_false_for_resolved_but_too_recent() {
        let now = now_unix();
        let mut b = Blocker::new(1, "s1", "human-approval", now - 10, "operator");
        b.resolved_unix = Some(now);
        // retention 1 year: too young
        assert!(!b.ttl_eligible(now, 365 * 86_400));
    }

    // ── Blocker::with_resolution ────────────────────────────────────────────────

    #[test]
    fn with_resolution_sets_resolved_unix() {
        let b = Blocker::new(1, "s1", "human-approval", 1_000, "operator");
        let resolved = b.with_resolution(2_000);
        assert_eq!(resolved.resolved_unix, Some(2_000));
    }

    #[test]
    fn with_resolution_preserves_other_fields() {
        let b = Blocker::new(7, "step-x", "rate-limit", 1_000, "qa-lead");
        let resolved = b.with_resolution(2_000);
        assert_eq!(resolved.run_id, 7);
        assert_eq!(resolved.step_id, "step-x");
        assert_eq!(resolved.blocker_kind, "rate-limit");
    }

    // ── well-known kind constants ───────────────────────────────────────────────

    #[test]
    fn well_known_kind_human_approval_is_stable() {
        assert_eq!(BLOCKER_KIND_HUMAN_APPROVAL, "human-approval");
    }

    #[test]
    fn well_known_kind_external_dep_is_stable() {
        assert_eq!(BLOCKER_KIND_EXTERNAL_DEP, "external-dependency");
    }

    #[test]
    fn well_known_kind_rate_limit_is_stable() {
        assert_eq!(BLOCKER_KIND_RATE_LIMIT, "rate-limit");
    }

    #[test]
    fn well_known_kinds_are_distinct() {
        assert_ne!(BLOCKER_KIND_HUMAN_APPROVAL, BLOCKER_KIND_EXTERNAL_DEP);
        assert_ne!(BLOCKER_KIND_EXTERNAL_DEP, BLOCKER_KIND_RATE_LIMIT);
        assert_ne!(BLOCKER_KIND_HUMAN_APPROVAL, BLOCKER_KIND_RATE_LIMIT);
    }

    // ── BlockersStore ───────────────────────────────────────────────────────────

    #[test]
    fn store_insert_succeeds_against_mem_pool() {
        let pool = MemPool::new();
        let store = BlockersStore::new(&pool);
        let b = Blocker::new(1, "s1", "human-approval", now_unix(), "operator");
        assert!(store.insert(&b).is_ok());
    }

    #[test]
    fn store_list_unresolved_returns_empty_on_mem_pool() {
        let pool = MemPool::new();
        let store = BlockersStore::new(&pool);
        let rows = store.list_unresolved(1);
        assert!(rows.is_ok());
        assert!(rows.unwrap().is_empty());
    }

    #[test]
    fn store_list_all_for_run_returns_empty_on_mem_pool() {
        let pool = MemPool::new();
        let store = BlockersStore::new(&pool);
        let rows = store.list_all_for_run(1);
        assert!(rows.is_ok());
        assert!(rows.unwrap().is_empty());
    }

    #[test]
    fn store_resolve_returns_not_found_on_mem_pool() {
        let pool = MemPool::new();
        let store = BlockersStore::new(&pool);
        let result = store.resolve(99, now_unix());
        // MemPool stub: signals real backend must be wired.
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("2461"));
    }

    #[test]
    fn store_list_ttl_expired_returns_empty_on_mem_pool() {
        let pool = MemPool::new();
        let store = BlockersStore::new(&pool);
        let rows = store.list_ttl_expired(now_unix(), 86_400);
        assert!(rows.is_ok());
        assert!(rows.unwrap().is_empty());
    }

    #[test]
    fn store_insert_all_known_kinds() {
        let pool = MemPool::new();
        let store = BlockersStore::new(&pool);
        for kind in [
            BLOCKER_KIND_HUMAN_APPROVAL,
            BLOCKER_KIND_EXTERNAL_DEP,
            BLOCKER_KIND_RATE_LIMIT,
        ] {
            let b = Blocker::new(1, "s1", kind, now_unix(), "operator");
            assert!(store.insert(&b).is_ok());
        }
    }

    #[test]
    fn store_insert_multiple_blockers_for_same_run() {
        let pool = MemPool::new();
        let store = BlockersStore::new(&pool);
        for i in 0..5 {
            let b = Blocker::new(1, format!("s{i}"), "human-approval", now_unix() + i, "op");
            assert!(store.insert(&b).is_ok());
        }
    }

    // ── TTL predicate ───────────────────────────────────────────────────────────

    #[test]
    fn ttl_predicate_gates_on_resolved_unix_not_null() {
        let pred = blocker_ttl_predicate(now_ms(), 86_400_000);
        assert!(pred.contains("resolved_unix IS NOT NULL"));
    }

    #[test]
    fn ttl_predicate_contains_retention_term() {
        let pred = blocker_ttl_predicate(1_000_000, 3_600_000);
        assert!(pred.contains("3600000"));
    }

    #[test]
    fn ttl_predicate_contains_now_ms() {
        let now = 1_700_000_000_000_i64;
        let pred = blocker_ttl_predicate(now, 86_400_000);
        assert!(pred.contains(&now.to_string()));
    }

    #[test]
    fn ttl_predicate_does_not_expire_unresolved() {
        // The predicate must gate on IS NOT NULL so unresolved rows are excluded.
        let pred = blocker_ttl_predicate(now_ms(), 0);
        assert!(pred.contains("IS NOT NULL"));
    }

    #[test]
    fn ttl_predicate_uses_division_arithmetic() {
        let pred = blocker_ttl_predicate(1_000_000_000, 86_400_000);
        assert!(pred.contains("/ 1000"));
    }

    // ── error helpers ───────────────────────────────────────────────────────────

    #[test]
    fn err_blocker_not_found_contains_code() {
        let err = err_blocker_not_found(5);
        assert!(err.to_string().contains("2461"));
    }

    #[test]
    fn err_blocker_not_found_contains_id() {
        let err = err_blocker_not_found(42);
        assert!(err.to_string().contains("42"));
    }

    #[test]
    fn err_blocker_insert_contains_code() {
        let err = err_blocker_insert("reason");
        assert!(err.to_string().contains("2460"));
    }

    #[test]
    fn err_blocker_insert_contains_detail() {
        let err = err_blocker_insert("disk full");
        assert!(err.to_string().contains("disk full"));
    }

    #[test]
    fn error_codes_2460_and_2461_are_distinct() {
        let e1 = err_blocker_insert("x").to_string();
        let e2 = err_blocker_not_found(1).to_string();
        assert_ne!(e1, e2);
    }

    // ── additional coverage ─────────────────────────────────────────────────────

    #[test]
    fn blocker_eq_self() {
        let b = Blocker::new(1, "s1", "human-approval", 1000, "op");
        assert_eq!(b, b.clone());
    }

    #[test]
    fn blocker_ne_different_step_id() {
        let b1 = Blocker::new(1, "s1", "human-approval", 1000, "op");
        let b2 = Blocker::new(1, "s2", "human-approval", 1000, "op");
        assert_ne!(b1, b2);
    }

    #[test]
    fn blocker_debug_non_empty() {
        let b = Blocker::new(1, "s1", "human-approval", now_unix(), "op");
        assert!(!format!("{b:?}").is_empty());
    }

    #[test]
    fn with_resolution_makes_is_resolved_true() {
        let b = Blocker::new(1, "s1", "human-approval", 1000, "op");
        let resolved = b.with_resolution(2000);
        assert!(resolved.is_resolved());
    }

    #[test]
    fn ttl_eligible_exactly_at_retention_boundary() {
        let mut b = Blocker::new(1, "s1", "human-approval", 0, "op");
        b.resolved_unix = Some(0);
        // retention=1000, now=1000 → age=1000 ≥ 1000 → eligible
        assert!(b.ttl_eligible(1000, 1000));
    }

    #[test]
    fn ttl_eligible_one_second_before_boundary_false() {
        let mut b = Blocker::new(1, "s1", "human-approval", 0, "op");
        b.resolved_unix = Some(0);
        // retention=1000, now=999 → age=999 < 1000 → not eligible
        assert!(!b.ttl_eligible(999, 1000));
    }

    #[test]
    fn store_resolve_error_contains_blocker_id() {
        let pool = MemPool::new();
        let store = BlockersStore::new(&pool);
        let err = store.resolve(77, now_unix()).unwrap_err();
        assert!(err.to_string().contains("77"));
    }

    #[test]
    fn blocker_age_secs_at_same_time_as_since_is_zero() {
        let since = 5_000_i64;
        let b = Blocker::new(1, "s1", "human-approval", since, "op");
        assert_eq!(b.age_secs(since), 0);
    }

    #[test]
    fn ttl_predicate_with_zero_retention_still_has_not_null_guard() {
        let pred = blocker_ttl_predicate(now_ms(), 0);
        assert!(pred.contains("IS NOT NULL"));
    }

    #[test]
    fn store_list_ttl_expired_different_retention_values_ok() {
        let pool = MemPool::new();
        let store = BlockersStore::new(&pool);
        for retention in [0, 3_600, 86_400, 365 * 86_400] {
            assert!(store.list_ttl_expired(now_unix(), retention).is_ok());
        }
    }

    // ── SqlitePool integration tests ────────────────────────────────────────────

    fn make_sqlite_pool_with_run() -> (crate::pool::SqlitePool, i64) {
        use crate::workflow_runs::{WorkflowRun, WorkflowRunsStore};
        let pool = crate::pool::SqlitePool::open_memory().unwrap();
        crate::migrations::run_migrations(&pool).unwrap();
        let runs_store = WorkflowRunsStore::new(&pool);
        let run_id = runs_store
            .insert(&WorkflowRun::new("test-flow", "m0", 1_700_000_000))
            .unwrap();
        (pool, run_id)
    }

    #[test]
    fn sqlite_insert_blocker_returns_nonzero_id() {
        let (pool, run_id) = make_sqlite_pool_with_run();
        let store = BlockersStore::new(&pool);
        let b = Blocker::new(
            run_id,
            "s1",
            BLOCKER_KIND_HUMAN_APPROVAL,
            1_700_000_000,
            "operator",
        );
        let id = store.insert(&b).unwrap();
        assert!(id > 0);
    }

    #[test]
    fn sqlite_list_all_for_run_returns_inserted() {
        let (pool, run_id) = make_sqlite_pool_with_run();
        let store = BlockersStore::new(&pool);
        let b = Blocker::new(
            run_id,
            "step-x",
            BLOCKER_KIND_RATE_LIMIT,
            1_700_000_001,
            "qa-lead",
        );
        store.insert(&b).unwrap();
        let rows = store.list_all_for_run(run_id).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].step_id, "step-x");
        assert_eq!(rows[0].blocker_kind, BLOCKER_KIND_RATE_LIMIT);
    }

    #[test]
    fn sqlite_list_unresolved_excludes_resolved() {
        let (pool, run_id) = make_sqlite_pool_with_run();
        let store = BlockersStore::new(&pool);
        let b1 = Blocker::new(
            run_id,
            "s1",
            BLOCKER_KIND_HUMAN_APPROVAL,
            1_700_000_000,
            "op",
        );
        let b2 = Blocker::new(run_id, "s2", BLOCKER_KIND_EXTERNAL_DEP, 1_700_000_001, "op");
        let id1 = store.insert(&b1).unwrap();
        store.insert(&b2).unwrap();
        // Resolve b1.
        store.resolve(id1, 1_700_001_000).unwrap();
        let unresolved = store.list_unresolved(run_id).unwrap();
        assert_eq!(unresolved.len(), 1);
        assert_eq!(unresolved[0].step_id, "s2");
    }

    #[test]
    fn sqlite_resolve_makes_blocker_resolved() {
        let (pool, run_id) = make_sqlite_pool_with_run();
        let store = BlockersStore::new(&pool);
        let b = Blocker::new(
            run_id,
            "s1",
            BLOCKER_KIND_HUMAN_APPROVAL,
            1_700_000_000,
            "op",
        );
        let id = store.insert(&b).unwrap();
        assert!(store.resolve(id, 1_700_001_000).is_ok());
        let all = store.list_all_for_run(run_id).unwrap();
        assert!(all[0].is_resolved());
        assert_eq!(all[0].resolved_unix, Some(1_700_001_000));
    }

    #[test]
    fn sqlite_resolve_nonexistent_returns_not_found() {
        let (pool, run_id) = make_sqlite_pool_with_run();
        let store = BlockersStore::new(&pool);
        // Insert to ensure table exists; resolve a non-existent id.
        store
            .insert(&Blocker::new(run_id, "s1", "kind", 1, "op"))
            .unwrap();
        let result = store.resolve(999_999, 1_700_001_000);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("2461"));
    }

    #[test]
    fn sqlite_list_ttl_expired_returns_old_resolved_rows() {
        let (pool, run_id) = make_sqlite_pool_with_run();
        let store = BlockersStore::new(&pool);
        let b = Blocker::new(run_id, "s1", BLOCKER_KIND_HUMAN_APPROVAL, 1_000, "op");
        let id = store.insert(&b).unwrap();
        // Resolve at ts=1_100.
        store.resolve(id, 1_100).unwrap();
        // TTL: now=2_000_000, retention=1_000 → threshold=1_999_000. Row qualifies.
        let expired = store.list_ttl_expired(2_000_000, 1_000).unwrap();
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].step_id, "s1");
    }
}
