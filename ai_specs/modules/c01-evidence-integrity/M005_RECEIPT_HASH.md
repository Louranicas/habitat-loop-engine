# M005 receipt_hash — receipt_hash.rs

> **File:** `crates/hle-core/src/evidence/receipt_hash.rs` | **LOC:** ~280 | **Tests:** ~35
> **Role:** Canonical receipt hashing; source of all proof identity

---

## Types at a Glance

| Type | Kind | Copy | Hash | Const | Purpose |
|---|---|---|---|---|---|
| `ReceiptHash` | newtype(`[u8; 32]`) | Yes | Yes | No | SHA-256 digest of canonical receipt fields |
| `ReceiptHashFields` | struct | No | No | No | Builder input: the exact fields fed to the hash function |
| `HashAlgorithm` | enum | Yes | Yes | Yes | Algorithm tag (currently only `Sha256` variant) |

---

## ReceiptHash

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ReceiptHash([u8; 32]);
```

`ReceiptHash` is the single source of proof identity in C01. Every receipt stored in
M007, every claim keyed in M006, and every recompute performed by M008 resolves to
this type. It corresponds directly to the `manifest_sha256` and `framework_sha256`
split-hash anchors defined in `HARNESS_CONTRACT.md` (anchors `^Manifest_sha256` and
`^Framework_sha256`) and satisfies the `schemas/receipt.schema.json` field pattern
`^[0-9a-f]{64}$`.

| Method | Signature | Notes |
|---|---|---|
| `from_bytes` | `const fn(bytes: [u8; 32]) -> Self` | `#[must_use]` — raw constructor for deserialization |
| `from_fields` | `fn(fields: &ReceiptHashFields) -> Result<Self, HleError>` | `#[must_use]` — canonical hashing path; errors on empty workflow name |
| `as_bytes` | `const fn(&self) -> &[u8; 32]` | `#[must_use]` — raw byte access |
| `to_hex` | `fn(&self) -> String` | `#[must_use]` — 64-char lowercase hex; matches schema `^[0-9a-f]{64}$` |
| `from_hex` | `fn(hex: &str) -> Result<Self, HleError>` | `#[must_use]` — parses 64-char lowercase hex; error code E2000 on malform |
| `zeroed` | `const fn() -> Self` | `#[must_use]` — sentinel `[0u8; 32]`; use only in tests/negative controls |

**Traits implemented:**

| Trait | Notes |
|---|---|
| `Display` | Emits first 16 hex chars + `…` for log brevity: `"3a7f9c…"` |
| `serde::Serialize` | Serializes as hex string (matches JSON schema) |
| `serde::Deserialize` | Deserializes from hex string; validates 64 chars |
| `AsRef<[u8]>` | Delegates to inner `[u8; 32]` |

---

## ReceiptHashFields

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptHashFields {
    /// Workflow identifier. Must be non-empty.
    pub workflow: String,
    /// Step identifier within the workflow.
    pub step_id: String,
    /// Verifier verdict string (e.g. "PASS", "FAIL", "AWAITING_HUMAN").
    pub verdict: String,
    /// `manifest_sha256` anchor value — scaffold manifest hash per HARNESS_CONTRACT.md.
    pub manifest_sha256: String,
    /// `framework_sha256` anchor value — source/framework provenance hash per HARNESS_CONTRACT.md.
    pub framework_sha256: String,
}
```

| Method | Signature | Notes |
|---|---|---|
| `new` | `fn(workflow, step_id, verdict, manifest_sha256, framework_sha256) -> Result<Self, HleError>` | `#[must_use]` — validates non-empty workflow; error code E2000 |
| `canonical_bytes` | `fn(&self) -> Vec<u8>` | `#[must_use]` — deterministic serialization fed to SHA-256; fields joined with `\x00` null byte separator |

**Traits implemented:** `Display` ("Fields(workflow/step_id)")

---

## HashAlgorithm

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HashAlgorithm {
    /// SHA-256 producing a 32-byte digest. Only supported variant.
    Sha256,
}
```

| Method | Signature | Notes |
|---|---|---|
| `as_str` | `const fn(self) -> &'static str` | `#[must_use]` — returns `"sha256"` |
| `digest_len` | `const fn(self) -> usize` | `#[must_use]` — returns `32` |

**Traits implemented:** `Display` ("sha256")

---

## Design Notes

- `ReceiptHash` is `Copy` intentionally: passing it by value to claim stores,
  verifiers, and storage layers is zero-cost and eliminates lifetime complexity.
- The internal `[u8; 32]` is never exposed as a mutable reference; callers cannot
  tamper with a hash after construction.
- `from_fields` uses the `sha2` crate (`sha2::Sha256`) operating on
  `ReceiptHashFields::canonical_bytes()`. The null-byte separator guarantees that
  `workflow="ab"`, `step_id="c"` produces a different digest than
  `workflow="a"`, `step_id="bc"`.
- `to_hex` / `from_hex` are the only serialization paths. There is no `u64` or
  integer encoding for a receipt hash — this prevents hash-space collisions from
  truncation.
- `manifest_sha256` and `framework_sha256` fields in `ReceiptHashFields` directly
  model the `^Manifest_sha256` / `^Framework_sha256` split hash anchors from
  `HARNESS_CONTRACT.md`. Receipts that predate the split carry an empty
  `framework_sha256`; the schema allows this via the optional `source_sha256`
  legacy alias but new receipts MUST populate both split fields.
- `ReceiptHash::zeroed()` exists only for test fixtures and negative controls; it
  must never appear in production receipt graphs.
- Workspace lints (`deny(unwrap_used)`, `deny(expect_used)`, `forbid(unsafe_code)`)
  apply to this module; the sha2 crate itself is pure safe Rust.

---

## Cluster Invariants

- **HLE-UP-001:** `receipt_hash.rs` is in `hle-core`, not `hle-verifier`. It is a
  neutral vocabulary type consumed by both executor and verifier sides. It does not
  produce verdicts. See [UP_EXECUTOR_VERIFIER_SPLIT](../../../ai_docs/use_patterns/UP_EXECUTOR_VERIFIER_SPLIT.md).
- The hex representation of any `ReceiptHash` must satisfy `^[0-9a-f]{64}$` as
  required by `schemas/receipt.schema.json`.
- `ReceiptHash::zeroed()` must never appear in a non-test receipt graph; any
  `zeroed()` value reaching `ReceiptsStore::append()` must be rejected by M007
  with error code E2020.

---

*M005 receipt_hash Spec v1.0 | 2026-05-10*
