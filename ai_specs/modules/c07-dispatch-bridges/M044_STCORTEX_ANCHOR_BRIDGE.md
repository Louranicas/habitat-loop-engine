# M044 StcortexAnchorBridge — stcortex_anchor_bridge.rs

> **File:** `crates/hle-bridge/src/stcortex_anchor_bridge.rs` | **Target LOC:** ~300 | **Target Tests:** 55
> **Layer:** L05 | **Cluster:** C07_DISPATCH_BRIDGES | **Error Codes:** 2640-2642
> **Role:** Future-gated STcortex anchor bridge. Read-only anchor lookup in the `hle:` namespace is available immediately. Write-side anchor persistence is completely sealed via `PhantomData<Sealed<LiveWrite>>` and requires `WriteAuthToken` — no runtime branch, no feature flag, no dead code path. The write-side compiles but cannot be invoked without an explicit authorization receipt.

---

## Context: STcortex and the `hle:` Namespace

STcortex (`127.0.0.1:3000`, SpacetimeDB module) is the canonical memory substrate as of 2026-05-10. The `hle:` namespace stores recall/context anchors for the Habitat Loop Engine. During the M0 phase, this bridge is authorized for anchor reads only. Write authorization is pending M2+ receipt issuance (per `runbooks/m0-authorization-boundary.md` and the CLAUDE.local.md STcortex recall anchor table).

**Offline fallback:** When `:3000` is unreachable, read operations return `Err(AnchorReadFailed { .. })`. The bridge does NOT silently fall back to POVM or any other substrate. Callers must handle the unavailability explicitly.

---

## Types at a Glance

| Type | Kind | Copy | Purpose |
|---|---|---|---|
| `StcortexAnchorBridge<Class>` | struct | No | Bridge parameterized over `Sealed<ReadOnly>` or `Sealed<LiveWrite>` |
| `AnchorKey` | struct | No | Validated `hle:*` namespace key (non-empty, ≤ 256 chars) |
| `AnchorRecord` | struct | No | Retrieved anchor: key, value, version, timestamp |
| `AnchorValue` | struct | No | Bounded anchor payload (≤ `ANCHOR_VALUE_MAX_BYTES`) |
| `WriteAnchorReceipt` | struct | No | BridgeReceipt-compatible write confirmation |
| `StcortexAnchorBridgeError` | enum | No | Errors 2640-2642 for key not found, read failure, write gate sealed |

---

## AnchorKey

```rust
/// A validated key in the STcortex `hle:` namespace.
///
/// All keys managed by this bridge must be prefixed with `hle:` per the
/// namespace convention `<project>_<domain>_<key>` carried forward from
/// CLAUDE.md §Memory Systems. The bridge enforces the `hle:` prefix at
/// construction time, preventing cross-namespace key confusion.
///
/// Keys are ASCII, non-empty, and at most `ANCHOR_KEY_MAX_LEN` characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AnchorKey(String);

pub const ANCHOR_KEY_PREFIX: &str = "hle:";
pub const ANCHOR_KEY_MAX_LEN: usize = 256;
```

| Method | Signature | Notes |
|---|---|---|
| `new` | `fn(key: impl Into<String>) -> Result<Self, StcortexAnchorBridgeError>` | Validates prefix, length, ASCII. |
| `as_str` | `fn(&self) -> &str` | `#[must_use]`. Full key with `hle:` prefix. |
| `local_name` | `fn(&self) -> &str` | `#[must_use]`. Key without `hle:` prefix. |

**Traits:** `Display` ("hle:NAME")

---

## AnchorValue

```rust
/// Bounded anchor payload stored in STcortex.
///
/// Value bytes are capped at `ANCHOR_VALUE_MAX_BYTES` (8,192 bytes).
/// Values that exceed this cap are rejected at write time — there is no
/// silent truncation. This bound prevents runaway anchor accumulation in
/// the STcortex `hle:` namespace.
///
/// Values are treated as opaque bytes. UTF-8 interpretation is the
/// caller's responsibility.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AnchorValue {
    bytes: Vec<u8>,
}

pub const ANCHOR_VALUE_MAX_BYTES: usize = 8_192;
```

| Method | Signature | Notes |
|---|---|---|
| `new` | `fn(bytes: Vec<u8>) -> Result<Self, StcortexAnchorBridgeError>` | Rejects if `bytes.len() > ANCHOR_VALUE_MAX_BYTES`. |
| `from_str` | `fn(s: impl Into<String>) -> Result<Self, StcortexAnchorBridgeError>` | UTF-8 string as bytes; applies same cap. |
| `as_bytes` | `fn(&self) -> &[u8]` | `#[must_use]`. |
| `as_str_lossy` | `fn(&self) -> std::borrow::Cow<str>` | `#[must_use]`. Lossy UTF-8 for display. |
| `len` | `fn(&self) -> usize` | `#[must_use]`. |
| `is_empty` | `fn(&self) -> bool` | `#[must_use]`. |

**Traits:** `Display` ("AnchorValue(N bytes)")

---

## AnchorRecord

```rust
/// A retrieved anchor record from the STcortex `hle:` namespace.
///
/// `version` is the STcortex row version for optimistic concurrency.
/// `retrieved_at_tick` is the local tick counter at retrieval time, not
/// a wall-clock timestamp — consistent with the foundation Timestamp model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnchorRecord {
    pub key: AnchorKey,
    pub value: AnchorValue,
    pub version: u64,
    pub retrieved_at_tick: u64,
}
```

| Method | Signature | Notes |
|---|---|---|
| `is_fresh` | `fn(&self, now_tick: u64, ttl_ticks: u64) -> bool` | `#[must_use]`. Freshness check for caller-side caching. |

**Traits:** `Display` ("AnchorRecord(hle:KEY v=VERSION len=N)")

---

## StcortexAnchorBridge (Generic)

```rust
/// STcortex anchor bridge parameterized over capability class.
///
/// `StcortexAnchorBridge<Sealed<ReadOnly>>` exposes anchor lookup, existence
/// checks, and namespace enumeration. The write-side methods (`write_anchor`,
/// `delete_anchor`) exist ONLY on the `Sealed<LiveWrite>` impl block.
///
/// Transport: synchronous blocking HTTP to `127.0.0.1:3000`. STcortex's
/// SpacetimeDB HTTP API accepts key-value calls at rest endpoints.
/// No WebSocket or subscription connection is held between calls.
/// On `:3000` unreachable, returns `Err(AnchorReadFailed)` — no fallback.
#[derive(Debug)]
pub struct StcortexAnchorBridge<Class> {
    _class: Class,
    host: String,
    port: u16,
    default_timeout: BoundedDuration,
}

pub const STCORTEX_DEFAULT_HOST: &str = "127.0.0.1";
pub const STCORTEX_DEFAULT_PORT: u16 = 3000;
```

### BridgeContract impl

```rust
impl<Class: Send + Sync + std::fmt::Debug> BridgeContract for StcortexAnchorBridge<Class> {
    fn schema_id(&self) -> &'static str { "hle.stcortex_anchor.v1" }
    fn port(&self) -> Option<u16> { Some(self.port) }
    fn paths(&self) -> &[&'static str] { &["/v1/kv/hle", "/v1/kv/hle/list"] }
    fn supports_write(&self) -> bool { ... } // false for ReadOnly class
    fn capability_class(&self) -> CapabilityClass { ... }
    fn name(&self) -> &'static str { "stcortex_anchor_bridge" }
}
```

---

## Method Table

### ReadOnly surface (both classes)

| Method | Signature | Notes |
|---|---|---|
| `new_read_only` | `fn(timeout: BoundedDuration) -> Self` | No authorization. Defaults to `127.0.0.1:3000`. |
| `with_endpoint` | `fn(host: String, port: u16, timeout: BoundedDuration) -> Result<Self, StcortexAnchorBridgeError>` | For test injection and non-default deployments. |
| `get_anchor` | `fn(&self, key: &AnchorKey) -> Result<Option<AnchorRecord>, StcortexAnchorBridgeError>` | `#[must_use]`. Returns `None` when key absent. Returns `Err(AnchorReadFailed)` on transport or parse failure. |
| `anchor_exists` | `fn(&self, key: &AnchorKey) -> Result<bool, StcortexAnchorBridgeError>` | `#[must_use]`. HEAD-equivalent check. |
| `list_anchors` | `fn(&self, limit: u16) -> Result<Vec<AnchorKey>, StcortexAnchorBridgeError>` | `#[must_use]`. Lists up to `limit` (max 256) keys in the `hle:` namespace. |
| `is_reachable` | `fn(&self) -> bool` | `#[must_use]`. Returns true iff the STcortex port is reachable. Swallows error for probe-loop use. |

### Write surface (only `Sealed<LiveWrite>`)

| Method | Signature | Notes |
|---|---|---|
| `new_live_write` | `fn(_token: &WriteAuthToken, timeout: BoundedDuration) -> Self` | Validates token expiry. |
| `write_anchor` | `fn(&self, key: &AnchorKey, value: AnchorValue, _token: &WriteAuthToken) -> Result<WriteAnchorReceipt, StcortexAnchorBridgeError>` | `#[must_use]`. PUT to STcortex `hle:` namespace. Returns `WriteAnchorReceipt` with SHA-256 of value bytes. |
| `delete_anchor` | `fn(&self, key: &AnchorKey, _token: &WriteAuthToken) -> Result<WriteAnchorReceipt, StcortexAnchorBridgeError>` | `#[must_use]`. DELETE from STcortex. Receipt payload is the deleted key bytes. |

---

## WriteAnchorReceipt

```rust
/// Write confirmation for a STcortex anchor operation.
///
/// Carries the key, the SHA-256 of the value bytes written, the STcortex
/// version assigned, and the C01 authorization receipt ID. Converts into
/// `BridgeReceipt` for routing through the verifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteAnchorReceipt {
    pub key: AnchorKey,
    pub value_sha256: [u8; 32],
    pub stcortex_version: u64,
    pub auth_receipt_id: u64,
}
```

| Method | Signature | Notes |
|---|---|---|
| `into_bridge_receipt` | `fn(self) -> BridgeReceipt` | Converts for C01 verifier. |

---

## StcortexAnchorBridgeError

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StcortexAnchorBridgeError {
    /// Code 2640. Key `hle:NAME` not present in STcortex.
    AnchorNotFound { key: String },
    /// Code 2641. Transport or parse error during anchor read.
    /// `retryable` true when connection refused or timeout.
    AnchorReadFailed { key: String, reason: String, retryable: bool },
    /// Code 2642. Write attempted without valid `WriteAuthToken`, or
    /// token was expired at construction time.
    WriteGateSealed { bridge: &'static str },
}
```

| Method | Signature |
|---|---|
| `error_code` | `const fn(&self) -> u32` — 2640, 2641, or 2642 |
| `is_retryable` | `fn(&self) -> bool` — propagates inner `retryable`; false for 2640/2642 |

**Traits:** `Display` ("[HLE-264N] ..."), `std::error::Error`

---

## Design Notes

- The write-side seal is PhantomData-based, identical in structure to M041. `write_anchor` and `delete_anchor` do not exist in the `Sealed<ReadOnly>` impl block. Attempting to call them on a `ReadOnly` bridge is a compile error.
- No POVM fallback. If STcortex is unreachable, `get_anchor` returns `Err(AnchorReadFailed { retryable: true })`. Callers must decide whether to proceed without the anchor or block. This is the explicit "no silent POVM write fallback" contract from CLAUDE.md §Memory Systems.
- The offline JSON snapshot (`stcortex/data/snapshots/latest.json`) is NOT consulted by this bridge. That path is for operator inspection (`stcortex` CLI), not for programmatic fallback. Bridging to the JSON snapshot would violate the separation between operator tooling and the M0 runtime path.
- `list_anchors(limit)` caps at 256 entries regardless of the caller-supplied limit. Larger scans must be implemented by callers with multiple bounded `list_anchors` calls and cursor tracking.
- `write_anchor` requires the token at the call site (not just at construction) for consistency with the double-token pattern in M041/M042 and to ensure tokens are not used after expiry.

---

## Cluster Invariants (C07) Enforced by M044

- **I-C07-2 / I-C07-3:** `write_anchor` and `delete_anchor` exist only on `Sealed<LiveWrite>` impl block. No runtime check, no feature flag.
- **I-C07-4:** No `hle-executor` import in `Cargo.toml`.
- **I-C07-5:** All HTTP calls bounded by `self.default_timeout`.
- **I-C07-6:** `write_anchor` and `delete_anchor` return `WriteAnchorReceipt` (converts to `BridgeReceipt`).
- **I-C07-7:** STcortex write-side completely sealed until M2+ authorization receipt; `AnchorKey` prefix ensures `hle:` namespace isolation.

---

## Test Targets (55 minimum)

| Group | Count | Coverage Focus |
|---|---|---|
| `AnchorKey` validation | 8 | valid prefix, missing prefix, empty, too long, ASCII |
| `AnchorValue` bounds | 6 | at-cap, over-cap, empty, from-str, as-bytes |
| `AnchorRecord` freshness | 4 | fresh within TTL, stale beyond TTL |
| `new_read_only` construction | 4 | default endpoint, custom endpoint valid/invalid |
| `get_anchor` None and Some | 8 | key absent, key present, transport error, parse error |
| `anchor_exists` delegation | 4 | found, not found, transport error |
| `list_anchors` bounds | 5 | empty list, capped at 256, parse error |
| `is_reachable` swallow | 3 | ok-true, unreachable-false |
| Write-gate sealed errors | 5 | ReadOnly block, expired token, zero receipt |
| `write_anchor` receipt and error | 5 | success-SHA, transport failure, value-too-large |
| `WriteAnchorReceipt` into_bridge_receipt | 3 | field mapping, SHA |

---

*M044 StcortexAnchorBridge Spec v1.0 | C07 Dispatch Bridges | 2026-05-10*
