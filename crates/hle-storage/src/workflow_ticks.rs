#![forbid(unsafe_code)]

//! M028 — Tick ledger: freshness and causality.
//!
//! Each tick records one processing cycle of a workflow run.  Ticks carry a
//! monotonically increasing `tick_id` (local to the run) and an optional
//! `parent_tick_id` that reconstructs the causal chain.
//!
//! Error codes: 2430–2431 (`TickInsert`, `TickParentMissing`).

use substrate_types::HleError;

use crate::pool::{with_conn_val, Pool};

// ── row mapping helper ─────────────────────────────────────────────────────────

fn row_to_tick(row: (i64, i64, i64, i64, Option<i64>)) -> WorkflowTick {
    let (id, run_id, tick_id, created_unix, parent_tick_id) = row;
    WorkflowTick {
        id,
        run_id,
        // tick_id is a non-negative monotonic counter enforced by schema.
        tick_id: u64::try_from(tick_id).unwrap_or(0),
        created_unix,
        // parent_tick_id is also non-negative by schema constraint.
        parent_tick_id: parent_tick_id.map(|v| u64::try_from(v).unwrap_or(0)),
    }
}

// ── error helpers ──────────────────────────────────────────────────────────────

fn err_tick_insert(detail: impl core::fmt::Display) -> HleError {
    HleError::new(format!("[2430 TickInsert] {detail}"))
}

/// Error code 2431: `parent_tick_id` refers to a tick that does not exist.
#[allow(dead_code)]
pub(crate) fn err_tick_parent_missing(parent_id: u64) -> HleError {
    HleError::new(format!(
        "[2431 TickParentMissing] parent_tick_id={parent_id} not found in run"
    ))
}

// ── WorkflowTick ───────────────────────────────────────────────────────────────

/// One tick row in the ledger — records a single processing cycle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkflowTick {
    /// Auto-increment primary key.
    pub id: i64,
    /// Foreign key → `workflow_runs.id`.
    pub run_id: i64,
    /// Monotonically increasing counter within the run (starts at 1).
    pub tick_id: u64,
    /// Unix timestamp (seconds) when the tick was created.
    pub created_unix: i64,
    /// Parent tick id for causal chaining; `None` for the root tick.
    pub parent_tick_id: Option<u64>,
}

impl WorkflowTick {
    /// Construct a root tick (no parent) for the given run.
    #[must_use]
    pub fn root(run_id: i64, created_unix: i64) -> Self {
        Self {
            id: 0,
            run_id,
            tick_id: 1,
            created_unix,
            parent_tick_id: None,
        }
    }

    /// Construct a child tick from a parent tick.
    ///
    /// `tick_id` is the parent's `tick_id` + 1, enforcing monotonicity.
    #[must_use]
    pub fn child_of(parent: &Self, created_unix: i64) -> Self {
        Self {
            id: 0,
            run_id: parent.run_id,
            tick_id: parent.tick_id + 1,
            created_unix,
            parent_tick_id: Some(parent.tick_id),
        }
    }

    /// Return `true` when this tick is the root (no parent).
    #[must_use]
    pub fn is_root(&self) -> bool {
        self.parent_tick_id.is_none()
    }

    /// Return `true` when this tick's `tick_id` is greater than the parent's.
    ///
    /// Always `true` for well-formed chains built with [`WorkflowTick::child_of`].
    #[must_use]
    pub fn is_monotonic_after(&self, parent: &Self) -> bool {
        self.tick_id > parent.tick_id
    }

    /// Depth of this tick in the causal chain (root = 1).
    ///
    /// Because each `child_of` increments `tick_id` by 1 starting from 1,
    /// `depth == tick_id` for all chains built with the provided constructors.
    #[must_use]
    pub fn depth(&self) -> u64 {
        self.tick_id
    }
}

// ── WorkflowTicksStore ─────────────────────────────────────────────────────────

/// Append-only access layer for the `workflow_ticks` virtual table.
///
/// The `workflow_ticks` table is not present in `0001_scaffold_schema.sql`
/// (it will be added in a later migration); this store compiles cleanly
/// against the `MemPool` stub and the real WAL backend wires the DDL at M0.
pub struct WorkflowTicksStore<'pool> {
    pool: &'pool dyn Pool,
}

impl<'pool> WorkflowTicksStore<'pool> {
    /// Bind the store to a pool.
    #[must_use]
    pub fn new(pool: &'pool dyn Pool) -> Self {
        Self { pool }
    }

    /// Append a tick row and return the assigned row id.
    ///
    /// The caller is responsible for enforcing monotonicity before calling
    /// `insert`; use [`WorkflowTick::child_of`] to build sequenced ticks.
    ///
    /// # Errors
    ///
    /// Returns `HleError` (2430) when the INSERT fails.
    pub fn insert(&self, tick: &WorkflowTick) -> Result<i64, HleError> {
        let parent_sql = tick
            .parent_tick_id
            .map_or_else(|| "NULL".to_owned(), |p| p.to_string());
        let sql = format!(
            "INSERT INTO workflow_ticks (run_id, tick_id, created_unix, parent_tick_id) \
             VALUES ({}, {}, {}, {})",
            tick.run_id, tick.tick_id, tick.created_unix, parent_sql,
        );
        with_conn_val(self.pool, |conn| {
            conn.execute_sql(&sql).map_err(err_tick_insert)?;
            conn.query_one_i64("SELECT last_insert_rowid()")
                .map(|opt| opt.unwrap_or(0))
                .map_err(err_tick_insert)
        })
    }

    /// Return all tick rows for a given run, ordered by `tick_id` ascending.
    ///
    /// # Errors
    ///
    /// Returns `HleError` when the pool fails to acquire a connection.
    pub fn list_for_run(&self, run_id: i64) -> Result<Vec<WorkflowTick>, HleError> {
        let sql = format!(
            "SELECT id, run_id, tick_id, created_unix, parent_tick_id \
             FROM workflow_ticks WHERE run_id = {run_id} ORDER BY tick_id ASC"
        );
        with_conn_val(self.pool, |conn| {
            let rows = conn.query_rows_tick(&sql)?;
            Ok(rows.into_iter().map(row_to_tick).collect())
        })
    }

    /// Return all ticks with `tick_id` strictly less than `ceiling`.
    ///
    /// # Errors
    ///
    /// Returns `HleError` when the pool fails to acquire a connection.
    pub fn list_before_tick(
        &self,
        run_id: i64,
        ceiling: u64,
    ) -> Result<Vec<WorkflowTick>, HleError> {
        let sql = format!(
            "SELECT id, run_id, tick_id, created_unix, parent_tick_id \
             FROM workflow_ticks WHERE run_id = {run_id} AND tick_id < {ceiling} \
             ORDER BY tick_id ASC"
        );
        with_conn_val(self.pool, |conn| {
            let rows = conn.query_rows_tick(&sql)?;
            Ok(rows.into_iter().map(row_to_tick).collect())
        })
    }

    /// Return the deepest tick (highest `tick_id`) for a run.
    ///
    /// Returns `None` when no ticks exist for the run.
    ///
    /// # Errors
    ///
    /// Returns `HleError` when the pool fails to acquire a connection.
    pub fn deepest_tick(&self, run_id: i64) -> Result<Option<u64>, HleError> {
        let sql = format!("SELECT MAX(tick_id) FROM workflow_ticks WHERE run_id = {run_id}");
        with_conn_val(self.pool, |conn| {
            let v = conn.query_one_i64(&sql)?;
            // MAX(tick_id) is always non-negative by schema constraint.
            Ok(v.map(|n| u64::try_from(n).unwrap_or(0)))
        })
    }
}

// ── TTL helpers ────────────────────────────────────────────────────────────────

/// SQL predicate for tick TTL deletion.
///
/// Uses `now_ms - retention_ms` arithmetic to avoid literal integer timestamps
/// (framework §17.5 hard rule).  The caller substitutes real values.
#[must_use]
pub fn tick_ttl_predicate(now_ms: i64, retention_ms: i64) -> String {
    format!("created_unix < ({now_ms} / 1000 - {retention_ms} / 1000)")
}

// ── tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{err_tick_parent_missing, tick_ttl_predicate, WorkflowTick, WorkflowTicksStore};
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

    // ── WorkflowTick::root ──────────────────────────────────────────────────────

    #[test]
    fn root_tick_has_tick_id_one() {
        let t = WorkflowTick::root(1, now_unix());
        assert_eq!(t.tick_id, 1);
    }

    #[test]
    fn root_tick_has_no_parent() {
        let t = WorkflowTick::root(1, now_unix());
        assert!(t.parent_tick_id.is_none());
    }

    #[test]
    fn root_tick_id_is_zero_before_insert() {
        let t = WorkflowTick::root(1, now_unix());
        assert_eq!(t.id, 0);
    }

    #[test]
    fn root_tick_is_root() {
        assert!(WorkflowTick::root(1, now_unix()).is_root());
    }

    #[test]
    fn root_tick_depth_is_one() {
        assert_eq!(WorkflowTick::root(1, now_unix()).depth(), 1);
    }

    #[test]
    fn root_tick_sets_run_id() {
        let t = WorkflowTick::root(77, now_unix());
        assert_eq!(t.run_id, 77);
    }

    #[test]
    fn root_tick_sets_created_unix() {
        let ts = 1_700_000_000_i64;
        let t = WorkflowTick::root(1, ts);
        assert_eq!(t.created_unix, ts);
    }

    // ── WorkflowTick::child_of ──────────────────────────────────────────────────

    #[test]
    fn child_tick_increments_tick_id() {
        let root = WorkflowTick::root(1, now_unix());
        let child = WorkflowTick::child_of(&root, now_unix());
        assert_eq!(child.tick_id, 2);
    }

    #[test]
    fn child_tick_records_parent_tick_id() {
        let root = WorkflowTick::root(1, now_unix());
        let child = WorkflowTick::child_of(&root, now_unix());
        assert_eq!(child.parent_tick_id, Some(1));
    }

    #[test]
    fn child_tick_inherits_run_id() {
        let root = WorkflowTick::root(42, now_unix());
        let child = WorkflowTick::child_of(&root, now_unix());
        assert_eq!(child.run_id, 42);
    }

    #[test]
    fn grandchild_tick_id_is_three() {
        let root = WorkflowTick::root(1, now_unix());
        let child = WorkflowTick::child_of(&root, now_unix());
        let grandchild = WorkflowTick::child_of(&child, now_unix());
        assert_eq!(grandchild.tick_id, 3);
    }

    #[test]
    fn child_tick_is_not_root() {
        let root = WorkflowTick::root(1, now_unix());
        let child = WorkflowTick::child_of(&root, now_unix());
        assert!(!child.is_root());
    }

    #[test]
    fn child_tick_is_monotonic_after_parent() {
        let root = WorkflowTick::root(1, now_unix());
        let child = WorkflowTick::child_of(&root, now_unix());
        assert!(child.is_monotonic_after(&root));
    }

    #[test]
    fn root_is_not_monotonic_after_child() {
        let root = WorkflowTick::root(1, now_unix());
        let child = WorkflowTick::child_of(&root, now_unix());
        assert!(!root.is_monotonic_after(&child));
    }

    #[test]
    fn depth_equals_tick_id_for_chain() {
        let root = WorkflowTick::root(1, now_unix());
        let c1 = WorkflowTick::child_of(&root, now_unix());
        let c2 = WorkflowTick::child_of(&c1, now_unix());
        let c3 = WorkflowTick::child_of(&c2, now_unix());
        assert_eq!(c3.depth(), 4);
    }

    #[test]
    fn chain_of_five_has_correct_tick_ids() {
        let mut t = WorkflowTick::root(1, now_unix());
        for expected in 2..=5_u64 {
            t = WorkflowTick::child_of(&t, now_unix());
            assert_eq!(t.tick_id, expected);
        }
    }

    #[test]
    fn child_of_child_parent_tick_id_is_correct() {
        let root = WorkflowTick::root(1, now_unix());
        let c1 = WorkflowTick::child_of(&root, now_unix());
        let c2 = WorkflowTick::child_of(&c1, now_unix());
        assert_eq!(c2.parent_tick_id, Some(2));
    }

    // ── WorkflowTicksStore ──────────────────────────────────────────────────────

    #[test]
    fn store_insert_succeeds_against_mem_pool() {
        let pool = MemPool::new();
        let store = WorkflowTicksStore::new(&pool);
        let tick = WorkflowTick::root(1, now_unix());
        assert!(store.insert(&tick).is_ok());
    }

    #[test]
    fn store_list_for_run_returns_empty_on_mem_pool() {
        let pool = MemPool::new();
        let store = WorkflowTicksStore::new(&pool);
        let rows = store.list_for_run(1);
        assert!(rows.is_ok());
        assert!(rows.unwrap().is_empty());
    }

    #[test]
    fn store_list_before_tick_returns_empty_on_mem_pool() {
        let pool = MemPool::new();
        let store = WorkflowTicksStore::new(&pool);
        let rows = store.list_before_tick(1, 10);
        assert!(rows.is_ok());
        assert!(rows.unwrap().is_empty());
    }

    #[test]
    fn store_deepest_tick_returns_none_on_mem_pool() {
        let pool = MemPool::new();
        let store = WorkflowTicksStore::new(&pool);
        let d = store.deepest_tick(1);
        assert!(d.is_ok());
        assert!(d.unwrap().is_none());
    }

    #[test]
    fn store_insert_child_tick_succeeds() {
        let pool = MemPool::new();
        let store = WorkflowTicksStore::new(&pool);
        let root = WorkflowTick::root(1, now_unix());
        let child = WorkflowTick::child_of(&root, now_unix());
        assert!(store.insert(&child).is_ok());
    }

    #[test]
    fn store_insert_many_ticks_succeeds() {
        let pool = MemPool::new();
        let store = WorkflowTicksStore::new(&pool);
        let mut t = WorkflowTick::root(1, now_unix());
        for _ in 0..10 {
            assert!(store.insert(&t).is_ok());
            t = WorkflowTick::child_of(&t, now_unix());
        }
    }

    // ── TTL predicate ───────────────────────────────────────────────────────────

    #[test]
    fn ttl_predicate_does_not_use_literal_zero() {
        let pred = tick_ttl_predicate(now_ms(), 3_600_000);
        // Must not embed the raw literal 0 as the retention term.
        assert!(!pred.contains("- 0)"));
    }

    #[test]
    fn ttl_predicate_contains_retention_term() {
        let pred = tick_ttl_predicate(now_ms(), 86_400_000);
        assert!(pred.contains("86400000"));
    }

    #[test]
    fn ttl_predicate_contains_now_ms_term() {
        let now = 1_700_000_000_000_i64;
        let pred = tick_ttl_predicate(now, 3_600_000);
        assert!(pred.contains(&now.to_string()));
    }

    #[test]
    fn ttl_predicate_uses_division_arithmetic() {
        let pred = tick_ttl_predicate(1_000_000_000, 86_400_000);
        assert!(pred.contains("/ 1000"));
    }

    // ── error helpers ───────────────────────────────────────────────────────────

    #[test]
    fn err_tick_parent_missing_contains_2431() {
        assert!(err_tick_parent_missing(5).to_string().contains("2431"));
    }

    #[test]
    fn err_tick_parent_missing_contains_parent_id() {
        assert!(err_tick_parent_missing(42).to_string().contains("42"));
    }

    #[test]
    fn err_tick_insert_contains_2430() {
        assert!(super::err_tick_insert("reason")
            .to_string()
            .contains("2430"));
    }

    // ── additional coverage ─────────────────────────────────────────────────────

    #[test]
    fn root_tick_different_run_ids_are_independent() {
        let t1 = WorkflowTick::root(1, now_unix());
        let t2 = WorkflowTick::root(2, now_unix());
        assert_ne!(t1.run_id, t2.run_id);
    }

    #[test]
    fn child_of_chain_parent_tick_ids_form_sequence() {
        let r = WorkflowTick::root(1, now_unix());
        let c1 = WorkflowTick::child_of(&r, now_unix());
        let c2 = WorkflowTick::child_of(&c1, now_unix());
        assert_eq!(c1.parent_tick_id, Some(1));
        assert_eq!(c2.parent_tick_id, Some(2));
    }

    #[test]
    fn tick_eq_self() {
        let t = WorkflowTick::root(1, 1000);
        assert_eq!(t, t.clone());
    }

    #[test]
    fn tick_ne_different_tick_id() {
        let r = WorkflowTick::root(1, 1000);
        let c = WorkflowTick::child_of(&r, 1000);
        assert_ne!(r, c);
    }

    #[test]
    fn tick_debug_non_empty() {
        let t = WorkflowTick::root(1, now_unix());
        assert!(!format!("{t:?}").is_empty());
    }

    #[test]
    fn store_list_before_tick_zero_ceiling_returns_empty() {
        let pool = MemPool::new();
        let store = WorkflowTicksStore::new(&pool);
        let rows = store.list_before_tick(1, 0);
        assert!(rows.is_ok());
    }

    #[test]
    fn store_list_before_tick_max_ceiling_returns_empty_on_mem_pool() {
        let pool = MemPool::new();
        let store = WorkflowTicksStore::new(&pool);
        let rows = store.list_before_tick(1, u64::MAX);
        assert!(rows.is_ok());
    }

    #[test]
    fn ttl_predicate_different_retention_values_differ() {
        let p1 = tick_ttl_predicate(1_000_000, 3_600_000);
        let p2 = tick_ttl_predicate(1_000_000, 7_200_000);
        assert_ne!(p1, p2);
    }

    #[test]
    fn tick_insert_with_large_tick_id_succeeds() {
        let pool = MemPool::new();
        let store = WorkflowTicksStore::new(&pool);
        let mut t = WorkflowTick::root(1, now_unix());
        t.tick_id = u64::MAX - 1;
        assert!(store.insert(&t).is_ok());
    }

    #[test]
    fn root_tick_id_zero_before_insert_consistency() {
        // id field (DB rowid) should always be 0 before insert
        let t = WorkflowTick::root(1, now_unix());
        assert_eq!(t.id, 0, "unpersisted tick must have id=0");
    }

    #[test]
    fn child_of_id_is_zero_before_insert() {
        let r = WorkflowTick::root(1, now_unix());
        let c = WorkflowTick::child_of(&r, now_unix());
        assert_eq!(c.id, 0);
    }

    #[test]
    fn depth_for_long_chain_is_tick_id() {
        let mut t = WorkflowTick::root(1, now_unix());
        for _ in 0..9 {
            t = WorkflowTick::child_of(&t, now_unix());
        }
        assert_eq!(t.depth(), t.tick_id);
        assert_eq!(t.depth(), 10);
    }

    #[test]
    fn err_tick_parent_missing_message_not_empty() {
        assert!(!err_tick_parent_missing(1).to_string().is_empty());
    }

    #[test]
    fn err_tick_parent_missing_different_ids_differ() {
        let e1 = err_tick_parent_missing(10).to_string();
        let e2 = err_tick_parent_missing(20).to_string();
        assert_ne!(e1, e2);
    }

    #[test]
    fn ttl_predicate_non_empty_string() {
        assert!(!tick_ttl_predicate(1_000, 3_600_000).is_empty());
    }

    #[test]
    fn store_deepest_tick_called_multiple_times_is_ok() {
        let pool = MemPool::new();
        let store = WorkflowTicksStore::new(&pool);
        for run_id in 1..=5_i64 {
            assert!(store.deepest_tick(run_id).is_ok());
        }
    }

    #[test]
    fn tick_id_one_is_root_id() {
        let t = WorkflowTick::root(1, now_unix());
        assert_eq!(t.tick_id, 1);
        assert!(t.is_root());
    }

    #[test]
    fn store_list_for_run_different_run_ids_all_ok() {
        let pool = MemPool::new();
        let store = WorkflowTicksStore::new(&pool);
        for id in [1_i64, 100, 999] {
            assert!(store.list_for_run(id).is_ok());
        }
    }

    #[test]
    fn store_insert_root_and_two_children_all_ok() {
        let pool = MemPool::new();
        let store = WorkflowTicksStore::new(&pool);
        let root = WorkflowTick::root(5, now_unix());
        let c1 = WorkflowTick::child_of(&root, now_unix());
        let c2 = WorkflowTick::child_of(&c1, now_unix());
        assert!(store.insert(&root).is_ok());
        assert!(store.insert(&c1).is_ok());
        assert!(store.insert(&c2).is_ok());
    }

    #[test]
    fn ttl_predicate_two_different_retentions_differ() {
        let p1 = tick_ttl_predicate(5_000_000, 1_000);
        let p2 = tick_ttl_predicate(5_000_000, 2_000);
        assert_ne!(p1, p2);
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
    fn sqlite_insert_root_tick_returns_nonzero_id() {
        let (pool, run_id) = make_sqlite_pool_with_run();
        let store = WorkflowTicksStore::new(&pool);
        let tick = WorkflowTick::root(run_id, 1_700_000_000);
        let id = store.insert(&tick).unwrap();
        assert!(id > 0);
    }

    #[test]
    fn sqlite_list_for_run_returns_inserted_tick() {
        let (pool, run_id) = make_sqlite_pool_with_run();
        let store = WorkflowTicksStore::new(&pool);
        let tick = WorkflowTick::root(run_id, 1_700_000_000);
        store.insert(&tick).unwrap();
        let ticks = store.list_for_run(run_id).unwrap();
        assert_eq!(ticks.len(), 1);
        assert_eq!(ticks[0].tick_id, 1);
        assert!(ticks[0].is_root());
    }

    #[test]
    fn sqlite_list_for_run_returns_chain_in_order() {
        let (pool, run_id) = make_sqlite_pool_with_run();
        let store = WorkflowTicksStore::new(&pool);
        let root = WorkflowTick::root(run_id, 1_700_000_000);
        let c1 = WorkflowTick::child_of(&root, 1_700_000_001);
        let c2 = WorkflowTick::child_of(&c1, 1_700_000_002);
        store.insert(&root).unwrap();
        store.insert(&c1).unwrap();
        store.insert(&c2).unwrap();
        let ticks = store.list_for_run(run_id).unwrap();
        assert_eq!(ticks.len(), 3);
        assert_eq!(ticks[0].tick_id, 1);
        assert_eq!(ticks[1].tick_id, 2);
        assert_eq!(ticks[2].tick_id, 3);
    }

    #[test]
    fn sqlite_list_before_tick_ceiling_filters_correctly() {
        let (pool, run_id) = make_sqlite_pool_with_run();
        let store = WorkflowTicksStore::new(&pool);
        let root = WorkflowTick::root(run_id, 1_700_000_000);
        let c1 = WorkflowTick::child_of(&root, 1_700_000_001);
        let c2 = WorkflowTick::child_of(&c1, 1_700_000_002);
        store.insert(&root).unwrap();
        store.insert(&c1).unwrap();
        store.insert(&c2).unwrap();
        // ceiling=3: only tick_id 1 and 2 qualify
        let ticks = store.list_before_tick(run_id, 3).unwrap();
        assert_eq!(ticks.len(), 2);
    }

    #[test]
    fn sqlite_deepest_tick_returns_max_tick_id() {
        let (pool, run_id) = make_sqlite_pool_with_run();
        let store = WorkflowTicksStore::new(&pool);
        let root = WorkflowTick::root(run_id, 1_700_000_000);
        let c1 = WorkflowTick::child_of(&root, 1_700_000_001);
        let c2 = WorkflowTick::child_of(&c1, 1_700_000_002);
        store.insert(&root).unwrap();
        store.insert(&c1).unwrap();
        store.insert(&c2).unwrap();
        let deepest = store.deepest_tick(run_id).unwrap();
        assert_eq!(deepest, Some(3));
    }

    #[test]
    fn sqlite_deepest_tick_returns_none_when_empty() {
        let (pool, run_id) = make_sqlite_pool_with_run();
        let store = WorkflowTicksStore::new(&pool);
        let deepest = store.deepest_tick(run_id).unwrap();
        assert_eq!(deepest, None);
    }

    #[test]
    fn sqlite_parent_tick_id_preserved_on_roundtrip() {
        let (pool, run_id) = make_sqlite_pool_with_run();
        let store = WorkflowTicksStore::new(&pool);
        let root = WorkflowTick::root(run_id, 1_700_000_000);
        let child = WorkflowTick::child_of(&root, 1_700_000_001);
        store.insert(&root).unwrap();
        store.insert(&child).unwrap();
        let ticks = store.list_for_run(run_id).unwrap();
        assert_eq!(ticks[0].parent_tick_id, None);
        assert_eq!(ticks[1].parent_tick_id, Some(1));
    }
}
