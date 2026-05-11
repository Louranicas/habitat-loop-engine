#![forbid(unsafe_code)]

//! M029 — Bounded evidence path/blob store (append-only).
//!
//! Evidence rows record file paths, inline text blobs, and SHA-256 digests
//! produced during workflow execution.  Inline blobs are rejected when they
//! exceed [`MAX_INLINE_BYTES`]; callers must store large outputs as path
//! references instead.
//!
//! No `UPDATE` or `DELETE` is exposed.  TTL deletion uses the
//! `now_ms - retention_ms` predicate pattern (framework §17.5).
//!
//! Error codes: 2440–2442 (`EvidenceSizeExceeded`, `EvidenceKindUnknown`, `EvidenceInsert`).

use substrate_types::HleError;

use crate::pool::{with_conn_val, Pool};

// ── row mapping helper ─────────────────────────────────────────────────────────

fn row_to_evidence(row: (i64, i64, String, String, String, i64)) -> Result<EvidenceRow, HleError> {
    let (id, run_id, kind_str, path_or_inline, sha256, size_bytes) = row;
    let evidence_kind = EvidenceKind::parse_str(&kind_str)?;
    Ok(EvidenceRow {
        id,
        run_id,
        evidence_kind,
        path_or_inline,
        sha256,
        // size_bytes is always non-negative in the schema (INTEGER NOT NULL).
        size_bytes: u64::try_from(size_bytes).unwrap_or(0),
    })
}

// ── constants ──────────────────────────────────────────────────────────────────

/// Maximum byte length of an inline evidence payload.
///
/// Blobs larger than this must be stored as a path reference; the inline
/// field must then be set to a valid filesystem path string, and `size_bytes`
/// records the full on-disk size.
pub const MAX_INLINE_BYTES: usize = 4_096;

// ── error helpers ──────────────────────────────────────────────────────────────

fn err_evidence_size(size: usize) -> HleError {
    HleError::new(format!(
        "[2440 EvidenceSizeExceeded] inline payload {size} bytes exceeds MAX_INLINE_BYTES ({MAX_INLINE_BYTES})"
    ))
}

fn err_evidence_kind(kind: &str) -> HleError {
    HleError::new(format!(
        "[2441 EvidenceKindUnknown] unknown evidence kind: {kind}"
    ))
}

fn err_evidence_insert(detail: impl core::fmt::Display) -> HleError {
    HleError::new(format!("[2442 EvidenceInsert] {detail}"))
}

// ── EvidenceKind ───────────────────────────────────────────────────────────────

/// Classification of an evidence artifact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvidenceKind {
    /// Captured standard-output of a step command.
    Stdout,
    /// Captured standard-error of a step command.
    Stderr,
    /// A file artifact produced by a step (e.g. a JSON report).
    Artifact,
}

impl EvidenceKind {
    /// Wire string used in SQL and JSONL.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
            Self::Artifact => "artifact",
        }
    }

    /// Parse from the wire string.
    ///
    /// # Errors
    ///
    /// Returns `HleError` (2441) for unknown kind strings.
    pub fn parse_str(s: &str) -> Result<Self, HleError> {
        match s {
            "stdout" => Ok(Self::Stdout),
            "stderr" => Ok(Self::Stderr),
            "artifact" => Ok(Self::Artifact),
            other => Err(err_evidence_kind(other)),
        }
    }

    /// All valid kind strings (used in round-trip tests).
    #[must_use]
    pub const fn all_strs() -> [&'static str; 3] {
        ["stdout", "stderr", "artifact"]
    }

    /// Return `true` when this kind captures process output (stdout or stderr).
    #[must_use]
    pub const fn is_output(self) -> bool {
        matches!(self, Self::Stdout | Self::Stderr)
    }
}

// ── EvidenceRow ────────────────────────────────────────────────────────────────

/// One row in the `evidence_store` table.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceRow {
    /// Auto-increment primary key.
    pub id: i64,
    /// Foreign key → `workflow_runs.id`.
    pub run_id: i64,
    /// Classification of this evidence artifact.
    pub evidence_kind: EvidenceKind,
    /// Either an inline text payload (≤ `MAX_INLINE_BYTES`) or a filesystem path.
    pub path_or_inline: String,
    /// Lowercase hex SHA-256 of the evidence content.
    pub sha256: String,
    /// Byte size of the evidence content (full on-disk size for path refs).
    pub size_bytes: u64,
}

impl EvidenceRow {
    /// Construct a new evidence descriptor (id 0 = not yet persisted).
    ///
    /// # Errors
    ///
    /// Returns `HleError` (2440) when `path_or_inline` is treated as an inline
    /// blob (i.e. `is_inline` is `true`) and its byte length exceeds
    /// [`MAX_INLINE_BYTES`].
    pub fn new(
        run_id: i64,
        evidence_kind: EvidenceKind,
        path_or_inline: impl Into<String>,
        sha256: impl Into<String>,
        size_bytes: u64,
        is_inline: bool,
    ) -> Result<Self, HleError> {
        let path_or_inline = path_or_inline.into();
        if is_inline && path_or_inline.len() > MAX_INLINE_BYTES {
            return Err(err_evidence_size(path_or_inline.len()));
        }
        Ok(Self {
            id: 0,
            run_id,
            evidence_kind,
            path_or_inline,
            sha256: sha256.into(),
            size_bytes,
        })
    }

    /// Return `true` if the evidence refers to an on-disk path rather than
    /// inline content.  Heuristic: the field starts with `/` or `.` or a drive
    /// letter, OR `size_bytes` exceeds `MAX_INLINE_BYTES`.
    #[must_use]
    pub fn is_path_ref(&self) -> bool {
        self.size_bytes > MAX_INLINE_BYTES as u64
            || self.path_or_inline.starts_with('/')
            || self.path_or_inline.starts_with('.')
    }

    /// Content-addressable identity key: `"{run_id}/{sha256}"`.
    #[must_use]
    pub fn content_key(&self) -> String {
        format!("{}/{}", self.run_id, self.sha256)
    }
}

// ── EvidenceStore ──────────────────────────────────────────────────────────────

/// Append-only access layer for the `evidence_store` table.
pub struct EvidenceStore<'pool> {
    pool: &'pool dyn Pool,
}

impl<'pool> EvidenceStore<'pool> {
    /// Bind the store to a pool.
    #[must_use]
    pub fn new(pool: &'pool dyn Pool) -> Self {
        Self { pool }
    }

    /// Append an evidence row and return the assigned row id.
    ///
    /// # Errors
    ///
    /// Returns `HleError` (2442) when the pool connection or INSERT fails.
    pub fn insert(&self, row: &EvidenceRow) -> Result<i64, HleError> {
        let sql = format!(
            "INSERT INTO evidence_store \
             (run_id, evidence_kind, path_or_inline, sha256, size_bytes) \
             VALUES ({}, '{}', '{}', '{}', {})",
            row.run_id,
            row.evidence_kind.as_str(),
            sql_escape(&row.path_or_inline),
            sql_escape(&row.sha256),
            row.size_bytes,
        );
        with_conn_val(self.pool, |conn| {
            conn.execute_sql(&sql).map_err(err_evidence_insert)?;
            conn.query_one_i64("SELECT last_insert_rowid()")
                .map(|opt| opt.unwrap_or(0))
                .map_err(err_evidence_insert)
        })
    }

    /// Return all evidence rows for a run, ordered by id ascending.
    ///
    /// # Errors
    ///
    /// Returns `HleError` when the pool fails to acquire a connection.
    pub fn list_for_run(&self, run_id: i64) -> Result<Vec<EvidenceRow>, HleError> {
        let sql = format!(
            "SELECT id, run_id, evidence_kind, path_or_inline, sha256, size_bytes \
             FROM evidence_store WHERE run_id = {run_id} ORDER BY id ASC"
        );
        with_conn_val(self.pool, |conn| {
            let rows = conn.query_rows_evidence(&sql)?;
            rows.into_iter().map(row_to_evidence).collect()
        })
    }

    /// Return all evidence rows matching a specific kind for a run.
    ///
    /// # Errors
    ///
    /// Returns `HleError` when the pool fails to acquire a connection.
    pub fn list_by_kind(
        &self,
        run_id: i64,
        kind: EvidenceKind,
    ) -> Result<Vec<EvidenceRow>, HleError> {
        let sql = format!(
            "SELECT id, run_id, evidence_kind, path_or_inline, sha256, size_bytes \
             FROM evidence_store WHERE run_id = {run_id} AND evidence_kind = '{}' \
             ORDER BY id ASC",
            kind.as_str(),
        );
        with_conn_val(self.pool, |conn| {
            let rows = conn.query_rows_evidence(&sql)?;
            rows.into_iter().map(row_to_evidence).collect()
        })
    }

    /// Return the first evidence row with a given SHA-256 hash for a run.
    ///
    /// Used for content-addressable deduplication checks.
    ///
    /// # Errors
    ///
    /// Returns `HleError` when the pool fails to acquire a connection.
    pub fn find_by_sha256(
        &self,
        run_id: i64,
        sha256: &str,
    ) -> Result<Option<EvidenceRow>, HleError> {
        let sql = format!(
            "SELECT id, run_id, evidence_kind, path_or_inline, sha256, size_bytes \
             FROM evidence_store WHERE run_id = {run_id} AND sha256 = '{}' \
             ORDER BY id ASC LIMIT 1",
            sql_escape(sha256),
        );
        with_conn_val(self.pool, |conn| {
            let rows = conn.query_rows_evidence(&sql)?;
            Ok(rows
                .into_iter()
                .map(row_to_evidence)
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .next())
        })
    }
}

// ── TTL helpers ────────────────────────────────────────────────────────────────

/// SQL predicate for evidence TTL deletion.
///
/// Uses `now_ms - retention_ms` to avoid literal integer timestamps
/// (framework §17.5).
#[must_use]
pub fn evidence_ttl_predicate(now_ms: i64, retention_ms: i64) -> String {
    format!("created_unix < ({now_ms} / 1000 - {retention_ms} / 1000)")
}

// ── helpers ────────────────────────────────────────────────────────────────────

fn sql_escape(s: &str) -> String {
    s.replace('\'', "''")
}

// ── tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{
        evidence_ttl_predicate, EvidenceKind, EvidenceRow, EvidenceStore, MAX_INLINE_BYTES,
    };
    use crate::pool::MemPool;

    // ── EvidenceKind wire strings ───────────────────────────────────────────────

    #[test]
    fn evidence_kind_stdout_str_is_stable() {
        assert_eq!(EvidenceKind::Stdout.as_str(), "stdout");
    }

    #[test]
    fn evidence_kind_stderr_str_is_stable() {
        assert_eq!(EvidenceKind::Stderr.as_str(), "stderr");
    }

    #[test]
    fn evidence_kind_artifact_str_is_stable() {
        assert_eq!(EvidenceKind::Artifact.as_str(), "artifact");
    }

    // ── EvidenceKind parse ──────────────────────────────────────────────────────

    #[test]
    fn evidence_kind_parses_stdout() {
        assert_eq!(EvidenceKind::parse_str("stdout"), Ok(EvidenceKind::Stdout));
    }

    #[test]
    fn evidence_kind_parses_stderr() {
        assert_eq!(EvidenceKind::parse_str("stderr"), Ok(EvidenceKind::Stderr));
    }

    #[test]
    fn evidence_kind_parses_artifact() {
        assert_eq!(
            EvidenceKind::parse_str("artifact"),
            Ok(EvidenceKind::Artifact)
        );
    }

    #[test]
    fn evidence_kind_rejects_unknown() {
        assert!(EvidenceKind::parse_str("binary").is_err());
    }

    #[test]
    fn evidence_kind_error_contains_code() {
        let err = EvidenceKind::parse_str("binary").unwrap_err();
        assert!(err.to_string().contains("2441"));
    }

    #[test]
    fn evidence_kind_error_contains_bad_value() {
        let err = EvidenceKind::parse_str("unknown-kind").unwrap_err();
        assert!(err.to_string().contains("unknown-kind"));
    }

    #[test]
    fn evidence_kind_rejects_empty_string() {
        assert!(EvidenceKind::parse_str("").is_err());
    }

    #[test]
    fn evidence_kind_rejects_uppercase() {
        assert!(EvidenceKind::parse_str("STDOUT").is_err());
    }

    #[test]
    fn evidence_kind_all_strs_roundtrip() {
        for s in EvidenceKind::all_strs() {
            assert!(EvidenceKind::parse_str(s).is_ok(), "failed to parse: {s}");
        }
    }

    #[test]
    fn evidence_kind_all_strs_are_unique() {
        let strs = EvidenceKind::all_strs();
        let mut seen = std::collections::HashSet::new();
        for s in strs {
            assert!(seen.insert(s), "duplicate: {s}");
        }
    }

    // ── EvidenceKind helpers ────────────────────────────────────────────────────

    #[test]
    fn evidence_kind_stdout_is_output() {
        assert!(EvidenceKind::Stdout.is_output());
    }

    #[test]
    fn evidence_kind_stderr_is_output() {
        assert!(EvidenceKind::Stderr.is_output());
    }

    #[test]
    fn evidence_kind_artifact_is_not_output() {
        assert!(!EvidenceKind::Artifact.is_output());
    }

    // ── EvidenceRow::new ────────────────────────────────────────────────────────

    #[test]
    fn evidence_row_new_rejects_inline_above_max() {
        let large = "x".repeat(MAX_INLINE_BYTES + 1);
        let result = EvidenceRow::new(1, EvidenceKind::Stdout, large, "aa", 0, true);
        assert!(result.is_err());
    }

    #[test]
    fn evidence_row_size_error_contains_code() {
        let large = "x".repeat(MAX_INLINE_BYTES + 1);
        let err = EvidenceRow::new(1, EvidenceKind::Stdout, large, "aa", 0, true).unwrap_err();
        assert!(err.to_string().contains("2440"));
    }

    #[test]
    fn evidence_row_new_accepts_inline_at_max() {
        let at_limit = "y".repeat(MAX_INLINE_BYTES);
        let result = EvidenceRow::new(1, EvidenceKind::Stdout, at_limit, "bb", 0, true);
        assert!(result.is_ok());
    }

    #[test]
    fn evidence_row_new_accepts_inline_below_max() {
        let result = EvidenceRow::new(1, EvidenceKind::Stdout, "tiny", "cc", 4, true);
        assert!(result.is_ok());
    }

    #[test]
    fn evidence_row_new_accepts_path_above_max_when_not_inline() {
        let long_path = "/tmp/".to_owned() + &"a".repeat(MAX_INLINE_BYTES + 1);
        let result = EvidenceRow::new(1, EvidenceKind::Artifact, long_path, "cc", 999_999, false);
        assert!(result.is_ok());
    }

    #[test]
    fn evidence_row_new_id_is_zero() {
        let row = EvidenceRow::new(1, EvidenceKind::Stdout, "data", "sha", 4, true).unwrap();
        assert_eq!(row.id, 0);
    }

    #[test]
    fn evidence_row_new_sets_run_id() {
        let row = EvidenceRow::new(7, EvidenceKind::Stderr, "data", "sha", 4, true).unwrap();
        assert_eq!(row.run_id, 7);
    }

    #[test]
    fn evidence_row_new_sets_sha256() {
        let row = EvidenceRow::new(1, EvidenceKind::Stdout, "data", "deadbeef", 4, true).unwrap();
        assert_eq!(row.sha256, "deadbeef");
    }

    #[test]
    fn evidence_row_new_sets_size_bytes() {
        let row = EvidenceRow::new(1, EvidenceKind::Stdout, "data", "sha", 128, true).unwrap();
        assert_eq!(row.size_bytes, 128);
    }

    #[test]
    fn evidence_row_new_sets_kind() {
        let row = EvidenceRow::new(1, EvidenceKind::Artifact, "/tmp/f", "sha", 0, false).unwrap();
        assert_eq!(row.evidence_kind, EvidenceKind::Artifact);
    }

    // ── EvidenceRow helpers ─────────────────────────────────────────────────────

    #[test]
    fn content_key_combines_run_id_and_sha256() {
        let row = EvidenceRow::new(5, EvidenceKind::Stdout, "data", "abc123", 4, true).unwrap();
        assert_eq!(row.content_key(), "5/abc123");
    }

    #[test]
    fn content_key_is_unique_per_sha() {
        let r1 = EvidenceRow::new(1, EvidenceKind::Stdout, "a", "sha1", 1, true).unwrap();
        let r2 = EvidenceRow::new(1, EvidenceKind::Stdout, "b", "sha2", 1, true).unwrap();
        assert_ne!(r1.content_key(), r2.content_key());
    }

    #[test]
    fn is_path_ref_true_for_absolute_path() {
        let row =
            EvidenceRow::new(1, EvidenceKind::Artifact, "/tmp/file", "sha", 0, false).unwrap();
        assert!(row.is_path_ref());
    }

    #[test]
    fn is_path_ref_true_for_large_size() {
        let row = EvidenceRow::new(
            1,
            EvidenceKind::Artifact,
            "large",
            "sha",
            MAX_INLINE_BYTES as u64 + 1,
            false,
        )
        .unwrap();
        assert!(row.is_path_ref());
    }

    #[test]
    fn is_path_ref_false_for_small_inline() {
        let row = EvidenceRow::new(1, EvidenceKind::Stdout, "hello", "sha", 5, true).unwrap();
        assert!(!row.is_path_ref());
    }

    // ── EvidenceStore ───────────────────────────────────────────────────────────

    #[test]
    fn store_insert_succeeds_against_mem_pool() {
        let pool = MemPool::new();
        let store = EvidenceStore::new(&pool);
        let row = EvidenceRow::new(1, EvidenceKind::Stdout, "hello", "deadbeef", 5, true).unwrap();
        assert!(store.insert(&row).is_ok());
    }

    #[test]
    fn store_list_for_run_returns_empty_on_mem_pool() {
        let pool = MemPool::new();
        let store = EvidenceStore::new(&pool);
        let rows = store.list_for_run(1);
        assert!(rows.is_ok());
        assert!(rows.unwrap().is_empty());
    }

    #[test]
    fn store_list_by_kind_returns_empty_on_mem_pool() {
        let pool = MemPool::new();
        let store = EvidenceStore::new(&pool);
        let rows = store.list_by_kind(1, EvidenceKind::Stdout);
        assert!(rows.is_ok());
        assert!(rows.unwrap().is_empty());
    }

    #[test]
    fn store_find_by_sha256_returns_none_on_mem_pool() {
        let pool = MemPool::new();
        let store = EvidenceStore::new(&pool);
        let result = store.find_by_sha256(1, "deadbeef");
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn store_insert_all_evidence_kinds() {
        let pool = MemPool::new();
        let store = EvidenceStore::new(&pool);
        for kind in [
            EvidenceKind::Stdout,
            EvidenceKind::Stderr,
            EvidenceKind::Artifact,
        ] {
            let row = EvidenceRow::new(1, kind, "data", "sha", 4, true).unwrap();
            assert!(store.insert(&row).is_ok());
        }
    }

    // ── TTL predicate ───────────────────────────────────────────────────────────

    #[test]
    fn ttl_predicate_does_not_use_literal_zero_retention() {
        let pred = evidence_ttl_predicate(1_000_000, 86_400_000);
        assert!(!pred.contains("- 0)"));
    }

    #[test]
    fn max_inline_bytes_constant_is_4096() {
        assert_eq!(MAX_INLINE_BYTES, 4_096);
    }

    #[test]
    fn ttl_predicate_contains_now_ms() {
        let now = 1_700_000_000_000_i64;
        let pred = evidence_ttl_predicate(now, 86_400_000);
        assert!(pred.contains(&now.to_string()));
    }

    #[test]
    fn ttl_predicate_contains_retention_ms() {
        let pred = evidence_ttl_predicate(1_000_000, 7_776_000_000);
        assert!(pred.contains("7776000000"));
    }

    #[test]
    fn ttl_predicate_uses_division_arithmetic() {
        let pred = evidence_ttl_predicate(1_000_000_000, 86_400_000);
        assert!(pred.contains("/ 1000"));
    }

    // ── additional coverage ─────────────────────────────────────────────────────

    #[test]
    fn evidence_row_eq_same_values() {
        let r1 = EvidenceRow::new(1, EvidenceKind::Stdout, "hello", "sha", 5, true).unwrap();
        let r2 = r1.clone();
        assert_eq!(r1, r2);
    }

    #[test]
    fn evidence_row_ne_different_sha() {
        let r1 = EvidenceRow::new(1, EvidenceKind::Stdout, "a", "sha1", 1, true).unwrap();
        let r2 = EvidenceRow::new(1, EvidenceKind::Stdout, "a", "sha2", 1, true).unwrap();
        assert_ne!(r1, r2);
    }

    #[test]
    fn evidence_row_debug_non_empty() {
        let r = EvidenceRow::new(1, EvidenceKind::Stdout, "data", "sha", 4, true).unwrap();
        assert!(!format!("{r:?}").is_empty());
    }

    #[test]
    fn evidence_kind_debug_non_empty() {
        assert!(!format!("{:?}", EvidenceKind::Artifact).is_empty());
    }

    #[test]
    fn evidence_row_empty_sha_accepted() {
        let r = EvidenceRow::new(1, EvidenceKind::Stdout, "data", "", 4, true);
        assert!(r.is_ok());
    }

    #[test]
    fn store_insert_then_list_multiple_kinds() {
        let pool = MemPool::new();
        let store = EvidenceStore::new(&pool);
        for kind in EvidenceKind::all_strs() {
            let ek = EvidenceKind::parse_str(kind).unwrap();
            let row = EvidenceRow::new(1, ek, "d", "s", 1, true).unwrap();
            assert!(store.insert(&row).is_ok());
            assert!(store.list_by_kind(1, ek).is_ok());
        }
    }

    #[test]
    fn evidence_ttl_predicate_non_empty() {
        assert!(!evidence_ttl_predicate(1_000, 86_400_000).is_empty());
    }

    #[test]
    fn evidence_row_size_bytes_preserved() {
        let r = EvidenceRow::new(1, EvidenceKind::Stdout, "x", "s", 42, true).unwrap();
        assert_eq!(r.size_bytes, 42);
    }

    #[test]
    fn evidence_kind_copy_trait() {
        let k = EvidenceKind::Stdout;
        let _copy = k;
        assert_eq!(k, EvidenceKind::Stdout);
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
    fn sqlite_insert_evidence_returns_nonzero_id() {
        let (pool, run_id) = make_sqlite_pool_with_run();
        let store = EvidenceStore::new(&pool);
        let row =
            EvidenceRow::new(run_id, EvidenceKind::Stdout, "hello", "deadbeef", 5, true).unwrap();
        let id = store.insert(&row).unwrap();
        assert!(id > 0);
    }

    #[test]
    fn sqlite_list_for_run_returns_inserted_row() {
        let (pool, run_id) = make_sqlite_pool_with_run();
        let store = EvidenceStore::new(&pool);
        let row =
            EvidenceRow::new(run_id, EvidenceKind::Stdout, "output", "sha1", 6, true).unwrap();
        store.insert(&row).unwrap();
        let rows = store.list_for_run(run_id).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].evidence_kind, EvidenceKind::Stdout);
        assert_eq!(rows[0].sha256, "sha1");
    }

    #[test]
    fn sqlite_list_by_kind_filters_correctly() {
        let (pool, run_id) = make_sqlite_pool_with_run();
        let store = EvidenceStore::new(&pool);
        let stdout_row =
            EvidenceRow::new(run_id, EvidenceKind::Stdout, "out", "sha-out", 3, true).unwrap();
        let stderr_row =
            EvidenceRow::new(run_id, EvidenceKind::Stderr, "err", "sha-err", 3, true).unwrap();
        store.insert(&stdout_row).unwrap();
        store.insert(&stderr_row).unwrap();
        let stdout_rows = store.list_by_kind(run_id, EvidenceKind::Stdout).unwrap();
        assert_eq!(stdout_rows.len(), 1);
        assert_eq!(stdout_rows[0].evidence_kind, EvidenceKind::Stdout);
    }

    #[test]
    fn sqlite_find_by_sha256_returns_matching_row() {
        let (pool, run_id) = make_sqlite_pool_with_run();
        let store = EvidenceStore::new(&pool);
        let row = EvidenceRow::new(
            run_id,
            EvidenceKind::Artifact,
            "/tmp/f",
            "abc123",
            10,
            false,
        )
        .unwrap();
        store.insert(&row).unwrap();
        let found = store.find_by_sha256(run_id, "abc123").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().sha256, "abc123");
    }

    #[test]
    fn sqlite_find_by_sha256_returns_none_for_missing() {
        let (pool, run_id) = make_sqlite_pool_with_run();
        let store = EvidenceStore::new(&pool);
        let found = store.find_by_sha256(run_id, "nonexistent").unwrap();
        assert!(found.is_none());
    }

    #[test]
    fn sqlite_size_bytes_preserved_on_roundtrip() {
        let (pool, run_id) = make_sqlite_pool_with_run();
        let store = EvidenceStore::new(&pool);
        let row = EvidenceRow::new(run_id, EvidenceKind::Stdout, "data", "sha", 128, true).unwrap();
        store.insert(&row).unwrap();
        let rows = store.list_for_run(run_id).unwrap();
        assert_eq!(rows[0].size_bytes, 128);
    }
}
