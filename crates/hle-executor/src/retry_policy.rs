//! M019 `RetryPolicy` — explicit bounded retry semantics.
//!
//! Provides [`RetryPolicy`], [`BackoffStrategy`], and [`RetryBudget`] so that
//! every retry loop in C03 has a declared ceiling on attempt count and
//! inter-attempt wait time.  There is no `Infinite` variant — total attempt
//! count is always knowable at policy-construction time (`C12_UNBOUNDED_COLLECTIONS`).
//!
//! Error codes: 2240–2241.

use std::fmt;
use std::num::NonZeroU32;
use std::thread;
use std::time::Duration;

// ── RetryPolicyError ─────────────────────────────────────────────────────────

/// Errors produced by M019 retry-policy construction and execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetryPolicyError {
    /// `[HLE-2240]` All retry attempts were exhausted without success.
    RetryExhausted {
        /// Total attempts consumed.
        attempts: u32,
        /// String representation of the last error.
        last_error: String,
    },
    /// `[HLE-2241]` The policy itself is invalid (mismatched `NoRetry`/attempts,
    /// zero duration, etc.).
    InvalidRetryPolicy {
        /// Human-readable reason.
        reason: String,
    },
    /// `[HLE-2241]` Exponential backoff `cap < base`.
    BackoffOverflow {
        /// Human-readable reason.
        reason: String,
    },
}

impl RetryPolicyError {
    /// HLE error code: 2240 for exhaustion, 2241 for invalid config.
    #[must_use]
    pub const fn error_code(&self) -> u32 {
        match self {
            Self::RetryExhausted { .. } => 2240,
            Self::InvalidRetryPolicy { .. } | Self::BackoffOverflow { .. } => 2241,
        }
    }

    /// All `RetryPolicyError` variants are non-retryable.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        false
    }
}

impl fmt::Display for RetryPolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RetryExhausted {
                attempts,
                last_error,
            } => write!(
                f,
                "[HLE-2240] retry exhausted after {attempts} attempt(s): {last_error}"
            ),
            Self::InvalidRetryPolicy { reason } => {
                write!(f, "[HLE-2241] invalid retry policy: {reason}")
            }
            Self::BackoffOverflow { reason } => {
                write!(f, "[HLE-2241] backoff overflow: {reason}")
            }
        }
    }
}

impl std::error::Error for RetryPolicyError {}

// ── RetryableError trait ──────────────────────────────────────────────────────

/// Trait for error types that can declare retryability.
///
/// Implemented by [`crate::local_runner::RunnerError`] (M016) so that
/// [`RetryPolicy::execute`] can discriminate transient spawn failures from
/// non-retryable command rejections.
pub trait RetryableError {
    /// Returns `true` when the error represents a transient infrastructure
    /// failure that is safe to retry.
    fn is_retryable(&self) -> bool;
}

// ── BackoffStrategy ──────────────────────────────────────────────────────────

/// Strategy for the sleep between retry attempts.
///
/// All variants are `Copy` and carry no heap allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackoffStrategy {
    /// No sleep between attempts; `max_attempts` must equal 1.
    NoRetry,
    /// Fixed sleep of `d` between every attempt.
    Fixed(Duration),
    /// Exponential backoff: sleep doubles each attempt, capped at `cap`.
    ///
    /// Attempt 1 sleeps `base`, attempt 2 sleeps `base * 2`, ..., clamped to
    /// `cap`.  Total max wait = `max_attempts * cap`.
    Exponential {
        /// Starting sleep duration.
        base: Duration,
        /// Maximum sleep per inter-attempt gap.
        cap: Duration,
    },
}

impl BackoffStrategy {
    /// Maximum sleep for a single inter-attempt wait.
    #[must_use]
    pub const fn max_sleep(self) -> Duration {
        match self {
            Self::NoRetry => Duration::ZERO,
            Self::Fixed(d) => d,
            Self::Exponential { cap, .. } => cap,
        }
    }

    /// Compute the sleep duration for attempt number `n` (1-indexed).
    ///
    /// Attempt 1 has no sleep (the first call never waits).  The first
    /// *inter-attempt* sleep is at `n == 2`.
    #[must_use]
    pub fn sleep_for(self, n: u32) -> Duration {
        match self {
            Self::NoRetry => Duration::ZERO,
            Self::Fixed(d) => d,
            Self::Exponential { base, cap } => {
                // 2^(n-1) * base, clamped to cap.
                // Use checked_shl to avoid overflow; fall back to u64::MAX.
                let shift = n.saturating_sub(1);
                let factor: u64 = if shift >= 64 {
                    u64::MAX
                } else {
                    1_u64 << shift
                };
                let factor_u32 = u32::try_from(factor).unwrap_or(u32::MAX);
                base.saturating_mul(factor_u32).min(cap)
            }
        }
    }

    /// Upper bound on total wait across `attempts` retries.
    ///
    /// Note: attempt 1 has no wait, so there are `attempts - 1` inter-attempt
    /// sleeps at most.
    #[must_use]
    pub fn total_max_wait(self, attempts: u32) -> Duration {
        self.max_sleep().saturating_mul(attempts.saturating_sub(1))
    }
}

impl fmt::Display for BackoffStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoRetry => f.write_str("NoRetry"),
            Self::Fixed(d) => write!(f, "Fixed({d:?})"),
            Self::Exponential { base, cap } => {
                write!(f, "Exponential(base={base:?},cap={cap:?})")
            }
        }
    }
}

// ── RetryPolicy ──────────────────────────────────────────────────────────────

/// Bounded retry configuration.
///
/// `max_attempts` is a [`NonZeroU32`] so that zero-attempt policies cannot be
/// constructed.  The maximum total wait across all retries is
/// `max_attempts * backoff.max_sleep()`, which is always finite
/// (`C12_UNBOUNDED_COLLECTIONS` invariant).
///
/// `RetryPolicy` is `Copy` because [`BackoffStrategy`] is `Copy` and
/// [`NonZeroU32`] is `Copy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    max_attempts: NonZeroU32,
    backoff: BackoffStrategy,
}

impl RetryPolicy {
    /// Single attempt; no retry.  The default for step execution in C03.
    pub const NO_RETRY: Self = Self {
        max_attempts: NonZeroU32::MIN, // 1
        backoff: BackoffStrategy::NoRetry,
    };

    /// Three attempts with a fixed 200 ms inter-attempt wait.
    ///
    /// Used for transient spawn failures (EAGAIN).
    ///
    /// `NonZeroU32::MIN` is 1; we use `NonZeroU32::MIN.saturating_add(2)` to
    /// express 3 without `unsafe`, since `forbid(unsafe_code)` applies
    /// workspace-wide.
    pub const SPAWN_RETRY_3: Self = Self {
        max_attempts: NonZeroU32::MIN.saturating_add(2), // 1 + 2 = 3
        backoff: BackoffStrategy::Fixed(Duration::from_millis(200)),
    };

    /// Alias for [`NO_RETRY`][Self::NO_RETRY].
    #[must_use]
    pub const fn no_retry() -> Self {
        Self::NO_RETRY
    }

    /// Construct with explicit `max_attempts` and `backoff`.
    ///
    /// # Errors
    ///
    /// Returns [`RetryPolicyError::InvalidRetryPolicy`] when `backoff` is
    /// `NoRetry` and `max_attempts > 1`.
    /// Returns [`RetryPolicyError::BackoffOverflow`] when `backoff` is
    /// `Exponential` with `cap < base`.
    pub fn new(
        max_attempts: NonZeroU32,
        backoff: BackoffStrategy,
    ) -> Result<Self, RetryPolicyError> {
        if matches!(backoff, BackoffStrategy::NoRetry) && max_attempts.get() > 1 {
            return Err(RetryPolicyError::InvalidRetryPolicy {
                reason: String::from("NoRetry backoff requires max_attempts == 1"),
            });
        }
        if let BackoffStrategy::Exponential { base, cap } = backoff {
            if cap < base {
                return Err(RetryPolicyError::BackoffOverflow {
                    reason: format!("cap ({cap:?}) < base ({base:?})"),
                });
            }
        }
        Ok(Self {
            max_attempts,
            backoff,
        })
    }

    /// Convenience constructor for a fixed-backoff policy.
    ///
    /// # Errors
    ///
    /// Returns [`RetryPolicyError::InvalidRetryPolicy`] when `attempts` is
    /// zero.
    pub fn fixed(attempts: u32, wait_ms: u64) -> Result<Self, RetryPolicyError> {
        let n = NonZeroU32::new(attempts).ok_or_else(|| RetryPolicyError::InvalidRetryPolicy {
            reason: String::from("attempts must be non-zero"),
        })?;
        Self::new(n, BackoffStrategy::Fixed(Duration::from_millis(wait_ms)))
    }

    /// Convenience constructor for an exponential-backoff policy.
    ///
    /// # Errors
    ///
    /// Returns errors from [`RetryPolicy::new`].
    pub fn exponential(attempts: u32, base_ms: u64, cap_ms: u64) -> Result<Self, RetryPolicyError> {
        let n = NonZeroU32::new(attempts).ok_or_else(|| RetryPolicyError::InvalidRetryPolicy {
            reason: String::from("attempts must be non-zero"),
        })?;
        Self::new(
            n,
            BackoffStrategy::Exponential {
                base: Duration::from_millis(base_ms),
                cap: Duration::from_millis(cap_ms),
            },
        )
    }

    /// The ceiling on attempt count.
    #[must_use]
    pub const fn max_attempts(&self) -> u32 {
        self.max_attempts.get()
    }

    /// The configured backoff strategy.
    #[must_use]
    pub const fn backoff(&self) -> BackoffStrategy {
        self.backoff
    }

    /// Create a fresh [`RetryBudget`] for one execution run.
    #[must_use]
    pub fn budget(&self) -> RetryBudget {
        RetryBudget {
            remaining: self.max_attempts.get(),
            policy: *self,
            attempt: 0,
        }
    }

    /// Execute `f` up to `max_attempts` times, sleeping between attempts
    /// according to the backoff strategy.
    ///
    /// Retry continues only when the closure returns an `Err` where
    /// `E::is_retryable()` is `true` and budget remains.  Non-retryable
    /// errors propagate immediately.
    ///
    /// # Errors
    ///
    /// Returns [`RetryPolicyError::RetryExhausted`] when all attempts are
    /// consumed without success, wrapping the last error's display string.
    /// Returns the closure's non-retryable error (wrapped via `Into`) when one
    /// is encountered.
    pub fn execute<T, E, F>(&self, f: F) -> Result<T, RetryPolicyError>
    where
        F: Fn() -> Result<T, E>,
        E: RetryableError + fmt::Display,
    {
        let mut budget = self.budget();
        loop {
            budget.next_attempt()?;
            match f() {
                Ok(value) => return Ok(value),
                Err(err) if err.is_retryable() && budget.has_remaining() => {
                    // Transient error with budget remaining — loop.
                }
                Err(err) => {
                    return Err(RetryPolicyError::RetryExhausted {
                        attempts: self.max_attempts.get() - budget.remaining(),
                        last_error: err.to_string(),
                    });
                }
            }
        }
    }
}

// ── RetryBudget ───────────────────────────────────────────────────────────────

/// Execution-time state for one retry run.
///
/// Obtained from [`RetryPolicy::budget`] and consumed by
/// [`RetryBudget::next_attempt`].  `RetryBudget` is not `Copy` — it is a
/// one-shot mutable cursor.
#[derive(Debug)]
pub struct RetryBudget {
    remaining: u32,
    policy: RetryPolicy,
    attempt: u32,
}

impl RetryBudget {
    /// Returns `true` when at least one attempt slot remains.
    #[must_use]
    pub fn has_remaining(&self) -> bool {
        self.remaining > 0
    }

    /// The number of attempt slots remaining (including the current slot if
    /// `next_attempt` has not yet been called for this slot).
    #[must_use]
    pub fn remaining(&self) -> u32 {
        self.remaining
    }

    /// Consume one attempt slot.
    ///
    /// Sleeps according to the backoff strategy before the second and
    /// subsequent attempts.  Does not sleep on the first attempt (`attempt == 1`).
    ///
    /// # Errors
    ///
    /// Returns [`RetryPolicyError::RetryExhausted`] when no slots remain.
    pub fn next_attempt(&mut self) -> Result<(), RetryPolicyError> {
        if self.remaining == 0 {
            return Err(RetryPolicyError::RetryExhausted {
                attempts: self.policy.max_attempts.get(),
                last_error: String::from("budget exhausted"),
            });
        }
        self.attempt = self.attempt.saturating_add(1);
        self.remaining = self.remaining.saturating_sub(1);

        // Sleep before every attempt after the first.
        if self.attempt > 1 {
            let sleep = self.policy.backoff.sleep_for(self.attempt);
            if !sleep.is_zero() {
                thread::sleep(sleep);
            }
        }
        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── BackoffStrategy ───────────────────────────────────────────────────────

    #[test]
    fn no_retry_max_sleep_is_zero() {
        assert_eq!(BackoffStrategy::NoRetry.max_sleep(), Duration::ZERO);
    }

    #[test]
    fn fixed_max_sleep_equals_configured_duration() {
        let s = BackoffStrategy::Fixed(Duration::from_millis(200));
        assert_eq!(s.max_sleep(), Duration::from_millis(200));
    }

    #[test]
    fn exponential_max_sleep_equals_cap() {
        let s = BackoffStrategy::Exponential {
            base: Duration::from_millis(100),
            cap: Duration::from_secs(5),
        };
        assert_eq!(s.max_sleep(), Duration::from_secs(5));
    }

    #[test]
    fn exponential_sleep_for_attempt_1_equals_base() {
        let s = BackoffStrategy::Exponential {
            base: Duration::from_millis(100),
            cap: Duration::from_secs(5),
        };
        assert_eq!(s.sleep_for(1), Duration::from_millis(100));
    }

    #[test]
    fn exponential_sleep_for_attempt_2_equals_base_times_2() {
        let s = BackoffStrategy::Exponential {
            base: Duration::from_millis(100),
            cap: Duration::from_secs(5),
        };
        assert_eq!(s.sleep_for(2), Duration::from_millis(200));
    }

    #[test]
    fn exponential_sleep_is_capped() {
        let s = BackoffStrategy::Exponential {
            base: Duration::from_millis(100),
            cap: Duration::from_millis(300),
        };
        // attempt 5 → 2^4 * 100ms = 1600ms, capped at 300ms
        assert_eq!(s.sleep_for(5), Duration::from_millis(300));
    }

    #[test]
    fn total_max_wait_single_attempt_is_zero() {
        let s = BackoffStrategy::Fixed(Duration::from_millis(200));
        assert_eq!(s.total_max_wait(1), Duration::ZERO);
    }

    #[test]
    fn total_max_wait_three_attempts_is_two_sleeps() {
        let s = BackoffStrategy::Fixed(Duration::from_millis(200));
        assert_eq!(s.total_max_wait(3), Duration::from_millis(400));
    }

    #[test]
    fn backoff_strategy_display_no_retry() {
        assert_eq!(BackoffStrategy::NoRetry.to_string(), "NoRetry");
    }

    #[test]
    fn backoff_strategy_display_fixed() {
        let s = BackoffStrategy::Fixed(Duration::from_millis(100));
        assert!(s.to_string().starts_with("Fixed("));
    }

    #[test]
    fn backoff_strategy_display_exponential() {
        let s = BackoffStrategy::Exponential {
            base: Duration::from_millis(100),
            cap: Duration::from_secs(5),
        };
        assert!(s.to_string().starts_with("Exponential("));
    }

    // ── RetryPolicy ───────────────────────────────────────────────────────────

    #[test]
    fn no_retry_constant_has_one_attempt() {
        assert_eq!(RetryPolicy::NO_RETRY.max_attempts(), 1);
    }

    #[test]
    fn spawn_retry_3_has_three_attempts() {
        assert_eq!(RetryPolicy::SPAWN_RETRY_3.max_attempts(), 3);
    }

    #[test]
    fn new_rejects_no_retry_with_multiple_attempts() {
        let err = RetryPolicy::new(
            NonZeroU32::new(2).expect("2 is non-zero"),
            BackoffStrategy::NoRetry,
        );
        assert!(err.is_err());
    }

    #[test]
    fn new_rejects_exponential_with_cap_less_than_base() {
        let err = RetryPolicy::new(
            NonZeroU32::new(3).expect("3 is non-zero"),
            BackoffStrategy::Exponential {
                base: Duration::from_millis(500),
                cap: Duration::from_millis(100),
            },
        );
        assert!(err.is_err());
    }

    #[test]
    fn fixed_ctor_rejects_zero_attempts() {
        assert!(RetryPolicy::fixed(0, 200).is_err());
    }

    #[test]
    fn exponential_ctor_rejects_zero_attempts() {
        assert!(RetryPolicy::exponential(0, 100, 5_000).is_err());
    }

    #[test]
    fn fixed_ctor_succeeds() {
        let policy = RetryPolicy::fixed(3, 200).expect("valid");
        assert_eq!(policy.max_attempts(), 3);
    }

    // ── RetryBudget ───────────────────────────────────────────────────────────

    #[test]
    fn budget_starts_with_full_remaining() {
        let policy = RetryPolicy::NO_RETRY;
        let budget = policy.budget();
        assert_eq!(budget.remaining(), 1);
        assert!(budget.has_remaining());
    }

    #[test]
    fn budget_next_attempt_consumes_slot() {
        let policy = RetryPolicy::fixed(2, 0).expect("valid");
        let mut budget = policy.budget();
        budget.next_attempt().expect("first attempt");
        assert_eq!(budget.remaining(), 1);
    }

    #[test]
    fn budget_next_attempt_exhaustion_returns_error() {
        let policy = RetryPolicy::NO_RETRY;
        let mut budget = policy.budget();
        budget.next_attempt().expect("first attempt ok");
        let err = budget.next_attempt();
        assert!(err.is_err());
    }

    // ── RetryPolicyError ─────────────────────────────────────────────────────

    #[test]
    fn retry_exhausted_error_code_is_2240() {
        let e = RetryPolicyError::RetryExhausted {
            attempts: 3,
            last_error: String::from("boom"),
        };
        assert_eq!(e.error_code(), 2240);
    }

    #[test]
    fn invalid_policy_error_code_is_2241() {
        let e = RetryPolicyError::InvalidRetryPolicy {
            reason: String::from("zero"),
        };
        assert_eq!(e.error_code(), 2241);
    }

    #[test]
    fn backoff_overflow_error_code_is_2241() {
        let e = RetryPolicyError::BackoffOverflow {
            reason: String::from("cap<base"),
        };
        assert_eq!(e.error_code(), 2241);
    }

    #[test]
    fn retry_policy_error_not_retryable() {
        let e = RetryPolicyError::RetryExhausted {
            attempts: 1,
            last_error: String::from("x"),
        };
        assert!(!e.is_retryable());
    }

    #[test]
    fn retry_exhausted_display_contains_hle_2240() {
        let e = RetryPolicyError::RetryExhausted {
            attempts: 3,
            last_error: String::from("boom"),
        };
        assert!(e.to_string().contains("[HLE-2240]"));
    }

    #[test]
    fn invalid_policy_display_contains_hle_2241() {
        let e = RetryPolicyError::InvalidRetryPolicy {
            reason: String::from("test"),
        };
        assert!(e.to_string().contains("[HLE-2241]"));
    }

    // ── BackoffStrategy — additional ──────────────────────────────────────────

    #[test]
    fn no_retry_sleep_for_any_n_is_zero() {
        let s = BackoffStrategy::NoRetry;
        for n in 0..=5_u32 {
            assert_eq!(s.sleep_for(n), Duration::ZERO, "n={n}");
        }
    }

    #[test]
    fn fixed_sleep_for_every_n_equals_fixed_duration() {
        let d = Duration::from_millis(150);
        let s = BackoffStrategy::Fixed(d);
        for n in 1..=5_u32 {
            assert_eq!(s.sleep_for(n), d, "n={n}");
        }
    }

    #[test]
    fn exponential_sleep_for_attempt_3_equals_base_times_4() {
        let s = BackoffStrategy::Exponential {
            base: Duration::from_millis(100),
            cap: Duration::from_secs(10),
        };
        // attempt 3 → 2^2 * 100ms = 400ms
        assert_eq!(s.sleep_for(3), Duration::from_millis(400));
    }

    #[test]
    fn exponential_sleep_for_attempt_4_equals_base_times_8() {
        let s = BackoffStrategy::Exponential {
            base: Duration::from_millis(100),
            cap: Duration::from_secs(10),
        };
        // attempt 4 → 2^3 * 100ms = 800ms
        assert_eq!(s.sleep_for(4), Duration::from_millis(800));
    }

    #[test]
    fn exponential_sleep_for_very_large_n_does_not_overflow() {
        let s = BackoffStrategy::Exponential {
            base: Duration::from_millis(100),
            cap: Duration::from_secs(60),
        };
        // n=200 would be 2^199 * 100ms — saturating_mul clamps, cap applies.
        let sleep = s.sleep_for(200);
        assert_eq!(sleep, Duration::from_secs(60));
    }

    #[test]
    fn total_max_wait_zero_attempts_is_zero() {
        let s = BackoffStrategy::Fixed(Duration::from_millis(100));
        assert_eq!(s.total_max_wait(0), Duration::ZERO);
    }

    #[test]
    fn total_max_wait_no_retry_always_zero() {
        let s = BackoffStrategy::NoRetry;
        for n in 0..=5_u32 {
            assert_eq!(s.total_max_wait(n), Duration::ZERO, "n={n}");
        }
    }

    #[test]
    fn backoff_strategy_is_copy() {
        let s = BackoffStrategy::Fixed(Duration::from_millis(200));
        let t = s; // Copy
        assert_eq!(s, t);
    }

    // ── RetryPolicy — additional ──────────────────────────────────────────────

    #[test]
    fn no_retry_const_backoff_is_no_retry() {
        assert_eq!(RetryPolicy::NO_RETRY.backoff(), BackoffStrategy::NoRetry);
    }

    #[test]
    fn spawn_retry_3_backoff_is_fixed_200ms() {
        assert_eq!(
            RetryPolicy::SPAWN_RETRY_3.backoff(),
            BackoffStrategy::Fixed(Duration::from_millis(200))
        );
    }

    #[test]
    fn no_retry_fn_matches_constant() {
        assert_eq!(RetryPolicy::no_retry(), RetryPolicy::NO_RETRY);
    }

    #[test]
    fn exponential_ctor_creates_correct_policy() {
        let p = RetryPolicy::exponential(5, 100, 5_000).expect("ok");
        assert_eq!(p.max_attempts(), 5);
        if let BackoffStrategy::Exponential { base, cap } = p.backoff() {
            assert_eq!(base, Duration::from_millis(100));
            assert_eq!(cap, Duration::from_millis(5_000));
        } else {
            panic!("wrong backoff strategy");
        }
    }

    #[test]
    fn exponential_ctor_rejects_cap_less_than_base() {
        let err = RetryPolicy::exponential(3, 1000, 100);
        assert!(err.is_err());
    }

    #[test]
    fn retry_policy_is_copy() {
        let p = RetryPolicy::NO_RETRY;
        let q = p; // Copy
        assert_eq!(p, q);
    }

    #[test]
    fn retry_policy_execute_succeeds_on_first_try() {
        let policy = RetryPolicy::NO_RETRY;

        #[derive(Debug)]
        struct TestErr;
        impl RetryableError for TestErr {
            fn is_retryable(&self) -> bool {
                false
            }
        }
        impl fmt::Display for TestErr {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("test error")
            }
        }

        let result: Result<u32, RetryPolicyError> = policy.execute(|| Ok::<u32, TestErr>(42));
        assert_eq!(result.expect("success"), 42);
    }

    #[test]
    fn retry_policy_execute_exhausts_on_non_retryable_error() {
        let policy = RetryPolicy::fixed(3, 0).expect("ok");

        #[derive(Debug)]
        struct NonRetryable;
        impl RetryableError for NonRetryable {
            fn is_retryable(&self) -> bool {
                false
            }
        }
        impl fmt::Display for NonRetryable {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("non-retryable")
            }
        }

        let calls = std::cell::Cell::new(0_u32);
        let result: Result<u32, RetryPolicyError> = policy.execute(|| {
            calls.set(calls.get() + 1);
            Err::<u32, NonRetryable>(NonRetryable)
        });
        assert!(result.is_err());
        // Should stop after first non-retryable error (1 call only).
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn retry_policy_execute_retries_on_retryable_error() {
        let policy = RetryPolicy::fixed(3, 0).expect("ok");

        #[derive(Debug)]
        struct Transient;
        impl RetryableError for Transient {
            fn is_retryable(&self) -> bool {
                true
            }
        }
        impl fmt::Display for Transient {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("transient")
            }
        }

        let calls = std::cell::Cell::new(0_u32);
        let result: Result<u32, RetryPolicyError> = policy.execute(|| {
            calls.set(calls.get() + 1);
            if calls.get() == 3 {
                Ok(99)
            } else {
                Err::<u32, Transient>(Transient)
            }
        });
        assert_eq!(result.expect("succeeded on 3rd try"), 99);
        assert_eq!(calls.get(), 3);
    }

    // ── RetryBudget — additional ──────────────────────────────────────────────

    #[test]
    fn budget_has_remaining_false_after_all_slots_consumed() {
        let policy = RetryPolicy::fixed(2, 0).expect("ok");
        let mut budget = policy.budget();
        budget.next_attempt().expect("1st");
        budget.next_attempt().expect("2nd");
        assert!(!budget.has_remaining());
    }

    #[test]
    fn budget_remaining_decrements_correctly() {
        let policy = RetryPolicy::fixed(4, 0).expect("ok");
        let mut budget = policy.budget();
        assert_eq!(budget.remaining(), 4);
        budget.next_attempt().expect("1st");
        assert_eq!(budget.remaining(), 3);
        budget.next_attempt().expect("2nd");
        assert_eq!(budget.remaining(), 2);
    }

    #[test]
    fn budget_exhausted_error_code_is_2240() {
        let policy = RetryPolicy::NO_RETRY;
        let mut budget = policy.budget();
        budget.next_attempt().expect("1st");
        let err = budget.next_attempt().expect_err("exhausted");
        assert_eq!(err.error_code(), 2240);
    }

    // ── RetryPolicyError — additional ────────────────────────────────────────

    #[test]
    fn retry_exhausted_display_contains_attempt_count() {
        let e = RetryPolicyError::RetryExhausted {
            attempts: 5,
            last_error: String::from("boom"),
        };
        assert!(e.to_string().contains('5'));
    }

    #[test]
    fn backoff_overflow_display_contains_hle_2241() {
        let e = RetryPolicyError::BackoffOverflow {
            reason: String::from("cap<base"),
        };
        assert!(e.to_string().contains("[HLE-2241]"));
    }

    #[test]
    fn retry_policy_error_implements_std_error() {
        let e: Box<dyn std::error::Error> = Box::new(RetryPolicyError::RetryExhausted {
            attempts: 1,
            last_error: String::from("x"),
        });
        assert!(e.to_string().contains("[HLE-2240]"));
    }

    #[test]
    fn retry_policy_error_clone_equality() {
        let e = RetryPolicyError::InvalidRetryPolicy {
            reason: String::from("test"),
        };
        assert_eq!(e.clone(), e);
    }

    #[test]
    fn invalid_retry_policy_error_is_not_retryable() {
        let e = RetryPolicyError::InvalidRetryPolicy {
            reason: String::from("x"),
        };
        assert!(!e.is_retryable());
    }

    #[test]
    fn backoff_overflow_error_is_not_retryable() {
        let e = RetryPolicyError::BackoffOverflow {
            reason: String::from("cap<base"),
        };
        assert!(!e.is_retryable());
    }
}
