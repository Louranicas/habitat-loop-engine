# M036 Runbook Manual Evidence — `crates/hle-runbook/src/manual_evidence.rs`

> **Layer:** L07 | **Cluster:** C06 Runbook Semantics | **Error Codes:** 2570
> **Role:** Operator evidence attachment model — hash-bound, immutable, ledger-registered.
> **LOC target:** ~260 | **Test target:** ≥50

---

## Purpose

M036 defines the model for evidence that a human operator attaches to a runbook phase. Unlike probe output (machine-generated, automated), manual evidence is attached explicitly: a log file, a screenshot reference, a command transcript, or a brief note. The critical design rule is that SHA-256 is computed at construction time. Once a `ManualEvidence` instance is created, its content hash cannot change. This makes the evidence chain tamper-evident without a dedicated signature infrastructure.

Manual evidence integrates with M025 (`evidence_store` from C05 Persistence Ledger) for persistence, and with M001 (`receipt_hash` from C01 Evidence Integrity) for hash computation. The receipt ID returned by the evidence store is stored in the `ManualEvidence` instance, making the ledger the source of truth for retrieval.

---

## Types at a Glance

| Type | Kind | Notes |
|------|------|-------|
| `ManualEvidence` | struct | Single operator-attached evidence item; immutable after construction |
| `EvidenceKind` | enum | Classifies the nature of the attached evidence |
| `EvidenceAttachment` | enum | Content-or-path union at construction time |
| `EvidenceBuilder` | struct | Builder for `ManualEvidence`; computes SHA-256 before returning |
| `EvidenceStore` | trait | Abstraction for persisting evidence (implemented by C05 storage) |
| `EvidenceError` | enum | Single variant covering code 2570 |

---

## Enum: `EvidenceKind`

```rust
/// Classifies the nature of operator-attached evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceKind {
    /// Raw command output captured from a terminal session.
    CommandOutput,
    /// Path to a log file on the local filesystem.
    LogFile,
    /// Inline operator note (text written directly into the runbook).
    OperatorNote,
    /// Reference to an external artifact (URL, Obsidian note, ticket ID).
    ExternalReference,
    /// Screenshot or image path (binary; content hash covers the file bytes).
    Screenshot,
    /// Structured JSON/TOML/YAML output from a probe or tool.
    StructuredOutput,
}
```

| Method | Signature | Notes |
|--------|-----------|-------|
| `as_str` | `const fn(&self) -> &'static str` | "command_output" / "log_file" / etc. |
| `is_binary` | `const fn(&self) -> bool` | True for `Screenshot` |
| `is_inline` | `const fn(&self) -> bool` | True for `CommandOutput`, `OperatorNote`, `StructuredOutput` |

---

## Enum: `EvidenceAttachment`

```rust
/// Holds the evidence content at construction time.
///
/// The `Path` variant references a filesystem path; the content bytes are read
/// during `EvidenceBuilder::build` for SHA-256 computation and then released.
/// Only the path string and hash are retained in `ManualEvidence`.
#[derive(Debug, Clone)]
pub enum EvidenceAttachment {
    /// Inline content — small enough to store directly.
    Inline(String),
    /// Filesystem path — large content; only path + hash retained after build.
    Path(std::path::PathBuf),
}
```

---

## Struct: `ManualEvidence`

```rust
/// A single operator-attached evidence item for a runbook phase.
///
/// # Invariants
/// - `sha256` equals `sha256(content_or_path_bytes)` as of `attached_at`.
/// - No mutation methods exist; this struct is immutable after construction.
/// - `receipt_id` is None until `EvidenceStore::persist` is called.
#[derive(Debug, Clone, PartialEq)]
pub struct ManualEvidence {
    /// Unique evidence identifier (UUID-v4 generated at construction).
    pub id: String,
    /// Nature of the attached evidence.
    pub kind: EvidenceKind,
    /// For inline evidence: the content string. For path evidence: the path as a string.
    pub content_or_path: String,
    /// SHA-256 hex digest of `content_or_path_bytes` at construction time.
    pub sha256: String,
    /// Timestamp when the evidence was attached (foundation `Timestamp`).
    pub attached_at: Timestamp,
    /// Agent or operator who attached this evidence.
    pub attached_by: AgentId,
    /// Receipt ID assigned by the persistence layer after `EvidenceStore::persist`.
    /// None until the evidence has been persisted.
    pub receipt_id: Option<String>,
    /// Human note describing what this evidence shows.
    pub description: Option<String>,
}
```

| Method | Signature | Notes |
|--------|-----------|-------|
| `is_persisted` | `fn(&self) -> bool` | `receipt_id.is_some()` |
| `verify_hash` | `fn(&self) -> Result<(), EvidenceError>` | Recomputes SHA-256 for inline content; rejects path evidence (must re-read file externally) |
| `age` | `fn(&self, now: Timestamp) -> u64` | Ticks since `attached_at` |

**Traits:** `Display` ("Evidence(id=X, kind=CommandOutput, sha256=abc...123, persisted=true)")

---

## Struct: `EvidenceBuilder`

```rust
/// Builder for `ManualEvidence`. Computes SHA-256 before returning.
///
/// All methods are `#[must_use]`. `build` is the only fallible step.
pub struct EvidenceBuilder {
    /* private fields */
}

impl EvidenceBuilder {
    /// Create a builder for inline content evidence.
    #[must_use]
    pub fn inline(kind: EvidenceKind, content: impl Into<String>) -> Self;

    /// Create a builder for filesystem path evidence.
    /// The file is read during `build()` for hash computation.
    #[must_use]
    pub fn path(kind: EvidenceKind, path: std::path::PathBuf) -> Self;

    #[must_use]
    pub fn attached_by(self, agent: AgentId) -> Self;

    #[must_use]
    pub fn description(self, desc: impl Into<String>) -> Self;

    /// Compute SHA-256, generate UUID, set timestamp, and return `ManualEvidence`.
    ///
    /// For `path` builders, reads the file content to compute the hash.
    /// Returns `Err(EvidenceError::HashMismatch)` if the file cannot be read.
    pub fn build(self) -> Result<ManualEvidence, EvidenceError>;
}
```

---

## Trait: `EvidenceStore`

```rust
/// Abstraction for persisting manual evidence. Implemented by C05 `evidence_store`.
///
/// This trait is defined in M036 so C06 does not take a hard compile-time dependency
/// on C05 storage types. The concrete impl is injected at the call site.
pub trait EvidenceStore: Send + Sync {
    /// Persist the evidence and return a receipt ID.
    ///
    /// The receipt ID is assigned by the storage layer and stored in
    /// `ManualEvidence::receipt_id` by the caller after this returns.
    fn persist(
        &self,
        evidence: &ManualEvidence,
    ) -> Result<String, EvidenceError>;

    /// Retrieve evidence by receipt ID.
    fn get(&self, receipt_id: &str) -> Result<ManualEvidence, EvidenceError>;
}
```

---

## SHA-256 Computation

SHA-256 is computed using the `sha2` crate (already in the workspace dependency graph via C01). The computation is:

```rust
use sha2::{Digest, Sha256};

fn compute_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
```

For inline evidence: `bytes = content.as_bytes()`.
For path evidence: `bytes = std::fs::read(path)?`.

The `sha256` field in `ManualEvidence` is always a 64-character lowercase hex string.

---

## Error: `EvidenceError`

```rust
/// Errors produced by evidence operations. Error code 2570.
#[derive(Debug)]
pub enum EvidenceError {
    /// Code 2570 — Recomputed SHA-256 does not match stored hash.
    HashMismatch {
        evidence_id: String,
        stored: String,
        recomputed: String,
    },
    /// Code 2570 variant — File could not be read for hash computation.
    FileRead {
        path: String,
        reason: String,
    },
    /// Code 2570 variant — Persistence layer rejected the evidence.
    PersistFailed { reason: String },
}
```

`EvidenceError` implements `ErrorClassifier`:
- `HashMismatch` → code 2570, severity High, retryable=false
- `FileRead` → code 2570, severity Medium, retryable=true, transient=true
- `PersistFailed` → code 2570, severity Medium, retryable=true

---

## Design Notes

- `ManualEvidence` has no `set_*` methods. The SHA-256 is computed once in `EvidenceBuilder::build` and never recalculated on the struct itself. `verify_hash` on inline evidence is a defensive check that re-hashes the stored content string — it does not update the stored hash.
- The `EvidenceAttachment` enum is private to the builder and does not appear in the public `ManualEvidence` struct. After construction, the attachment is collapsed to `content_or_path: String` + `sha256: String`. This simplifies serialization and prevents accidental re-reading of large files.
- `EvidenceStore` is a trait (not a concrete type) so that tests can inject a `Vec`-backed in-memory store without pulling in SQLite. The production impl lives in C05 `evidence_store`.
- The `uuid` crate (already used by M035) generates evidence item IDs. No additional dependency.
- Evidence attached to replay fixtures (M038) uses `AgentId::system()` as `attached_by` because the attachment is automated by the replay harness, not an operator.

---

## Cluster Invariants (this module)

- `ManualEvidence::sha256` always equals `sha256(content_or_path_bytes)` at the time of `EvidenceBuilder::build`. No mutation method changes it thereafter.
- `EvidenceBuilder::build` for path evidence returns `Err(EvidenceError::FileRead)` if the file does not exist or is not readable. It does not create a `ManualEvidence` with a placeholder hash.
- `EvidenceStore::persist` is the only method that may set `ManualEvidence::receipt_id`. The evidence struct itself does not have a `set_receipt_id` method.
- `verify_hash` is only available for inline evidence (returns a descriptive error for path evidence instructing the caller to re-read the file).

---

*M036 Runbook Manual Evidence | C06 Runbook Semantics | Habitat Loop Engine | 2026-05-10*
