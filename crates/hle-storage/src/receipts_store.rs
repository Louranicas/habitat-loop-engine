#![forbid(unsafe_code)]

// End-to-end stack cross-reference: terminal implementation node for
// M007_RECEIPTS_STORE.md / L02_PERSISTENCE.md / C01_EVIDENCE_INTEGRITY (cluster).
// Spec: ai_specs/modules/c01-evidence-integrity/M007_RECEIPTS_STORE.md.
//
// STUB: compile-safe skeleton. The `Pool` trait is declared locally here as a
// placeholder; the real implementation comes from M021 (C05) when that cluster's
// leaf files are authored. See TODO comment near `Pool` trait definition.
//
// `ReceiptsStore::append` and `get` return stub `Err`/`Ok(None)` bodies. The
// append-only invariant is structurally preserved: no `update`, `delete`,
// `truncate`, or `upsert` methods exist on any public surface.

use std::fmt;
use std::sync::Arc;

use substrate_types::HleError;

use hle_core::evidence::receipt_hash::{ReceiptHash, ReceiptHashFields};

// ── Pool placeholder ─────────────────────────────────────────────────────────
//
// TODO(C05/M021): replace this local `Pool` trait with the real
// `hle_storage::pool::Pool` once that module is authored. The `ReceiptsStore`
// constructor accepts `Arc<dyn Pool + Send + Sync>` so the swap is additive.

/// Minimal connection-pool abstraction. C05 (M021) owns the real implementation.
///
/// This stub allows `ReceiptsStore` to compile without the full pool crate. The
/// production implementation will require WAL-mode `SQLite` and single-writer
/// semantics per the M007 spec.
pub trait Pool: fmt::Debug + Send + Sync {
    /// Returns a human-readable pool identifier for debug output.
    fn pool_id(&self) -> &str;
}

// ── StoredReceipt ─────────────────────────────────────────────────────────────

/// A fully persisted receipt record including both split-hash anchors.
///
/// `StoredReceipt` is the canonical in-memory representation of a row in the
/// `receipts` WAL-SQLite table. Primary key is `hash`; no surrogate integer key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredReceipt {
    /// Canonical SHA-256 hash of the receipt fields. Primary key.
    pub hash: ReceiptHash,
    /// Workflow identifier.
    pub workflow: String,
    /// Step identifier within the workflow.
    pub step_id: String,
    /// Verifier verdict string: `"PASS"`, `"FAIL"`, or `"AWAITING_HUMAN"`.
    pub verdict: String,
    /// Scaffold manifest hash — `^Manifest_sha256` anchor from `HARNESS_CONTRACT.md`.
    pub manifest_sha256: String,
    /// Source/framework provenance hash — `^Framework_sha256` anchor from `HARNESS_CONTRACT.md`.
    pub framework_sha256: String,
    /// Monotonic append counter (no wall clock; no chrono/SystemTime).
    pub appended_at: u64,
    /// Optional counter-evidence locator from `schemas/receipt.schema.json`.
    pub counter_evidence_locator: Option<String>,
}

impl StoredReceipt {
    /// Build a `StoredReceipt` by hashing `fields` and recording the verdict.
    ///
    /// Delegates to `ReceiptHash::from_fields`; propagates `[E2000] HashInput`.
    ///
    /// # Errors
    ///
    /// Returns `Err` when `fields` are invalid (empty workflow) or hashing fails.
    pub fn from_fields(
        fields: &ReceiptHashFields,
        verdict: impl Into<String>,
        appended_at: u64,
    ) -> Result<Self, HleError> {
        let hash = ReceiptHash::from_fields(fields)?;
        Ok(Self {
            hash,
            workflow: fields.workflow.clone(),
            step_id: fields.step_id.clone(),
            verdict: verdict.into(),
            manifest_sha256: fields.manifest_sha256.clone(),
            framework_sha256: fields.framework_sha256.clone(),
            appended_at,
            counter_evidence_locator: None,
        })
    }

    /// Validate required fields are non-empty and hash fields are plausible.
    ///
    /// # Errors
    ///
    /// Returns `Err` when workflow, `step_id`, or verdict are blank, or when
    /// `manifest_sha256` / `framework_sha256` have wrong lengths (must be 64
    /// hex chars when non-empty, per `HARNESS_CONTRACT.md`).
    pub fn validate(&self) -> Result<(), HleError> {
        if self.workflow.trim().is_empty() {
            return Err(HleError::new(
                "[E2021] StorageIo: workflow must be non-empty",
            ));
        }
        if self.step_id.trim().is_empty() {
            return Err(HleError::new(
                "[E2021] StorageIo: step_id must be non-empty",
            ));
        }
        if self.verdict.trim().is_empty() {
            return Err(HleError::new(
                "[E2021] StorageIo: verdict must be non-empty",
            ));
        }
        // Validate split-hash anchors length when populated.
        for (name, val) in [
            ("manifest_sha256", &self.manifest_sha256),
            ("framework_sha256", &self.framework_sha256),
        ] {
            if !val.is_empty() && val.len() != 64 {
                return Err(HleError::new(format!(
                    "[E2021] StorageIo: {name} must be 64 hex chars when non-empty, got {}",
                    val.len()
                )));
            }
        }
        Ok(())
    }

    /// Returns the 64-character lowercase hex of the receipt's primary key.
    #[must_use]
    pub fn to_hex_hash(&self) -> String {
        self.hash.to_hex()
    }
}

impl fmt::Display for StoredReceipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "StoredReceipt({}:{}@{}/{})",
            self.hash, self.verdict, self.workflow, self.step_id,
        )
    }
}

// ── ReceiptsQuery ─────────────────────────────────────────────────────────────

/// Typed query parameters for `ReceiptsStore::query`.
///
/// Build with the `with_*` builder methods. `limit` is clamped to `1..=1000`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptsQuery {
    /// Filter by workflow name. `None` means all workflows.
    pub workflow: Option<String>,
    /// Filter by step identifier. `None` means all steps.
    pub step_id: Option<String>,
    /// Filter by verdict string. `None` means all verdicts.
    pub verdict: Option<String>,
    /// Maximum rows to return. Always in `1..=1000`.
    pub limit: usize,
    /// Offset for pagination. Default 0.
    pub offset: usize,
}

impl ReceiptsQuery {
    /// Default query: no filters, `limit=100`, `offset=0`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            workflow: None,
            step_id: None,
            verdict: None,
            limit: 100,
            offset: 0,
        }
    }

    /// Filter by workflow name.
    #[must_use]
    pub fn with_workflow(mut self, workflow: impl Into<String>) -> Self {
        self.workflow = Some(workflow.into());
        self
    }

    /// Filter by step identifier.
    #[must_use]
    pub fn with_step_id(mut self, step_id: impl Into<String>) -> Self {
        self.step_id = Some(step_id.into());
        self
    }

    /// Filter by verdict string.
    #[must_use]
    pub fn with_verdict(mut self, verdict: impl Into<String>) -> Self {
        self.verdict = Some(verdict.into());
        self
    }

    /// Set the maximum rows to return; clamped to `1..=1000`.
    #[must_use]
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit.clamp(1, 1000);
        self
    }

    /// Set the pagination offset.
    #[must_use]
    pub fn with_offset(mut self, offset: usize) -> Self {
        self.offset = offset;
        self
    }
}

impl Default for ReceiptsQuery {
    fn default() -> Self {
        Self::new()
    }
}

// ── AppendResult ──────────────────────────────────────────────────────────────

/// Acknowledgment that an `append()` call landed durably.
///
/// `AppendResult` is `#[must_use]` — discarding it loses the audit record that
/// the append occurred.
#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppendResult {
    /// The hash of the appended receipt (confirmation).
    pub hash: ReceiptHash,
    /// Monotonic counter value at time of append.
    pub appended_at: u64,
    /// Row count after the append (for observability).
    pub total_count: u64,
}

// ── ReceiptsStore ─────────────────────────────────────────────────────────────

/// Append-only, hash-keyed receipt persistence.
///
/// Holds a reference-counted handle to the shared database pool (M021/C05).
/// All writes are append-only: there is no `update`, `delete`, `truncate`, or
/// `upsert` method. Adding any such method is an architectural violation of the
/// append-only principle per `HLE-UP` / M007 spec.
pub struct ReceiptsStore {
    pool: Arc<dyn Pool>,
}

impl fmt::Debug for ReceiptsStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ReceiptsStore(pool={})", self.pool.pool_id())
    }
}

impl ReceiptsStore {
    /// Construct with a shared pool handle.
    #[must_use]
    pub fn new(pool: Arc<dyn Pool>) -> Self {
        Self { pool }
    }

    /// Append a receipt to the store; returns a confirmation record.
    ///
    /// # Errors
    ///
    /// Returns `Err` (`[E2020] AppendConflict`) when:
    /// - `receipt.hash == ReceiptHash::zeroed()` (sentinel rejected per spec), or
    /// - a receipt with this hash already exists.
    ///
    /// Returns `Err` (`[E2021] StorageIo`) on I/O or pool errors.
    ///
    /// # Stub notice
    ///
    /// The current body always returns `Err([E2021] StorageIo: stub —
    /// pool not wired)` until the real pool integration (C05/M021) lands.
    pub fn append(&self, receipt: &StoredReceipt) -> Result<AppendResult, HleError> {
        if receipt.hash == ReceiptHash::zeroed() {
            return Err(HleError::new(
                "[E2020] AppendConflict: zeroed hash rejected by append-only store",
            ));
        }
        receipt.validate()?;
        // STUB: real implementation acquires a pool connection and executes
        // INSERT OR ABORT INTO receipts (...) inside a WAL transaction.
        Err(HleError::new(
            "[E2021] StorageIo: stub — pool not wired; C05/M021 required",
        ))
    }

    /// Retrieve a receipt by hash.
    ///
    /// Returns `Ok(None)` when the hash is absent (not an error).
    ///
    /// # Errors
    ///
    /// Returns `Err` (`[E2021] StorageIo`) on pool/I/O errors.
    ///
    /// # Stub notice
    ///
    /// Current body returns `Ok(None)` — stub until pool is wired.
    pub fn get(&self, _hash: ReceiptHash) -> Result<Option<StoredReceipt>, HleError> {
        // STUB: real implementation executes SELECT ... FROM receipts WHERE hash = ?
        Ok(None)
    }

    /// Lightweight presence check without fetching the full record.
    ///
    /// # Errors
    ///
    /// Returns `Err` (`[E2021] StorageIo`) on pool/I/O errors.
    ///
    /// # Stub notice
    ///
    /// Current body returns `Ok(false)` — stub until pool is wired.
    pub fn exists(&self, _hash: ReceiptHash) -> Result<bool, HleError> {
        Ok(false)
    }

    /// Return a filtered, bounded list of receipts.
    ///
    /// # Errors
    ///
    /// Returns `Err` (`[E2021] StorageIo`) on pool/I/O errors.
    ///
    /// # Stub notice
    ///
    /// Current body returns `Ok(vec![])` — stub until pool is wired.
    pub fn query(&self, _q: &ReceiptsQuery) -> Result<Vec<StoredReceipt>, HleError> {
        Ok(Vec::new())
    }

    /// Total row count in the receipt store.
    ///
    /// # Errors
    ///
    /// Returns `Err` (`[E2021] StorageIo`) on pool/I/O errors.
    ///
    /// # Stub notice
    ///
    /// Current body returns `Ok(0)` — stub until pool is wired.
    pub fn count(&self) -> Result<u64, HleError> {
        Ok(0)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    // ── MemPool — in-memory pool for tests ────────────────────────────────────
    //
    // The production `append` path returns a stub error because the real
    // SQLite pool (C05/M021) is not wired yet. For test coverage we wire a
    // `MemPool` that satisfies the `Pool` trait and backs a full in-memory
    // implementation of `ReceiptsStore`'s behaviour via `MemReceiptsStore`.

    #[derive(Debug)]
    struct NullPool;
    impl Pool for NullPool {
        fn pool_id(&self) -> &str {
            "null-pool"
        }
    }

    /// Thread-safe in-memory receipt store that mirrors the append-only API.
    ///
    /// Replaces the stub path for tests that need real round-trip behaviour.
    /// Does NOT implement `Pool`; it is a standalone test helper.
    #[derive(Debug, Default)]
    struct MemReceiptsStore {
        inner: Mutex<MemStoreInner>,
    }

    #[derive(Debug, Default)]
    struct MemStoreInner {
        rows: HashMap<[u8; 32], StoredReceipt>,
        seq: u64,
    }

    impl MemReceiptsStore {
        fn append(&self, receipt: &StoredReceipt) -> Result<AppendResult, HleError> {
            if receipt.hash == ReceiptHash::zeroed() {
                return Err(HleError::new(
                    "[E2020] AppendConflict: zeroed hash rejected by append-only store",
                ));
            }
            receipt.validate()?;
            let mut guard = self
                .inner
                .lock()
                .map_err(|_| HleError::new("[E2021] StorageIo: lock poisoned during mem append"))?;
            let key = *receipt.hash.as_bytes();
            if guard.rows.contains_key(&key) {
                return Err(HleError::new(format!(
                    "[E2020] AppendConflict: hash {} already present",
                    receipt.hash.to_hex()
                )));
            }
            guard.seq = guard.seq.saturating_add(1);
            let total = guard.rows.len() as u64 + 1;
            let seq = guard.seq;
            guard.rows.insert(key, receipt.clone());
            Ok(AppendResult {
                hash: receipt.hash,
                appended_at: seq,
                total_count: total,
            })
        }

        fn get(&self, hash: ReceiptHash) -> Option<StoredReceipt> {
            let guard = self
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            guard.rows.get(hash.as_bytes()).cloned()
        }

        fn exists(&self, hash: ReceiptHash) -> bool {
            let guard = self
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            guard.rows.contains_key(hash.as_bytes())
        }

        fn count(&self) -> u64 {
            let guard = self
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            guard.rows.len() as u64
        }

        fn query(&self, q: &ReceiptsQuery) -> Vec<StoredReceipt> {
            let guard = self
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            guard
                .rows
                .values()
                .filter(|r| {
                    q.workflow.as_deref().map_or(true, |wf| r.workflow == wf)
                        && q.step_id.as_deref().map_or(true, |s| r.step_id == s)
                        && q.verdict.as_deref().map_or(true, |v| r.verdict == v)
                })
                .skip(q.offset)
                .take(q.limit)
                .cloned()
                .collect()
        }
    }

    // ── helpers ────────────────────────────────────────────────────────────────

    fn null_store() -> ReceiptsStore {
        ReceiptsStore::new(Arc::new(NullPool))
    }

    fn make_receipt(discriminator: u8) -> StoredReceipt {
        let mut bytes = [0u8; 32];
        bytes[0] = discriminator;
        bytes[1] = 0xAB; // non-zero so it's not the zeroed sentinel
        StoredReceipt {
            hash: ReceiptHash::from_bytes(bytes),
            workflow: String::from("demo"),
            step_id: String::from("s1"),
            verdict: String::from("PASS"),
            manifest_sha256: String::new(),
            framework_sha256: String::new(),
            appended_at: 1,
            counter_evidence_locator: None,
        }
    }

    fn make_receipt_full(
        discriminator: u8,
        workflow: &str,
        step_id: &str,
        verdict: &str,
    ) -> StoredReceipt {
        let mut r = make_receipt(discriminator);
        r.workflow = workflow.to_owned();
        r.step_id = step_id.to_owned();
        r.verdict = verdict.to_owned();
        r
    }

    fn make_mem() -> MemReceiptsStore {
        MemReceiptsStore::default()
    }

    // ── StoredReceipt::validate ───────────────────────────────────────────────

    #[test]
    fn stored_receipt_validate_rejects_empty_workflow() {
        let mut r = make_receipt(1);
        r.workflow = String::new();
        assert!(r.validate().is_err());
    }

    #[test]
    fn stored_receipt_validate_rejects_blank_workflow() {
        let mut r = make_receipt(2);
        r.workflow = String::from("   ");
        assert!(r.validate().is_err());
    }

    #[test]
    fn stored_receipt_validate_rejects_empty_step_id() {
        let mut r = make_receipt(3);
        r.step_id = String::new();
        assert!(r.validate().is_err());
    }

    #[test]
    fn stored_receipt_validate_rejects_empty_verdict() {
        let mut r = make_receipt(4);
        r.verdict = String::new();
        assert!(r.validate().is_err());
    }

    #[test]
    fn stored_receipt_validate_rejects_wrong_length_manifest_sha256() {
        let mut r = make_receipt(5);
        r.manifest_sha256 = String::from("tooshort");
        assert!(r.validate().is_err());
    }

    #[test]
    fn stored_receipt_validate_rejects_wrong_length_framework_sha256() {
        let mut r = make_receipt(6);
        r.framework_sha256 = String::from("tooshort");
        assert!(r.validate().is_err());
    }

    #[test]
    fn stored_receipt_validate_accepts_empty_sha_anchors() {
        let r = make_receipt(7);
        assert!(r.validate().is_ok());
    }

    #[test]
    fn stored_receipt_validate_accepts_64_char_manifest_sha256() {
        let mut r = make_receipt(8);
        r.manifest_sha256 = "a".repeat(64);
        assert!(r.validate().is_ok());
    }

    #[test]
    fn stored_receipt_validate_accepts_64_char_framework_sha256() {
        let mut r = make_receipt(9);
        r.framework_sha256 = "b".repeat(64);
        assert!(r.validate().is_ok());
    }

    #[test]
    fn stored_receipt_validate_error_contains_e2021_for_empty_workflow() {
        let mut r = make_receipt(10);
        r.workflow = String::new();
        let err = r.validate().unwrap_err();
        assert!(err.to_string().contains("E2021"), "got: {err}");
    }

    // ── StoredReceipt::to_hex_hash / display ──────────────────────────────────

    #[test]
    fn stored_receipt_to_hex_hash_is_64_chars() {
        let r = make_receipt(11);
        assert_eq!(r.to_hex_hash().len(), 64);
    }

    #[test]
    fn stored_receipt_to_hex_hash_matches_hash_to_hex() {
        let r = make_receipt(12);
        assert_eq!(r.to_hex_hash(), r.hash.to_hex());
    }

    #[test]
    fn stored_receipt_display_is_nonempty() {
        let r = make_receipt(13);
        assert!(!format!("{r}").is_empty());
    }

    #[test]
    fn stored_receipt_display_contains_workflow() {
        let r = make_receipt_full(14, "my-workflow", "s1", "PASS");
        assert!(format!("{r}").contains("my-workflow"));
    }

    // ── StoredReceipt::from_fields ────────────────────────────────────────────

    #[test]
    fn from_fields_produces_consistent_hash() {
        let fields =
            hle_core::evidence::receipt_hash::ReceiptHashFields::new("wf", "s1", "PASS", "", "")
                .expect("fields ok");
        let r1 = StoredReceipt::from_fields(&fields, "PASS", 1).expect("from_fields ok");
        let r2 = StoredReceipt::from_fields(&fields, "PASS", 2).expect("from_fields ok");
        assert_eq!(r1.hash, r2.hash);
    }

    #[test]
    fn from_fields_rejects_empty_workflow_via_hash_failure() {
        let bad_fields = hle_core::evidence::receipt_hash::ReceiptHashFields {
            workflow: String::new(),
            step_id: String::from("s1"),
            verdict: String::from("PASS"),
            manifest_sha256: String::new(),
            framework_sha256: String::new(),
        };
        assert!(StoredReceipt::from_fields(&bad_fields, "PASS", 0).is_err());
    }

    // ── ReceiptsStore (NullPool) — stub paths ─────────────────────────────────

    #[test]
    fn append_rejects_zeroed_hash() {
        let store = null_store();
        let mut zeroed = make_receipt(0);
        zeroed.hash = ReceiptHash::zeroed();
        zeroed.appended_at = 0;
        let result = store.append(&zeroed);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("E2020"), "got: {msg}");
    }

    #[test]
    fn stub_append_returns_error_when_pool_not_wired() {
        let store = null_store();
        let r = make_receipt(20);
        let result = store.append(&r);
        assert!(result.is_err());
    }

    #[test]
    fn get_returns_none_for_missing_hash() {
        let store = null_store();
        let mut bytes = [0u8; 32];
        bytes[0] = 0xFF;
        let hash = ReceiptHash::from_bytes(bytes);
        assert_eq!(store.get(hash).expect("get must not error"), None);
    }

    #[test]
    fn exists_returns_false_from_stub() {
        let store = null_store();
        assert!(!store.exists(make_receipt(21).hash).expect("must not error"));
    }

    #[test]
    fn count_returns_zero_from_stub() {
        let store = null_store();
        assert_eq!(store.count().expect("count must not error"), 0);
    }

    #[test]
    fn query_returns_empty_from_stub() {
        let store = null_store();
        let results = store.query(&ReceiptsQuery::new()).expect("must not error");
        assert!(results.is_empty());
    }

    // ── ReceiptsQuery builder ─────────────────────────────────────────────────

    #[test]
    fn receipts_query_limit_clamps_to_one_minimum() {
        let q = ReceiptsQuery::new().with_limit(0);
        assert_eq!(q.limit, 1);
    }

    #[test]
    fn receipts_query_limit_clamps_to_1000_maximum() {
        let q = ReceiptsQuery::new().with_limit(99_999);
        assert_eq!(q.limit, 1000);
    }

    #[test]
    fn receipts_query_default_limit_is_100() {
        assert_eq!(ReceiptsQuery::new().limit, 100);
    }

    #[test]
    fn receipts_query_default_offset_is_zero() {
        assert_eq!(ReceiptsQuery::new().offset, 0);
    }

    #[test]
    fn receipts_query_with_workflow_sets_filter() {
        let q = ReceiptsQuery::new().with_workflow("demo");
        assert_eq!(q.workflow.as_deref(), Some("demo"));
    }

    #[test]
    fn receipts_query_with_step_id_sets_filter() {
        let q = ReceiptsQuery::new().with_step_id("s99");
        assert_eq!(q.step_id.as_deref(), Some("s99"));
    }

    #[test]
    fn receipts_query_with_verdict_sets_filter() {
        let q = ReceiptsQuery::new().with_verdict("FAIL");
        assert_eq!(q.verdict.as_deref(), Some("FAIL"));
    }

    #[test]
    fn receipts_query_with_offset_sets_value() {
        let q = ReceiptsQuery::new().with_offset(50);
        assert_eq!(q.offset, 50);
    }

    #[test]
    fn receipts_query_builder_chaining() {
        let q = ReceiptsQuery::new()
            .with_workflow("wf")
            .with_step_id("s1")
            .with_verdict("PASS")
            .with_limit(10)
            .with_offset(5);
        assert_eq!(q.workflow.as_deref(), Some("wf"));
        assert_eq!(q.step_id.as_deref(), Some("s1"));
        assert_eq!(q.verdict.as_deref(), Some("PASS"));
        assert_eq!(q.limit, 10);
        assert_eq!(q.offset, 5);
    }

    #[test]
    fn receipts_query_default_is_no_filters() {
        let q = ReceiptsQuery::default();
        assert!(q.workflow.is_none());
        assert!(q.step_id.is_none());
        assert!(q.verdict.is_none());
    }

    #[test]
    fn receipts_query_limit_one_is_minimum_after_clamp() {
        let q = ReceiptsQuery::new().with_limit(1);
        assert_eq!(q.limit, 1);
    }

    // ── AppendResult must_use / fields ────────────────────────────────────────

    #[test]
    fn append_result_carries_hash() {
        let mem = make_mem();
        let r = make_receipt(30);
        let result = mem.append(&r).expect("append must succeed");
        assert_eq!(result.hash, r.hash);
    }

    #[test]
    fn append_result_total_count_is_one_after_first_append() {
        let mem = make_mem();
        let result = mem.append(&make_receipt(31)).expect("append ok");
        assert_eq!(result.total_count, 1);
    }

    #[test]
    fn append_result_total_count_increments() {
        let mem = make_mem();
        let _ = mem.append(&make_receipt(32)).expect("append ok");
        let r2 = mem.append(&make_receipt(33)).expect("append ok");
        assert_eq!(r2.total_count, 2);
    }

    // ── MemReceiptsStore — append-only invariant ──────────────────────────────

    #[test]
    fn mem_append_and_get_roundtrip() {
        let mem = make_mem();
        let r = make_receipt(40);
        let _ = mem.append(&r).expect("append ok");
        let retrieved = mem.get(r.hash).expect("must be present");
        assert_eq!(retrieved.hash, r.hash);
        assert_eq!(retrieved.workflow, r.workflow);
    }

    #[test]
    fn mem_append_rejects_zeroed_hash() {
        let mem = make_mem();
        let mut zeroed = make_receipt(0);
        zeroed.hash = ReceiptHash::zeroed();
        assert!(mem.append(&zeroed).is_err());
    }

    #[test]
    fn mem_append_rejects_duplicate_hash() {
        let mem = make_mem();
        let r = make_receipt(41);
        let _ = mem.append(&r).expect("first append ok");
        let err = mem.append(&r).unwrap_err();
        assert!(err.to_string().contains("E2020"), "got: {err}");
    }

    #[test]
    fn mem_get_returns_none_for_absent_hash() {
        let mem = make_mem();
        assert!(mem.get(make_receipt(42).hash).is_none());
    }

    #[test]
    fn mem_exists_false_before_append() {
        let mem = make_mem();
        assert!(!mem.exists(make_receipt(43).hash));
    }

    #[test]
    fn mem_exists_true_after_append() {
        let mem = make_mem();
        let r = make_receipt(44);
        let _ = mem.append(&r).expect("append ok");
        assert!(mem.exists(r.hash));
    }

    #[test]
    fn mem_count_zero_initially() {
        let mem = make_mem();
        assert_eq!(mem.count(), 0);
    }

    #[test]
    fn mem_count_reflects_appends() {
        let mem = make_mem();
        let _ = mem.append(&make_receipt(45)).expect("ok");
        assert_eq!(mem.count(), 1);
        let _ = mem.append(&make_receipt(46)).expect("ok");
        assert_eq!(mem.count(), 2);
    }

    // ── MemReceiptsStore — query ───────────────────────────────────────────────

    #[test]
    fn mem_query_no_filter_returns_all() {
        let mem = make_mem();
        let _ = mem.append(&make_receipt(50)).expect("ok");
        let _ = mem.append(&make_receipt(51)).expect("ok");
        let results = mem.query(&ReceiptsQuery::new());
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn mem_query_workflow_filter_narrows_results() {
        let mem = make_mem();
        let mut r1 = make_receipt(52);
        r1.workflow = String::from("alpha");
        let mut r2 = make_receipt(53);
        r2.workflow = String::from("beta");
        let _ = mem.append(&r1).expect("ok");
        let _ = mem.append(&r2).expect("ok");
        let q = ReceiptsQuery::new().with_workflow("alpha");
        let results = mem.query(&q);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].workflow, "alpha");
    }

    #[test]
    fn mem_query_step_id_filter_narrows_results() {
        let mem = make_mem();
        let mut r1 = make_receipt(54);
        r1.step_id = String::from("step-a");
        let mut r2 = make_receipt(55);
        r2.step_id = String::from("step-b");
        let _ = mem.append(&r1).expect("ok");
        let _ = mem.append(&r2).expect("ok");
        let q = ReceiptsQuery::new().with_step_id("step-a");
        assert_eq!(mem.query(&q).len(), 1);
    }

    #[test]
    fn mem_query_verdict_filter_narrows_results() {
        let mem = make_mem();
        let mut r1 = make_receipt(56);
        r1.verdict = String::from("PASS");
        let mut r2 = make_receipt(57);
        r2.verdict = String::from("FAIL");
        let _ = mem.append(&r1).expect("ok");
        let _ = mem.append(&r2).expect("ok");
        let q = ReceiptsQuery::new().with_verdict("FAIL");
        assert_eq!(mem.query(&q).len(), 1);
    }

    #[test]
    fn mem_query_limit_truncates_results() {
        let mem = make_mem();
        for d in 60..70u8 {
            let _ = mem.append(&make_receipt(d)).expect("ok");
        }
        let q = ReceiptsQuery::new().with_limit(3);
        let results = mem.query(&q);
        assert!(results.len() <= 3);
    }

    #[test]
    fn mem_query_offset_skips_rows() {
        let mem = make_mem();
        for d in 70..74u8 {
            let _ = mem.append(&make_receipt(d)).expect("ok");
        }
        let all = mem.query(&ReceiptsQuery::new());
        let offset_results = mem.query(&ReceiptsQuery::new().with_offset(2));
        assert!(offset_results.len() <= all.len().saturating_sub(2));
    }

    // ── ReceiptsStore debug ───────────────────────────────────────────────────

    #[test]
    fn receipts_store_debug_is_nonempty() {
        let store = null_store();
        assert!(!format!("{store:?}").is_empty());
    }

    #[test]
    fn receipts_store_debug_contains_pool_id() {
        let store = null_store();
        assert!(format!("{store:?}").contains("null-pool"));
    }
}
