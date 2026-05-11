# M029 Evidence Store — Bounded Evidence Blob and Filesystem Index

> **Module ID:** M029 | **Cluster:** C05_PERSISTENCE_LEDGER | **Layer:** L02
> **Source:** `crates/hle-storage/src/evidence_store.rs`
> **Error Codes:** 2440–2442
> **Role:** Append-only store for evidence artifacts. Accepts either an inline blob (capped
> at `MAX_EVIDENCE_BYTES`) or a filesystem path + SHA-256 hash pair. Every row is keyed by
> its SHA-256 hash — duplicate SHAs are rejected, making the store a content-addressable
> evidence surface. No UPDATE or DELETE is ever issued by this module's public API.

---

## Types at a Glance

| Type | Kind | Purpose |
|------|------|---------|
| `EvidenceKind` | enum | Classifies what artifact this evidence represents |
| `EvidenceBody` | enum | Inline blob or filesystem path + SHA pair |
| `EvidenceRecord` | struct | Deserialized row from `evidence_store` |
| `NewEvidence` | struct | Insert descriptor — caller-assembled before insert |
| `EvidenceStoreConfig` | struct | `MAX_EVIDENCE_BYTES` and optional root path |
| `EvidenceStore` | struct | Table abstraction over `Arc<dyn Pool>` |

---

## Constants

```rust
/// Maximum inline blob size in bytes. Blobs larger than this must be stored on the
/// filesystem and referenced by path + SHA.
pub const MAX_EVIDENCE_BYTES: usize = 512 * 1024; // 512 KiB
```

---

## `EvidenceKind` Enum

```rust
/// Classifies the type of artifact this evidence row represents.
/// Stored as a text column; new variants may be added in future migrations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EvidenceKind {
    /// Raw command output (stdout/stderr captured as a blob).
    CommandOutput,
    /// File at a specific filesystem path; body is a `FilePath` variant.
    FilePath,
    /// JSON-structured test result summary.
    TestReport,
    /// Cargo / compiler diagnostic output.
    DiagnosticOutput,
    /// Verifier-generated receipt (links verifier_results_store rows).
    VerifierReceipt,
    /// Human-authored notes attached during `AwaitingHuman` resolution.
    HumanNote,
}

impl EvidenceKind {
    /// Canonical database string for this variant.
    #[must_use]
    pub const fn as_str(self) -> &'static str;

    /// Parse from the database string.
    ///
    /// # Errors
    /// Returns [`StorageError::EvidenceKindUnknown`] (2441) for unrecognised strings.
    pub fn from_str(s: &str) -> Result<Self, StorageError>;
}

impl std::fmt::Display for EvidenceKind { /* delegates to as_str */ }
```

---

## `EvidenceBody` Enum

```rust
/// Payload of an evidence row: either an inline blob or an external path + hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceBody {
    /// Inline byte content. Must be `<= MAX_EVIDENCE_BYTES`.
    Blob(Vec<u8>),
    /// Absolute filesystem path to the artifact, plus its pre-computed SHA-256.
    /// The path must exist at insert time (verified by the store).
    FilePath {
        /// Absolute path to the artifact on disk.
        path: std::path::PathBuf,
        /// SHA-256 of the file content at insert time (hex-encoded, 64 chars).
        sha256_hex: String,
    },
}

impl EvidenceBody {
    /// Return the SHA-256 (hex) of this body.
    ///
    /// For `Blob`, computes SHA-256 of the byte content.
    /// For `FilePath`, returns the pre-supplied `sha256_hex`.
    ///
    /// # Errors
    /// Returns [`StorageError::EvidenceSizeExceeded`] if blob exceeds `MAX_EVIDENCE_BYTES`.
    pub fn sha256_hex(&self) -> Result<String, StorageError>;

    /// Byte length: blob length, or 0 for `FilePath` (content is on disk).
    #[must_use]
    pub fn inline_len(&self) -> usize;
}
```

---

## `EvidenceRecord` Struct

```rust
/// Deserialized row from the `evidence_store` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceRecord {
    pub id: i64,
    pub run_id: i64,
    /// The tick at which this evidence was recorded (`workflow_ticks.id`).
    /// Links this row to a causal position in the tick ledger.
    pub tick_id: i64,
    pub kind: EvidenceKind,
    /// Hex-encoded SHA-256 of the content. Primary content-addressable key.
    pub sha256_hex: String,
    /// Inline blob content. `None` when body is a `FilePath`.
    pub blob: Option<Vec<u8>>,
    /// Absolute filesystem path. `None` when body is a `Blob`.
    pub fs_path: Option<String>,
    /// Unix epoch seconds — supplied by caller.
    pub created_unix: i64,
    /// If this row supersedes an earlier evidence row, that row's id.
    pub supersedes_id: Option<i64>,
}
```

---

## `NewEvidence` Struct

```rust
/// Data required to insert a new evidence row.
#[derive(Debug, Clone)]
pub struct NewEvidence {
    pub run_id: i64,
    pub tick_id: i64,
    pub kind: EvidenceKind,
    pub body: EvidenceBody,
    /// Unix epoch seconds — supplied by caller, not by the store.
    pub created_unix: i64,
    /// Optional supersession pointer. When set, the referenced row must exist.
    pub supersedes_id: Option<i64>,
}
```

---

## `EvidenceStoreConfig` Struct

```rust
/// Configuration for the evidence store. Injected at construction.
#[derive(Debug, Clone)]
pub struct EvidenceStoreConfig {
    /// Override for the inline blob size cap. Defaults to [`MAX_EVIDENCE_BYTES`].
    pub max_blob_bytes: usize,
}

impl EvidenceStoreConfig {
    #[must_use]
    pub const fn new() -> Self;

    #[must_use]
    pub const fn with_max_blob_bytes(self, n: usize) -> Self;
}

impl Default for EvidenceStoreConfig {
    fn default() -> Self { Self::new() }
}
```

---

## `EvidenceStore` Struct and Methods

```rust
#[derive(Debug, Clone)]
pub struct EvidenceStore {
    pool: std::sync::Arc<dyn Pool>,
    config: EvidenceStoreConfig,
}

impl EvidenceStore {
    /// Construct the store. Pool must have migrations applied.
    #[must_use]
    pub fn new(pool: std::sync::Arc<dyn Pool>) -> Self;

    /// Construct the store with explicit configuration.
    #[must_use]
    pub fn with_config(pool: std::sync::Arc<dyn Pool>, config: EvidenceStoreConfig) -> Self;

    /// Insert a new evidence row. Returns the assigned `id`.
    ///
    /// Validation sequence (all must pass before INSERT):
    /// 1. If body is `Blob`: size must be `<= config.max_blob_bytes` (2440).
    /// 2. If body is `FilePath`: path must exist on disk (2441).
    /// 3. Compute / verify `sha256_hex`.
    /// 4. Reject duplicate `sha256_hex` (2442).
    ///
    /// # Errors
    /// - [`StorageError::EvidenceSizeExceeded`] (2440) if blob is too large.
    /// - [`StorageError::EvidenceKindUnknown`] (2441) if `kind` cannot be serialised.
    /// - [`StorageError::EvidenceInsert`] (2442) on SQL failure or SHA conflict.
    pub fn insert(&self, ev: &NewEvidence) -> Result<i64, StorageError>;

    /// Fetch a single evidence row by its database `id`.
    ///
    /// # Errors
    /// Returns [`StorageError::Storage`] (2499) on query failure.
    /// Returns `Ok(None)` if not found.
    pub fn get_by_id(&self, id: i64) -> Result<Option<EvidenceRecord>, StorageError>;

    /// Fetch a single evidence row by its SHA-256 hex.
    /// Used by M030 (verifier_results_store) to look up the receipt being certified.
    ///
    /// # Errors
    /// Returns `Ok(None)` if no row has this SHA. [`StorageError::Storage`] (2499) on failure.
    pub fn get_by_sha(&self, sha256_hex: &str) -> Result<Option<EvidenceRecord>, StorageError>;

    /// Fetch all evidence rows for a run, ordered by `created_unix` ascending.
    ///
    /// # Errors
    /// Returns [`StorageError::Storage`] (2499) on query failure.
    pub fn list_for_run(&self, run_id: i64) -> Result<Vec<EvidenceRecord>, StorageError>;

    /// Fetch all evidence rows for a run filtered by kind.
    ///
    /// # Errors
    /// Returns [`StorageError::Storage`] (2499) on query failure.
    pub fn list_for_run_by_kind(
        &self,
        run_id: i64,
        kind: EvidenceKind,
    ) -> Result<Vec<EvidenceRecord>, StorageError>;

    /// Check whether a SHA-256 already exists in the store.
    ///
    /// # Errors
    /// Returns [`StorageError::Storage`] (2499) on query failure.
    pub fn sha_exists(&self, sha256_hex: &str) -> Result<bool, StorageError>;

    /// Delete evidence rows whose `created_unix` is older than the retention window.
    /// Predicate: `created_unix < (strftime('%s','now') - retention_secs)`.
    ///
    /// **Never uses a literal integer** (framework §17.5).
    ///
    /// # Errors
    /// Returns [`StorageError::Storage`] (2499) on query failure.
    pub fn delete_older_than(&self, retention_secs: i64) -> Result<usize, StorageError>;
}
```

---

## Schema DDL (planned migration `0003_evidence_store.sql`)

```sql
CREATE TABLE IF NOT EXISTS evidence_store (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id         INTEGER NOT NULL REFERENCES workflow_runs(id),
    tick_id        INTEGER NOT NULL REFERENCES workflow_ticks(id),
    kind           TEXT    NOT NULL,
    sha256_hex     TEXT    NOT NULL UNIQUE,
    blob           BLOB,
    fs_path        TEXT,
    created_unix   INTEGER NOT NULL,
    supersedes_id  INTEGER REFERENCES evidence_store(id)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_evidence_run_id   ON evidence_store(run_id);
CREATE INDEX IF NOT EXISTS idx_evidence_tick_id  ON evidence_store(tick_id);
CREATE INDEX IF NOT EXISTS idx_evidence_sha      ON evidence_store(sha256_hex);

-- Constraint: blob and fs_path are mutually exclusive
-- Enforced at application layer (EvidenceBody::sha256_hex validation).
```

---

## Append-Only Enforcement

M029 exposes no `update` or `delete` methods in its public API. The only permitted
mutation post-insert is the TTL sweep (`delete_older_than`), which is a bounded,
retention-policy operation, not a correction path. All corrections are expressed as new
`NewEvidence` rows with `supersedes_id` pointing to the replaced row.

---

## SQL Patterns

```sql
-- insert blob
INSERT INTO evidence_store
    (run_id, tick_id, kind, sha256_hex, blob, fs_path, created_unix, supersedes_id)
VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7);

-- insert filepath
INSERT INTO evidence_store
    (run_id, tick_id, kind, sha256_hex, blob, fs_path, created_unix, supersedes_id)
VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?7);

-- get by SHA (used by verifier_results_store FK check)
SELECT * FROM evidence_store WHERE sha256_hex = ?1 LIMIT 1;

-- TTL sweep — retention_secs is a bound parameter, never a literal
DELETE FROM evidence_store WHERE created_unix < (strftime('%s','now') - ?1);
```

---

## Method/Trait Table

| Method | Returns | Error Codes |
|--------|---------|-------------|
| `insert` | `Result<i64, StorageError>` | 2440, 2441, 2442 |
| `get_by_id` | `Result<Option<EvidenceRecord>, StorageError>` | 2499 |
| `get_by_sha` | `Result<Option<EvidenceRecord>, StorageError>` | 2499 |
| `list_for_run` | `Result<Vec<EvidenceRecord>, StorageError>` | 2499 |
| `list_for_run_by_kind` | `Result<Vec<EvidenceRecord>, StorageError>` | 2499 |
| `sha_exists` | `Result<bool, StorageError>` | 2499 |
| `delete_older_than` | `Result<usize, StorageError>` | 2499 |

---

## Design Notes

1. **Content-addressable by SHA.** `sha256_hex` has a UNIQUE constraint. This prevents
   accidental double-insertion of the same artifact and enables M030 to reference evidence
   without a join — just the SHA is sufficient to validate the link.

2. **Blob cap is configurable in tests.** `EvidenceStoreConfig::with_max_blob_bytes` allows
   tests to use tiny caps (e.g., 16 bytes) to exercise the size-exceeded path without
   allocating large buffers.

3. **Filesystem path validation at insert time.** A `FilePath` body whose `path` does not
   exist on disk at insert time is rejected with error 2441. This prevents phantom evidence
   references that a verifier would later fail to locate.

4. **`supersedes_id` enables corrections without mutation.** When evidence needs to be
   corrected (e.g., a file was re-hashed after modification), a new row is inserted with
   `supersedes_id` pointing to the old row. The old row remains readable for audit.

5. **No time calls inside the store.** `created_unix` is always caller-supplied.

6. **Cross-cluster.** M030 (`verifier_results_store`) stores a `receipt_sha` that must
   match a `sha256_hex` in this table. M030 validates that match via `get_by_sha` before
   inserting a verdict row.

---

## Test Targets (minimum 50)

- `insert_blob_succeeds`: blob within limit inserts, returns positive id
- `insert_blob_too_large_rejected`: blob above `MAX_EVIDENCE_BYTES` returns 2440
- `insert_blob_at_exact_limit`: blob of exactly `MAX_EVIDENCE_BYTES` succeeds
- `insert_filepath_existing_path`: FilePath with existing file inserts successfully
- `insert_filepath_missing_rejected`: FilePath with non-existent path returns 2441
- `insert_duplicate_sha_rejected`: second insert with same sha256_hex returns 2442
- `sha_exists_true_after_insert`: sha_exists returns true after successful insert
- `sha_exists_false_before_insert`: sha_exists returns false for unknown SHA
- `get_by_id_round_trip`: insert then get_by_id returns matching record
- `get_by_id_absent`: get_by_id for missing id returns Ok(None)
- `get_by_sha_found`: get_by_sha returns the correct record
- `get_by_sha_absent`: get_by_sha for unknown SHA returns Ok(None)
- `list_for_run_ordered`: list_for_run returns rows ascending by created_unix
- `list_for_run_empty`: empty vec for run with no evidence
- `list_for_run_by_kind_filters`: only requested kind returned
- `list_for_run_by_kind_none_match`: empty vec when no rows of that kind
- `supersedes_id_accepted`: insert with valid supersedes_id succeeds
- `supersedes_id_absent_accepted`: None supersedes_id is valid
- `delete_older_than_removes_old`: rows older than retention swept
- `delete_older_than_spares_recent`: rows within retention window not swept
- `kind_as_str_round_trip`: all EvidenceKind variants round-trip through from_str/as_str
- `kind_unknown_string_rejected`: EvidenceKindUnknown (2441) for random string
- `config_custom_max_blob_bytes`: custom cap enforced by store
- `evidence_body_sha256_blob`: SHA-256 is computed correctly for blob body
- `evidence_body_sha256_filepath`: SHA-256 returned from FilePath is the pre-supplied value

---

*M029 Evidence Store Spec v1.0 | C05_PERSISTENCE_LEDGER | habitat-loop-engine*
