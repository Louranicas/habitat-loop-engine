# C07 Dispatch Bridges — Cluster Overview

> **Cluster:** C07_DISPATCH_BRIDGES | **Layer:** L05 | **Modules:** 6 (M040-M045)
> **Error Code Range:** 2600-2699 | **Source Crate:** `crates/hle-bridge/`
> **Synergy:** Zellij/Atuin/DevOps-V3/STcortex/Watcher bridges share contract parity and read-only/live-write gates

---

## Purpose

C07 provides the passive adapter surface between the Habitat Loop Engine and external Habitat services. Every bridge in this cluster is a typed, bounded adapter — it never initiates background work, never holds persistent state beyond a call's duration, and never depends on `hle-executor`. The data-flow direction is strictly:

```
hle-executor → hle-bridge → external service
```

M040 (`bridge_contract`) defines the shared trait and capability model that enforces this contract at the type level. The five satellite bridges (M041-M045) each implement `BridgeContract` and add their own domain types, read-only probe surfaces, and — where applicable — a compile-time sealed write gate.

The governing constraint from `runbooks/m0-authorization-boundary.md`: **C07 is a read-only bridge surface until explicit M2+ write authorization is granted.** Write-side adapters exist in the type system as sealed types that no external crate can construct without an explicit `WriteAuthToken`.

---

## File Map

```
crates/hle-bridge/src/
├── lib.rs                    # crate root; re-exports public bridge surfaces
├── bridge_contract.rs        # M040 — BridgeContract trait, CapabilityClass, WriteAuthToken
├── zellij_dispatch.rs        # M041 — ZellijPacket, ZellijDispatch adapter
├── atuin_qi_bridge.rs        # M042 — AtuinQiBridge, ScriptEntry, ScriptStatus
├── devops_v3_probe.rs        # M043 — DevopsV3Probe, ProbeTarget, HealthSignal (read-only)
├── stcortex_anchor_bridge.rs # M044 — StcortexAnchorBridge, AnchorKey, AnchorRecord (read-gated)
└── watcher_notice_writer.rs  # M045 — WatcherNoticeWriter, NoticeReceipt (append-only file)
```

---

## Dependency Graph (Internal)

```
bridge_contract.rs (M040)
    ├──> zellij_dispatch.rs (M041)      [implements BridgeContract; uses WriteAuthToken for write gate]
    ├──> atuin_qi_bridge.rs (M042)      [implements BridgeContract; uses WriteAuthToken for register/run]
    ├──> devops_v3_probe.rs (M043)      [implements BridgeContract; no write surface]
    ├──> stcortex_anchor_bridge.rs (M044) [implements BridgeContract; write gate via Sealed<LiveWrite>]
    └──> watcher_notice_writer.rs (M045) [implements BridgeContract; append-only file write]
```

M040 is the only internal dependency. No bridge depends on another bridge. The graph is a pure fan-out from M040.

---

## Cross-Cluster Dependencies

| Direction | From | To | Reason |
|---|---|---|---|
| C07 → C03 | M041, M042, M043, M044 | `timeout_policy` (M014), `retry_policy` (M015) | All network-adjacent calls use C03 `BoundedDuration` for timeouts; retry bounded by `RetryPolicy` |
| C07 → C01 | M045, M041 | `receipt_hash` (M001), `receipts_store` (M003) | Write outcomes and notice receipts route through M008 verifier; SHA chain from M001 |
| C07 → C02 | M041, M042, M044 | `claim_authority` (M006) | Write-side actions require an authority receipt before `WriteAuthToken` can be constructed |
| hle-executor → C07 | `phase_executor` (M013) | M041, M042, M043 | Executor calls bridges as passive adapters; bridges never call executor |
| C08 → C07 | `cli_run` (M043 C08) | M043, M045 | CLI surfaces invoke probes and watcher writes for operator inspection |

---

## Concurrency Architecture

| Strategy | Where | Rationale |
|---|---|---|
| Synchronous blocking I/O | M043 (HTTP probe), M042 (Atuin CLI) | One-shot M0; AP29 forbids async without explicit boundary isolation |
| Append-only file I/O | M045 | `OpenOptions::append(true)` on a path; POSIX append writes are atomic for small payloads |
| No shared state | All M040-M045 | Bridges are stateless value-type adapters; no `Arc`, `Mutex`, or `RwLock` |
| PhantomData type gate | M041, M044 | `Sealed<Class>` marker prevents write-path instantiation at compile time without `WriteAuthToken` |

---

## Design Principles

1. **Read-only by default.** `CapabilityClass::ReadOnly` is the default for every bridge. Changing to `LiveWrite` requires explicit construction of `WriteAuthToken`, which is opaque outside the authorization gate.
2. **Type-system enforcement, not runtime check.** Write gates use `PhantomData<Sealed<Class>>` marker types. A bridge in `ReadOnly` class cannot call write methods because those methods are only defined on `Sealed<LiveWrite>` variants. The compiler enforces this — not a runtime `if` branch.
3. **Bridges are passive adapters.** No bridge spawns background tasks, registers timers, or holds persistent network connections. Each call is bounded and foreground.
4. **No dependency on `hle-executor`.** The DAG flows one direction: executor calls bridges. Bridges that attempted to call the executor would create a cycle and violate the layer contract.
5. **Every call is bounded.** Timeouts reuse C03 `BoundedDuration`; output sizes reuse `BoundedString`. No bridge accepts raw `u64` timeouts or unbounded `String` outputs.
6. **All write outcomes produce receipts.** Write operations in M041, M042, and M045 produce a `BridgeReceipt` carrying a SHA-256 hash. Receipts route through the verifier (C01 M004/M005) before any claim is promoted.
7. **`#[must_use]` on all probe and write return values.** Callers cannot silently drop a `Result<HealthSignal>` or `Result<BridgeReceipt>`.

---

## Error Strategy (Codes 2600-2699)

| Code | Variant | Source | Retryable | Notes |
|---|---|---|---|---|
| 2600 | `ContractViolation { schema_id, reason }` | M040 | No | Bridge contract invariant broken (port mismatch, path mismatch) |
| 2601 | `CapabilityDenied { required, actual }` | M040 | No | Caller requested write on ReadOnly bridge |
| 2610 | `DispatchFailed { tab, pane_label, reason }` | M041 | Conditional | Zellij write-chars failure; retryable if EAGAIN |
| 2611 | `PacketTooLarge { size_bytes, cap_bytes }` | M041 | No | Zellij action payload exceeds cap |
| 2612 | `WriteGateSealed { bridge }` | M041, M044 | No | Write attempted without `WriteAuthToken` |
| 2620 | `ScriptNotFound { name }` | M042 | No | Atuin script missing from registry |
| 2621 | `ScriptRunFailed { name, exit_code }` | M042 | Conditional | Script returned non-zero; retryable if transient |
| 2622 | `EnumerationFailed { reason }` | M042 | Conditional | Atuin `scripts list` failed |
| 2630 | `ProbeTimeout { target, elapsed_ms }` | M043 | Yes | HTTP health probe timed out; always retryable |
| 2631 | `ProbeUnreachable { target, reason }` | M043 | Yes | Connection refused or DNS failure |
| 2632 | `ProbeResponseInvalid { target, status_code }` | M043 | No | 4xx/5xx response; server-side error |
| 2640 | `AnchorNotFound { key }` | M044 | No | STcortex `hle:` namespace key missing |
| 2641 | `AnchorReadFailed { key, reason }` | M044 | Conditional | Transport or parse error on anchor read |
| 2642 | `WriteGateSealed { bridge }` | M044 | No | STcortex write attempted before M2+ authorization |
| 2650 | `NoticeWriteFailed { path, reason }` | M045 | Conditional | File append failed; retryable if ENOSPC resolved |
| 2651 | `NoticeTooLarge { size_bytes, cap_bytes }` | M045 | No | Notice payload exceeds append cap |

All variants implement `std::fmt::Display`. Error codes appear as `[HLE-26NN]` in display output for grep-ability in verifier logs.

---

## Cluster Invariants

| ID | Invariant | Enforcement |
|---|---|---|
| I-C07-1 | Every bridge defaults to `CapabilityClass::ReadOnly` | `BridgeContract::capability_class` default impl returns `ReadOnly` |
| I-C07-2 | Live-write requires `WriteAuthToken`; no external crate can construct one without the authorization gate | `WriteAuthToken` is `#[non_exhaustive]` and constructed only through `AuthGate::issue_token` |
| I-C07-3 | Write-side on M041 and M044 is PhantomData-sealed, not a runtime check | Method signatures carry `_token: &WriteAuthToken` or `Sealed<LiveWrite>` marker |
| I-C07-4 | Bridges MUST NOT import `hle-executor`; direction is executor → bridge | Enforced by crate dependency graph in `Cargo.toml`; no circular dep allowed |
| I-C07-5 | Every bridge call is bounded (timeout, output cap) | All network calls use `BoundedDuration` from C03; outputs use `BoundedString` |
| I-C07-6 | All write outcomes route through the verifier via `BridgeReceipt` | M041/M042/M045 return `BridgeReceipt`; verifier checks SHA before promoting claim |
| I-C07-7 | STcortex write-side completely sealed until M2+ authorization receipt exists | `StcortexAnchorBridge::write_anchor` only compiles when caller holds `WriteAuthToken` from authorized gate |

---

## Quality Gate Template

```bash
# Run from workspace root
cargo check --package hle-bridge 2>&1 | tail -20
cargo clippy --package hle-bridge -- -D warnings 2>&1 | tail -20
cargo clippy --package hle-bridge -- -D warnings -W clippy::pedantic 2>&1 | tail -20
cargo test --package hle-bridge --lib 2>&1 | tail -30

# Zero-tolerance checks
grep -rn 'unwrap()\|expect()\|panic!\|unsafe' crates/hle-bridge/src/ && echo FAIL || echo PASS
grep -rn 'hle-executor\|hle_executor' crates/hle-bridge/Cargo.toml && echo FAIL_CYCLIC_DEP || echo PASS

# Write-gate seal check (no direct WriteAuthToken construction outside auth gate)
grep -rn 'WriteAuthToken {' crates/hle-bridge/src/ | grep -v 'auth_gate\|#\[cfg(test' && echo FAIL || echo PASS
```

Minimum test targets: 50 tests per module (300 total for C07).

---

*C07 Dispatch Bridges Cluster Overview | HLE spec v1.0 | 2026-05-10*
