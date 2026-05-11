# M042 AtuinQiBridge — atuin_qi_bridge.rs

> **File:** `crates/hle-bridge/src/atuin_qi_bridge.rs` | **Target LOC:** ~300 | **Target Tests:** 55
> **Layer:** L05 | **Cluster:** C07_DISPATCH_BRIDGES | **Error Codes:** 2620-2622
> **Role:** Integration bridge for the framework's `hle-*` Atuin script registry. Provides read-only enumerate/status/last-run surfaces; write-side (register/run) is compile-time sealed via `Sealed<Class>` PhantomData until M2+ authorization.

---

## Types at a Glance

| Type | Kind | Copy | Purpose |
|---|---|---|---|
| `AtuinQiBridge<Class>` | struct | No | Bridge adapter parameterized over `Sealed<ReadOnly>` or `Sealed<LiveWrite>` |
| `ScriptEntry` | struct | No | Immutable record of a discovered `hle-*` Atuin script |
| `ScriptStatus` | struct | No | Last-run outcome for a script: exit code, stdout preview, elapsed |
| `ScriptName` | struct | No | Validated `hle-*` prefixed script name (non-empty, ≤ 128 chars) |
| `RunReceipt` | struct | No | BridgeReceipt-compatible record of a bounded script run |
| `AtuinQiBridgeError` | enum | No | Errors 2620-2622 for script not found, run failures, enumeration failures |

---

## ScriptName

```rust
/// A validated Atuin script name following the `hle-*` convention.
///
/// All scripts managed by this bridge must have names prefixed with `hle-`.
/// Names are ASCII, non-empty, and at most 128 characters. The prefix is
/// enforced at construction time; callers cannot accidentally target
/// non-HLE scripts through this bridge.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ScriptName(String);

pub const SCRIPT_NAME_PREFIX: &str = "hle-";
pub const SCRIPT_NAME_MAX_LEN: usize = 128;
```

| Method | Signature | Notes |
|---|---|---|
| `new` | `fn(name: impl Into<String>) -> Result<Self, AtuinQiBridgeError>` | Validates prefix, length, and ASCII-only. |
| `as_str` | `fn(&self) -> &str` | `#[must_use]`. |
| `without_prefix` | `fn(&self) -> &str` | `#[must_use]`. Strips `hle-` for display. |

**Traits:** `Display` ("hle-NAME")

---

## ScriptEntry

```rust
/// Immutable discovered record for a single `hle-*` Atuin script.
///
/// Populated by `AtuinQiBridge::enumerate`. Fields reflect what
/// `atuin scripts list --format json` returns for each entry. The
/// `description` field is optional — scripts without embedded docs
/// return `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptEntry {
    pub name: ScriptName,
    pub description: Option<String>,
    pub registered_at_epoch_s: u64,
    pub content_sha256: [u8; 32],
}
```

| Method | Signature | Notes |
|---|---|---|
| `new` | `fn(name: ScriptName, content: &[u8]) -> Self` | Computes `content_sha256` from `content`. |
| `hex_sha` | `fn(&self) -> String` | `#[must_use]`. Lowercase hex of `content_sha256`. |
| `is_hle_script` | `fn(&self) -> bool` | `#[must_use]`. Always true (validated at `ScriptName` level). |

**Traits:** `Display` ("ScriptEntry(hle-NAME sha=SHORT)")

---

## ScriptStatus

```rust
/// Outcome snapshot for the most recent run of a named script.
///
/// Populated by `AtuinQiBridge::last_run_status`. `stdout_preview` is
/// bounded to `SCRIPT_STATUS_PREVIEW_CAP` (512 bytes) — longer outputs
/// are truncated with a `[TRUNCATED]` suffix. This bound prevents unbounded
/// memory from long-running script output accumulation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptStatus {
    pub name: ScriptName,
    pub exit_code: i32,
    pub elapsed_ms: u64,
    pub stdout_preview: String,       // bounded to SCRIPT_STATUS_PREVIEW_CAP
    pub last_run_tick: u64,
}

pub const SCRIPT_STATUS_PREVIEW_CAP: usize = 512;
```

| Method | Signature | Notes |
|---|---|---|
| `is_success` | `fn(&self) -> bool` | `#[must_use]`. `exit_code == 0`. |
| `is_stale` | `fn(&self, now_tick: u64, ttl_ticks: u64) -> bool` | `#[must_use]`. True when `now_tick - last_run_tick > ttl_ticks`. |

**Traits:** `Display` ("ScriptStatus(hle-NAME exit=N elapsed=Ms)")

---

## AtuinQiBridge (Generic)

```rust
/// Atuin QI script registry bridge parameterized over capability class.
///
/// `AtuinQiBridge<Sealed<ReadOnly>>` exposes enumerate, last-run status,
/// and probe operations. `AtuinQiBridge<Sealed<LiveWrite>>` additionally
/// exposes `run_script` and `register_script`, which are absent from the
/// `ReadOnly` impl block. The compiler enforces this separation — there is
/// no runtime capability check guarding the write path.
///
/// All subprocess calls use `atuin scripts` subcommands bounded by
/// C03 `BoundedDuration`. The bridge is stateless: no persistent Atuin
/// session or connection is held between calls.
#[derive(Debug)]
pub struct AtuinQiBridge<Class> {
    _class: Class,
}
```

### BridgeContract impl

```rust
impl<Class: Send + Sync + std::fmt::Debug> BridgeContract for AtuinQiBridge<Class> {
    fn schema_id(&self) -> &'static str { "hle.atuin_qi.v1" }
    fn port(&self) -> Option<u16> { None }   // CLI/filesystem bridge; no TCP port
    fn paths(&self) -> &[&'static str] {
        &["atuin scripts list", "atuin scripts run", "atuin scripts add"]
    }
    fn supports_write(&self) -> bool { ... } // false for ReadOnly class
    fn capability_class(&self) -> CapabilityClass { ... }
    fn name(&self) -> &'static str { "atuin_qi_bridge" }
}
```

---

## Method Table

### ReadOnly surface (both `Sealed<ReadOnly>` and `Sealed<LiveWrite>`)

| Method | Signature | Notes |
|---|---|---|
| `new_read_only` | `fn() -> AtuinQiBridge<Sealed<ReadOnly>>` | No authorization required. |
| `probe` | `fn(&self, timeout: BoundedDuration) -> Result<bool, AtuinQiBridgeError>` | `#[must_use]`. Checks whether `atuin` binary is on PATH and `atuin scripts list` exits 0. Returns `false` if absent or non-zero, not `Err`. `Err` only for subprocess infrastructure failures. |
| `enumerate` | `fn(&self, timeout: BoundedDuration) -> Result<Vec<ScriptEntry>, AtuinQiBridgeError>` | `#[must_use]`. Calls `atuin scripts list --filter hle-` and parses output into `Vec<ScriptEntry>`. Returns `Err(EnumerationFailed)` on non-zero exit or parse error. Output is bounded: at most 1,024 script entries. |
| `last_run_status` | `fn(&self, name: &ScriptName, timeout: BoundedDuration) -> Result<Option<ScriptStatus>, AtuinQiBridgeError>` | `#[must_use]`. Returns `None` if the script has never been run. Queries Atuin history for the named script. |
| `script_exists` | `fn(&self, name: &ScriptName, timeout: BoundedDuration) -> Result<bool, AtuinQiBridgeError>` | `#[must_use]`. Convenience wrapper: enumerate then linear search. Bounded. |

### Write surface (only `Sealed<LiveWrite>`)

| Method | Signature | Notes |
|---|---|---|
| `new_live_write` | `fn(_token: &WriteAuthToken) -> AtuinQiBridge<Sealed<LiveWrite>>` | Validates token expiry. |
| `run_script` | `fn(&self, name: &ScriptName, _token: &WriteAuthToken, timeout: BoundedDuration) -> Result<RunReceipt, AtuinQiBridgeError>` | `#[must_use]`. Executes `atuin scripts run NAME` as a bounded one-shot subprocess. Returns `RunReceipt` with SHA-256 of stdout. Fails fast on non-zero exit with `ScriptRunFailed`. |
| `register_script` | `fn(&self, name: &ScriptName, content: &[u8], _token: &WriteAuthToken, timeout: BoundedDuration) -> Result<BridgeReceipt, AtuinQiBridgeError>` | `#[must_use]`. Calls `atuin scripts add` to register a new HLE script. Returns `BridgeReceipt` with SHA-256 of `content`. |

---

## RunReceipt

```rust
/// BridgeReceipt-compatible record for a completed bounded script run.
///
/// Carries the script name, exit code, stdout SHA-256, and elapsed time.
/// Converts into `BridgeReceipt` for routing through the C01 verifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunReceipt {
    pub script_name: ScriptName,
    pub exit_code: i32,
    pub elapsed_ms: u64,
    pub stdout_sha256: [u8; 32],
    pub auth_receipt_id: u64,
}
```

| Method | Signature | Notes |
|---|---|---|
| `into_bridge_receipt` | `fn(self) -> BridgeReceipt` | Converts to C01-routable `BridgeReceipt`. |
| `is_success` | `fn(&self) -> bool` | `#[must_use]`. `exit_code == 0`. |

---

## AtuinQiBridgeError

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AtuinQiBridgeError {
    /// Code 2620. Named `hle-*` script not found in the Atuin registry.
    ScriptNotFound { name: String },
    /// Code 2621. Script run returned non-zero exit code.
    /// `retryable` is true for transient failures (SIGTERM, resource exhaustion).
    ScriptRunFailed { name: String, exit_code: i32, retryable: bool },
    /// Code 2622. `atuin scripts list` failed or returned unparseable output.
    EnumerationFailed { reason: String, retryable: bool },
}
```

| Method | Signature |
|---|---|
| `error_code` | `const fn(&self) -> u32` — 2620, 2621, or 2622 |
| `is_retryable` | `fn(&self) -> bool` — propagates inner `retryable` field |

**Traits:** `Display` ("[HLE-262N] ..."), `std::error::Error`

---

## Design Notes

- The `enumerate` method uses `--filter hle-` to restrict Atuin output to `hle-*` scripts, preventing this bridge from accidentally surfacing unrelated shell scripts. The prefix constraint is also enforced at the `ScriptName` type level.
- `stdout_preview` in `ScriptStatus` is hard-bounded at 512 bytes at parse time. Callers needing full stdout must run the script directly via `run_script` and hash the output themselves; the status surface is for monitoring, not data extraction.
- Subprocess calls do not use `set -e` / `set -euo pipefail` conventions — they inspect the exit code explicitly, matching the fleet scripting conventions from `CLAUDE.md §Shell Scripting Conventions`. A zero-match `enumerate` returns an empty `Vec`, not an `Err`.
- `register_script` and `run_script` both require the `WriteAuthToken` at the call site, not just at construction. This double-token pattern is consistent with M041.
- The bridge emits no background threads and holds no persistent Atuin session. Each call forks a bounded subprocess and waits for termination within the provided `BoundedDuration`.

---

## Cluster Invariants (C07) Enforced by M042

- **I-C07-2 / I-C07-3:** `run_script` and `register_script` exist only on `Sealed<LiveWrite>` impl block.
- **I-C07-4:** No `hle-executor` import in `Cargo.toml`.
- **I-C07-5:** All subprocess calls bounded by caller-supplied `BoundedDuration`.
- **I-C07-6:** `run_script` returns `RunReceipt` (converts to `BridgeReceipt`); `register_script` returns `BridgeReceipt` directly.

---

## Test Targets (55 minimum)

| Group | Count | Coverage Focus |
|---|---|---|
| `ScriptName` validation | 8 | valid prefix, missing prefix, empty, too long, non-ASCII |
| `ScriptEntry` SHA correctness | 6 | SHA matches content bytes, `hex_sha` format |
| `ScriptStatus` staleness and display | 6 | is_stale boundary, stdout truncation, is_success |
| `probe` binary absent and present | 6 | absent PATH, present PATH, timeout respected |
| `enumerate` parse variants | 8 | empty list, one script, many scripts, parse error |
| `script_exists` delegation | 4 | found, not found, enumerate error |
| `last_run_status` None and Some | 4 | never-run, last-run-entry, parse error |
| Write-gate sealed errors | 5 | ReadOnly block, expired token, zero receipt |
| `run_script` receipt and failure | 5 | success-SHA, non-zero-exit, retryable flag |
| `RunReceipt` into_bridge_receipt | 3 | field mapping, is_success |

---

*M042 AtuinQiBridge Spec v1.0 | C07 Dispatch Bridges | 2026-05-10*
