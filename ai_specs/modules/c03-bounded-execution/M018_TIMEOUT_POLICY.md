# M018 TimeoutPolicy — timeout_policy.rs

> **File:** `crates/hle-executor/src/timeout_policy.rs` | **Target LOC:** ~200 | **Target Tests:** 50
> **Layer:** L03 | **Cluster:** C03_BOUNDED_EXECUTION | **Error Codes:** 2230-2231
> **Role:** TERM-to-KILL bounded timeout policy primitive. Encodes the two-phase escalation pattern from `substrate_emit` (`-TERM` then 100ms sleep then `-KILL`) as a typed, validated value type with declared durations.

---

## Types at a Glance

| Type | Kind | Copy | Purpose |
|---|---|---|---|
| `TimeoutPolicy` | struct | Yes | Graceful + hard-kill duration pair |
| `TimeoutPolicyBuilder` | struct | No | Validated builder for `TimeoutPolicy` |
| `EscalationOutcome` | enum | Yes | Whether TERM or KILL was necessary |
| `TimeoutPolicyError` | enum | No | Errors 2230-2231 |

---

## TimeoutPolicy

```rust
/// A two-phase process-termination policy: send SIGTERM, wait `hard_kill`,
/// then send SIGKILL to the process group.
///
/// Both durations are `BoundedDuration` (M015) to ensure they are non-zero
/// and within valid ranges. `hard_kill` must be strictly less than `graceful`
/// to prevent the escalation window from exceeding the declared timeout.
///
/// `TimeoutPolicy` is `Copy` because it contains only two `Duration` fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeoutPolicy {
    graceful: Duration,   // total wall-clock timeout before SIGTERM is sent
    hard_kill: Duration,  // time between SIGTERM and SIGKILL (the "grace window")
}
```

### Constants

```rust
impl TimeoutPolicy {
    /// Default policy matching substrate_emit: 30s graceful, 100ms hard-kill window.
    pub const DEFAULT: TimeoutPolicy = TimeoutPolicy {
        graceful:  Duration::from_secs(30),
        hard_kill: Duration::from_millis(100),
    };

    /// Minimal policy for tests: 10ms graceful, 50ms hard-kill window.
    /// Note: hard_kill > graceful is intentional here as a test fixture only;
    /// production policies must have hard_kill < graceful.
    pub const TEST_FAST: TimeoutPolicy = TimeoutPolicy {
        graceful:  Duration::from_millis(10),
        hard_kill: Duration::from_millis(50),
    };
}
```

### Methods

| Method | Signature | Notes |
|---|---|---|
| `builder` | `fn() -> TimeoutPolicyBuilder` | #[must_use] |
| `graceful` | `const fn(&self) -> Duration` | #[must_use] |
| `hard_kill` | `const fn(&self) -> Duration` | #[must_use] |
| `total_budget` | `fn(&self) -> Duration` | #[must_use]. `graceful + hard_kill` |
| `apply` | `fn(&self, child: &mut Child) -> Result<EscalationOutcome, TimeoutPolicyError>` | Blocking. Sends TERM then KILL. |

### apply algorithm

```rust
pub fn apply(&self, child: &mut Child) -> Result<EscalationOutcome, TimeoutPolicyError> {
    let started = Instant::now();

    // Poll at 10ms intervals until done or graceful timeout
    while started.elapsed() < self.graceful {
        match child.try_wait() {
            Ok(Some(_)) => return Ok(EscalationOutcome::ExitedCleanly),
            Ok(None) => {}
            Err(e) => return Err(TimeoutPolicyError::TimeoutElapsed {
                graceful_ms: self.graceful.as_millis() as u64,
                hard_kill_ms: self.hard_kill.as_millis() as u64,
            }),
        }
        thread::sleep(Duration::from_millis(10));
    }

    // Graceful timeout elapsed — send SIGTERM to process group
    terminate_process_group(child.id(), "-TERM");
    thread::sleep(self.hard_kill);

    // Hard-kill window elapsed — send SIGKILL to process group
    terminate_process_group(child.id(), "-KILL");
    child.kill().ok(); // belt-and-suspenders: also kill the direct child

    Ok(EscalationOutcome::KilledByPolicy)
}

fn terminate_process_group(pid: u32, signal: &str) {
    Command::new("/usr/bin/kill")
        .arg(signal)
        .arg(format!("-{pid}"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok();
}
```

The `terminate_process_group` helper matches the existing `substrate_emit::terminate_child_group` function exactly. It sends the signal to the negative PID (`-pid`) which targets the entire process group created by `Command::process_group(0)` in `LocalRunner`.

---

## TimeoutPolicyBuilder

```rust
#[derive(Debug, Default)]
pub struct TimeoutPolicyBuilder {
    graceful: Option<Duration>,
    hard_kill: Option<Duration>,
}

impl TimeoutPolicyBuilder {
    #[must_use] pub fn graceful(self, d: Duration) -> Self;
    #[must_use] pub fn hard_kill(self, d: Duration) -> Self;
    /// Validates: graceful non-zero, hard_kill non-zero, hard_kill < graceful.
    pub fn build(self) -> Result<TimeoutPolicy, TimeoutPolicyError>;
}
```

Validation rules enforced by `build()`:

1. `graceful` must be non-zero.
2. `hard_kill` must be non-zero.
3. `hard_kill` must be strictly less than `graceful`. (Rationale: if `hard_kill >= graceful`, the TERM→KILL escalation window would arrive before or simultaneously with the graceful deadline, making the two-phase escalation meaningless.)

---

## EscalationOutcome

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscalationOutcome {
    /// The child exited on its own before the graceful timeout.
    ExitedCleanly,
    /// The child was sent SIGTERM then SIGKILL; the process group was killed.
    KilledByPolicy,
}

impl EscalationOutcome {
    #[must_use] pub const fn was_killed(self) -> bool;
    #[must_use] pub const fn as_str(self) -> &'static str; // "clean" | "killed"
}
```

**Traits:** `Display` ("ExitedCleanly" / "KilledByPolicy")

---

## TimeoutPolicyError

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimeoutPolicyError {
    /// Code 2230. Escalation completed; child was killed by the policy.
    /// This is informational — it does not prevent `CommandOutput` from being returned.
    TimeoutElapsed { graceful_ms: u64, hard_kill_ms: u64 },
    /// Code 2231. Policy values are invalid (zero duration, inverted range).
    InvalidTimeoutPolicy { reason: String },
}
```

`TimeoutElapsed` is an informational error: `LocalRunner` converts it to `CommandOutput { timed_out: true, exit_code: None }` rather than propagating it as a hard error. The receipt message records `"local command timed out after Ns"` matching the existing `substrate_emit` format.

**Traits:** `Display` ("[HLE-2230] timeout elapsed: graceful=30s hard_kill=100ms"), `std::error::Error`

---

## Design Notes

- `TimeoutPolicy` is `Copy` because it contains only two `Duration` values, which are themselves `Copy`. This avoids cloning at `LocalRunner::run` call sites.
- The `DEFAULT` associated constant matches the hardcoded values in `substrate_emit` (`LOCAL_COMMAND_TIMEOUT = Duration::from_secs(30)` and `thread::sleep(Duration::from_millis(100))`). M018 makes these configurable and validated.
- `TEST_FAST` has `hard_kill > graceful` which would normally be rejected by `build()`. It is a `const` that bypasses the builder, used only in test harnesses to make tests run fast. Production code always uses the builder.
- The 10ms poll interval (`thread::sleep(Duration::from_millis(10))`) is inherited from the existing substrate_emit implementation. This is not a busy-spin and is acceptable for foreground M0 execution.
- `apply` does not use `tokio::time::timeout` because C03 is synchronous (AP29 compliance). The thread-sleep poll is the correct pattern here.
- `child.kill().ok()` after `terminate_process_group` with `-KILL` is belt-and-suspenders: in rare cases the process group signal may not reach a child that has already been orphaned from its group. The direct kill ensures the process struct is cleaned up.

---

## Cluster Invariants Enforced by M018

- **I-C03-4:** Every `LocalRunner` invocation is bounded by a `TimeoutPolicy`; there is no code path where a child process can run indefinitely.
- **I-C03-5:** SIGTERM is always sent before SIGKILL; SIGKILL is never the first signal.
- **I-C03-6:** TERM and KILL are sent to the process group (`-PID`), not just the direct child, covering all descendants spawned by the command.

---

*M018 TimeoutPolicy Spec v1.0 | C03 Bounded Execution | 2026-05-10*
