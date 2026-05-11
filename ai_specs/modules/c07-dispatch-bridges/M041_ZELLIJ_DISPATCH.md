# M041 ZellijDispatch — zellij_dispatch.rs

> **File:** `crates/hle-bridge/src/zellij_dispatch.rs` | **Target LOC:** ~280 | **Target Tests:** 55
> **Layer:** L05 | **Cluster:** C07_DISPATCH_BRIDGES | **Error Codes:** 2610-2612
> **Role:** Typed adapter for Zellij pane dispatch packets. Converts executor-issued `ZellijPacket` values into bounded `zellij action write-chars` invocations. Write-side is compile-time sealed via `Sealed<Class>` PhantomData — no runtime capability check governs the write gate.

---

## Types at a Glance

| Type | Kind | Copy | Purpose |
|---|---|---|---|
| `ZellijDispatch<Class>` | struct | No | Bridge adapter parameterized over `Sealed<ReadOnly>` or `Sealed<LiveWrite>` |
| `ZellijPacket` | struct | No | Bounded dispatch payload: tab, pane label, chars, trailing CR byte |
| `PaneTarget` | struct | No | Validated (tab, pane-label) coordinate pair |
| `DispatchOutcome` | enum | Yes | `Sent { bytes }` / `Skipped { reason }` — always returned, never panics |
| `ZellijDispatchError` | enum | No | Errors 2610-2612 for dispatch failures |

---

## ZellijPacket

```rust
/// A bounded, validated dispatch payload for a single Zellij pane.
///
/// Constructed via `ZellijPacket::builder`. The `chars` field is capped at
/// `ZELLIJ_PACKET_CHAR_CAP` (4,096 bytes). Exceeding the cap is a construction
/// error, not a truncation — callers must split large payloads before dispatch.
///
/// The optional `trailing_cr` flag appends ASCII 0x0D (Carriage Return / byte 13)
/// after `chars`, which is required to "submit" input in fleet CC panes per
/// `feedback_fleet_submit_cr_byte.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZellijPacket {
    pub target: PaneTarget,
    pub chars: String,           // capped at ZELLIJ_PACKET_CHAR_CAP
    pub trailing_cr: bool,
    pub timeout: BoundedDuration, // from C03; default 5s
}

pub const ZELLIJ_PACKET_CHAR_CAP: usize = 4_096;
```

| Method | Signature | Notes |
|---|---|---|
| `builder` | `fn(target: PaneTarget) -> ZellijPacketBuilder` | Entry point. |
| `byte_len` | `fn(&self) -> usize` | `#[must_use]`. Length of chars + optional CR. |
| `validate` | `fn(&self) -> Result<(), ZellijDispatchError>` | Checks cap; returns `PacketTooLarge` if exceeded. |

**Traits:** `Display` ("ZellijPacket(tab=N pane=LABEL len=B)")

---

## PaneTarget

```rust
/// Validated (tab-index, pane-label) coordinate pair.
///
/// Tab index is 0-based. Pane label is a non-empty ASCII identifier matching
/// the label set in the Zellij KDL layout. Validation rejects empty labels and
/// tab indices >= `ZELLIJ_MAX_TAB` (64).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PaneTarget {
    pub tab: u8,
    pub pane_label: String,
}

pub const ZELLIJ_MAX_TAB: u8 = 63;
```

| Method | Signature | Notes |
|---|---|---|
| `new` | `fn(tab: u8, pane_label: impl Into<String>) -> Result<Self, ZellijDispatchError>` | Validates tab bound and non-empty label. |
| `tab` | `fn(&self) -> u8` | `#[must_use]`. |
| `pane_label` | `fn(&self) -> &str` | `#[must_use]`. |

**Traits:** `Display` ("tab=N:LABEL")

---

## ZellijDispatch (Generic)

```rust
/// Zellij pane dispatch bridge parameterized over capability class.
///
/// `ZellijDispatch<Sealed<ReadOnly>>` exposes only `probe` and `list_panes`.
/// `ZellijDispatch<Sealed<LiveWrite>>` additionally exposes `dispatch_packet`,
/// which is absent from the `ReadOnly` impl block entirely — no runtime gate,
/// no feature flag. The compiler enforces the boundary.
///
/// The bridge is stateless: it holds no connection, spawns no background tasks,
/// and makes no network calls. Every call invokes `zellij action write-chars`
/// via a bounded subprocess.
#[derive(Debug)]
pub struct ZellijDispatch<Class> {
    _class: Class,
    schema: &'static str,
}
```

### BridgeContract impl

```rust
impl<Class: Send + Sync + std::fmt::Debug> BridgeContract for ZellijDispatch<Class> {
    fn schema_id(&self) -> &'static str { "hle.zellij.v1" }
    fn port(&self) -> Option<u16> { None }   // filesystem/CLI bridge
    fn paths(&self) -> &[&'static str] { &["zellij action write-chars", "zellij action write"] }
    fn supports_write(&self) -> bool { ... } // false for ReadOnly class
    fn capability_class(&self) -> CapabilityClass { ... }
    fn name(&self) -> &'static str { "zellij_dispatch" }
}
```

---

## Method Table

### ReadOnly surface (both `Sealed<ReadOnly>` and `Sealed<LiveWrite>`)

| Method | Signature | Notes |
|---|---|---|
| `new_read_only` | `fn() -> ZellijDispatch<Sealed<ReadOnly>>` | Constructor. No authorization required. |
| `probe` | `fn(&self) -> Result<DispatchOutcome, ZellijDispatchError>` | `#[must_use]`. Checks whether `zellij` binary is present on PATH. Returns `Skipped` if absent, `Sent { bytes: 0 }` if reachable. Bounded to C03 `BoundedDuration::default()` (1s). |
| `validate_packet` | `fn(&self, packet: &ZellijPacket) -> Result<(), ZellijDispatchError>` | `#[must_use]`. Pure validation; no subprocess. |

### Write surface (only `Sealed<LiveWrite>`)

| Method | Signature | Notes |
|---|---|---|
| `new_live_write` | `fn(_token: &WriteAuthToken) -> ZellijDispatch<Sealed<LiveWrite>>` | Constructor. Requires `WriteAuthToken` from `AuthGate`. Token validated for expiry before construction. |
| `dispatch_packet` | `fn(&self, packet: &ZellijPacket, _token: &WriteAuthToken) -> Result<BridgeReceipt, ZellijDispatchError>` | `#[must_use]`. Executes `zellij action write-chars --target-pane LABEL CHARS` followed by `zellij action write TAB 13` when `trailing_cr = true`. Returns `BridgeReceipt` with SHA-256 of the dispatched chars. Bounded by `packet.timeout`. |

---

## DispatchOutcome

```rust
/// Outcome of a Zellij dispatch attempt. Never panics on failure — all
/// error conditions produce `Err(ZellijDispatchError)` not a panic or `unwrap`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchOutcome {
    /// Dispatch completed; `bytes` is the byte count written to the pane.
    Sent { bytes: usize },
    /// Dispatch skipped for a non-error reason (e.g. dry-run mode, binary absent).
    Skipped { reason: SkipReason },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    BinaryAbsent,
    DryRun,
}
```

---

## ZellijDispatchError

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZellijDispatchError {
    /// Code 2610. `zellij action write-chars` subprocess returned non-zero.
    /// Retryable when `retryable = true` (e.g. EAGAIN from the TTY layer).
    DispatchFailed { tab: u8, pane_label: String, reason: String, retryable: bool },
    /// Code 2611. Packet `chars` field exceeds `ZELLIJ_PACKET_CHAR_CAP`.
    PacketTooLarge { size_bytes: usize, cap_bytes: usize },
    /// Code 2612. Write method called without a valid `WriteAuthToken`, or
    /// token presented to `new_live_write` was already expired.
    WriteGateSealed { bridge: &'static str },
}
```

| Method | Signature |
|---|---|
| `error_code` | `const fn(&self) -> u32` — 2610, 2611, or 2612 |
| `is_retryable` | `fn(&self) -> bool` — true for `DispatchFailed { retryable: true }` only |

**Traits:** `Display` ("[HLE-261N] ..."), `std::error::Error`

---

## Design Notes

- The write-side gate is implemented as a missing method, not a hidden `if` branch. `ZellijDispatch<Sealed<ReadOnly>>` simply does not define `dispatch_packet`. Calling it on a `ReadOnly`-parameterized value is a compile error, not a runtime panic.
- Subprocess invocation uses `std::process::Command` with explicit timeout enforcement from `BoundedDuration`. The process is not spawned via `tokio::process`; AP29 forbids async in this layer without explicit boundary isolation.
- The CR byte (`zellij action write TAB 13`) is issued as a separate subprocess call after `write-chars`, matching `feedback_fleet_submit_cr_byte.md` which requires byte 13 (not `\n`) for fleet CC pane submission.
- `dispatch_packet` requires the `WriteAuthToken` at the call site even after the bridge is constructed with `new_live_write`. This double-token pattern ensures the token is live and unexpired at the moment of dispatch, preventing stale-token attacks.
- `BridgeReceipt` payload is the UTF-8 bytes of `packet.chars` (not the subprocess arguments), so the SHA captures the logical content dispatched rather than the CLI invocation syntax.

---

## Cluster Invariants (C07) Enforced by M041

- **I-C07-2 / I-C07-3:** Write gate is PhantomData-sealed. `dispatch_packet` exists only on `Sealed<LiveWrite>` impl block.
- **I-C07-4:** No `use hle_executor` or `hle-executor` dependency in `Cargo.toml`.
- **I-C07-5:** `packet.timeout` carries `BoundedDuration` from C03; subprocess call respects it.
- **I-C07-6:** `dispatch_packet` returns `Result<BridgeReceipt, _>`, not `Result<(), _>`.

---

## Test Targets (55 minimum)

| Group | Count | Coverage Focus |
|---|---|---|
| `PaneTarget` validation | 8 | empty label, tab-too-large, ASCII valid, Unicode invalid |
| `ZellijPacket` builder and cap | 10 | at-cap, over-cap, trailing-cr flag, timeout default |
| `ReadOnly` construction and probe | 8 | new_read_only, probe binary-absent, probe reachable |
| `validate_packet` | 6 | valid packet, oversized, missing chars |
| `WriteGateSealed` errors | 8 | expired token, ReadOnly class block, zero receipt_id |
| `dispatch_packet` mock path | 10 | success receipt, SHA correctness, CR-byte flag |
| `DispatchOutcome` exhaustiveness | 5 | Sent/Skipped/BinaryAbsent/DryRun display |

---

*M041 ZellijDispatch Spec v1.0 | C07 Dispatch Bridges | 2026-05-10*
