# M043 DevopsV3Probe — devops_v3_probe.rs

> **File:** `crates/hle-bridge/src/devops_v3_probe.rs` | **Target LOC:** ~260 | **Target Tests:** 50
> **Layer:** L05 | **Cluster:** C07_DISPATCH_BRIDGES | **Error Codes:** 2630-2632
> **Role:** Read-only probe surface for the DevOps Engine V3 service (port 8082). Exposes health, readiness, and structured metric collection. Has NO write surface whatsoever — not gated by PhantomData, simply absent. The bridge hardwires `CapabilityClass::ReadOnly` and provides no write constructors.

---

## Types at a Glance

| Type | Kind | Copy | Purpose |
|---|---|---|---|
| `DevopsV3Probe` | struct | No | Read-only probe bridge for DevOps V3 (:8082/health) |
| `ProbeTarget` | struct | No | Validated `(host, port, path)` coordinate; defaults to DevOps V3 |
| `HealthSignal` | struct | No | Parsed health response from `/health`: status, uptime, version |
| `ReadinessSignal` | enum | Yes | `Ready` / `Degraded { reason }` / `Unreachable` |
| `DevopsV3ProbeError` | enum | No | Errors 2630-2632 for timeout, unreachable, invalid response |

---

## ProbeTarget

```rust
/// Validated `(host, port, path)` coordinate for an HTTP probe.
///
/// Defaults to DevOps V3: host `127.0.0.1`, port `8082`, path `/health`.
/// Custom targets must pass validation: host non-empty, port non-zero,
/// path begins with `/`, total URI length <= `PROBE_URI_MAX_LEN`.
///
/// This type is shared within C07 as a probe coordinate primitive.
/// M043 provides `ProbeTarget::devops_v3()` as a constructor for the
/// canonical target.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProbeTarget {
    pub host: String,
    pub port: u16,
    pub path: String,
}

pub const PROBE_URI_MAX_LEN: usize = 256;
pub const DEVOPS_V3_DEFAULT_PORT: u16 = 8082;
pub const DEVOPS_V3_HEALTH_PATH: &str = "/health";
```

| Method | Signature | Notes |
|---|---|---|
| `new` | `fn(host: impl Into<String>, port: u16, path: impl Into<String>) -> Result<Self, DevopsV3ProbeError>` | Validates constraints. |
| `devops_v3` | `fn() -> Self` | `#[must_use]`. Returns the canonical `127.0.0.1:8082/health` target. Infallible (validated at const level). |
| `uri` | `fn(&self) -> String` | `#[must_use]`. `"http://HOST:PORT/PATH"`. |

**Traits:** `Display` ("ProbeTarget(HOST:PORT/PATH)")

---

## HealthSignal

```rust
/// Structured health response from a DevOps V3 `/health` endpoint.
///
/// The JSON response is parsed and bounded: `details` is capped at
/// `HEALTH_SIGNAL_DETAILS_CAP` (1,024 bytes). Fields that are absent in
/// the response default to sentinel values (version = "unknown",
/// uptime_s = 0) rather than failing the parse.
///
/// `#[must_use]` is applied to all query methods so callers cannot
/// silently discard the health determination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthSignal {
    pub target: ProbeTarget,
    pub status_code: u16,
    pub is_healthy: bool,
    pub version: String,
    pub uptime_s: u64,
    pub details: String,         // bounded to HEALTH_SIGNAL_DETAILS_CAP
    pub probe_elapsed_ms: u64,
}

pub const HEALTH_SIGNAL_DETAILS_CAP: usize = 1_024;
```

| Method | Signature | Notes |
|---|---|---|
| `parse` | `fn(target: ProbeTarget, status_code: u16, body: &str, elapsed_ms: u64) -> Self` | Parses body JSON; uses defaults for missing fields; truncates `details`. |
| `is_healthy` | `fn(&self) -> bool` | `#[must_use]`. True when `status_code == 200 && is_healthy`. |
| `readiness` | `fn(&self) -> ReadinessSignal` | `#[must_use]`. Derives from `is_healthy` and status_code. |
| `latency_ms` | `fn(&self) -> u64` | `#[must_use]`. `probe_elapsed_ms`. |

**Traits:** `Display` ("HealthSignal(HOST:PORT status=N healthy=B uptime=Us)")

---

## ReadinessSignal

```rust
/// High-level readiness determination derived from a `HealthSignal`.
///
/// `Ready` means `status_code == 200` and the health body indicates healthy.
/// `Degraded` covers 2xx with `is_healthy = false` or partial body parse.
/// `Unreachable` covers non-2xx, connection refused, or parse-level failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadinessSignal {
    Ready,
    Degraded { reason: String },
    Unreachable,
}
```

**Traits:** `Display` ("Ready" / "Degraded(REASON)" / "Unreachable")

---

## DevopsV3Probe

```rust
/// Read-only probe bridge for DevOps Engine V3.
///
/// This bridge has NO write surface. `supports_write` returns false
/// unconditionally. There is no `LiveWrite`-parameterized variant,
/// no write constructor, and no `WriteAuthToken` parameter in any method.
/// The bridge is hardwired to `CapabilityClass::ReadOnly`.
///
/// All I/O is synchronous blocking HTTP via `std::net::TcpStream` or
/// a thin `ureq`-style call (single-threaded, bounded by timeout).
/// No async runtime is used; AP29 forbids async in this layer.
#[derive(Debug)]
pub struct DevopsV3Probe {
    default_target: ProbeTarget,
    default_timeout: BoundedDuration,
}
```

### BridgeContract impl

```rust
impl BridgeContract for DevopsV3Probe {
    fn schema_id(&self) -> &'static str { "hle.devops_v3.v1" }
    fn port(&self) -> Option<u16> { Some(DEVOPS_V3_DEFAULT_PORT) }
    fn paths(&self) -> &[&'static str] { &[DEVOPS_V3_HEALTH_PATH, "/readyz"] }
    fn supports_write(&self) -> bool { false }
    fn capability_class(&self) -> CapabilityClass { CapabilityClass::ReadOnly }
    fn name(&self) -> &'static str { "devops_v3_probe" }
}
```

---

## Method Table

| Method | Signature | Notes |
|---|---|---|
| `new` | `fn(timeout: BoundedDuration) -> Self` | Uses `ProbeTarget::devops_v3()` as default target. |
| `with_target` | `fn(target: ProbeTarget, timeout: BoundedDuration) -> Self` | Allows non-default targets for testing and alternative deployments. |
| `probe_health` | `fn(&self) -> Result<HealthSignal, DevopsV3ProbeError>` | `#[must_use]`. Performs a single bounded HTTP GET to `/health`. On timeout returns `Err(ProbeTimeout)`. On connection failure returns `Err(ProbeUnreachable)`. On 4xx/5xx returns `Err(ProbeResponseInvalid)`. On 2xx parses body into `HealthSignal`. |
| `probe_health_target` | `fn(&self, target: &ProbeTarget) -> Result<HealthSignal, DevopsV3ProbeError>` | `#[must_use]`. Same as `probe_health` but against an explicit target. |
| `readiness` | `fn(&self) -> Result<ReadinessSignal, DevopsV3ProbeError>` | `#[must_use]`. Calls `probe_health` and maps to `ReadinessSignal`. |
| `is_reachable` | `fn(&self) -> bool` | `#[must_use]`. Returns `true` iff `probe_health` returns `Ok(_)`. Swallows the error — for use in probing loops where presence/absence is sufficient. |
| `latency_ms` | `fn(&self) -> Result<u64, DevopsV3ProbeError>` | `#[must_use]`. Measures round-trip of a single `/health` probe. |

---

## DevopsV3ProbeError

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DevopsV3ProbeError {
    /// Code 2630. HTTP probe did not complete within `BoundedDuration`.
    /// Always retryable — timeout indicates transient load or slow start.
    ProbeTimeout { target: String, elapsed_ms: u64 },
    /// Code 2631. Connection refused, DNS failure, or network partition.
    /// Retryable — service may be starting.
    ProbeUnreachable { target: String, reason: String },
    /// Code 2632. Service responded but with 4xx or 5xx.
    /// Not retryable — server-side error requires operator attention.
    ProbeResponseInvalid { target: String, status_code: u16 },
}
```

| Method | Signature |
|---|---|
| `error_code` | `const fn(&self) -> u32` — 2630, 2631, or 2632 |
| `is_retryable` | `const fn(&self) -> bool` — true for 2630/2631; false for 2632 |
| `target_uri` | `fn(&self) -> &str` | `#[must_use]`. For logging. |

**Traits:** `Display` ("[HLE-263N] ..."), `std::error::Error`

---

## Design Notes

- M043 is the only C07 bridge with no write-side type parameter. There is no `DevopsV3Probe<Sealed<LiveWrite>>` — the struct is not generic. This makes the read-only constraint structurally total: no amount of `WriteAuthToken` construction can produce a write-capable version of this bridge.
- HTTP I/O uses synchronous blocking sockets bounded by `default_timeout`. The framework's one-shot/local-M0 constraint (AP29 analog) means no Tokio runtime is available at L05; the bridge must be usable in a single-threaded synchronous context.
- The `details` field in `HealthSignal` is bounded at 1,024 bytes at parse time, not at HTTP response time. Callers must not rely on `details` for structured data beyond the first kilobyte.
- `probe_response_invalid` (2632) is intentionally NOT retryable. A 5xx from DevOps V3 indicates a server-side problem that a retry will not resolve; it requires operator investigation, not automatic retry loops.
- `is_reachable` swallows errors deliberately for use in lightweight health-check loops. Callers that need the error cause must use `probe_health` directly.
- Framework §17.11 and `S09_DEVOPS_V3_READ_ONLY_INTEGRATION.md` both mandate read-only access to DevOps V3 for the M0 phase. M043 enforces this at the type level — there is no write path to accidentally enable.

---

## Cluster Invariants (C07) Enforced by M043

- **I-C07-1:** `capability_class` returns `CapabilityClass::ReadOnly` unconditionally; `supports_write` returns `false` unconditionally.
- **I-C07-4:** No `hle-executor` import in `Cargo.toml`.
- **I-C07-5:** All HTTP calls bounded by `self.default_timeout` (or caller-provided `BoundedDuration`).

---

## Test Targets (50 minimum)

| Group | Count | Coverage Focus |
|---|---|---|
| `ProbeTarget` validation | 8 | default, custom valid, empty host, zero port, long URI |
| `HealthSignal` parse variants | 10 | 200-healthy, 200-unhealthy, missing fields, truncated details |
| `ReadinessSignal` derivation | 5 | Ready, Degraded-partial, Degraded-unhealthy, Unreachable |
| `DevopsV3Probe` construction | 4 | default timeout, custom target |
| `probe_health` mock HTTP | 10 | 200 ok, 500 invalid, connection-refused, timeout |
| `readiness` mapping | 5 | Ready maps, Degraded maps, Unreachable maps |
| `is_reachable` swallow | 3 | ok-true, err-false, timeout-false |
| Error code and retryability | 5 | 2630/2631 retryable, 2632 not |

---

*M043 DevopsV3Probe Spec v1.0 | C07 Dispatch Bridges | 2026-05-10*
