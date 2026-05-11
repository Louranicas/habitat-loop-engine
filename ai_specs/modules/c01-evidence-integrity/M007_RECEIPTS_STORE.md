# M007 receipts_store — receipts_store.rs

> **File:** `crates/hle-storage/src/receipts_store.rs` | **LOC:** ~380 | **Tests:** ~40
> **Role:** Append-only receipt persistence

---

## Types at a Glance

| Type | Kind | Copy | Hash | Const | Purpose |
|---|---|---|---|---|---|
| `ReceiptsStore` | struct | No | No | No | Append-only, hash-keyed receipt persistence |
| `StoredReceipt` | struct | No | No | No | Full persisted receipt record including both hash anchors |
| `ReceiptsQuery` | struct | No | No | No | Typed query parameters for retrieval |
| `AppendResult` | struct | No | No | No | Confirmation record returned by `append()` |

---

## ReceiptsStore

```rust
pub struct ReceiptsStore {
    pool: Arc<hle_storage::pool::Pool>,
}
```

`ReceiptsStore` is the only durable surface for receipt data in C01. It holds a
reference-counted handle to the shared database pool from M021 (C05). All writes go
through WAL-mode SQLite with a single append operation; no row is ever updated or
deleted after insertion.

| Method | Signature | Notes |
|---|---|---|
| `new` | `fn(pool: Arc<Pool>) -> Self` | `#[must_use]` |
| `append` | `fn(&self, receipt: &StoredReceipt) -> Result<AppendResult, HleError>` | Write path; errors E2020 if `receipt.hash` already exists; errors E2021 on I/O |
| `get` | `fn(&self, hash: ReceiptHash) -> Result<Option<StoredReceipt>, HleError>` | `#[must_use]` — read path; `None` means not found (not an error) |
| `exists` | `fn(&self, hash: ReceiptHash) -> Result<bool, HleError>` | `#[must_use]` — lightweight presence check without fetching the full record |
| `query` | `fn(&self, q: &ReceiptsQuery) -> Result<Vec<StoredReceipt>, HleError>` | `#[must_use]` — filtered list; always bounded by `q.limit` |
| `count` | `fn(&self) -> Result<u64, HleError>` | `#[must_use]` — total row count |

**No `update`, `delete`, `truncate`, or `upsert` methods exist on this type.** Any
attempt to add such methods is an architectural violation of the append-only principle.

**Traits implemented:** `Debug` (shows pool address, not contents)

---

## StoredReceipt

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredReceipt {
    /// Canonical SHA-256 hash of the receipt fields. Primary key.
    pub hash: ReceiptHash,
    /// Workflow identifier.
    pub workflow: String,
    /// Step identifier within the workflow.
    pub step_id: String,
    /// Verifier verdict string: "PASS", "FAIL", or "AWAITING_HUMAN".
    pub verdict: String,
    /// Scaffold manifest hash per HARNESS_CONTRACT.md `^Manifest_sha256`.
    pub manifest_sha256: String,
    /// Source/framework provenance hash per HARNESS_CONTRACT.md `^Framework_sha256`.
    pub framework_sha256: String,
    /// Monotonic append counter (no wall clock; no chrono/SystemTime).
    pub appended_at: u64,
    /// Optional locator for counter-evidence (from schemas/receipt.schema.json).
    pub counter_evidence_locator: Option<String>,
}
```

| Method | Signature | Notes |
|---|---|---|
| `from_fields` | `fn(fields: &ReceiptHashFields, verdict, appended_at) -> Result<Self, HleError>` | `#[must_use]` — calls `ReceiptHash::from_fields`; propagates E2000 |
| `validate` | `fn(&self) -> Result<(), HleError>` | `#[must_use]` — checks non-empty workflow/step_id/verdict; validates hex len of hash fields |
| `to_hex_hash` | `fn(&self) -> String` | `#[must_use]` — delegates to `self.hash.to_hex()` |

**Traits implemented:** `Display` ("StoredReceipt(3a7f9c…:PASS@demo/s1)")

---

## ReceiptsQuery

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptsQuery {
    /// Filter by workflow name. `None` means all workflows.
    pub workflow: Option<String>,
    /// Filter by step identifier. `None` means all steps.
    pub step_id: Option<String>,
    /// Filter by verdict string. `None` means all verdicts.
    pub verdict: Option<String>,
    /// Maximum rows to return. Clamped to 1..=1000 at construction.
    pub limit: usize,
    /// Offset for pagination. Default 0.
    pub offset: usize,
}
```

| Method | Signature | Notes |
|---|---|---|
| `new` | `const fn() -> Self` | `#[must_use]` — default: no filters, limit=100, offset=0 |
| `with_workflow` | `fn(self, workflow: impl Into<String>) -> Self` | `#[must_use]` — builder chain |
| `with_step_id` | `fn(self, step_id: impl Into<String>) -> Self` | `#[must_use]` — builder chain |
| `with_verdict` | `fn(self, verdict: impl Into<String>) -> Self` | `#[must_use]` — builder chain |
| `with_limit` | `fn(self, limit: usize) -> Self` | `#[must_use]` — clamps to 1..=1000 |
| `with_offset` | `fn(self, offset: usize) -> Self` | `#[must_use]` — builder chain |

---

## AppendResult

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppendResult {
    /// The hash of the appended receipt (confirmation).
    pub hash: ReceiptHash,
    /// Monotonic counter value at time of append.
    pub appended_at: u64,
    /// Row count after append (for observability).
    pub total_count: u64,
}
```

`AppendResult` is `#[must_use]` — callers must consume or explicitly discard it.
It is the only acknowledgment that an append actually landed. If `append()` returns
`Ok(result)`, the receipt is durable in the WAL-committed SQLite row.

---

## SQL Schema (Proposed)

```sql
CREATE TABLE IF NOT EXISTS receipts (
    hash                    TEXT NOT NULL PRIMARY KEY,  -- 64-char hex
    workflow                TEXT NOT NULL,
    step_id                 TEXT NOT NULL,
    verdict                 TEXT NOT NULL,
    manifest_sha256         TEXT NOT NULL,
    framework_sha256        TEXT NOT NULL,
    appended_at             INTEGER NOT NULL,           -- monotonic u64
    counter_evidence_locator TEXT                       -- nullable
) STRICT;
```

`PRIMARY KEY` on `hash` enforces uniqueness at the database level in addition to
the `exists()` check performed before `INSERT`. This provides defense in depth for
the E2020 `AppendConflict` error.

---

## Design Notes

- `ReceiptsStore` lives in `hle-storage`, which depends on `hle-core` for
  `ReceiptHash` and `HleError` but does NOT depend on `hle-verifier`. Storage
  is neutral — it holds bytes, not verdicts.
- The pool dependency comes from M021 (C05 `pool`). `ReceiptsStore::new` takes
  `Arc<Pool>` so multiple stores can share one pool without double-initialization.
- `append()` performs the `exists()` check inside a transaction to prevent
  time-of-check/time-of-use races on the uniqueness constraint.
- `query()` bounds `limit` to 1..=1000 to prevent unbounded result sets. Callers
  needing full enumeration must paginate via `offset`.
- There is no streaming or cursor API. C01 storage is for bounded retrieval;
  large-scale analytics belong in an observability layer, not here.
- `appended_at` is a monotonic `u64` counter, not wall-clock time. It provides
  causal ordering within a process run but does not provide cross-process timestamps.
- `counter_evidence_locator` maps directly to the `counter_evidence_locator` field
  in `schemas/receipt.schema.json` (required field in the schema, optional in Rust
  because legacy receipts may omit it).

---

## Cluster Invariants

- **Append-only.** There is no `update()`, `delete()`, `upsert()`, or `truncate()`
  method. Adding any such method constitutes an architectural regression.
- **Hash as primary key.** `StoredReceipt::hash` is the sole lookup key. No
  integer surrogate key is used.
- **Dual-hash anchors.** Every `StoredReceipt` carries both `manifest_sha256` and
  `framework_sha256` matching the `^Manifest_sha256` / `^Framework_sha256` split
  anchors from `HARNESS_CONTRACT.md`.
- **HLE-UP-001.** `receipts_store.rs` is in `hle-storage`, not `hle-verifier`.
  It cannot emit verdicts or transition claim states. See
  [UP_EXECUTOR_VERIFIER_SPLIT](../../../ai_docs/use_patterns/UP_EXECUTOR_VERIFIER_SPLIT.md).
- **Zeroed hash rejection.** `ReceiptsStore::append()` must reject any receipt
  whose `hash == ReceiptHash::zeroed()` with error E2020.

---

*M007 receipts_store Spec v1.0 | 2026-05-10*
