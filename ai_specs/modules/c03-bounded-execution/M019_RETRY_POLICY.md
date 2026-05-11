# M019 RetryPolicy — retry_policy.rs

> **File:** `crates/hle-executor/src/retry_policy.rs` | **Target LOC:** ~260 | **Target Tests:** 55
> **Layer:** L03 | **Cluster:** C03_BOUNDED_EXECUTION | **Error Codes:** 2240-2241
> **Role:** Explicit bounded retry semantics. Provides `RetryPolicy`, `BackoffStrategy`, and
> `RetryBudget` so that every retry loop in C03 has a declared ceiling on attempt count and
> inter-attempt wait time. There is no `Infinite` variant — total attempt count is always
> knowable at policy construction time (C12_UNBOUNDED_COLLECTIONS).

---

## Types at a Glance

| Type | Kind | Copy | Purpose |
|---|---|---|---|
| `RetryPolicy` | struct | Yes | Declared max attempts + backoff strategy; `Copy` because it contains only primitives |
| `BackoffStrategy` | enum | Yes | `NoRetry`, `Fixed(Duration)`, `Exponential { base, cap }` |
| `RetryBudget` | struct | No | Mutable execution handle tracking remaining attempts and next sleep |
| `RetryPolicyError` | enum | No | Errors 2240-2241 |

---

## RetryPolicy

```rust
/// Bounded retry configuration.
///
/// `max_attempts` is a `NonZeroU32` so that zero-attempt policies cannot be
/// constructed. The maximum total wait across all retries is `max_attempts * backoff.max_sleep()`,
/// which is always finite (C12_UNBOUNDED_COLLECTIONS invariant).
///
/// `RetryPolicy` is `Copy` because `BackoffStrategy` is `Copy` and `NonZeroU32` is `Copy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    max_attempts: NonZeroU32,
    backoff: BackoffStrategy,
}
```

### Constants

```rust
impl RetryPolicy {
    /// Single attempt; no retry. The default for step execution in C03.
    pub const NO_RETRY: RetryPolicy = RetryPolicy {
        max_attempts: NonZeroU32::MIN,  // 1
        backoff: BackoffStrategy::NoRetry,
    };

    /// Three attempts with a fixed 200ms inter-attempt wait.
    /// Used for transient spawn failures (EAGAIN).
    pub const SPAWN_RETRY_3: RetryPolicy = RetryPolicy {
        max_attempts: unsafe { NonZeroU32::new_unchecked(3) },
        backoff: BackoffStrategy::Fixed(Duration::from_millis(200)),
    };
}
```

`SPAWN_RETRY_3::new_unchecked` is the only `unsafe` in M019, and it is behind a named
constant so it is auditable in a single location. The value 3 is statically correct;
no runtime check is required.

### Methods

| Method | Signature | Notes |
|---|---|---|
| `new` | `fn(max_attempts: NonZeroU32, backoff: BackoffStrategy) -> Result<Self, RetryPolicyError>` | Validates that `backoff` is consistent with `max_attempts` (see below). `#[must_use]` |
| `no_retry` | `const fn() -> Self` | `#[must_use]`. Alias for `NO_RETRY`. |
| `fixed` | `fn(attempts: u32, wait_ms: u64) -> Result<Self, RetryPolicyError>` | `#[must_use]`. Convenience constructor. |
| `exponential` | `fn(attempts: u32, base_ms: u64, cap_ms: u64) -> Result<Self, RetryPolicyError>` | `#[must_use]`. Validates `cap_ms >= base_ms`. |
| `max_attempts` | `const fn(&self) -> u32` | `#[must_use]`. The ceiling on attempt count. |
| `backoff` | `const fn(&self) -> BackoffStrategy` | `#[must_use]` |
| `budget` | `fn(&self) -> RetryBudget` | `#[must_use]`. Creates a fresh `RetryBudget` for one execution run. |
| `execute` | `fn<T, E, F>(&self, f: F) -> Result<T, E> where F: Fn() -> Result<T, E>, E: RetryableError` | `#[must_use]`. Executes `f` up to `max_attempts` times; sleeps between attempts per `backoff`. |

### Validation rules for `new`

1. If `backoff` is `BackoffStrategy::NoRetry` and `max_attempts > 1`, returns
   `Err(RetryPolicyError::InvalidRetryPolicy { reason: "NoRetry backoff requires max_attempts == 1" })`.
2. For `Exponential { base, cap }`, `cap` must be `>= base`; otherwise returns
   `Err(RetryPolicyError::BackoffOverflow { reason: "cap < base" })`.

---

## BackoffStrategy

```rust
/// Strategy for the sleep between retry attempts.
///
/// All variants are `Copy` and carry no heap allocation.
/// Durations are raw `Duration` rather than `BoundedDuration` because
/// `BackoffStrategy` itself is a policy type — the valid ranges are
/// enforced by `RetryPolicy::new` at construction time, not by
/// individual field wrapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackoffStrategy {
    /// No sleep between attempts; `max_attempts` must equal 1.
    NoRetry,
    /// Fixed sleep of `d` between every attempt.
    Fixed(Duration),
    /// Exponential backoff: sleep doubles each attempt, capped at `cap`.
    ///
    /// Attempt 1 sleeps `base`, attempt 2 sleeps `base * 2`, ...,
    /// clamped to `cap`. Total max wait = `max_attempts * cap`.
    Exponential { base: Duration, cap: Duration },
}

impl BackoffStrategy {
    /// Maximum sleep for a single inter-attempt wait.
    #[must_use]
    pub const fn max_sleep(self) -> Duration {
        match self {
            Self::NoRetry          => Duration::ZERO,
            Self::Fixed(d)         => d,
            Self::Exponential { cap, .. } => cap,
        }
    }

    /// Compute the sleep for attempt number `n` (1-indexed).
    #[must_use]
    pub fn sleep_for(self, n: u32) -> Duration {
        match self {
            Self::NoRetry => Duration::ZERO,
            Self::Fixed(d) => d,
            Self::Exponential { base, cap } => {
                // 2^(n-1) * base, clamped to cap
                let factor = 1_u64.saturating_shl(n.saturating_sub(1));
                base.saturating_mul(factor as u32).min(cap)
            }
        }
    }

    /// Upper bound on total wait across `attempts` retries.
    #[must_use]
    pub fn total_max_wait(self, attempts: u32) -> Duration {
        self.max_sleep().saturating_mul(attempts.saturating_sub(1))
    }
}
```

**Traits:** `Display` ("NoRetry" / "Fixed(200ms)" / "Exponential(base=100ms,cap=5s)")

---

## RetryBudget

```rust
/// Execution-time state for one retry run.
///
/// Obtained from `RetryPolicy::budget()` and consumed by `RetryBudget::next_attempt`.
/// `RetryBudget` is deliberately not `Copy` — it is a one-shot mutable cursor.
#[derive(Debug)]
pub struct RetryBudget {
    remaining: u32,
    policy: RetryPolicy,
    attempt: u32,  // 1-indexed, for backoff calculation
}

impl RetryBudget {
    /// Returns `true` when at least one attempt remains.
    #[must_use]
    pub fn has_remaining(&self) -> bool;

    /// Returns the number of attempts remaining (including the current one).
    #[must_use]
    pub fn remaining(&self) -> u32;

    /// Consumes one attempt slot and sleeps per the backoff strategy.
    ///
    /// Returns `Ok(())` when the slot was consumed; `Err(RetryExhausted)` when
    /// no slots remain. Does not sleep on the first attempt (attempt == 1).
    pub fn next_attempt(&mut self) -> Result<(), RetryPolicyError>;
}
```

### Usage pattern

```rust
// Typical usage inside PhaseExecutor::run_step_with_retry:
let mut budget = self.retry_policy.budget();
loop {
    budget.next_attempt()?;       // Err if exhausted; sleeps on attempt >= 2
    match self.runner.run(&step.command) {
        Ok(output) => return Ok((output.to_step_state(), output.combined_message)),
        Err(e) if e.is_retryable() && budget.has_remaining() => {
            // log the transient error and loop
        }
        Err(e) => return Err(e.into()),
    }
}
```

The `execute` convenience method on `RetryPolicy` wraps this pattern for closures that
return `Result<T, E>` where `E: RetryableError`. Both `RetryBudget` (manual) and
`execute` (closure) are valid; prefer `execute` for simple call sites.

---

## RetryableError Trait

```rust
/// Trait for error types that can declare retryability.
///
/// Implemented by `RunnerError` (M016) so that `RetryPolicy::execute` can
/// discriminate retryable spawn failures from non-retryable command rejections.
pub trait RetryableError {
    fn is_retryable(&self) -> bool;
}
```

`RunnerError::SpawnFailed` is retryable when the OS error kind is
`ErrorKind::ResourceBusy` or `ErrorKind::WouldBlock` (EAGAIN). All other `RunnerError`
variants return `false`.

`PhaseExecutorError` does not implement `RetryableError` — retry is a runner-level
concern, not a phase-sequencer concern.

---

## RetryPolicyError

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetryPolicyError {
    /// Code 2240. All retry attempts were exhausted without success.
    RetryExhausted { attempts: u32, last_error: String },
    /// Code 2241. The policy itself is invalid (zero attempts, negative backoff,
    /// mismatched NoRetry/attempts, or cap < base for Exponential).
    InvalidRetryPolicy { reason: String },
    /// Code 2241 (same code, sub-case). Exponential backoff cap < base.
    BackoffOverflow { reason: String },
}

impl RetryPolicyError {
    #[must_use] pub const fn error_code(&self) -> u32 {
        match self {
            Self::RetryExhausted { .. }   => 2240,
            Self::InvalidRetryPolicy { .. } | Self::BackoffOverflow { .. } => 2241,
        }
    }
    #[must_use] pub const fn is_retryable(&self) -> bool { false }
}
```

**Traits:** `Display` ("[HLE-2240] retry exhausted after N attempts: ..."), `std::error::Error`

---

## Design Notes

- `NonZeroU32` for `max_attempts` is the key safety property: it makes `RetryPolicy` with
  zero attempts unrepresentable at the type level, not just at runtime. The compiler
  enforces the C12 (no unbounded collections) invariant — there is no path to an infinite
  loop.

- The `SPAWN_RETRY_3` constant uses `unsafe { NonZeroU32::new_unchecked(3) }` in a
  `const` context because `NonZeroU32::new(3)` is not yet a `const fn` in stable Rust
  1.75. This is the only `unsafe` in M019 and is trivially auditable. The value 3 is
  correct; the `new_unchecked` invariant (value is non-zero) holds.

- `BackoffStrategy::sleep_for` uses `saturating_mul` and `saturating_shl` to prevent
  overflow in the exponential case. The `cap` ensures the sleep never grows larger than
  declared even if `attempts` is large — total wait is always `O(max_attempts * cap)`.

- Retry does not apply to `StepState::Failed` outcomes from a command that runs
  successfully but returns a non-zero exit code. That is a correctness failure, not a
  transient infrastructure failure. `LocalRunner` returns `CommandOutput { exit_code:
  Some(nonzero) }` in that case; `is_retryable()` returns `false` for correctness
  failures.

- `RetryPolicy::execute` accepts a closure so that call sites in M017 do not need to
  manually manage `RetryBudget`. The closure API is preferable for simple step invocations;
  `RetryBudget` remains available for callers that need interleaved logic between attempts.

- The `total_max_wait` method on `BackoffStrategy` allows callers to enforce a
  pre-flight check: if `backoff.total_max_wait(policy.max_attempts())` exceeds an operator
  deadline, the policy should be tightened. This is a documentation-level contract; M019
  does not automatically enforce a global deadline beyond per-attempt sleep.

---

## Cluster Invariants Enforced by M019

- **I-C03-7:** All retry loops in C03 use `RetryPolicy`; bare `for` or `loop` constructs
  with manual counters are forbidden (C12 / `HLE-UP-003`).
- **I-C03-8:** `max_attempts` is always finite and non-zero; `Infinite` retry is
  unrepresentable in M019's type system.
- **I-C03-9:** Retry applies only to transient infrastructure failures (`RunnerError::SpawnFailed`
  with EAGAIN). Semantic failures (`StepState::Failed`, `CommandRejected`) are not retried.

---

## Cross-Cluster Events

- `RetryPolicy` is consumed by M017 (`PhaseExecutor`) to configure per-step retry.
- `RetryExhausted` propagates upward as a `PhaseExecutorError::StepFailed` so the
  verifier sees a `Failed` receipt with the retry history embedded in the message.
- M019 does not import from C01, C04, C05, C06, or C07. Data flows outward.

---

*M019 RetryPolicy Spec v1.0 | C03 Bounded Execution | 2026-05-10*
