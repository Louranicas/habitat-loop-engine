# M040 BridgeContract — bridge_contract.rs

> **File:** `crates/hle-bridge/src/bridge_contract.rs` | **Target LOC:** ~260 | **Target Tests:** 55
> **Layer:** L05 | **Cluster:** C07_DISPATCH_BRIDGES | **Error Codes:** 2600-2601
> **Role:** Shared trait and capability model that all C07 bridges implement. Defines `CapabilityClass`, the sealed `WriteAuthToken`, and the `BridgeContract` trait that enforces read-only-by-default across all bridge adapters.

---

## Types at a Glance

| Type | Kind | Copy | Purpose |
|---|---|---|---|
| `CapabilityClass` | enum | Yes | ReadOnly / LiveWrite / LiveWriteAuthorized — capability tier for a bridge |
| `Sealed<Class>` | struct | Yes (when `Class: Copy`) | PhantomData marker preventing write-path use without authorization |
| `WriteAuthToken` | struct | No | Opaque receipt that unlocks write-side bridge methods; not constructible outside `AuthGate` |
| `AuthGate` | struct | No | Single entry point for issuing `WriteAuthToken`; validates authority receipt before issuance |
| `BridgeReceipt` | struct | No | SHA-256-tagged outcome of a bridge write operation; routes to C01 verifier |
| `BridgeContractError` | enum | No | Errors 2600-2601 for contract violations and capability denial |
| `BridgeContract` | trait | — | Core trait every C07 bridge implements |

---

## CapabilityClass

```rust
/// Capability tier for a bridge adapter.
///
/// The default for ALL bridges is `ReadOnly`. Upgrading to `LiveWrite`
/// or `LiveWriteAuthorized` requires constructing a `WriteAuthToken` through
/// `AuthGate::issue_token`, which validates an authority receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CapabilityClass {
    /// Bridge performs only read / probe / enumerate operations.
    /// No state in any external system is modified.
    ReadOnly,
    /// Bridge can perform write operations when a valid `WriteAuthToken` is held.
    /// Token must be presented at every write call site.
    LiveWrite,
    /// Bridge write-path has been explicitly authorized via an M2+ receipt.
    /// Superset of `LiveWrite`; carries the authorization receipt ID.
    LiveWriteAuthorized { receipt_id: u64 },
}
```

| Method | Signature | Notes |
|---|---|---|
| `is_read_only` | `const fn(&self) -> bool` | `#[must_use]`. True only for `ReadOnly`. |
| `allows_write` | `const fn(&self) -> bool` | `#[must_use]`. True for `LiveWrite` and `LiveWriteAuthorized`. |
| `requires_token` | `const fn(&self) -> bool` | `#[must_use]`. Always true for `LiveWrite`; true for `LiveWriteAuthorized`. |

**Traits:** `Display` ("ReadOnly" / "LiveWrite" / "LiveWriteAuthorized(receipt=N)"), `Default` → `ReadOnly`

---

## Sealed PhantomData Marker

```rust
use std::marker::PhantomData;

/// Zero-sized compile-time marker. Bridges parameterized over `Sealed<ReadOnly>`
/// cannot expose write-path methods. Bridges parameterized over `Sealed<LiveWrite>`
/// can expose write-path methods only when a `WriteAuthToken` is also held.
///
/// This is NOT a runtime check. The compiler enforces capability class at the
/// impl level — wrong-class bridges simply do not have the write methods defined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Sealed<Class>(PhantomData<Class>);

/// Marker type for read-only bridge capability class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ReadOnly;

/// Marker type for live-write bridge capability class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct LiveWrite;
```

Bridges that wish to expose write methods are parameterized as `BridgeName<Sealed<LiveWrite>>`. The `ReadOnly`-parameterized variant does not compile the write-method bodies. This is the compile-time enforcement described in cluster invariant I-C07-2 and I-C07-3.

---

## WriteAuthToken

```rust
/// Opaque receipt that unlocks write-side bridge methods.
///
/// Cannot be constructed outside `AuthGate::issue_token`. The `#[non_exhaustive]`
/// attribute, combined with `pub(crate)` fields, ensures external crates cannot
/// build a token via struct literal syntax.
///
/// Internally carries the authority receipt ID and an expiry tick so stale
/// tokens are rejected before being presented to the bridge.
#[derive(Debug)]
#[non_exhaustive]
pub struct WriteAuthToken {
    pub(crate) receipt_id: u64,
    pub(crate) issued_at_tick: u64,
    pub(crate) expires_at_tick: u64,
}
```

| Method | Signature | Notes |
|---|---|---|
| `receipt_id` | `fn(&self) -> u64` | `#[must_use]`. The C01 receipt that authorized this token. |
| `is_expired` | `fn(&self, current_tick: u64) -> bool` | `#[must_use]`. True when `current_tick >= expires_at_tick`. |

**Traits:** `Display` ("WriteAuthToken(receipt=N, expires=T)")

---

## AuthGate

```rust
/// Single entry point for issuing `WriteAuthToken`.
///
/// Validates that a C01/C02 authority receipt exists and that the
/// requested `CapabilityClass` is `LiveWrite` or `LiveWriteAuthorized`.
/// Rejects issuance if the receipt is expired, forged, or the class is `ReadOnly`.
#[derive(Debug, Default)]
pub struct AuthGate;
```

| Method | Signature | Notes |
|---|---|---|
| `issue_token` | `fn(&self, receipt_id: u64, class: CapabilityClass, ttl_ticks: u64) -> Result<WriteAuthToken, BridgeContractError>` | Returns `Err(CapabilityDenied)` when `class == ReadOnly`. Returns `Err(ContractViolation)` when `receipt_id` is zero (unvalidated). |

---

## BridgeReceipt

```rust
/// SHA-256-tagged outcome of a bridge write operation.
///
/// Every write operation that successfully completes produces a `BridgeReceipt`.
/// The receipt carries the SHA-256 hash of the written payload so the C01 verifier
/// can independently recompute and validate the write outcome.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BridgeReceipt {
    pub schema_id: &'static str,
    pub operation: String,
    pub payload_sha256: [u8; 32],
    pub timestamp_tick: u64,
    pub auth_receipt_id: Option<u64>,
}
```

| Method | Signature | Notes |
|---|---|---|
| `new` | `fn(schema_id: &'static str, operation: impl Into<String>, payload: &[u8]) -> Self` | Computes SHA-256 from `payload`. `auth_receipt_id` defaults to `None`. |
| `with_auth` | `fn(self, token: &WriteAuthToken) -> Self` | Builder; sets `auth_receipt_id = Some(token.receipt_id())`. |
| `hex_sha` | `fn(&self) -> String` | `#[must_use]`. Lowercase hex string of `payload_sha256`. |

**Traits:** `Display` ("BridgeReceipt(schema=ID op=OP sha=HEXSHORT)")

---

## BridgeContract Trait

```rust
/// Core trait every C07 bridge implements.
///
/// Expresses the static contract properties of a bridge: what schema it
/// conforms to, which port/paths it targets, whether it supports writes,
/// and which capability class it presents.
///
/// All methods are `&self` for `Arc<dyn BridgeContract>` compatibility.
/// No method may panic or call `unwrap`/`expect`.
pub trait BridgeContract: Send + Sync + std::fmt::Debug {
    /// Unique schema identifier for this bridge, e.g. `"hle.zellij.v1"`.
    fn schema_id(&self) -> &'static str;

    /// Optional TCP port the bridge targets. `None` for filesystem/CLI bridges.
    fn port(&self) -> Option<u16>;

    /// HTTP paths or filesystem paths this bridge operates on.
    fn paths(&self) -> &[&'static str];

    /// Whether this bridge instance can perform write operations.
    /// MUST return `false` for all `ReadOnly`-class bridges.
    fn supports_write(&self) -> bool;

    /// The capability class this bridge currently presents.
    fn capability_class(&self) -> CapabilityClass;

    /// Human-readable bridge name for logging and error messages.
    fn name(&self) -> &'static str;
}
```

| Method | Default Impl | Notes |
|---|---|---|
| `schema_id` | — | Required. No default. |
| `port` | — | Required. No default. |
| `paths` | — | Required. No default. |
| `supports_write` | `fn(&self) -> bool { self.capability_class().allows_write() }` | Derived from capability class; bridges may override for finer control. |
| `capability_class` | `fn(&self) -> CapabilityClass { CapabilityClass::ReadOnly }` | **Default is `ReadOnly`.** Bridges must opt in to write capability. |
| `name` | — | Required. No default. |

---

## BridgeContractError

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeContractError {
    /// Code 2600. A bridge contract invariant was broken at runtime.
    ContractViolation { schema_id: &'static str, reason: String },
    /// Code 2601. Caller requested a capability the bridge does not hold.
    CapabilityDenied { required: CapabilityClass, actual: CapabilityClass },
}
```

| Method | Signature |
|---|---|
| `error_code` | `const fn(&self) -> u32` — 2600 or 2601 |
| `is_retryable` | `const fn(&self) -> bool` — always false |

**Traits:** `Display` ("[HLE-2600] contract violation ..."), `std::error::Error`

---

## Design Notes

- `WriteAuthToken` uses `#[non_exhaustive]` and `pub(crate)` fields. External crates cannot construct it via `WriteAuthToken { .. }` struct literal syntax, and `#[non_exhaustive]` prevents exhaustive destructuring. This is the primary structural enforcement, complementing the PhantomData approach in individual bridges.
- `AuthGate` is a zero-cost stateless value type. The real validation occurs at receipt resolution time (C01/C02). `AuthGate::issue_token` is a thin type-system gate, not a live service call.
- `BridgeReceipt::payload_sha256` uses a fixed `[u8; 32]` array (SHA-256), avoiding heap allocation in the receipt hot path. The `hex_sha` method allocates only on the display path.
- The `BridgeContract` trait uses `&self` throughout, enabling `Arc<dyn BridgeContract>` usage in executor contexts without requiring interior mutability.

---

## Cluster Invariants Enforced by M040

- **I-C07-1:** `CapabilityClass::ReadOnly` is the `Default` impl for `CapabilityClass` and the default return value of `BridgeContract::capability_class`. No bridge can "forget" to set its capability class and accidentally become a write bridge.
- **I-C07-2:** `WriteAuthToken` is structurally unforgeable outside `AuthGate`. `#[non_exhaustive]` + `pub(crate)` fields + no public constructor outside `AuthGate::issue_token`.
- **I-C07-6:** `BridgeReceipt` is the canonical write-outcome type. Write methods must return `Result<BridgeReceipt, _>` not bare `Result<(), _>`, ensuring the SHA chain is always produced.

---

*M040 BridgeContract Spec v1.0 | C07 Dispatch Bridges | 2026-05-10*
