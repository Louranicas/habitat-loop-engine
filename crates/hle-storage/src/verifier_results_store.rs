#![forbid(unsafe_code)]

//! M030 — Verifier verdict ledger (append-only).
//!
//! [`VerifierResult`] records one verifier verdict per step per run.  The
//! store is strictly append-only: no `UPDATE` or `DELETE` is exposed.
//! Corrections are appended as new rows.
//!
//! Verdicts align with the verifier authority defined in `substrate-verify`:
//! `Pass | Fail | AwaitingHuman`.
//!
//! Error codes: 2450–2451 (`VerifierInsert`, `VerifierVerdictInvalid`).

use substrate_types::HleError;

use crate::pool::{with_conn_val, Pool};

// ── row mapping helper ─────────────────────────────────────────────────────────

fn row_to_verifier_result(
    row: (i64, i64, String, String, String, String),
) -> Result<VerifierResult, HleError> {
    let (id, run_id, step_id, verdict_str, receipt_sha, verifier_version) = row;
    let verdict = Verdict::parse_str(&verdict_str)?;
    Ok(VerifierResult {
        id,
        run_id,
        step_id,
        verdict,
        receipt_sha,
        verifier_version,
    })
}

// ── error helpers ──────────────────────────────────────────────────────────────

fn err_verifier_insert(detail: impl core::fmt::Display) -> HleError {
    HleError::new(format!("[2450 VerifierInsert] {detail}"))
}

fn err_verifier_verdict_invalid(v: &str) -> HleError {
    HleError::new(format!(
        "[2451 VerifierVerdictInvalid] unknown verdict: {v}"
    ))
}

// ── Verdict ────────────────────────────────────────────────────────────────────

/// Verifier-authoritative verdict for one step.
///
/// Only the verifier may produce `Pass` from an executor receipt — callers
/// must never construct `Pass` without a prior `verify_step` call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Pass,
    Fail,
    AwaitingHuman,
}

impl Verdict {
    /// Wire string used in SQL and JSONL.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::AwaitingHuman => "AWAITING_HUMAN",
        }
    }

    /// Parse from the wire string.
    ///
    /// # Errors
    ///
    /// Returns `HleError` (2451) for unknown verdict strings.
    pub fn parse_str(s: &str) -> Result<Self, HleError> {
        match s {
            "PASS" => Ok(Self::Pass),
            "FAIL" => Ok(Self::Fail),
            "AWAITING_HUMAN" => Ok(Self::AwaitingHuman),
            other => Err(err_verifier_verdict_invalid(other)),
        }
    }

    /// Return `true` for terminal verdicts (Pass, Fail).
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Pass | Self::Fail)
    }

    /// Return `true` when the verdict indicates success.
    #[must_use]
    pub const fn is_pass(self) -> bool {
        matches!(self, Self::Pass)
    }

    /// Return `true` when the verdict indicates failure.
    #[must_use]
    pub const fn is_fail(self) -> bool {
        matches!(self, Self::Fail)
    }

    /// All valid wire strings (for round-trip testing).
    #[must_use]
    pub const fn all_strs() -> [&'static str; 3] {
        ["PASS", "FAIL", "AWAITING_HUMAN"]
    }
}

// ── VerifierResult ─────────────────────────────────────────────────────────────

/// One row in the verifier results ledger.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifierResult {
    /// Auto-increment primary key.
    pub id: i64,
    /// Foreign key → `workflow_runs.id`.
    pub run_id: i64,
    /// Step identifier within the workflow definition.
    pub step_id: String,
    /// Verifier-issued verdict for this step.
    pub verdict: Verdict,
    /// SHA-256 hex of the receipt that was verified.
    pub receipt_sha: String,
    /// Version tag of the verifier that produced this result.
    pub verifier_version: String,
}

impl VerifierResult {
    /// Construct a new verifier result (id 0 = not yet persisted).
    #[must_use]
    pub fn new(
        run_id: i64,
        step_id: impl Into<String>,
        verdict: Verdict,
        receipt_sha: impl Into<String>,
        verifier_version: impl Into<String>,
    ) -> Self {
        Self {
            id: 0,
            run_id,
            step_id: step_id.into(),
            verdict,
            receipt_sha: receipt_sha.into(),
            verifier_version: verifier_version.into(),
        }
    }

    /// Return `true` when the verifier version string is non-empty.
    #[must_use]
    pub fn has_version(&self) -> bool {
        !self.verifier_version.is_empty()
    }

    /// Stable receipt identity: `"{run_id}/{step_id}/{receipt_sha}"`.
    #[must_use]
    pub fn receipt_key(&self) -> String {
        format!("{}/{}/{}", self.run_id, self.step_id, self.receipt_sha)
    }
}

// ── VerifierResultsStore ───────────────────────────────────────────────────────

/// Append-only access layer for the `verifier_results_store` table.
pub struct VerifierResultsStore<'pool> {
    pool: &'pool dyn Pool,
}

impl<'pool> VerifierResultsStore<'pool> {
    /// Bind the store to a pool.
    #[must_use]
    pub fn new(pool: &'pool dyn Pool) -> Self {
        Self { pool }
    }

    /// Append a verifier result row and return the assigned row id.
    ///
    /// # Errors
    ///
    /// Returns `HleError` (2450) when the INSERT fails.
    pub fn append(&self, result: &VerifierResult) -> Result<i64, HleError> {
        let sql = format!(
            "INSERT INTO verifier_results_store \
             (run_id, step_id, verdict, receipt_sha, verifier_version) \
             VALUES ({}, '{}', '{}', '{}', '{}')",
            result.run_id,
            sql_escape(&result.step_id),
            result.verdict.as_str(),
            sql_escape(&result.receipt_sha),
            sql_escape(&result.verifier_version),
        );
        with_conn_val(self.pool, |conn| {
            conn.execute_sql(&sql).map_err(err_verifier_insert)?;
            conn.query_one_i64("SELECT last_insert_rowid()")
                .map(|opt| opt.unwrap_or(0))
                .map_err(err_verifier_insert)
        })
    }

    /// Return all verifier results for a run, ordered by id ascending.
    ///
    /// # Errors
    ///
    /// Returns `HleError` when the pool fails to acquire a connection.
    pub fn list_for_run(&self, run_id: i64) -> Result<Vec<VerifierResult>, HleError> {
        let sql = format!(
            "SELECT id, run_id, step_id, verdict, receipt_sha, verifier_version \
             FROM verifier_results_store WHERE run_id = {run_id} ORDER BY id ASC"
        );
        with_conn_val(self.pool, |conn| {
            let rows = conn.query_rows_verifier(&sql)?;
            rows.into_iter().map(row_to_verifier_result).collect()
        })
    }

    /// Return the most recent verdict for a given step within a run.
    ///
    /// Returns `None` when no result has been recorded yet.
    ///
    /// # Errors
    ///
    /// Returns `HleError` when the pool fails to acquire a connection.
    pub fn latest_verdict_for_step(
        &self,
        run_id: i64,
        step_id: &str,
    ) -> Result<Option<Verdict>, HleError> {
        // Use query_rows_verifier with a 6-col projection; col 2 carries the verdict string.
        let inner_sql = format!(
            "SELECT 0,0,verdict,'','','' FROM verifier_results_store \
             WHERE run_id = {run_id} AND step_id = '{}' ORDER BY id DESC LIMIT 1",
            sql_escape(step_id),
        );
        with_conn_val(self.pool, |conn| {
            let rows = conn.query_rows_verifier(&inner_sql)?;
            match rows.into_iter().next() {
                None => Ok(None),
                Some(row) => {
                    let verdict = Verdict::parse_str(&row.2)?;
                    Ok(Some(verdict))
                }
            }
        })
    }

    /// Return all results for a specific step across the run, ordered by id.
    ///
    /// # Errors
    ///
    /// Returns `HleError` when the pool fails to acquire a connection.
    pub fn list_for_step(
        &self,
        run_id: i64,
        step_id: &str,
    ) -> Result<Vec<VerifierResult>, HleError> {
        let sql = format!(
            "SELECT id, run_id, step_id, verdict, receipt_sha, verifier_version \
             FROM verifier_results_store \
             WHERE run_id = {run_id} AND step_id = '{}' ORDER BY id ASC",
            sql_escape(step_id),
        );
        with_conn_val(self.pool, |conn| {
            let rows = conn.query_rows_verifier(&sql)?;
            rows.into_iter().map(row_to_verifier_result).collect()
        })
    }

    /// Compute the pass rate (0.0–1.0) for a run from the stored rows.
    ///
    /// Returns `None` when there are no rows (avoids division by zero).
    ///
    /// # Errors
    ///
    /// Returns `HleError` when the pool fails to acquire a connection.
    pub fn pass_rate(&self, run_id: i64) -> Result<Option<f64>, HleError> {
        let sql_total =
            format!("SELECT COUNT(*) FROM verifier_results_store WHERE run_id = {run_id}");
        let sql_passes = format!(
            "SELECT COUNT(*) FROM verifier_results_store \
             WHERE run_id = {run_id} AND verdict = 'PASS'"
        );
        with_conn_val(self.pool, |conn| {
            let total = conn.query_one_i64(&sql_total)?.unwrap_or(0);
            if total == 0 {
                return Ok(None);
            }
            let passes = conn.query_one_i64(&sql_passes)?.unwrap_or(0);
            #[allow(clippy::cast_precision_loss)]
            Ok(Some(passes as f64 / total as f64))
        })
    }
}

// ── TTL helpers ────────────────────────────────────────────────────────────────

/// SQL predicate for verifier results TTL deletion.
///
/// Uses `now_ms - retention_ms` arithmetic (framework §17.5 — no literal ints).
#[must_use]
pub fn verifier_ttl_predicate(now_ms: i64, retention_ms: i64) -> String {
    format!("created_unix < ({now_ms} / 1000 - {retention_ms} / 1000)")
}

// ── helpers ────────────────────────────────────────────────────────────────────

fn sql_escape(s: &str) -> String {
    s.replace('\'', "''")
}

// ── tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{verifier_ttl_predicate, Verdict, VerifierResult, VerifierResultsStore};
    use crate::pool::MemPool;

    // ── Verdict wire strings ────────────────────────────────────────────────────

    #[test]
    fn verdict_pass_str_is_stable() {
        assert_eq!(Verdict::Pass.as_str(), "PASS");
    }

    #[test]
    fn verdict_fail_str_is_stable() {
        assert_eq!(Verdict::Fail.as_str(), "FAIL");
    }

    #[test]
    fn verdict_awaiting_human_str_is_stable() {
        assert_eq!(Verdict::AwaitingHuman.as_str(), "AWAITING_HUMAN");
    }

    // ── Verdict parse ───────────────────────────────────────────────────────────

    #[test]
    fn verdict_parses_pass() {
        assert_eq!(Verdict::parse_str("PASS"), Ok(Verdict::Pass));
    }

    #[test]
    fn verdict_parses_fail() {
        assert_eq!(Verdict::parse_str("FAIL"), Ok(Verdict::Fail));
    }

    #[test]
    fn verdict_parses_awaiting_human() {
        assert_eq!(
            Verdict::parse_str("AWAITING_HUMAN"),
            Ok(Verdict::AwaitingHuman)
        );
    }

    #[test]
    fn verdict_rejects_unknown() {
        assert!(Verdict::parse_str("MAYBE").is_err());
    }

    #[test]
    fn verdict_error_contains_code() {
        let err = Verdict::parse_str("MAYBE").unwrap_err();
        assert!(err.to_string().contains("2451"));
    }

    #[test]
    fn verdict_error_contains_bad_value() {
        let err = Verdict::parse_str("BOGUS").unwrap_err();
        assert!(err.to_string().contains("BOGUS"));
    }

    #[test]
    fn verdict_rejects_lowercase_pass() {
        assert!(Verdict::parse_str("pass").is_err());
    }

    #[test]
    fn verdict_rejects_empty_string() {
        assert!(Verdict::parse_str("").is_err());
    }

    #[test]
    fn verdict_all_strs_roundtrip() {
        for s in Verdict::all_strs() {
            assert!(Verdict::parse_str(s).is_ok(), "failed to parse: {s}");
        }
    }

    #[test]
    fn verdict_all_strs_are_unique() {
        let strs = Verdict::all_strs();
        let mut seen = std::collections::HashSet::new();
        for s in strs {
            assert!(seen.insert(s), "duplicate: {s}");
        }
    }

    // ── Verdict helpers ─────────────────────────────────────────────────────────

    #[test]
    fn verdict_pass_is_terminal() {
        assert!(Verdict::Pass.is_terminal());
    }

    #[test]
    fn verdict_fail_is_terminal() {
        assert!(Verdict::Fail.is_terminal());
    }

    #[test]
    fn verdict_awaiting_human_is_not_terminal() {
        assert!(!Verdict::AwaitingHuman.is_terminal());
    }

    #[test]
    fn verdict_pass_is_pass() {
        assert!(Verdict::Pass.is_pass());
    }

    #[test]
    fn verdict_fail_is_not_pass() {
        assert!(!Verdict::Fail.is_pass());
    }

    #[test]
    fn verdict_awaiting_human_is_not_pass() {
        assert!(!Verdict::AwaitingHuman.is_pass());
    }

    #[test]
    fn verdict_fail_is_fail() {
        assert!(Verdict::Fail.is_fail());
    }

    #[test]
    fn verdict_pass_is_not_fail() {
        assert!(!Verdict::Pass.is_fail());
    }

    #[test]
    fn verdict_awaiting_human_is_not_fail() {
        assert!(!Verdict::AwaitingHuman.is_fail());
    }

    // ── VerifierResult construction ─────────────────────────────────────────────

    #[test]
    fn verifier_result_new_keeps_run_id() {
        let r = VerifierResult::new(7, "s1", Verdict::Pass, "sha", "v1");
        assert_eq!(r.run_id, 7);
    }

    #[test]
    fn verifier_result_new_keeps_step_id() {
        let r = VerifierResult::new(1, "s2", Verdict::Fail, "sha", "v1");
        assert_eq!(r.step_id, "s2");
    }

    #[test]
    fn verifier_result_new_keeps_verdict() {
        let r = VerifierResult::new(1, "s1", Verdict::AwaitingHuman, "sha", "v1");
        assert_eq!(r.verdict, Verdict::AwaitingHuman);
    }

    #[test]
    fn verifier_result_new_id_is_zero() {
        let r = VerifierResult::new(1, "s1", Verdict::Pass, "sha", "v1");
        assert_eq!(r.id, 0);
    }

    #[test]
    fn verifier_result_new_keeps_receipt_sha() {
        let r = VerifierResult::new(1, "s1", Verdict::Pass, "deadbeef", "v1");
        assert_eq!(r.receipt_sha, "deadbeef");
    }

    #[test]
    fn verifier_result_new_keeps_verifier_version() {
        let r = VerifierResult::new(1, "s1", Verdict::Pass, "sha", "v2.1.0");
        assert_eq!(r.verifier_version, "v2.1.0");
    }

    #[test]
    fn verifier_result_has_version_true() {
        let r = VerifierResult::new(1, "s1", Verdict::Pass, "sha", "v1");
        assert!(r.has_version());
    }

    #[test]
    fn verifier_result_has_version_false_for_empty() {
        let r = VerifierResult::new(1, "s1", Verdict::Pass, "sha", "");
        assert!(!r.has_version());
    }

    #[test]
    fn receipt_key_combines_run_step_sha() {
        let r = VerifierResult::new(3, "step-a", Verdict::Pass, "abc123", "v1");
        assert_eq!(r.receipt_key(), "3/step-a/abc123");
    }

    // ── VerifierResultsStore ────────────────────────────────────────────────────

    #[test]
    fn store_append_succeeds_against_mem_pool() {
        let pool = MemPool::new();
        let store = VerifierResultsStore::new(&pool);
        let result = VerifierResult::new(1, "s1", Verdict::Pass, "deadbeef", "v1.0");
        assert!(store.append(&result).is_ok());
    }

    #[test]
    fn store_list_for_run_returns_empty_on_mem_pool() {
        let pool = MemPool::new();
        let store = VerifierResultsStore::new(&pool);
        let rows = store.list_for_run(1);
        assert!(rows.is_ok());
        assert!(rows.unwrap().is_empty());
    }

    #[test]
    fn store_latest_verdict_returns_none_on_mem_pool() {
        let pool = MemPool::new();
        let store = VerifierResultsStore::new(&pool);
        let v = store.latest_verdict_for_step(1, "s1");
        assert!(v.is_ok());
        assert!(v.unwrap().is_none());
    }

    #[test]
    fn store_list_for_step_returns_empty_on_mem_pool() {
        let pool = MemPool::new();
        let store = VerifierResultsStore::new(&pool);
        let rows = store.list_for_step(1, "s1");
        assert!(rows.is_ok());
        assert!(rows.unwrap().is_empty());
    }

    #[test]
    fn store_pass_rate_returns_none_on_mem_pool() {
        let pool = MemPool::new();
        let store = VerifierResultsStore::new(&pool);
        let rate = store.pass_rate(1);
        assert!(rate.is_ok());
        assert!(rate.unwrap().is_none());
    }

    #[test]
    fn store_append_all_verdict_variants() {
        let pool = MemPool::new();
        let store = VerifierResultsStore::new(&pool);
        for verdict in [Verdict::Pass, Verdict::Fail, Verdict::AwaitingHuman] {
            let r = VerifierResult::new(1, "s1", verdict, "sha", "v1");
            assert!(store.append(&r).is_ok());
        }
    }

    #[test]
    fn store_append_multiple_results_for_same_step() {
        let pool = MemPool::new();
        let store = VerifierResultsStore::new(&pool);
        for i in 0..5 {
            let sha = format!("sha{i}");
            let r = VerifierResult::new(1, "s1", Verdict::Pass, sha, "v1");
            assert!(store.append(&r).is_ok());
        }
    }

    // ── TTL predicate ───────────────────────────────────────────────────────────

    #[test]
    fn ttl_predicate_uses_dynamic_now_term() {
        let now_ms = 1_000_000_i64;
        let pred = verifier_ttl_predicate(now_ms, 3_600_000);
        assert!(pred.contains("1000000"));
    }

    #[test]
    fn ttl_predicate_uses_dynamic_retention_term() {
        let pred = verifier_ttl_predicate(1_000_000, 86_400_000);
        assert!(pred.contains("86400000"));
    }

    #[test]
    fn ttl_predicate_uses_division_arithmetic() {
        let pred = verifier_ttl_predicate(1_000_000, 86_400_000);
        assert!(pred.contains("/ 1000"));
    }

    #[test]
    fn ttl_predicate_different_values_produce_different_strings() {
        let p1 = verifier_ttl_predicate(1_000, 3_600_000);
        let p2 = verifier_ttl_predicate(2_000, 3_600_000);
        assert_ne!(p1, p2);
    }

    // ── additional coverage ─────────────────────────────────────────────────────

    #[test]
    fn verdict_copy_trait() {
        let v = Verdict::Pass;
        let _copy = v;
        assert_eq!(v, Verdict::Pass);
    }

    #[test]
    fn verdict_debug_non_empty() {
        assert!(!format!("{:?}", Verdict::Fail).is_empty());
    }

    #[test]
    fn verifier_result_eq_self() {
        let r = VerifierResult::new(1, "s1", Verdict::Pass, "sha", "v1");
        assert_eq!(r, r.clone());
    }

    #[test]
    fn verifier_result_ne_different_verdict() {
        let r1 = VerifierResult::new(1, "s1", Verdict::Pass, "sha", "v1");
        let r2 = VerifierResult::new(1, "s1", Verdict::Fail, "sha", "v1");
        assert_ne!(r1, r2);
    }

    #[test]
    fn verifier_result_debug_non_empty() {
        let r = VerifierResult::new(1, "s1", Verdict::Pass, "sha", "v1");
        assert!(!format!("{r:?}").is_empty());
    }

    #[test]
    fn receipt_key_different_runs_differ() {
        let r1 = VerifierResult::new(1, "s1", Verdict::Pass, "sha", "v1");
        let r2 = VerifierResult::new(2, "s1", Verdict::Pass, "sha", "v1");
        assert_ne!(r1.receipt_key(), r2.receipt_key());
    }

    #[test]
    fn store_pass_rate_returns_ok_always() {
        let pool = MemPool::new();
        let store = VerifierResultsStore::new(&pool);
        assert!(store.pass_rate(99).is_ok());
    }

    #[test]
    fn verifier_result_verifier_version_can_be_empty() {
        let r = VerifierResult::new(1, "s1", Verdict::Pass, "sha", "");
        assert!(!r.has_version());
    }

    #[test]
    fn ttl_predicate_non_empty() {
        assert!(!verifier_ttl_predicate(1_000, 3_600_000).is_empty());
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
    fn sqlite_append_returns_nonzero_id() {
        let (pool, run_id) = make_sqlite_pool_with_run();
        let store = VerifierResultsStore::new(&pool);
        let result = VerifierResult::new(run_id, "s1", Verdict::Pass, "sha", "v1");
        let id = store.append(&result).unwrap();
        assert!(id > 0);
    }

    #[test]
    fn sqlite_list_for_run_returns_inserted_result() {
        let (pool, run_id) = make_sqlite_pool_with_run();
        let store = VerifierResultsStore::new(&pool);
        let r = VerifierResult::new(run_id, "step-a", Verdict::Pass, "deadbeef", "v1");
        store.append(&r).unwrap();
        let rows = store.list_for_run(run_id).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].step_id, "step-a");
        assert_eq!(rows[0].verdict, Verdict::Pass);
    }

    #[test]
    fn sqlite_list_for_step_filters_correctly() {
        let (pool, run_id) = make_sqlite_pool_with_run();
        let store = VerifierResultsStore::new(&pool);
        store
            .append(&VerifierResult::new(
                run_id,
                "s1",
                Verdict::Pass,
                "sha1",
                "v1",
            ))
            .unwrap();
        store
            .append(&VerifierResult::new(
                run_id,
                "s2",
                Verdict::Fail,
                "sha2",
                "v1",
            ))
            .unwrap();
        store
            .append(&VerifierResult::new(
                run_id,
                "s1",
                Verdict::Fail,
                "sha3",
                "v1",
            ))
            .unwrap();
        let s1_rows = store.list_for_step(run_id, "s1").unwrap();
        assert_eq!(s1_rows.len(), 2);
        for r in &s1_rows {
            assert_eq!(r.step_id, "s1");
        }
    }

    #[test]
    fn sqlite_latest_verdict_for_step_returns_most_recent() {
        let (pool, run_id) = make_sqlite_pool_with_run();
        let store = VerifierResultsStore::new(&pool);
        store
            .append(&VerifierResult::new(
                run_id,
                "s1",
                Verdict::Fail,
                "sha1",
                "v1",
            ))
            .unwrap();
        store
            .append(&VerifierResult::new(
                run_id,
                "s1",
                Verdict::Pass,
                "sha2",
                "v1",
            ))
            .unwrap();
        let latest = store.latest_verdict_for_step(run_id, "s1").unwrap();
        assert_eq!(latest, Some(Verdict::Pass));
    }

    #[test]
    fn sqlite_latest_verdict_returns_none_when_no_rows() {
        let (pool, run_id) = make_sqlite_pool_with_run();
        let store = VerifierResultsStore::new(&pool);
        let v = store
            .latest_verdict_for_step(run_id, "nonexistent")
            .unwrap();
        assert!(v.is_none());
    }

    #[test]
    fn sqlite_pass_rate_all_pass() {
        let (pool, run_id) = make_sqlite_pool_with_run();
        let store = VerifierResultsStore::new(&pool);
        for i in 0..4 {
            store
                .append(&VerifierResult::new(
                    run_id,
                    format!("s{i}"),
                    Verdict::Pass,
                    format!("sha{i}"),
                    "v1",
                ))
                .unwrap();
        }
        let rate = store.pass_rate(run_id).unwrap();
        assert_eq!(rate, Some(1.0));
    }

    #[test]
    fn sqlite_pass_rate_returns_none_when_no_rows() {
        let (pool, run_id) = make_sqlite_pool_with_run();
        let store = VerifierResultsStore::new(&pool);
        assert_eq!(store.pass_rate(run_id).unwrap(), None);
    }

    #[test]
    fn sqlite_pass_rate_half_pass() {
        let (pool, run_id) = make_sqlite_pool_with_run();
        let store = VerifierResultsStore::new(&pool);
        store
            .append(&VerifierResult::new(
                run_id,
                "s1",
                Verdict::Pass,
                "sha1",
                "v1",
            ))
            .unwrap();
        store
            .append(&VerifierResult::new(
                run_id,
                "s2",
                Verdict::Fail,
                "sha2",
                "v1",
            ))
            .unwrap();
        let rate = store.pass_rate(run_id).unwrap().unwrap();
        assert!((rate - 0.5).abs() < f64::EPSILON);
    }
}
